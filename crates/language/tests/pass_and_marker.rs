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
    let def = language.traits.iter().find(|t| t.name == "Blank").expect("Blank");
    assert!(def.marker, "marker 要立起來");
    assert!(!def.global, "但不得順手把 global 也打開");

    let global = Language::parse("global trait Core:\n    pass\n").expect("parses");
    let def = global.traits.iter().find(|t| t.name == "Core").expect("Core");
    assert!(def.global);
    assert!(!def.marker);
}
