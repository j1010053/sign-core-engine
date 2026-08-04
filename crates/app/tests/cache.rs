//! 步驟 21-5 出口:**快取身分保證正確性,不靠任何失效通知**。
//!
//! 這一組全部在證同一件事的不同面向:**鍵完整涵蓋輸入**。
//! 若鍵漏了任何一項輸入,就會出現「換了輸入卻拿到舊答案」——而那是靜默的錯,
//! 沒有任何機制會補救(因為本設計刻意不維護依賴表)。

use conlang_app::cache::{
    ContentDigest, DiffKey, LexiconFilterKey, LexiconKey, QueryCache, ViewKey,
};
use conlang_language::{compile_system, CompiledSystem, Language, LanguageDocument};
use conlang_query::{diff_vector, lexicon, DiffVector, Lexicon, LexiconFilter, SortKey, ViewConfig};

const ONE: &str = "sign dog:\n    belongs Noun\n    phon:\n        /dog/\n    sem:\n        \
senses:\n            core = DOG\nsign run:\n    belongs Verb\n    phon:\n        /run/\n";

const TWO: &str = "sign dog:\n    belongs Noun\n    phon:\n        /dog/\n    sem:\n        \
senses:\n            core = DOG\nsign walk:\n    belongs Verb\n    phon:\n        /wok/\n";

/// 與 [`ONE`] 共用 sign id(同 namespace、同文件序),但**多一個詞**——
/// 故 `diff(ONE→THREE)` 是「生一個」、反向是「滅一個」,方向分得開。
const THREE: &str = "sign dog:\n    belongs Noun\n    phon:\n        /dog/\n    sem:\n        \
senses:\n            core = DOG\nsign run:\n    belongs Verb\n    phon:\n        /run/\n\
sign swim:\n    belongs Verb\n    phon:\n        /swim/\n";

fn document(source: &str, namespace: &str) -> LanguageDocument {
    LanguageDocument::import_new_root(source, namespace).expect("parses")
}

fn system(source: &str) -> CompiledSystem {
    compile_system(Language::parse(source).expect("parses")).expect("compiles")
}

fn key(document: &LanguageDocument, filter: &LexiconFilter, view: &ViewConfig) -> LexiconKey {
    LexiconKey {
        document: ContentDigest::of(document),
        filter: LexiconFilterKey::from(filter),
        view: ViewKey::from(view),
    }
}

// ── 鍵完整涵蓋輸入 ───────────────────────────────────────────────────────

/// 同輸入 ⇒ 命中,且**不重算**。
///
/// 判別性:`compute` 用一個計數器,第二次若被呼叫就表示沒命中。
#[test]
fn identical_inputs_hit_the_cache_and_do_not_recompute() {
    let document = document(ONE, "cache:a");
    let system = system(ONE);
    let (filter, view) = (LexiconFilter::all(), ViewConfig::default());
    let mut cache: QueryCache<LexiconKey, Lexicon> = QueryCache::new();

    let mut computed = 0;
    let run = |cache: &mut QueryCache<LexiconKey, Lexicon>, computed: &mut i32| {
        cache.get_or_insert_with(key(&document, &filter, &view), || {
            *computed += 1;
            lexicon(&system, &filter, &view)
        })
    };

    let first = run(&mut cache, &mut computed);
    let second = run(&mut cache, &mut computed);
    assert_eq!(computed, 1, "第二次必須命中,不得重算");
    assert_eq!(first, second);
    assert_eq!(cache.stats(), (1, 1), "一次命中、一次未命中");
}

/// 🔑 **換文件 ⇒ 不得命中。**
///
/// 判別性:若鍵漏了文件那一項,這裡會拿到第一份文件的詞典——靜默的錯答案。
#[test]
fn a_different_document_never_returns_the_previous_answer() {
    let (a, b) = (document(ONE, "cache:a"), document(TWO, "cache:b"));
    let (sa, sb) = (system(ONE), system(TWO));
    let (filter, view) = (LexiconFilter::all(), ViewConfig::default());
    let mut cache: QueryCache<LexiconKey, Lexicon> = QueryCache::new();

    let first =
        cache.get_or_insert_with(key(&a, &filter, &view), || lexicon(&sa, &filter, &view));
    let second =
        cache.get_or_insert_with(key(&b, &filter, &view), || lexicon(&sb, &filter, &view));

    assert_ne!(first, second, "兩份文件的詞典不同");
    assert!(first.entries.iter().any(|e| e.name == "run"));
    assert!(second.entries.iter().any(|e| e.name == "walk"));
    assert!(!second.entries.iter().any(|e| e.name == "run"), "不得混入前一份");
    assert_eq!(cache.len(), 2);
}

/// 🔑 **換過濾條件 ⇒ 不得命中。**
#[test]
fn a_different_filter_never_returns_the_previous_answer() {
    let document = document(ONE, "cache:a");
    let system = system(ONE);
    let view = ViewConfig::default();
    let mut cache: QueryCache<LexiconKey, Lexicon> = QueryCache::new();

    let all = LexiconFilter::all();
    let nouns = LexiconFilter::all().with_category("Nominal");

    let a = cache.get_or_insert_with(key(&document, &all, &view), || {
        lexicon(&system, &all, &view)
    });
    let b = cache.get_or_insert_with(key(&document, &nouns, &view), || {
        lexicon(&system, &nouns, &view)
    });

    assert_eq!(a.entries.len(), 2);
    assert_eq!(b.entries.len(), 1, "只剩名詞");
    assert_eq!(cache.stats(), (0, 2), "兩次都是未命中");
}

/// 🔑 **換呈現設定 ⇒ 不得命中**(排序影響輸出)。
#[test]
fn a_different_view_never_returns_the_previous_answer() {
    let document = document(ONE, "cache:a");
    let system = system(ONE);
    let filter = LexiconFilter::all();
    let mut cache: QueryCache<LexiconKey, Lexicon> = QueryCache::new();

    let by_name = ViewConfig { sort: SortKey::Name };
    let by_form = ViewConfig {
        sort: SortKey::UnderlyingForm,
    };
    cache.get_or_insert_with(key(&document, &filter, &by_name), || {
        lexicon(&system, &filter, &by_name)
    });
    cache.get_or_insert_with(key(&document, &filter, &by_form), || {
        lexicon(&system, &filter, &by_form)
    });

    assert_eq!(cache.len(), 2, "兩種呈現各一項");
}

/// 🔑 **`diff` 的鍵是有序對。**
///
/// `diff_vector(a, b)` 與 `(b, a)` 的生滅互換,不是同一個結果。
/// 判別性:若鍵把兩者視為同一組,第二次會命中並回傳生滅顛倒的答案。
#[test]
fn the_diff_key_distinguishes_direction() {
    // **同 namespace**:sign 以 SignId 對齊,故多出來的那個才算「生」。
    // 不同 namespace 會讓每個 sign 都對不上(全生全滅),方向就分不開了。
    let (a, b) = (document(ONE, "cache:d"), document(THREE, "cache:d"));
    let mut cache: QueryCache<DiffKey, DiffVector> = QueryCache::new();

    let forward = cache.get_or_insert_with(DiffKey::new(&a, &b), || diff_vector(&a, &b));
    let backward = cache.get_or_insert_with(DiffKey::new(&b, &a), || diff_vector(&b, &a));

    assert_eq!(cache.len(), 2, "兩個方向是兩項");
    assert_eq!(forward.born, backward.died, "生滅互換");
    assert_eq!(forward.died, backward.born);
    assert_ne!(
        (forward.born, forward.died),
        (backward.born, backward.died),
        "前提:這兩份文件確實一生一滅,否則本測試無判別性"
    );
}

// ── 舊項不必主動作廢 ─────────────────────────────────────────────────────

/// 🔑 **回到舊輸入時,舊項仍然正確可用。**
///
/// 這是「不維護依賴表」的正當性所在:A → B → A 的第三步必須命中 A 的舊項,
/// 而且那一項**沒有因為中間算過 B 而變髒**。
///
/// 若正確性靠的是失效通知而非鍵,這種來回切換就得每次重算。
#[test]
fn returning_to_an_earlier_input_hits_the_still_valid_old_entry() {
    let (a, b) = (document(ONE, "cache:a"), document(TWO, "cache:b"));
    let (sa, sb) = (system(ONE), system(TWO));
    let (filter, view) = (LexiconFilter::all(), ViewConfig::default());
    let mut cache: QueryCache<LexiconKey, Lexicon> = QueryCache::new();

    let first = cache.get_or_insert_with(key(&a, &filter, &view), || lexicon(&sa, &filter, &view));
    cache.get_or_insert_with(key(&b, &filter, &view), || lexicon(&sb, &filter, &view));
    let again = cache.get_or_insert_with(key(&a, &filter, &view), || {
        panic!("回到 A 必須命中,不該重算")
    });

    assert_eq!(first, again);
    assert_eq!(cache.stats(), (1, 2));
    // 且與現算的結果一致——命中的不是一份過期資料
    assert_eq!(again, lexicon(&sa, &filter, &view));
}

/// 垃圾回收**不影響正確性**——丟掉只是少一次命中,不是答案變了。
#[test]
fn garbage_collection_costs_a_recompute_but_never_correctness() {
    let (a, b) = (document(ONE, "cache:a"), document(TWO, "cache:b"));
    let (sa, sb) = (system(ONE), system(TWO));
    let (filter, view) = (LexiconFilter::all(), ViewConfig::default());
    let mut cache: QueryCache<LexiconKey, Lexicon> = QueryCache::new();

    let expected = cache.get_or_insert_with(key(&a, &filter, &view), || {
        lexicon(&sa, &filter, &view)
    });
    cache.get_or_insert_with(key(&b, &filter, &view), || lexicon(&sb, &filter, &view));
    assert_eq!(cache.len(), 2);

    // 只留 B 的
    let keep = key(&b, &filter, &view);
    cache.retain(|k| k == &keep);
    assert_eq!(cache.len(), 1);
    assert!(cache.peek(&key(&a, &filter, &view)).is_none());

    // 再問 A:重算一次,答案不變
    let recomputed =
        cache.get_or_insert_with(key(&a, &filter, &view), || lexicon(&sa, &filter, &view));
    assert_eq!(recomputed, expected, "回收之後算出來的必須一模一樣");
}

/// 內容相同的兩份文件共用快取項——內容定址的直接紅利。
///
/// 判別性:若 digest 取自節點身分而非內容,這裡會是兩項。
#[test]
fn two_documents_with_identical_content_share_one_entry() {
    let (a, b) = (document(ONE, "cache:a"), document(ONE, "cache:b"));
    assert_ne!(
        a.identities().root_namespace,
        b.identities().root_namespace,
        "前提:身分不同"
    );
    assert_eq!(
        ContentDigest::of(&a),
        ContentDigest::of(&b),
        "但內容相同 ⇒ 同一個 digest"
    );
}

/// 空快取的統計是 (0, 0),`peek` 不算命中。
#[test]
fn peeking_does_not_count_as_a_hit() {
    let document = document(ONE, "cache:a");
    let (filter, view) = (LexiconFilter::all(), ViewConfig::default());
    let cache: QueryCache<LexiconKey, Lexicon> = QueryCache::new();
    assert!(cache.is_empty());
    assert_eq!(cache.stats(), (0, 0));
    assert!(cache.peek(&key(&document, &filter, &view)).is_none());
    assert_eq!(cache.stats(), (0, 0), "peek 不進統計");
}
