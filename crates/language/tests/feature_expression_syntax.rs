//! Q5′:feature 的求值寫入是 `NAME =` + 縮排 `case:`,不是 `NAME =>`。
//!
//! 這個語言的賦值只有兩種形態:**`=` 寫入**(同一個 key 一個值)與 **`=>` 累積
//! 規則**(全部保留,按序跑)。而 `FeatureExpression` 的行為是鍵控寫入
//! ——`effective_sign` 按 `(dim, name)` 合併,一個 feature 一個——卻曾拼寫成
//! `=>`。改用 `=` 之後名實相符:`=` 的右端可以是字面值、值域,或一個算出值的
//! `case`。

use conlang_language::{compile_system, Language, SignItem};

const SOURCE: &str = "\
sign s:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = on
            outcome = enum(yes, no)
            outcome =
                case:
                    $self.syn.trigger == on:
                        yes
                    else:
                        no
";

#[test]
fn a_feature_expression_parses_and_round_trips_with_the_equals_spelling() {
    let language = Language::parse(SOURCE).expect("parse");
    let dump = language.dump();
    assert_eq!(dump, SOURCE, "canonical 必須逐字印回 `=` 形態");
    assert_eq!(Language::parse(&dump).expect("re-parse").dump(), dump);
}

#[test]
fn it_still_lands_as_a_feature_expression_item() {
    let language = Language::parse(SOURCE).expect("parse");
    assert!(
        language
            .sign_named("s")
            .expect("sign")
            .items
            .iter()
            .any(|item| matches!(item, SignItem::FeatureExpression(_))),
        "`=` + case 仍應產生 FeatureExpression,不是 Def 或 Rule"
    );
}

#[test]
fn the_case_still_computes_the_value() {
    let system = compile_system(Language::parse(SOURCE).expect("parse")).expect("compiles");
    let evaluated = system.evaluate_sign("s").expect("evaluates");
    let projection = evaluated
        .sign
        .project(conlang_language::Dim::Syn, &system.ontology);
    assert_eq!(projection.get("syn.outcome"), Some("yes"));
}

// ── 邊界 ──────────────────────────────────────────────────────────────────

/// 舊寫法要給出**指向遷移方向**的訊息,而不是掉進規則解析後報一個空 body。
#[test]
fn the_old_arrow_spelling_reports_how_to_migrate() {
    let old = SOURCE.replace("            outcome =\n", "            outcome =>\n");
    let error = Language::parse(&old)
        .expect_err("舊寫法不再合法")
        .to_string();
    assert!(
        error.contains("NAME =") && error.contains("accumulating rule"),
        "訊息要說出正確寫法與 `=>` 的真正語義:{error}"
    );
}

/// `=` 右端空白但下一行不是 `case:` 仍必須是明確錯誤——不能因為新增這個形態
/// 就讓「忘了寫值」變成模糊訊息。
#[test]
fn an_equals_with_nothing_after_it_is_still_an_error() {
    let broken = "sign s:\n    syn:\n        feature:\n            x = enum(a)\n            x =\n            y = enum(b)\n";
    let error = Language::parse(broken)
        .expect_err("空右端必須報錯")
        .to_string();
    assert!(error.contains("case"), "訊息要提到缺的是 case:{error}");
}

/// 正向控制組:一行 `=>` 規則不受影響,仍是累積規則。
#[test]
fn a_one_line_arrow_rule_is_untouched() {
    let source =
        "sign s:\n    syn:\n        feature:\n            a = enum(yes)\n            a => yes\n";
    let language = Language::parse(source).expect("parse");
    // canonical 會補上顯式的 `@stage`(規則的正規化),那與本條無關。
    assert!(language.dump().contains("a => yes @stage"));
    assert!(language
        .sign_named("s")
        .expect("sign")
        .items
        .iter()
        .any(|item| matches!(item, SignItem::FeatureRule(_))));
}
