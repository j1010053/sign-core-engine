//! sign 引用(application)的 **slot 具名賦值改用 `key: value`**,與步驟 15c 的
//! 層②③④ 呼叫語法統一——構式套用本來就是「帶具名參數的函數呼叫」。
//!
//! 舊寫法 `key = value` **仍接受**(既有檔案不失效),但 canonical printer 一律
//! 排 `:`,故非 canonical 輸入會正規化為不動點(printer 契約 P21)。

use conlang_language::Language;

/// 把 application 放進 `realization:` 的 case —— 那裡才會被解析成 `Expression`
/// (sign 頂層的 `phon: /…/` UR 是 **verbatim 模板**,printer 原樣輸出)。
fn source(application: &str) -> String {
    format!(
        "Symbol a\n\n\
         trait LocalNoun:\n\n\
         sign host:\n    belongs LocalNoun\n    syn:\n        slots:\n            head [LocalNoun]\n    phon:\n        /a/\n\n\
         sign user:\n    belongs LocalNoun\n    phon:\n        /a/\n        realization:\n            case:\n                else:\n                    /{{{application}.phon.ret}}/\n"
    )
}

fn lang(application: &str) -> String {
    Language::parse(&source(application))
        .unwrap_or_else(|error| panic!("parse {application:?}: {error}"))
        .dump()
}

#[test]
fn a_named_slot_argument_uses_a_colon() {
    let dumped = lang("host(head: {$self})");
    assert!(
        dumped.contains("host(head: {$self})"),
        "canonical form uses `:`:\n{dumped}"
    );
}

#[test]
fn the_legacy_equals_form_still_parses_and_normalizes() {
    // 舊檔不失效,但輸出正規化成 `:`(canonical printer 的既定行為)。
    let dumped = lang("host(head = {$self})");
    assert!(
        dumped.contains("host(head: {$self})"),
        "legacy `=` normalizes to `:`:\n{dumped}"
    );
    assert!(!dumped.contains("head = "), "{dumped}");
}

#[test]
fn a_dump_is_a_fixed_point() {
    let once = lang("host(head = {$self})");
    let twice = Language::parse(&once).expect("re-parse").dump();
    assert_eq!(once, twice, "正規化後為不動點");
}

/// 迴歸:分隔符只能在**頂層**切。內層 application 的 `:` 屬於內層,
/// 在外層誤切會把名字讀成 `value: Wrap(value`。
#[test]
fn a_nested_application_splits_only_at_the_top_level() {
    let dumped = lang("host(head: host(head: {$self}))");
    assert!(
        dumped.contains("host(head: host(head: {$self}))"),
        "nesting survives round-trip:\n{dumped}"
    );
}

/// 深度感知的副產物:**位置參數**若是巢狀 application,以前會被誤判成具名引數
/// 而報錯(內層的分隔符被當成本層的),現在正確視為位置參數。
#[test]
fn a_positional_nested_application_is_accepted() {
    let dumped = lang("host(host(head: {$self}))");
    assert!(
        dumped.contains("host(host(head: {$self}))"),
        "positional nested application:\n{dumped}"
    );
}

#[test]
fn a_non_identifier_before_the_separator_is_rejected() {
    assert!(
        Language::parse(&source("host(not an ident: {$self})")).is_err(),
        "a malformed argument name must be rejected, not silently treated as positional"
    );
}
