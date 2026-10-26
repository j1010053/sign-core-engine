use conlang_language::construction::{CxgError, SlotFiller, SlotMap};
use conlang_language::{
    compile_system, CaseBranchStatus, CompiledSystem, Dim, Language, OccurrenceCaseStatus,
    SystemDerivation, SystemError,
};

const INVENTORY: &str = "Symbol a\nSymbol e\nClass vowel {a, e}\n\n";
const OUTER: &str = "\nsign outer:\n    syn:\n        slots:\n            item [*]\n    phon:\n        /{$slot.item}/\n";
const FORMS: [(&str, &str); 3] = [
    ("case [syn.trigger]", "[on]"),
    ("case", "$self.syn.trigger == on"),
    ("case $self.syn.trigger", "== on"),
];

fn compile(text: &str) -> CompiledSystem {
    let language = Language::parse(text).unwrap();
    let canonical = language.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    compile_system(language).unwrap_or_else(|error| panic!("{error}"))
}

// Every assertion crosses a real derive boundary. Applied fillers first pass
// through evaluate_token; feature fixtures also force occurrence re-evaluation
// with a slot constraint, as actual compositions do.
fn derive_entry(system: &CompiledSystem, entry: usize) -> Result<SystemDerivation, SystemError> {
    match entry {
        0 => system.derive("atom", &[], &SlotMap::identity()),
        1 => system.derive(
            "outer",
            &[SlotFiller::sign("item", "atom")],
            &SlotMap::identity(),
        ),
        2 => {
            let deep = system.apply_construction("atom", &[], &SlotMap::identity())?;
            let evaluated = system.evaluate_token(deep)?;
            system.derive(
                "outer",
                &[SlotFiller::token("item", evaluated.as_token())],
                &SlotMap::identity(),
            )
        }
        _ => unreachable!(),
    }
}

fn feature_source(header: &str, row: &str, optional: bool, value: Option<&str>) -> String {
    let optional = if optional { "?" } else { "" };
    let value = value
        .map(|value| format!("            trigger = {value}\n"))
        .unwrap_or_default();
    let outer = OUTER.replace(
        "            item [*]\n",
        "            item [Fixture]\n        slot_features:\n            item.occurrence = bound\n",
    );
    format!("{INVENTORY}marker trait Fixture:\n\nsign atom:\n    belongs Fixture\n    syn:\n        slots:\n            unused [*]?\n        feature:\n            occurrence = enum(bound)?\n            trigger = enum(on, off){optional}\n{value}            outcome = enum(yes, no)\n            outcome =\n                {header}:\n                    {row}:\n                        case:\n                            base:\n                                yes\n                    base:\n                        no\n    phon:\n        /a/\n{outer}")
}

fn outcome(result: &SystemDerivation, entry: usize) -> Option<&str> {
    if entry == 0 {
        result
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.outcome")
            .map(|(_, value)| value.as_str())
    } else {
        result.token.fillers[0].scalar(Dim::Syn, "outcome")
    }
}

#[test]
fn feature_missing_reads_have_the_same_errors_for_every_syntax_and_entry() {
    for (header, row) in FORMS {
        let system = compile(&feature_source(header, row, false, None));
        for entry in 0..3 {
            let error = derive_entry(&system, entry).unwrap_err().to_string();
            assert!(
                error.contains("(P75)") && error.contains("syn.trigger"),
                "{header}, entry {entry}: {error}"
            );
        }
    }
}

#[test]
fn nested_feature_results_and_traces_agree_for_every_syntax_and_entry() {
    for (optional, value, expected) in [
        (true, None, "no"),
        (false, Some("on"), "yes"),
        (false, Some("off"), "no"),
        (false, Some("on | off"), "no"),
    ] {
        for (header, row) in FORMS {
            let system = compile(&feature_source(header, row, optional, value));
            for entry in 0..3 {
                let result = derive_entry(&system, entry).unwrap();
                assert_eq!(result.surface, "a");
                assert_eq!(
                    outcome(&result, entry),
                    Some(expected),
                    "{header}, entry {entry}"
                );
                let trace = if entry == 0 {
                    result
                        .cases
                        .iter()
                        .map(|record| {
                            (
                                record.branch,
                                record.status == CaseBranchStatus::Matched,
                                record.source,
                            )
                        })
                        .collect::<Vec<_>>()
                } else {
                    result.occurrences[0]
                        .cases
                        .iter()
                        .filter(|record| record.target == "syn.feature.outcome")
                        .map(|record| {
                            (
                                record.branch,
                                record.status == OccurrenceCaseStatus::Matched,
                                record.source,
                            )
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(trace.len(), 2, "{header}, entry {entry}: {trace:?}");
                assert_eq!(
                    trace
                        .iter()
                        .map(|(index, matched, _)| (*index, *matched))
                        .collect::<Vec<_>>(),
                    if expected == "yes" {
                        vec![(0, true), (0, true)]
                    } else {
                        vec![(0, false), (1, true)]
                    }
                );
                assert!(trace
                    .iter()
                    .all(|(_, _, source)| *source != conlang_language::SourceLocation::unknown()));
            }
        }
    }
}

#[test]
fn feature_wildcards_and_short_circuiting_do_not_read_missing_values() {
    let wildcard = feature_source("case [syn.trigger]", "[_]", false, None);
    let short_circuit = feature_source("case [syn.first, syn.trigger]", "[on, on]", false, None)
        .replace(
        "            trigger = enum",
        "            first = enum(on, off)\n            first = off\n            trigger = enum",
    );
    for (source, expected) in [(wildcard, "yes"), (short_circuit, "no")] {
        let system = compile(&source);
        for entry in 0..3 {
            assert_eq!(
                outcome(&derive_entry(&system, entry).unwrap(), entry),
                Some(expected)
            );
        }
    }
}

#[test]
fn nested_feature_cases_agree_in_syn_sem_and_prag() {
    for dim in [Dim::Syn, Dim::Sem, Dim::Prag] {
        let source = feature_source("case [syn.trigger]", "[on]", false, Some("on"));
        let source = if dim == Dim::Syn {
            source
        } else {
            source.replace(
                "            outcome = enum",
                &format!(
                    "    {}:\n        feature:\n            outcome = enum",
                    dim.keyword()
                ),
            )
        };
        let system = compile(&source);
        for entry in 0..3 {
            let result = derive_entry(&system, entry).unwrap();
            let value = if entry != 0 {
                result.token.fillers[0].scalar(dim, "outcome")
            } else {
                match dim {
                    Dim::Syn => outcome(&result, entry),
                    Dim::Sem => result.token.sem.features.get("outcome").map(String::as_str),
                    Dim::Prag => result
                        .token
                        .prag
                        .iter()
                        .find(|(path, _)| path == "prag.outcome")
                        .map(|(_, value)| value.as_str()),
                    Dim::Phon => unreachable!(),
                }
            };
            assert_eq!(value, Some("yes"), "{dim:?}, entry {entry}");
        }
    }
}

#[test]
fn nested_phon_templates_rules_and_trace_use_the_same_evaluator() {
    for leaf in ["/e/", "a => e", "e => a"] {
        let source = format!("{INVENTORY}sign atom:\n    syn:\n        slots:\n            unused [*]?\n    phon:\n        /a/\n        realization:\n            case:\n                base:\n                    case:\n                        base:\n                            {leaf}\n{OUTER}");
        let system = compile(&source);
        for entry in 0..3 {
            let result = derive_entry(&system, entry).unwrap();
            assert_eq!(result.surface, if leaf == "e => a" { "a" } else { "e" });
            let trace = if entry == 0 {
                result
                    .cases
                    .iter()
                    .map(|r| (r.branch, r.diagnostic_code, r.source))
                    .collect::<Vec<_>>()
            } else {
                result.occurrences[0]
                    .cases
                    .iter()
                    .filter(|r| r.target == "phon.realization")
                    .map(|r| (r.branch, r.diagnostic_code, r.source))
                    .collect::<Vec<_>>()
            };
            assert_eq!(trace.len(), 2, "entry {entry}: {trace:?}");
            assert_eq!(
                trace[0].1,
                (leaf == "e => a").then_some("REALIZATION_RULES_INERT")
            );
            assert_eq!(
                trace[1].1, None,
                "the outer branch must not claim to execute inner rules"
            );
            assert_ne!(trace[0].2, trace[1].2);
        }
    }
}

#[test]
fn feature_defaults_are_unchanged_for_implicit_and_empty_base() {
    for explicit in [false, true] {
        for base in [None, Some("no"), Some("yes | no")] {
            let mut source = feature_source("case [syn.trigger]", "[on]", false, Some("off"));
            source = source.replace(
                "                    base:\n                        no\n",
                if explicit {
                    "                    base:\n"
                } else {
                    ""
                },
            );
            if let Some(base) = base {
                source = source.replace(
                    "            outcome =\n",
                    &format!("            outcome = {base}\n            outcome =\n"),
                );
            }
            let system = compile(&source);
            if base == Some("yes | no") {
                // Intermediate representation assertion complements the full
                // derives below: retaining a base must keep two candidates.
                let stored = system
                    .evaluate_sign_with_context("atom", &conlang_language::DerivationContext::new())
                    .unwrap();
                assert!(stored.sign.items.iter().any(|item| matches!(item,
                    conlang_language::SignItem::FeatureValue(value)
                        if value.name == "outcome" && value.values == ["yes", "no"]
                )));
            }
            for entry in 0..3 {
                let result = derive_entry(&system, entry);
                if let Some(base) = base {
                    assert_eq!(outcome(&result.unwrap(), entry), Some(base));
                } else {
                    assert!(result
                        .unwrap_err()
                        .to_string()
                        .contains("CASE_DEFAULT_MISSING"));
                }
            }
        }
    }
}

#[test]
fn a_matched_sign_result_cannot_fall_through_to_base_in_any_case_syntax() {
    for (header, row) in FORMS {
        let source = format!("{INVENTORY}marker trait Needed:\nmarker trait Other:\n\nsign atom:\n    belongs Other\n    phon:\n        /a/\n\nsign Needs:\n    syn:\n        slots:\n            item [Needed]\n    phon:\n        /{{$slot.item}}/\n\nsign Current:\n    syn:\n        slots:\n            head [*]\n        feature:\n            trigger = enum(on, off)\n            trigger = on\n    phon:\n        /{{$slot.head}}/\n    {header}:\n        {row}:\n            Needs(item: {{$slot.head}})\n        base:\n            $self\n");
        let system = compile(&source);
        let error = system
            .derive(
                "Current",
                &[SlotFiller::sign("head", "atom")],
                &SlotMap::identity(),
            )
            .unwrap_err();
        assert!(
            matches!(
                error,
                SystemError::Construction(CxgError::CategoryMismatch { .. })
            ),
            "{header}: {error}"
        );
    }
}

#[test]
fn role_absence_stops_selection_and_retains_optional_and_partial_policy() {
    for optional_role in [false, true] {
        let role_optional = if optional_role { "?" } else { "" };
        let source = format!("{INVENTORY}trait Entity:\n    belongs Semantic\n\nsign atom:\n    belongs Entity\n    phon:\n        /a/\n\nsign Frame:\n    belongs SemanticFrame\n    syn:\n        slots:\n            selected [Entity]?\n            available [Entity]\n        feature:\n            trigger = enum(on, off)\n            trigger = on\n    sem:\n        roles:\n            agent [Entity]{role_optional}\n            agent =\n                case [syn.trigger]:\n                    [on]:\n                        {{$slot.selected}}\n                    base:\n                        {{$slot.available}}\n    phon:\n        /{{$slot.available}}/\n");
        let system = compile(&source);
        let result = system.derive(
            "Frame",
            &[SlotFiller::sign("available", "atom")],
            &SlotMap::identity(),
        );
        if optional_role {
            let result = result.unwrap();
            assert!(result.token.sem.role("agent").is_none());
            assert_eq!(result.cases.len(), 1);
            assert_eq!(result.cases[0].status, CaseBranchStatus::Matched);
            assert_eq!(result.cases[0].branch, 0);
        } else {
            assert!(matches!(
                result.unwrap_err(),
                SystemError::Construction(CxgError::MissingRoles { .. })
            ));
        }
        let partial_system = compile(&source.replace("selected [Entity]?", "selected [Entity]"));
        let deep = partial_system
            .apply_construction(
                "Frame",
                &[SlotFiller::sign("available", "atom")],
                &SlotMap::identity(),
            )
            .unwrap();
        let evaluated = partial_system.evaluate_token(deep).unwrap();
        assert!(!evaluated.as_token().is_saturated());
        assert!(evaluated.as_token().sem.role("agent").is_none());
    }
}

#[test]
fn trait_templates_share_nested_case_results_and_filler_trace() {
    let direct = feature_source("case [syn.trigger]", "[on]", false, Some("on"));
    let inherited = direct.replace("sign atom:", "trait Template:").replace(
        "\nsign outer:",
        "\nsign atom:\n    belongs Template\n\nsign outer:",
    );
    let direct_system = compile(&direct);
    let template_system = compile(&inherited);
    for entry in 0..3 {
        let a = derive_entry(&direct_system, entry).unwrap();
        let b = derive_entry(&template_system, entry).unwrap();
        assert_eq!(a.surface, b.surface);
        assert_eq!(outcome(&a, entry), Some("yes"));
        assert_eq!(outcome(&a, entry), outcome(&b, entry));
        if entry != 0 {
            let trace = |d: &SystemDerivation| {
                d.occurrences[0]
                    .cases
                    .iter()
                    .map(|r| (r.target.clone(), r.branch, r.status, r.diagnostic_code))
                    .collect::<Vec<_>>()
            };
            assert_eq!(trace(&a), trace(&b));
            assert!(b.token.fillers[0]
                .categories
                .iter()
                .any(|category| category == "Template"));
        }
    }
    let error_source = feature_source("case [syn.trigger]", "[on]", false, None)
        .replace("sign atom:", "trait Template:")
        .replace(
            "\nsign outer:",
            "\nsign atom:\n    belongs Template\n\nsign outer:",
        );
    let system = compile(&error_source);
    for entry in 0..3 {
        assert!(derive_entry(&system, entry)
            .unwrap_err()
            .to_string()
            .contains("(P75)"));
    }
}
