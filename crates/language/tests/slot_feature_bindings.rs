use conlang_language::construction::{OccurrenceCaseStatus, SlotFiller, SlotMap};
use conlang_language::{compile_system, CompileSystemError, Dim, Language};

const PRELUDE: &str = r#"Symbol a
Class vowel {a}

trait LocalCaseBearer:
    belongs Noun
    syn:
        feature:
            case = enum(nominative, accusative)

trait LocalCaseAssigner:
    belongs Predicate
    syn:
        feature:
            assigned_case = enum(nominative, accusative)

sign atom:
    belongs LocalCaseBearer
    phon:
        /a/

sign assigner:
    belongs LocalCaseAssigner
    syn:
        feature:
            assigned_case = accusative
    phon:
        /a/
"#;

fn validation_codes(body: &str) -> Vec<String> {
    let source = format!("{PRELUDE}\n{body}");
    let CompileSystemError::Validation(report) =
        compile_system(Language::parse(&source).unwrap()).unwrap_err()
    else {
        panic!("expected coded validation error");
    };
    report
        .errors()
        .map(|diagnostic| diagnostic.code.to_owned())
        .collect()
}

#[test]
fn slot_feature_contract_rejects_unknown_anysign_and_bad_domains() {
    let unknown_target = validation_codes(
        r#"sign Bad:
    syn:
        slots:
            item [LocalCaseBearer]
        slot_features:
            ghost.case = nominative
    phon:
        /{item}/
"#,
    );
    assert!(unknown_target.contains(&"SLOT_FEATURE_UNKNOWN_TARGET".to_owned()));

    let any_sign = validation_codes(
        r#"sign Bad:
    syn:
        slots:
            item [*]
        slot_features:
            item.case = nominative
    phon:
        /{item}/
"#,
    );
    assert!(any_sign.contains(&"SLOT_FEATURE_ANY_SIGN_TARGET".to_owned()));

    let out_of_domain = validation_codes(
        r#"sign Bad:
    syn:
        slots:
            item [LocalCaseBearer]
        slot_features:
            item.case = ergative
    phon:
        /{item}/
"#,
    );
    assert!(out_of_domain.contains(&"SLOT_FEATURE_VALUE_OUT_OF_DOMAIN".to_owned()));

    let unknown_source = validation_codes(
        r#"sign Bad:
    syn:
        slots:
            item [LocalCaseBearer]
        slot_features:
            item.case = $slot.ghost.syn.assigned_case
    phon:
        /{item}/
"#,
    );
    assert!(unknown_source.contains(&"SLOT_FEATURE_UNKNOWN_SOURCE".to_owned()));

    let domain_mismatch = validation_codes(
        r#"trait WideAssigner:
    belongs Predicate
    syn:
        feature:
            assigned_case = enum(nominative, ergative)
sign Bad:
    syn:
        slots:
            item [LocalCaseBearer]
            source [WideAssigner]
        slot_features:
            item.case = $slot.source.syn.assigned_case
    phon:
        /{source}{item}/
"#,
    );
    assert!(domain_mismatch.contains(&"SLOT_FEATURE_DOMAIN_MISMATCH".to_owned()));

    let duplicate = validation_codes(
        r#"sign Bad:
    syn:
        slots:
            item [LocalCaseBearer]
        slot_features:
            item.case = nominative
            item.case = accusative
    phon:
        /{item}/
"#,
    );
    assert!(duplicate.contains(&"SLOT_FEATURE_DUPLICATE_TARGET".to_owned()));
}

#[test]
fn optional_absence_is_skipped_and_filled_occurrence_is_contextualized() {
    let source = format!(
        r#"{PRELUDE}
sign OptionalCase:
    syn:
        slots:
            target [LocalCaseBearer]?
            source [LocalCaseAssigner]
        slot_features:
            target.case = $slot.source.syn.assigned_case
    phon:
        /{{source}}{{target}}/
"#
    );
    let system = compile_system(Language::parse(&source).unwrap()).unwrap();

    let absent = system
        .derive(
            "OptionalCase",
            &[SlotFiller::sign("source", "assigner")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(absent.surface, "a");

    let present = system
        .derive(
            "OptionalCase",
            &[
                SlotFiller::sign("target", "atom"),
                SlotFiller::sign("source", "assigner"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(present.surface, "aa");
    let target = present
        .token
        .fillers
        .iter()
        .find(|snapshot| snapshot.slot == "target")
        .unwrap();
    assert_eq!(target.scalar(Dim::Syn, "case"), Some("accusative"));
    assert!(system
        .effective_language()
        .sign_named("atom")
        .unwrap()
        .project(Dim::Syn, &system.ontology)
        .get("syn.case")
        .is_none());
}

#[test]
fn derived_token_downward_case_forwarding_recontextualizes_the_occurrence() {
    let source = format!(
        r#"{PRELUDE}
sign stem:
    belongs Noun
    phon:
        /a/
sign InnerNominal:
    belongs LocalCaseBearer
    syn:
        slots:
            stem [Noun]
    sem:
        feature:
            interpreted_case = enum(nominative, accusative)
            interpreted_case =>
                case:
                    $self.syn.case == accusative:
                        accusative
                    else:
                        nominative
        roles:
            argument [Entity]
            argument =
                case:
                    $self.syn.case == accusative:
                        {{stem}}
                    else:
                        {{stem}}
    prag:
        feature:
            discourse_case = enum(nominative, accusative)
            discourse_case => $self.sem.interpreted_case
    phon:
        /{{stem}}/
        realization:
            case:
                $self.syn.case == accusative:
                    /{{stem}}a/
                else:
                    /{{stem}}/
sign OuterCase:
    syn:
        slots:
            target [LocalCaseBearer]
            source [LocalCaseAssigner]
        slot_features:
            target.case = $slot.source.syn.assigned_case
    phon:
        /{{source}}{{target}}/
"#
    );
    let system = compile_system(Language::parse(&source).unwrap()).unwrap();
    let inner = system
        .derive(
            "InnerNominal",
            &[SlotFiller::sign("stem", "stem")],
            &SlotMap::identity(),
        )
        .unwrap();
    let outer = system
        .derive(
            "OuterCase",
            &[
                SlotFiller::token("target", &inner.token),
                SlotFiller::sign("source", "assigner"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    let target = outer
        .token
        .fillers
        .iter()
        .find(|snapshot| snapshot.slot == "target")
        .unwrap();
    assert_eq!(target.scalar(Dim::Syn, "case"), Some("accusative"));
    assert_eq!(
        target.scalar(Dim::Sem, "interpreted_case"),
        Some("accusative")
    );
    assert_eq!(
        target.scalar(Dim::Prag, "discourse_case"),
        Some("accusative")
    );
    assert_eq!(target.sem.role("argument").unwrap().source.sign, "stem");
    assert_eq!(outer.surface, "aaa");
    assert!(inner.token.syn.iter().all(|(path, _)| path != "syn.case"));
    let occurrence = outer
        .occurrences
        .iter()
        .find(|record| record.slot_path == "target")
        .unwrap();
    assert!(occurrence.reevaluated);
    assert_eq!(occurrence.realization.as_deref(), Some("aa"));
    assert!(occurrence.cases.iter().any(|record| {
        record.target == "sem.feature.interpreted_case"
            && record.branch == 0
            && record.status == OccurrenceCaseStatus::Matched
    }));
    assert!(occurrence.cases.iter().any(|record| {
        record.target == "sem.role.argument"
            && record.branch == 0
            && record.status == OccurrenceCaseStatus::Matched
    }));
    assert!(
        occurrence
            .committed_rules
            .iter()
            .filter(|record| record.status == conlang_language::synchronic::RuleStatus::Matched)
            .count()
            >= 1
    );
    assert_eq!(
        occurrence.constraints,
        vec![(Dim::Syn, "case".to_owned(), "accusative".to_owned())]
    );
}
