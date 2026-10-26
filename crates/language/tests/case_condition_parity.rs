use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{compile_system, Language};

fn source(header: &str, row: &str, optional: bool, value: Option<&str>) -> String {
    let optional = if optional { "?" } else { "" };
    let assignment = value
        .map(|v| format!("            number = {v}\n"))
        .unwrap_or_default();
    format!("Symbol a\nSymbol e\nClass vowel {{a, e}}\n\nsign atom:\n    syn:\n        slots:\n            unused [*]?\n        feature:\n            number = enum(sg, pl){optional}\n{assignment}    phon:\n        /a/\n        realization:\n            {header}:\n                {row}:\n                    /e/\n                base:\n\nsign outer:\n    syn:\n        slots:\n            item [*]\n    phon:\n        /{{$slot.item}}/\n")
}
const FORMS: [(&str, &str); 3] = [
    ("case [syn.number]", "[sg]"),
    ("case", "$self.syn.number == sg"),
    ("case $self.syn.number", "== sg"),
];

#[test]
fn required_missing_values_error_in_every_syntax_and_entry() {
    for (header, row) in FORMS {
        let system =
            compile_system(Language::parse(&source(header, row, false, None)).unwrap()).unwrap();
        let direct = system
            .derive("atom", &[], &SlotMap::identity())
            .unwrap_err();
        let filler = system
            .derive(
                "outer",
                &[SlotFiller::sign("item", "atom")],
                &SlotMap::identity(),
            )
            .unwrap_err();
        for error in [direct, filler] {
            assert!(error.to_string().contains("(P75)"), "{header}: {error}");
            assert!(error.to_string().contains("syn.number"), "{error}");
        }
    }
}

#[test]
fn optional_missing_and_present_values_agree_across_syntax_and_entry() {
    for (optional, value, expected) in [
        (true, None, "a"),
        (false, Some("sg"), "e"),
        (false, Some("pl"), "a"),
    ] {
        for (header, row) in FORMS {
            let system =
                compile_system(Language::parse(&source(header, row, optional, value)).unwrap())
                    .unwrap();
            let direct = system.derive("atom", &[], &SlotMap::identity()).unwrap();
            let filler = system
                .derive(
                    "outer",
                    &[SlotFiller::sign("item", "atom")],
                    &SlotMap::identity(),
                )
                .unwrap();
            assert_eq!(direct.surface, expected, "{header}");
            assert_eq!(filler.surface, expected, "{header}");
            let branch = if expected == "e" { 0 } else { 1 };
            assert_eq!(direct.cases.last().unwrap().branch, branch);
            assert_eq!(filler.occurrences[0].cases.last().unwrap().branch, branch);
        }
    }
}

#[test]
fn wildcard_does_not_read_a_missing_required_feature() {
    let system =
        compile_system(Language::parse(&source("case [syn.number]", "[_]", false, None)).unwrap())
            .unwrap();
    assert_eq!(
        system
            .derive("atom", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "e"
    );
    assert_eq!(
        system
            .derive(
                "outer",
                &[SlotFiller::sign("item", "atom")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "e"
    );
}

#[test]
fn malformed_bundle_in_a_filler_is_an_error_not_a_missed_branch() {
    let mut language =
        Language::parse(&source("case [syn.number]", "[sg]", false, Some("sg"))).unwrap();
    let system = compile_system(language.clone()).unwrap();
    for item in &mut language
        .signs
        .iter_mut()
        .find(|s| s.name == "atom")
        .unwrap()
        .items
    {
        if let conlang_language::SignItem::Realization(realization) = item {
            realization.expression.match_features.clear();
        }
    }
    let error = conlang_language::construction::apply_with(
        &language,
        &system.ontology,
        "outer",
        &[SlotFiller::sign("item", "atom")],
        &SlotMap::identity(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("arity mismatch"), "{error}");
}

#[test]
fn occurrence_token_uses_the_same_conditions_after_slot_binding() {
    for (header, row) in FORMS {
        let text = source(header, row, true, None)
            .replace(
                "sign atom:",
                "marker trait Numbered:\n\nsign atom:\n    belongs Numbered",
            )
            .replace(
                "            item [*]",
                "            item [Numbered]\n        slot_features:\n            item.number = sg",
            );
        let system = compile_system(Language::parse(&text).unwrap()).unwrap();
        let deep = system
            .apply_construction("atom", &[], &SlotMap::identity())
            .unwrap();
        let token = system.evaluate_token(deep).unwrap();
        let result = system
            .derive(
                "outer",
                &[SlotFiller::token("item", token.as_token())],
                &SlotMap::identity(),
            )
            .unwrap();
        assert_eq!(result.surface, "e", "{header}");
        assert_eq!(result.occurrences[0].cases.last().unwrap().branch, 0);
    }
}

#[test]
fn undecided_values_are_not_equal_to_one_candidate() {
    for (header, row) in FORMS {
        let system =
            compile_system(Language::parse(&source(header, row, false, Some("sg | pl"))).unwrap())
                .unwrap();
        assert_eq!(
            system
                .derive("atom", &[], &SlotMap::identity())
                .unwrap()
                .surface,
            "a",
            "{header}"
        );
        assert_eq!(
            system
                .derive(
                    "outer",
                    &[SlotFiller::sign("item", "atom")],
                    &SlotMap::identity()
                )
                .unwrap()
                .surface,
            "a",
            "{header}"
        );
    }
}

#[test]
fn slot_reads_share_required_and_optional_feature_policy() {
    for optional in [false, true] {
        for (header, row) in [
            ("case", "$slot.item.syn.number == sg"),
            ("case $slot.item.syn.number", "== sg"),
        ] {
            let mut text = source("case [syn.number]", "[sg]", optional, None);
            let start = text.find("        realization:").unwrap();
            let end = text.find("\nsign outer:").unwrap();
            text.replace_range(start..end, "");
            text.push_str(&format!("        realization:\n            {header}:\n                {row}:\n                    /e/\n                base:\n"));
            let system = compile_system(Language::parse(&text).unwrap()).unwrap();
            let result = system.derive(
                "outer",
                &[SlotFiller::sign("item", "atom")],
                &SlotMap::identity(),
            );
            if optional {
                assert_eq!(result.unwrap().surface, "a");
            } else {
                assert!(result.unwrap_err().to_string().contains("(P75)"));
            }
        }
    }
}
