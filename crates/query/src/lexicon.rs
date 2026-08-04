//! 詞典視圖:過濾 + 排序 + 四維摘要。
//!
//! # 三個容易做錯的地方
//!
//! **① 範疇過濾委給 ontology 的單一判定出處。** 以 `Nominal` 過濾時,
//! `belongs Noun` 的 sign 必須入選。
//!
//! 誠實說明:此處的輸入 (`sign_categories`) **恆為閉包**,而
//! `OntologyRegistry::categories_satisfy` 的文件已寫明「對閉包輸入,
//! `category_is_a` 與字串相等同義」——所以在本模組換成 `contains` 是**等價**的,
//! 沒有任何測試能區分。
//!
//! 仍然委派而不自己寫,是因為 `f1ee6aa` 的教訓:八個判定點寫成四種形態時,
//! 「它們是否等價」得靠手推,而手推當時就推錯了。這裡多一個自寫的判定,
//! 就是把那個問題再種回去一次;而一旦哪天輸入不再是閉包
//! (`SemanticDocumentV1` 由外部反序列化,可能只給葉範疇),等價就不成立了。
//!
//! **② gloss 住義項,不是 `sem.gloss` Def。** P71 §4.1 之後 `sem.gloss` 已退出
//! Def 路徑,內容住 `sem: senses:`。故取 gloss 一律經 [`SemNode`],不掃 Def。
//!
//! **③ View Config 只影響呈現,不影響入選集合。** 這是「派生視圖永不回寫資料層」
//! 的同一條線在讀取側的體現:換一個視角不該讓詞典少掉幾個詞,否則使用者會以為
//! 詞不見了。故 [`LexiconFilter`] 決定「有哪些」、[`ViewConfig`] 只決定「怎麼排」。

use conlang_language::sem::SemNode;
use conlang_language::{CompiledSystem, Dim, SignDef};

/// 「有哪些詞」——只有這裡能改變入選集合。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LexiconFilter {
    /// 範疇約束。**走 belongs 閉包**,不是字串相等。
    pub category: Option<String>,
    /// gloss 子字串(區分大小寫;大小寫摺疊是語言相關的,不在引擎猜)。
    pub gloss_contains: Option<String>,
    /// 是否收沒有底層形的 sign(如純構式)。預設**收**——藏起來會讓
    /// 「我的詞去哪了」變成無從查起的問題。
    pub without_underlying_form: bool,
}

impl LexiconFilter {
    /// 全收。
    pub fn all() -> LexiconFilter {
        LexiconFilter {
            without_underlying_form: true,
            ..LexiconFilter::default()
        }
    }

    pub fn with_category(mut self, category: impl Into<String>) -> LexiconFilter {
        self.category = Some(category.into());
        self
    }

    pub fn with_gloss_containing(mut self, needle: impl Into<String>) -> LexiconFilter {
        self.gloss_contains = Some(needle.into());
        self
    }
}

/// 排序鍵。**只影響順序,不影響入選集合。**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// sign 名(= `.lang` 的宣告名)。
    #[default]
    Name,
    /// 底層形;無 UR 者排在最後。
    UnderlyingForm,
    /// 主義項 gloss;無 gloss 者排在最後。
    Gloss,
}

/// 「怎麼呈現」——不得改變入選集合(見模組文件 ③)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewConfig {
    pub sort: SortKey,
}

/// 一個詞條。四維摘要都是**投影**,不是另存的副本(P39)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexiconEntry {
    pub name: String,
    /// belongs 閉包(維度中立,P38 v0.2)。
    pub categories: Vec<String>,
    /// 底層形 UR;表層形永不儲存(P1)。
    pub underlying_form: Option<String>,
    /// 主義項的 gloss。多義時取主義項(核心優先)。
    pub gloss: Option<String>,
    /// 全部義項——多義 = 多個義項節點,不是多個 gloss 欄位(P71 §4.1)。
    pub senses: Vec<(String, String)>,
    /// 各維的有效 Defs(繼承 ⊕ 本地),依 phon/syn/sem/prag 序。
    pub dimensions: Vec<(Dim, Vec<(String, String)>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexicon {
    pub entries: Vec<LexiconEntry>,
    /// 過濾前的 sign 總數——「篩掉了幾個」是使用者要看得見的數字,
    /// 否則空結果無從分辨是「沒有這種詞」還是「條件寫錯」。
    pub total_before_filter: usize,
}

fn entry_of(sign: &SignDef, system: &CompiledSystem) -> LexiconEntry {
    let semantics = SemNode::of_sign(sign, &system.ontology);
    let dimensions = Dim::all()
        .into_iter()
        .map(|dim| (dim, sign.project(dim, &system.ontology).defs))
        .collect();
    LexiconEntry {
        name: sign.name.clone(),
        categories: system.ontology.sign_categories(sign),
        underlying_form: sign.underlying_form().map(str::to_owned),
        gloss: semantics.field("gloss").map(str::to_owned),
        senses: semantics
            .senses
            .iter()
            .map(|sense| (sense.name.clone(), sense.gloss.clone()))
            .collect(),
        dimensions,
    }
}

fn admits(entry: &LexiconEntry, filter: &LexiconFilter, system: &CompiledSystem) -> bool {
    if let Some(required) = &filter.category {
        // 單一判定出處:不在這裡寫 `categories.contains(required)`。
        if !system
            .ontology
            .categories_satisfy(&entry.categories, required)
        {
            return false;
        }
    }
    if let Some(needle) = &filter.gloss_contains {
        match &entry.gloss {
            Some(gloss) if gloss.contains(needle.as_str()) => {}
            _ => return false,
        }
    }
    if !filter.without_underlying_form && entry.underlying_form.is_none() {
        return false;
    }
    true
}

/// 詞典視圖。**純函數**:同輸入同輸出,不碰任何外部狀態。
///
/// `filter` 決定有哪些、`view` 只決定怎麼排(見模組文件 ③)。
pub fn lexicon(system: &CompiledSystem, filter: &LexiconFilter, view: &ViewConfig) -> Lexicon {
    let signs = &system.effective_language().signs;
    let mut entries: Vec<LexiconEntry> = signs
        .iter()
        .map(|sign| entry_of(sign, system))
        .filter(|entry| admits(entry, filter, system))
        .collect();

    // 排序一律以 name 收尾 ⇒ 全序,故輸出決定性(P26 同精神:視圖也不得抖動)。
    match view.sort {
        SortKey::Name => entries.sort_by(|a, b| a.name.cmp(&b.name)),
        SortKey::UnderlyingForm => entries.sort_by(|a, b| {
            // None 排最後:`Option` 的自然序是 None < Some,故反過來比。
            (a.underlying_form.is_none(), &a.underlying_form, &a.name).cmp(&(
                b.underlying_form.is_none(),
                &b.underlying_form,
                &b.name,
            ))
        }),
        SortKey::Gloss => entries.sort_by(|a, b| {
            (a.gloss.is_none(), &a.gloss, &a.name).cmp(&(b.gloss.is_none(), &b.gloss, &b.name))
        }),
    }

    Lexicon {
        entries,
        total_before_filter: signs.len(),
    }
}
