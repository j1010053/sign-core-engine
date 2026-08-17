//! 步驟 21-1 出口:詞典視圖是**純函數**,且三個易錯處各有判別測試。
//!
//! 1. 範疇過濾走 ontology 閉包,不是字串相等;
//! 2. gloss 來自義項,不是 `sem.gloss` Def(P71 §4.1);
//! 3. View Config 只影響順序,**不影響入選集合**。

use conlang_language::{compile_system, Language};
use conlang_query::{lexicon, LexiconFilter, SortKey, ViewConfig};

/// `dog` 是 `Noun`(閉包含 `Nominal`),`run` 是 `Verb`,`gizmo` 無 UR。
const SOURCE: &str = r#"
sign dog:
    belongs Noun
    phon:
        /dog/
    sem:
        senses:
            core = DOG
            pet = PET

sign run:
    belongs Verb
    phon:
        /run/
    sem:
        senses:
            core = RUN

sign apple:
    belongs Noun
    phon:
        /apl/
    sem:
        senses:
            core = APPLE

sign gizmo:
    belongs Noun
    sem:
        senses:
            core = GIZMO
"#;

fn system() -> conlang_language::CompiledSystem {
    compile_system(Language::parse(SOURCE).expect("parses")).expect("compiles")
}

fn names(filter: &LexiconFilter) -> Vec<String> {
    lexicon(&system(), filter, &ViewConfig::default())
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

// ── ① 範疇過濾走閉包 ──────────────────────────────────────────────────────

/// 🔑 以 `Noun` 過濾時,`belongs Noun` 的 sign **必須**入選。
///
/// **本測試釘的是行為,不是實作選擇。** 誠實記下:把 `categories_satisfy` 換成
/// `categories.contains(required)` 本測試**不會紅**——`entry.categories` 恆為
/// `sign_categories` 的閉包,而 `categories_satisfy` 自己的文件已寫明
/// 「對閉包輸入兩者同義」。那是**等價突變**,不是測試缺口。
///
/// 仍走 `categories_satisfy` 的理由見 `lexicon.rs` 的模組文件 ①:單一判定出處。
#[test]
fn filtering_by_a_supertype_matches_signs_declared_with_a_subtype() {
    let found = names(&LexiconFilter::all().with_category("Nominal"));
    assert!(
        found.contains(&"dog".to_owned()),
        "belongs Noun 應被 Noun 選中:{found:?}"
    );
    assert!(!found.contains(&"run".to_owned()), "Verb 不該入選:{found:?}");

    // 前提檢查:`dog` 的本地宣告確實不是 `Noun`,否則本測試會因巧合而過
    let system = system();
    let dog = system
        .effective_language()
        .signs
        .iter()
        .find(|sign| sign.name == "dog")
        .expect("dog 在");
    let closure = system.ontology.sign_categories(dog);
    assert!(closure.contains(&"Nominal".to_owned()), "閉包含 Nominal");
    assert!(
        !dog.items.iter().any(|item| matches!(
            item,
            conlang_language::SignItem::TraitMount { name: name, kind: conlang_language::TraitMountKind::Declaration } if name == "Nominal"
        )),
        "本地並未直接 belongs Nominal——這正是本測試要區分的"
    );
}

/// 反向控制組:不存在的範疇篩出空集合,而非全收。
#[test]
fn an_unknown_category_selects_nothing_rather_than_everything() {
    assert!(names(&LexiconFilter::all().with_category("Preposition")).is_empty());
}

// ── ② gloss 來自義項 ─────────────────────────────────────────────────────

/// gloss 由主義項投影而來,且全部義項都列得出(多義 = 多個義項節點)。
#[test]
fn gloss_projects_from_senses_and_polysemy_is_visible() {
    let view = lexicon(&system(), &LexiconFilter::all(), &ViewConfig::default());
    let dog = view
        .entries
        .iter()
        .find(|entry| entry.name == "dog")
        .expect("dog 在");

    assert_eq!(dog.gloss.as_deref(), Some("DOG"), "主義項(core 優先)");
    assert_eq!(
        dog.senses,
        vec![
            ("core".to_owned(), "DOG".to_owned()),
            ("pet".to_owned(), "PET".to_owned()),
        ],
        "多義的兩個義項都要在"
    );
}

/// gloss 子字串過濾。
#[test]
fn filtering_by_gloss_substring_selects_only_matching_entries() {
    assert_eq!(names(&LexiconFilter::all().with_gloss_containing("DOG")), vec!["dog"]);
    assert!(names(&LexiconFilter::all().with_gloss_containing("ZZZ")).is_empty());
}

// ── ③ View 只影響順序 ────────────────────────────────────────────────────

/// 🔑 **換排序不得改變入選集合**。
///
/// 判別性:若哪天把過濾條件塞進 `ViewConfig`,兩邊的集合就會不同。
#[test]
fn the_view_config_reorders_but_never_changes_which_entries_are_included() {
    let system = system();
    let filter = LexiconFilter::all();

    let by_name = lexicon(&system, &filter, &ViewConfig { sort: SortKey::Name });
    let by_gloss = lexicon(&system, &filter, &ViewConfig { sort: SortKey::Gloss });
    let by_form = lexicon(
        &system,
        &filter,
        &ViewConfig {
            sort: SortKey::UnderlyingForm,
        },
    );

    let set = |view: &conlang_query::Lexicon| {
        let mut names: Vec<&str> = view.entries.iter().map(|e| e.name.as_str()).collect();
        names.sort();
        names.join(",")
    };
    assert_eq!(set(&by_name), set(&by_gloss));
    assert_eq!(set(&by_name), set(&by_form));

    // 而順序確實不同,否則上面三個相等只是因為排序沒作用
    fn order(view: &conlang_query::Lexicon) -> Vec<&str> {
        view.entries.iter().map(|e| e.name.as_str()).collect()
    }
    assert_eq!(order(&by_name), vec!["apple", "dog", "gizmo", "run"]);
    assert_eq!(order(&by_gloss), vec!["apple", "dog", "gizmo", "run"]);
    // apl < dog < run,無 UR 的 gizmo 墊底
    assert_eq!(order(&by_form), vec!["apple", "dog", "run", "gizmo"]);
}

/// 無 UR 的 sign 預設收,可用旗標排除;`total_before_filter` 使「篩掉幾個」可見。
#[test]
fn signs_without_an_underlying_form_are_included_by_default_and_the_total_is_reported() {
    let system = system();
    let all = lexicon(&system, &LexiconFilter::all(), &ViewConfig::default());
    assert!(all.entries.iter().any(|entry| entry.name == "gizmo"));
    assert_eq!(all.total_before_filter, 4);

    let only_pronounceable = lexicon(
        &system,
        &LexiconFilter::default(),
        &ViewConfig::default(),
    );
    assert!(!only_pronounceable
        .entries
        .iter()
        .any(|entry| entry.name == "gizmo"));
    assert_eq!(
        only_pronounceable.total_before_filter, 4,
        "分母是過濾**前**的總數,否則看不出篩掉了幾個"
    );
}

/// 決定性:同輸入兩次逐欄位相同(視圖不得抖動)。
#[test]
fn the_same_input_yields_the_same_view() {
    let system = system();
    let filter = LexiconFilter::all();
    let view = ViewConfig::default();
    assert_eq!(lexicon(&system, &filter, &view), lexicon(&system, &filter, &view));
}

/// 四維摘要是**投影**:`phon` 維帶得出 UR。
#[test]
fn each_entry_carries_its_four_dimensional_projection() {
    let system = system();
    let view = lexicon(&system, &LexiconFilter::all(), &ViewConfig::default());
    let dog = view.entries.iter().find(|e| e.name == "dog").unwrap();

    assert_eq!(dog.underlying_form.as_deref(), Some("dog"), "斜線已去掉");
    assert_eq!(dog.dimensions.len(), 4, "phon/syn/sem/prag 各一");
    let phon = &dog
        .dimensions
        .iter()
        .find(|(dim, _)| *dim == conlang_language::Dim::Phon)
        .expect("phon 維在")
        .1;
    assert!(
        phon.iter().any(|(path, value)| path == "phon" && value.contains("dog")),
        "phon 維的有效 Def 應含 UR:{phon:?}"
    );
}
