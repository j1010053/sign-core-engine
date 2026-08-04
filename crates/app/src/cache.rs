//! 派生視圖快取(流 D 框架 §6.2)。
//!
//! # 核心主張:**快取身分 ≠ 依賴失效**
//!
//! 一般作法是維護一張「誰受誰影響」表:某節點的 Language 變了 → 找出所有與它
//! 比較過的 diff → 逐一作廢。N 個節點下,改一個節點要觸碰 O(N) 的 pair cache。
//!
//! 本專案不需要那張表,因為**節點是 immutable 且內容定址**。只要鍵**完整涵蓋
//! 全部輸入**,輸入一變就自然 miss;舊項留著不會給出錯答案,之後垃圾回收即可。
//!
//! 故拆成兩件事:
//!
//! | | 用途 | 住哪 |
//! |---|---|---|
//! | **Cache identity** | **正確性**。鍵完整涵蓋輸入 | 本模組 |
//! | **Dependency invalidation** | **只**用於 UI subscription(該重畫哪個面板) | UI,不影響正確性 |
//!
//! # 鍵為什麼是「大輸入取 digest、小輸入存值」
//!
//! 框架原文寫「完整 input digest tuple 為鍵」。實作時對小輸入**不取 digest**:
//!
//! - 文件很大且比較昂貴 → 取內容 digest;
//! - `LexiconFilter` / `ViewConfig` / `GroupingOverride` 都很小,且已是
//!   `Eq + Ord`-able → **直接存值**。
//!
//! 理由是安全:替每個設定型別手寫一份 canonical 字串再雜湊,等於多開一個
//! 「兩份不同設定算出同一個鍵」的洞,而那種錯誤是**靜默回傳別人的結果**。
//! 直接存值沒有碰撞可能,成本只是幾十個位元組。
//!
//! # 誠實記下的一個保守處
//!
//! 文件 digest 取自 `LanguageDocument::source()` 的 sha256。兩個**內容相同但
//! 來自不同節點**的文件因此共用快取項(正確,且是想要的);反過來,同一份
//! 語言若 source 文字有無關差異(例如註解),會白白 miss 一次。
//! 那是**保守方向**——寧可多算一次,不可回傳錯的。

use conlang_language::{sha256_hex, LanguageDocument};
use std::collections::BTreeMap;

/// 一份文件的內容指紋。**鍵的一部分,不是身分**——同內容不同節點會得到同一個。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentDigest(String);

impl ContentDigest {
    pub fn of(document: &LanguageDocument) -> ContentDigest {
        ContentDigest(sha256_hex(document.source().as_bytes()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 一個以「完整輸入」為鍵的快取。
///
/// `K` 必須涵蓋**全部**影響 `V` 的輸入——這是正確性的唯一依據,
/// 沒有任何失效通知在背後補救。
#[derive(Debug)]
pub struct QueryCache<K: Ord, V> {
    entries: BTreeMap<K, V>,
    hits: u64,
    misses: u64,
}

impl<K: Ord, V> Default for QueryCache<K, V> {
    fn default() -> Self {
        QueryCache {
            entries: BTreeMap::new(),
            hits: 0,
            misses: 0,
        }
    }
}

impl<K: Ord + Clone, V: Clone> QueryCache<K, V> {
    pub fn new() -> QueryCache<K, V> {
        QueryCache::default()
    }

    /// 取;沒有就用 `compute` 算一份存起來。
    ///
    /// `compute` 取 `FnOnce` 而非值:**沒命中才算**,否則快取毫無意義。
    pub fn get_or_insert_with(&mut self, key: K, compute: impl FnOnce() -> V) -> V {
        if let Some(hit) = self.entries.get(&key) {
            self.hits += 1;
            return hit.clone();
        }
        self.misses += 1;
        let value = compute();
        self.entries.insert(key, value.clone());
        value
    }

    /// 只查不算。
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.entries.get(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 命中/未命中次數——UI 要顯示「快取有沒有在work」時的唯一依據。
    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    /// 垃圾回收:只留 `keep` 認可的鍵。
    ///
    /// **這不是失效**——被丟掉的項不是「錯的」,只是不再值得佔記憶體。
    /// 正確性完全由鍵保證,所以什麼時候回收、回收多少,都不影響答案。
    pub fn retain(&mut self, keep: impl Fn(&K) -> bool) {
        self.entries.retain(|key, _| keep(key));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// 詞典視圖的鍵:文件內容 + 過濾條件 + 呈現設定。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LexiconKey {
    pub document: ContentDigest,
    /// 直接存值,不取 digest(見模組文件)。
    pub filter: LexiconFilterKey,
    pub view: ViewKey,
}

/// `LexiconFilter` 的可排序鏡像。
///
/// 不直接用 `conlang_query::LexiconFilter` 當鍵,是因為它沒有 `Ord`
/// (也不該為了當鍵而在 query 側加——那會讓純函數層去遷就快取層的需求)。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LexiconFilterKey {
    pub category: Option<String>,
    pub gloss_contains: Option<String>,
    pub without_underlying_form: bool,
}

impl From<&conlang_query::LexiconFilter> for LexiconFilterKey {
    fn from(filter: &conlang_query::LexiconFilter) -> Self {
        LexiconFilterKey {
            category: filter.category.clone(),
            gloss_contains: filter.gloss_contains.clone(),
            without_underlying_form: filter.without_underlying_form,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewKey {
    pub sort: u8,
}

impl From<&conlang_query::ViewConfig> for ViewKey {
    fn from(view: &conlang_query::ViewConfig) -> Self {
        ViewKey {
            sort: match view.sort {
                conlang_query::SortKey::Name => 0,
                conlang_query::SortKey::UnderlyingForm => 1,
                conlang_query::SortKey::Gloss => 2,
            },
        }
    }
}

/// 差異向量的鍵。**有序對**——`diff_vector(a, b)` 與 `(b, a)` 的生滅互換,
/// 不是同一個結果,鍵不得把兩者混為一談。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiffKey {
    pub before: ContentDigest,
    pub after: ContentDigest,
}

impl DiffKey {
    pub fn new(before: &LanguageDocument, after: &LanguageDocument) -> DiffKey {
        DiffKey {
            before: ContentDigest::of(before),
            after: ContentDigest::of(after),
        }
    }
}
