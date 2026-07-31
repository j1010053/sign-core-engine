use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{compile_system, DerivationContext, Dim, Language};

const FIXTURE: &str = include_str!("fixtures/tutorial_complete.lang");
const TUTORIAL: &str = include_str!("../../../tutorials/共時lang語法教學_v1.md");

fn tagged_complete_example() -> String {
    let marker = "<!-- conlang-test: tutorial-complete -->";
    let normalized = TUTORIAL.replace("\r\n", "\n").replace('\r', "\n");
    let after = normalized.split_once(marker).unwrap().1;
    after
        .split_once("```lang\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0
        .to_owned()
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
