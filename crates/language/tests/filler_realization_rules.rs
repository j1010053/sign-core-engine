use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{system::compile_system, Language};

fn source(body: &str, membership: &str) -> String {
    format!(
        r#"Symbol a
Symbol e
Symbol i
Class vowel {{a, e, i}}
marker trait Changed:

sign atom:
    {membership}
    phon:
        /a/
        realization:
            case:
                $self == [Changed]:
{body}
                base:

sign outer:
    syn:
        slots:
            item [*]
    phon:
        /{{$slot.item}}a/
"#
    )
}

#[test]
fn lexical_filler_rules_run_in_order_and_stay_inside_the_filler() {
    let text = source(
        "                    a => e\n                    e => i",
        "belongs Changed",
    );
    let language = Language::parse(&text).unwrap();
    let original = language.clone();
    let system = compile_system(language).unwrap();
    for _ in 0..2 {
        let result = system
            .derive(
                "outer",
                &[SlotFiller::sign("item", "atom")],
                &SlotMap::identity(),
            )
            .unwrap();
        assert_eq!(result.surface, "ia");
        assert_eq!(result.occurrences[0].realization.as_deref(), Some("i"));
        assert_eq!(
            result.occurrences[0].cases.last().unwrap().diagnostic_code,
            None
        );
    }
    assert_eq!(system.language(), &original);
}

#[test]
fn unmatched_branch_does_not_run_rules() {
    let system =
        compile_system(Language::parse(&source("                    a => e", "")).unwrap())
            .unwrap();
    let result = system
        .derive(
            "outer",
            &[SlotFiller::sign("item", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.surface, "aa");
}

#[test]
fn branch_template_is_used_before_rules() {
    let system = compile_system(
        Language::parse(&source(
            "                    /e/\n                    e => i",
            "belongs Changed",
        ))
        .unwrap(),
    )
    .unwrap();
    let result = system
        .derive(
            "outer",
            &[SlotFiller::sign("item", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.surface, "ia");
}

#[test]
fn inert_rules_keep_the_diagnostic() {
    let system = compile_system(
        Language::parse(&source("                    e => i", "belongs Changed")).unwrap(),
    )
    .unwrap();
    let result = system
        .derive(
            "outer",
            &[SlotFiller::sign("item", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.surface, "aa");
    assert_eq!(
        result.occurrences[0].cases.last().unwrap().diagnostic_code,
        Some("REALIZATION_RULES_INERT")
    );
}

#[test]
fn global_rules_run_after_composition_not_inside_the_filler() {
    let text = source("                    a => e", "belongs Changed")
        + "\nglobal trait FinalPhon:\n    phon:\n        e => i\n";
    let system = compile_system(Language::parse(&text).unwrap()).unwrap();
    let result = system
        .derive(
            "outer",
            &[SlotFiller::sign("item", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.occurrences[0].realization.as_deref(), Some("e"));
    assert_eq!(result.surface, "ia");
}

#[test]
fn constrained_token_filler_runs_rules_after_occurrence_feature_binding() {
    let text = source("                    a => e", "belongs Changed")
        .replace(
            "marker trait Changed:",
            "trait Changed:\n    syn:\n        feature:\n            number = enum(sg, pl)?",
        )
        .replace(
            "    belongs Changed\n    phon:",
            "    belongs Changed\n    syn:\n        slots:\n            unused [*]?\n    phon:",
        )
        .replace("$self == [Changed]", "$self.syn.number == pl")
        .replace(
            "            item [*]",
            "            item [Changed]\n        slot_features:\n            item.number = pl",
        );
    let system = compile_system(Language::parse(&text).unwrap()).unwrap();
    let token = system
        .evaluate_token(
            system
                .apply_construction("atom", &[], &SlotMap::identity())
                .unwrap(),
        )
        .unwrap();
    let result = system
        .derive(
            "outer",
            &[SlotFiller::token("item", token.as_token())],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.surface, "ea");
    assert_eq!(result.occurrences[0].realization.as_deref(), Some("e"));
    assert_eq!(
        result.occurrences[0].cases.last().unwrap().target,
        "phon.realization"
    );
}

#[test]
fn outer_branch_receives_the_inner_realized_form() {
    let text = source("                    a => e", "belongs Changed")
        + "        realization:\n            case:\n                base:\n                    e => i\n";
    let system = compile_system(Language::parse(&text).unwrap()).unwrap();
    let result = system
        .derive(
            "outer",
            &[SlotFiller::sign("item", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.occurrences[0].realization.as_deref(), Some("e"));
    assert_eq!(result.surface, "ia");
}
