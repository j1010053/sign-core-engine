use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{check_language, compile_system, DerivationContext, Dim, Language};

const FIXTURE: &str = include_str!("fixtures/tutorial_complete.lang");
const TUTORIAL: &str = include_str!("../../../tutorials/共時lang語法教學_v1.md");

fn tagged_lang_example(name: &str) -> String {
    let marker = format!("<!-- conlang-test: {name} -->");
    let normalized = TUTORIAL.replace("\r\n", "\n").replace('\r', "\n");
    let after = normalized
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing tutorial example {name:?}"))
        .1;
    after
        .split_once("```lang\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0
        .to_owned()
}

fn tagged_complete_example() -> String {
    tagged_lang_example("tutorial-complete")
}

#[test]
fn documented_complete_grammar_is_the_compiled_fixture() {
    let documented = tagged_complete_example();
    assert_eq!(
        Language::parse(&documented).unwrap().dump(),
        Language::parse(FIXTURE).unwrap().dump()
    );
    compile_system(Language::parse(&documented).unwrap()).unwrap();
}

#[test]
fn documented_parameter_scope_and_narrower_bound_are_valid() {
    let language = Language::parse(&tagged_lang_example("parameterized-trait-scope")).unwrap();
    let report = check_language(&language);
    assert!(
        !report.has_errors(),
        "documented generic trait should validate: {:?}",
        report.diagnostics()
    );
}

#[test]
fn documented_unknown_categories_are_not_implicit_parameters() {
    let language =
        Language::parse(&tagged_lang_example("parameterized-trait-invalid-scope")).unwrap();
    let report = check_language(&language);
    let codes = report
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&"SLOT_UNKNOWN_CATEGORY"), "{codes:?}");
    assert!(codes.contains(&"ROLE_UNKNOWN_CONSTRAINT"), "{codes:?}");
}

#[test]
fn documented_unbounded_forwarding_is_rejected() {
    let language =
        Language::parse(&tagged_lang_example("parameterized-trait-invalid-bound")).unwrap();
    let report = check_language(&language);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "TYPE_PARAM_BOUND_VIOLATION"),
        "documented invalid bound forwarding should be rejected: {:?}",
        report.diagnostics()
    );
}

#[test]
fn tutorial_recursive_occurrence_is_four_dimensional_and_deterministic() {
    let system = compile_system(Language::parse(FIXTURE).unwrap()).unwrap();
    let np = system
        .derive_with_context(
            "TutorialNP",
            &[SlotFiller::sign("stem", "dog")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "plural"),
        )
        .unwrap();
    assert_eq!(np.surface, "dogs");

    let derive_clause = || {
        system.derive(
            "TutorialClause",
            &[
                SlotFiller::token("subject", &np.token),
                SlotFiller::sign("predicate", "run"),
            ],
            &SlotMap::identity(),
        )
    };
    let first = derive_clause().unwrap();
    let second = derive_clause().unwrap();
    assert_eq!(first.surface, "dogs run");
    assert_eq!(first.surface, second.surface);
    assert_eq!(first.token.syn, second.token.syn);
    assert_eq!(first.token.sem, second.token.sem);
    assert_eq!(first.token.prag, second.token.prag);
    assert_eq!(first.token.construction_id, second.token.construction_id);

    let subject = first
        .token
        .fillers
        .iter()
        .find(|snapshot| snapshot.slot == "subject")
        .unwrap();
    assert_eq!(subject.scalar(Dim::Syn, "number"), Some("plural"));
    assert_eq!(subject.scalar(Dim::Syn, "case"), Some("nominative"));
    assert_eq!(
        subject.scalar(Dim::Sem, "interpreted_case"),
        Some("nominative")
    );
    assert_eq!(
        subject.scalar(Dim::Prag, "discourse_case"),
        Some("nominative")
    );
    assert!(first
        .token
        .sem
        .roles
        .iter()
        .any(|(role, _)| role == "agent"));
    let occurrence = first
        .occurrences
        .iter()
        .find(|record| record.slot_path == "subject")
        .unwrap();
    assert!(occurrence.reevaluated);
    assert_eq!(occurrence.realization.as_deref(), Some("dogs"));
    assert!(!occurrence.committed_rules.is_empty());
}
