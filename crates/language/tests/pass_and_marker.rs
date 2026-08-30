//! `pass` / 空塊警告 / `marker trait` —— 塊的「空」有三種意思,語法要分得開。
//!
//! ## 這一組在補什麼
//!
//! 空塊一直是合法的(`==` 後面直接接 `==` 就是),而且 round-trip 逐位元穩定。
//! 問題是它**啞**:看不出是刻意留白、還是寫到一半。
//!
//! 三條規則各管一件事:
//!
//! 1. **空塊發警告**(不是錯誤)——既有檔案裡的空塊合法且穩定,一刀變錯誤會讓
//!    它們全部失效;B9 本來就有分級診斷,「該說但不該擋」正是警告的用途;
//! 2. **`pass`** 讓作者說「這一塊故意留白」,警告隨之閉嘴;
//! 3. **`marker trait`** 是**契約**而非現況:承諾永不帶內容,由驗證強制。
//!    差別在於改變它必須改宣告行(看得見),而不是往 body 塞一行(看不見)。
//!
//! 為什麼 `pass` 做成語法而不是註解:canonical printer 由 AST 印出,註解過不了
//! 一次 `dump()`,而 `.chg` 的 replay 與工作副本存檔全走 canonical 形式。

use conlang_language::{Language, LanguageDocument, LibrarySpec};

/// 只留本組關心的診斷——stdlib 自己的警告不該混進判斷。
fn codes(source: &str) -> Vec<String> {
    let document = LanguageDocument::import_new_root(source, "evo:pm").expect("parses");
    let system = conlang_language::system::compile_document(&document, &LibrarySpec::default())
        .expect("compiles");
    system
        .validation
        .diagnostics()
        .iter()
        .filter(|d| d.code.starts_with("BLOCK_") || d.code.starts_with("TRAIT_MARKER"))
        .map(|d| format!("{:?}/{}", d.severity, d.code))
        .collect()
}

/// 編譯不過的那些(錯誤級)走這裡。
fn compile_error(source: &str) -> String {
    let document = LanguageDocument::import_new_root(source, "evo:pm").expect("parses");
    let error = conlang_language::system::compile_document(&document, &LibrarySpec::default())
        .expect_err("該被拒");
    format!("{error:?}")
}

const SIGN: &str = "\nsign x:\n    belongs Blank\n    phon:\n        /a/\n";

// ── ① 空塊發警告,`pass` 讓它閉嘴 ─────────────────────────────────────────

#[test]
fn an_empty_block_warns_and_pass_silences_it() {
    let bare = format!("Symbol a\n\ntrait Blank:\n{SIGN}");
    assert_eq!(
        codes(&bare),
        ["Warning/BLOCK_EMPTY_WITHOUT_PASS"],
        "空塊要說話"
    );

    let passed = format!("Symbol a\n\ntrait Blank:\n    pass\n{SIGN}");
    assert!(codes(&passed).is_empty(), "寫了 pass 就不該再嘮叨");
}

/// **警告,不是錯誤**——既有檔案不得因此失效。
#[test]
fn an_empty_block_is_still_a_legal_document() {
    let bare = format!("Symbol a\n\ntrait Blank:\n{SIGN}");
    let document = LanguageDocument::import_new_root(&bare, "evo:pm").expect("parses");
    assert!(
        conlang_language::system::compile_document(&document, &LibrarySpec::default()).is_ok(),
        "空塊只該被念,不該被擋"
    );
}

/// 多塊之中夾一個空塊也要被看見——規則是**逐塊**的,不是「整個 trait 有沒有內容」。
#[test]
fn an_empty_block_between_two_full_ones_is_reported() {
    let source = format!(
        "Symbol a\n\ntrait Blank:\n    sem:\n        senses:\n            core = A\n    ==\n    ==\n    sem:\n        senses:\n            alt = B\n{SIGN}"
    );
    assert_eq!(codes(&source), ["Warning/BLOCK_EMPTY_WITHOUT_PASS"]);
}

// ── ② `pass` 與內容互斥 ──────────────────────────────────────────────────

#[test]
fn pass_next_to_content_is_a_contradiction() {
    let source = format!(
        "Symbol a\n\ntrait Blank:\n    pass\n    sem:\n        senses:\n            core = A\n{SIGN}"
    );
    assert!(
        compile_error(&source).contains("BLOCK_PASS_WITH_CONTENT"),
        "{}",
        compile_error(&source)
    );
}

// ── ③ `marker trait` 是契約 ──────────────────────────────────────────────

#[test]
fn a_marker_trait_needs_no_pass_and_carries_no_content() {
    let ok = format!("Symbol a\n\nmarker trait Blank:\n{SIGN}");
    assert!(
        codes(&ok).is_empty(),
        "marker 已經說明它為什麼是空的,不必再寫 pass"
    );

    let bad = format!(
        "Symbol a\n\nmarker trait Blank:\n    sem:\n        senses:\n            core = A\n{SIGN}"
    );
    assert!(
        compile_error(&bad).contains("TRAIT_MARKER_HAS_CONTENT"),
        "{}",
        compile_error(&bad)
    );
}

/// `marker` 進得了 canonical 形式——**契約要活過一次 `dump()`**,否則它跟註解
/// 沒兩樣(而註解過不了 canonical printer)。
#[test]
fn marker_and_pass_survive_a_canonical_round_trip() {
    for source in [
        "marker trait Blank:\n",
        "trait Blank:\n    pass\n",
        "trait Multi:\n    sem:\n        senses:\n            core = A\n    ==\n    pass\n",
    ] {
        let dumped = Language::parse(source).expect("parses").dump();
        assert_eq!(dumped, source, "canonical 形式必須逐位元穩定");
        let again = Language::parse(&dumped).expect("re-parses").dump();
        assert_eq!(again, dumped, "第二趟仍是不動點");
    }
}

/// 判別性:`marker` 不是把 `global` 借來用——兩者是不同的旗標,不得互相污染。
#[test]
fn marker_is_not_global() {
    let language = Language::parse("marker trait Blank:\n").expect("parses");
    let def = language
        .traits
        .iter()
        .find(|t| t.name == "Blank")
        .expect("Blank");
    assert!(def.marker, "marker 要立起來");
    assert!(!def.global, "但不得順手把 global 也打開");

    let global = Language::parse("global trait Core:\n    pass\n").expect("parses");
    let def = global
        .traits
        .iter()
        .find(|t| t.name == "Core")
        .expect("Core");
    assert!(def.global);
    assert!(!def.marker);
}

// ── ④ [A] 第 1 步:`belongs` 宣告了卻沒引用 ────────────────────────────────
//
// 兩階段設計:`belongs X` 宣告**本 sign 使用哪一種 trait**(身分與實參),
// `X[n]` 才是內容落點。今天投影仍然供給內容,所以這條只發警告——它的用途是
// 把遷移清單列出來,行為零改變。第 3 步才會關掉投影並升為錯誤。

const CONTENTFUL: &str =
    "Symbol a\n\ntrait Contentful:\n    sem:\n        senses:\n            core = THING\n";

fn belongs_codes(source: &str) -> Vec<String> {
    let document = LanguageDocument::import_new_root(source, "evo:br").expect("parses");
    let system = conlang_language::system::compile_document(&document, &LibrarySpec::default())
        .expect("compiles");
    system
        .validation
        .diagnostics()
        .iter()
        .filter(|d| &*d.code == "BELONGS_WITHOUT_REFERENCE" && d.message.contains("\"x\""))
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn belongs_without_a_reference_is_reported() {
    let bare = format!("{CONTENTFUL}\nsign x:\n    belongs Contentful\n    phon:\n        /a/\n");
    assert_eq!(belongs_codes(&bare), ["BELONGS_WITHOUT_REFERENCE"]);
}

#[test]
fn referencing_the_trait_silences_it() {
    for reference in ["Contentful[0]", "Contentful"] {
        let source =
            format!("{CONTENTFUL}\nsign x:\n    belongs Contentful\n    {reference}\n    phon:\n        /a/\n");
        assert!(
            belongs_codes(&source).is_empty(),
            "`{reference}` 應該算數:{source}"
        );
    }
}

/// **沒有內容可引用的 trait 豁免**——強迫寫一行把空集合引用進來是純噪音。
#[test]
fn a_trait_with_nothing_to_inline_needs_no_reference() {
    for declaration in ["marker trait Blank:\n", "trait Blank:\n    pass\n"] {
        let source = format!(
            "Symbol a\n\n{declaration}\nsign x:\n    belongs Blank\n    phon:\n        /a/\n"
        );
        assert!(
            belongs_codes(&source).is_empty(),
            "{declaration} 不該要求引用:{source}"
        );
    }
}

/// trait 自己 `belongs` 另一個 trait 時同樣要引用——傳遞性是逐層各自結清的。
#[test]
fn a_trait_that_belongs_another_is_checked_too() {
    let source = format!(
        "{CONTENTFUL}\ntrait Middle:\n    belongs Contentful\n\nsign x:\n    belongs Middle\n    Middle[0]\n    phon:\n        /a/\n"
    );
    let document = LanguageDocument::import_new_root(&source, "evo:br").expect("parses");
    let system = conlang_language::system::compile_document(&document, &LibrarySpec::default())
        .expect("compiles");
    assert!(
        system
            .validation
            .diagnostics()
            .iter()
            .any(|d| &*d.code == "BELONGS_WITHOUT_REFERENCE" && d.message.contains("\"Middle\"")),
        "Middle 對 Contentful 的引用也該被要求"
    );
}

/// **`X[n]` 不得獨立出現**:身分住在 `belongs` 上,展開點沒有可指的東西就不成立。
///
/// 暫為警告——這條的終點是把 `Belongs` 與 `TraitUse` 併成同一個 `SignItem`
/// (`block: None` = 宣告、`Some(n)` = 展開點),屆時它是**型別的良構條件**。
#[test]
fn an_expansion_point_without_its_declaration_is_reported() {
    let source = format!("{CONTENTFUL}\nsign x:\n    Contentful[0]\n    phon:\n        /a/\n");
    let document = LanguageDocument::import_new_root(&source, "evo:orphan").expect("parses");
    let system = conlang_language::system::compile_document(&document, &LibrarySpec::default())
        .expect("compiles");
    assert!(
        system
            .validation
            .diagnostics()
            .iter()
            .any(|d| &*d.code == "TRAIT_USE_WITHOUT_BELONGS" && d.message.contains("\"x\"")),
        "沒有 `belongs Contentful` 的 `Contentful[0]` 該被指出"
    );
}

/// 配上宣告就安靜——判別性:否則上一條測的可能是「這條規則永遠在叫」。
#[test]
fn an_expansion_point_with_its_declaration_is_fine() {
    let source =
        format!("{CONTENTFUL}\nsign x:\n    belongs Contentful\n    Contentful[0]\n    phon:\n        /a/\n");
    let document = LanguageDocument::import_new_root(&source, "evo:paired").expect("parses");
    let system = conlang_language::system::compile_document(&document, &LibrarySpec::default())
        .expect("compiles");
    assert!(
        !system
            .validation
            .diagnostics()
            .iter()
            .any(|d| &*d.code == "TRAIT_USE_WITHOUT_BELONGS" && d.message.contains("\"x\"")),
        "配對齊全就不該有話說"
    );
}

// ── ⑤ `belongs` 與 `X[n]` 是同一個項目 ────────────────────────────────────
//
// `X[n]` 不可能獨立出現(它是展開點,指哪一個掛載由 `belongs` 決定),而有
// `belongs` 就必須把展開點寫出來(否則內容不會進來)。兩者拆成兩種項目時,
// trait 名得在每一處重複帶一份,而「哪一份是權威」沒有型別上的答案。

/// 三種語法形式都映到 `SignItem::TraitMount`,靠 `kind` 區分。
#[test]
fn all_three_mount_forms_are_one_item_kind() {
    use conlang_language::{SignItem, TraitMountKind};
    let language = Language::parse(
        "trait Multi:\n    pass\n    ==\n    pass\n\nsign x:\n    belongs Multi\n    Multi[0]\n    Multi[1]\n",
    )
    .expect("parses");
    let kinds: Vec<&TraitMountKind> = language
        .signs
        .iter()
        .find(|s| s.name == "x")
        .expect("x")
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::TraitMount { kind, .. } => Some(kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            &TraitMountKind::Declaration,
            &TraitMountKind::Block(0),
            &TraitMountKind::Block(1)
        ],
        "belongs 是宣告,X[n] 是展開點,三者同一個項目型別"
    );
}

/// 🔑 **宣告必須活過展開。** 它是分類邊,不是展開對象——被當成「展開出空集合」
/// 的話,ontology 樹會在 ② 之後整棵消失。
#[test]
fn a_declaration_survives_expansion_while_the_expansion_point_is_consumed() {
    use conlang_language::{SignItem, TraitMountKind};
    let language = Language::parse(
        "trait Rich:\n    sem:\n        senses:\n            core = THING\n\nsign x:\n    belongs Rich\n    Rich[0]\n",
    )
    .expect("parses");
    let expanded = conlang_language::compile::compile(&language)
        .expect("compiles")
        .expanded;
    let items = &expanded
        .signs
        .iter()
        .find(|s| s.name == "x")
        .expect("x")
        .items;

    assert!(
        items.iter().any(|item| matches!(
            item,
            SignItem::TraitMount {
                kind: TraitMountKind::Declaration,
                ..
            }
        )),
        "分類邊要留下:{items:?}"
    );
    assert!(
        !items.iter().any(|item| matches!(
            item,
            SignItem::TraitMount {
                kind: TraitMountKind::Block(_) | TraitMountKind::Whole,
                ..
            }
        )),
        "展開點要被消耗掉:{items:?}"
    );
    assert!(
        items.iter().any(|item| matches!(item, SignItem::Sense(_))),
        "而它帶進來的內容要在:{items:?}"
    );
}

// ── ⑥ 展開看得到套件的 trait([A] 3-1)────────────────────────────────────
//
// 展開原本只查 `src.traits`(使用者語言自己的 trait),而投影用的 registry 是
// `[std, user]` 兩份建的。於是**顯式引用只對同一份文件裡宣告的 trait 有效**
// ——`belongs Noun` + `Noun[0]` 會報 `UnknownTrait`,而真實文件的 `belongs`
// 大多指向 std。那個缺口讓兩階段的主幹道不通。

/// 🔑 顯式引用一個 **std 的 trait**:先前是 `UnknownTrait`,現在要通得過。
#[test]
fn an_explicit_reference_to_a_package_trait_compiles() {
    let source = "Symbol a\n\nsign x:\n    belongs Noun\n    Noun[0]\n    phon:\n        /a/\n";
    let document = LanguageDocument::import_new_root(source, "evo:pkg").expect("parses");
    conlang_language::system::compile_document(&document, &LibrarySpec::default())
        .expect("套件的 trait 也要引用得到");
}

/// 本語言優先:同名時不會被套件的定義蓋掉。
///
/// 判別性:若查找順序反過來,`x` 會拿到 std `Noun` 的內容而不是本地的。
#[test]
fn a_local_trait_shadows_nothing_but_wins_the_lookup() {
    use conlang_language::SignItem;
    let source = "Symbol a\n\ntrait LocalOnly:\n    sem:\n        senses:\n            core = MINE\n\nsign x:\n    belongs LocalOnly\n    LocalOnly[0]\n    phon:\n        /a/\n";
    let language = Language::parse(source).expect("parses");
    let expanded = conlang_language::compile::compile(&language)
        .expect("compiles")
        .expanded;
    let senses: Vec<String> = expanded
        .signs
        .iter()
        .find(|s| s.name == "x")
        .expect("x")
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::Sense(sense) => Some(sense.gloss.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(senses, ["MINE"], "本地宣告的 trait 要贏");
}
