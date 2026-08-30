//! B1/B2:realization 與模板的兩條新診斷。
//!
//! - `TEMPLATE_ADJACENT_SLOTS_FUSED`(Warning):兩個引用緊鄰、中間無字元。
//!   融合本身合法(非串接形態就要這樣寫),故只出聲不擋。
//! - `REALIZATION_RULE_WITHOUT_BASE`(Error):分支帶規則卻沒有可改的形。

use conlang_language::system::check_language;
use conlang_language::Language;

fn codes(src: &str) -> Vec<String> {
    check_language(&Language::parse(src).expect("parse"))
        .diagnostics()
        .iter()
        .map(|d| d.code.to_owned())
        .collect()
}

const HEAD: &str = r#"Feature Type(*cons, vowel)
Symbol a [vowel]
Symbol i [vowel]
Symbol n [cons]
Symbol g [cons]
Symbol s [cons]
Class vowel {a, i}

trait Piece:
    syn:
        feature:
            k = enum(one)

sign bit:
    belongs Piece
    phon:
        /an/

"#;

/// 🔑 相鄰槽無分隔 = 兩個詞素黏成一個,`@stage word` 規則將無縫可命中。
#[test]
fn adjacent_slot_references_are_flagged() {
    let source = format!(
        "{HEAD}sign Fused:\n\
\x20   syn:\n\
\x20       slots:\n\
\x20           left [Piece]\n\
\x20           right [Piece]\n\
\x20   phon:\n\
\x20       /{{$slot.left}}{{$slot.right}}/\n"
    );
    assert!(
        codes(&source).contains(&"TEMPLATE_ADJACENT_SLOTS_FUSED".to_owned()),
        "應警告黏合: {:?}",
        codes(&source)
    );
}

/// 寫了 `+` 就不再警告——這正是 P96 要作者做的那件事。
#[test]
fn an_explicit_seam_silences_the_warning() {
    let source = format!(
        "{HEAD}sign Joined:\n\
\x20   syn:\n\
\x20       slots:\n\
\x20           left [Piece]\n\
\x20           right [Piece]\n\
\x20   phon:\n\
\x20       /{{$slot.left}}+{{$slot.right}}/\n"
    );
    assert!(
        !codes(&source).contains(&"TEMPLATE_ADJACENT_SLOTS_FUSED".to_owned()),
        "有縫不該警告: {:?}",
        codes(&source)
    );
}

/// 非串接形態:三個槽夾字面音段 = 一個詞根,合法且不該被警告。
#[test]
fn a_root_and_pattern_template_is_not_flagged() {
    let source = format!(
        "{HEAD}sign Root:\n\
\x20   syn:\n\
\x20       slots:\n\
\x20           c1 [Piece]\n\
\x20           c2 [Piece]\n\
\x20   phon:\n\
\x20       /{{$slot.c1}}a{{$slot.c2}}a/\n"
    );
    assert!(
        !codes(&source).contains(&"TEMPLATE_ADJACENT_SLOTS_FUSED".to_owned()),
        "詞根模板不該被警告: {:?}",
        codes(&source)
    );
}

/// B2:分支只有規則、sign 又沒有深層模板 → 規則沒有可改的對象。
#[test]
fn a_rule_only_branch_without_any_base_is_an_error() {
    let source = format!(
        "{HEAD}sign NoBase:\n\
\x20   belongs Piece\n\
\x20   phon:\n\
\x20       realization:\n\
\x20           case:\n\
\x20               else:\n\
\x20                   i => a\n"
    );
    assert!(
        codes(&source).contains(&"REALIZATION_RULE_WITHOUT_BASE".to_owned()),
        "無 base 應報錯: {:?}",
        codes(&source)
    );
}

/// 對照組:有深層模板時,純規則分支合法。
#[test]
fn a_rule_only_branch_with_a_deep_template_is_fine() {
    let source = format!(
        "{HEAD}sign WithBase:\n\
\x20   belongs Piece\n\
\x20   phon:\n\
\x20       /sing/\n\
\x20       realization:\n\
\x20           case:\n\
\x20               else:\n\
\x20                   i => a / _ n g\n"
    );
    assert!(
        !codes(&source).contains(&"REALIZATION_RULE_WITHOUT_BASE".to_owned()),
        "有 base 不該報錯: {:?}",
        codes(&source)
    );
}
