//! P93:realization 分支 body = phon block body(深層模板 + 若干規則)。
//!
//! 單行 `/…/` 分支 = `(模板, [])` 這個特例,行為與此前逐位元相同;多行分支
//! 走 `Expression::DimFragment { dim: Phon, … }`,body 由既有的 phon block
//! 解析器處理,無新文法。

use conlang_language::{Dim, Expression, Language, SignItem};

const INVENTORY: &str = "Feature Type(*cons, vowel)\n\
                         Symbol a [vowel]\n\
                         Symbol e [vowel]\n\
                         Symbol i [vowel]\n\
                         Symbol n [cons]\n\
                         Symbol g [cons]\n\
                         Symbol s [cons]\n\
                         Symbol r [cons]\n\
                         Class vowel {a, e, i}\n\n";

fn realization_of(language: &Language, sign: &str) -> conlang_language::TypedCase {
    language
        .signs
        .iter()
        .find(|candidate| candidate.name == sign)
        .expect("sign exists")
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::Realization(realization) => Some(realization.expression.clone()),
            _ => None,
        })
        .expect("sign carries a realization")
}

/// 🔑 一個分支帶模板 + 規則,解析成 phon fragment,body 是既有的 phon item。
#[test]
fn a_branch_may_carry_a_template_and_rules() {
    let source = format!(
        "{INVENTORY}\
trait TestAblaut:\n\
\x20   syn:\n\
\x20       feature:\n\
\x20           k = enum(one)\n\
\n\
sign sing:\n\
\x20   belongs TestAblaut\n\
\x20   phon:\n\
\x20       /sing/\n\
\x20       realization:\n\
\x20           case:\n\
\x20               $self == [TestAblaut]:\n\
\x20                   /singer/\n\
\x20                   i => a / _ n g\n\
\x20               else:\n\
\x20                   /sing/\n"
    );
    let language = Language::parse(&source).expect("parses");
    let case = realization_of(&language, "sing");
    assert_eq!(case.branches.len(), 2);

    let Expression::DimFragment { dim, items } = &case.branches[0].result else {
        panic!(
            "多行 phon 分支應為 DimFragment,得到 {:?}",
            case.branches[0].result
        );
    };
    assert_eq!(*dim, Dim::Phon);
    assert!(
        items.iter().any(|item| matches!(item, SignItem::Rule(_))),
        "body 應含規則: {items:?}"
    );
}

/// 單行分支維持純量 `PhonTemplate` —— `(模板, [])` 的特例不得改變表示,
/// 否則既有語料的 canonical 與 digest 會無故變動。
#[test]
fn a_single_line_branch_is_still_a_plain_template() {
    let source = format!(
        "{INVENTORY}\
sign sing:\n\
\x20   phon:\n\
\x20       /sing/\n\
\x20       realization:\n\
\x20           case:\n\
\x20               else:\n\
\x20                   /sang/\n"
    );
    let language = Language::parse(&source).expect("parses");
    let case = realization_of(&language, "sing");
    assert!(
        matches!(&case.branches[0].result, Expression::PhonTemplate(t) if t == "/sang/"),
        "單行分支應維持 PhonTemplate,得到 {:?}",
        case.branches[0].result
    );
}

/// `when:` 仍不得用於 phon(P93 §待決議:累積語意是獨立裁定,不隨 fragment 開放)。
#[test]
fn accumulate_selection_is_still_rejected_in_phon() {
    let source = format!(
        "{INVENTORY}\
sign sing:\n\
\x20   phon:\n\
\x20       /sing/\n\
\x20       realization:\n\
\x20           when:\n\
\x20               else:\n\
\x20                   /sang/\n"
    );
    // 擋在 realization parser(它只收 `case:`),比 `is_fragment_context` 更前面。
    // 兩道都在:即使日後放寬前者,後者仍會擋下累積語意。
    let error = Language::parse(&source).expect_err("`when:` 不得用於 phon");
    assert!(
        error.to_string().contains("case:"),
        "訊息應要求 `case:`: {error}"
    );
}

// ── 端到端:分支規則實際跑在展開後的形上 ────────────────────────────────

use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::system::compile_system;

const PAST_FORM: &str = r#"Feature Type(*cons, vowel)
Symbol a [vowel]
Symbol e [vowel]
Symbol i [vowel]
Symbol n [cons]
Symbol g [cons]
Symbol s [cons]
Symbol d [cons]
Class vowel {a, e, i}

trait TestVerb:
    syn:
        feature:
            k = enum(one)

trait TestAblaut:
    belongs TestVerb

sign sing:
    belongs TestAblaut
    phon:
        /sing/

sign sin:
    belongs TestVerb
    phon:
        /sin/

sign sang:
    belongs TestAblaut
    phon:
        /sang/

sign PastForm:
    syn:
        slots:
            stem [TestVerb]
    phon:
        /{$slot.stem}/
        realization:
            case:
                $slot.stem == [TestAblaut]:
                    i => a / _ n g
                else:
                    /{$slot.stem}ed/
"#;

fn past_of(stem: &str) -> String {
    let language = Language::parse(PAST_FORM).expect("parses");
    let system = compile_system(language).expect("compiles");
    system
        .derive(
            "PastForm",
            &[SlotFiller::sign("stem", stem)],
            &SlotMap::identity(),
        )
        .expect("derives")
        .surface
}

/// 🔑 純規則分支(單行,無模板):base 沿用深層形,規則對它跑。
/// `sing` → 深層 `/sing/` → `i => a / _ n g` → `sang`。
#[test]
fn a_rule_only_branch_transforms_the_deep_form() {
    assert_eq!(past_of("sing"), "sang");
}

/// 對照組:同一個構式,非 ablaut 詞幹落到 else 的模板分支。
#[test]
fn the_else_branch_still_builds_from_a_template() {
    assert_eq!(past_of("sin"), "sined");
}

/// A5:分支內的模板與規則都要印得回來,且 dump 是不動點(P21)。
#[test]
fn a_rule_branch_round_trips() {
    let language = Language::parse(PAST_FORM).expect("parses");
    let dumped = language.dump();
    assert!(
        dumped.contains("i => a / _ n g"),
        "分支規則應印出:\n{dumped}"
    );
    let reparsed = Language::parse(&dumped).expect("re-parses");
    assert_eq!(reparsed.dump(), dumped, "dump 應為不動點:\n{dumped}");
}

/// B3:分支靠範疇選中,規則卻對這個形毫無作用 —— 範疇掛錯或環境寫錯,
/// 兩者都是靜默的。`sang` 掛著 `[TestAblaut]` 但沒有 `i` 可換。
#[test]
fn a_branch_whose_rules_do_nothing_is_flagged() {
    let language = Language::parse(PAST_FORM).expect("parses");
    let system = compile_system(language).expect("compiles");
    let derivation = system
        .derive(
            "PastForm",
            &[SlotFiller::sign("stem", "sang")],
            &SlotMap::identity(),
        )
        .expect("derives");
    assert_eq!(derivation.surface, "sang", "規則無作用,形不變");
    assert!(
        derivation
            .cases
            .iter()
            .any(|record| record.diagnostic_code == Some("REALIZATION_RULES_INERT")),
        "應標記規則無作用: {:?}",
        derivation.cases
    );
}

/// 對照組:規則真的動了就不標記。
#[test]
fn a_branch_whose_rules_fire_is_not_flagged() {
    let language = Language::parse(PAST_FORM).expect("parses");
    let system = compile_system(language).expect("compiles");
    let derivation = system
        .derive(
            "PastForm",
            &[SlotFiller::sign("stem", "sing")],
            &SlotMap::identity(),
        )
        .expect("derives");
    assert!(
        !derivation
            .cases
            .iter()
            .any(|record| record.diagnostic_code == Some("REALIZATION_RULES_INERT")),
        "規則有作用不該標記: {:?}",
        derivation.cases
    );
}
