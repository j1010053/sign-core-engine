use conlang_language::{compile_system, Language};
use conlang_query::{lexicon, paradigms, LexiconFilter, ParadigmBranchSelector, ViewConfig};

const SOURCE: &str = r#"paradigm BE:
    case [syn.person]:
        [third]: is()
        base: be()

sign be:
    syn:
        feature:
            person = enum(first, third)
            person = first | third

sign is:
    syn:
        feature:
            person = enum(first, third)
            person = third
"#;

#[test]
fn paradigm_has_a_separate_view_and_never_becomes_a_lexicon_sign() {
    let system = compile_system(Language::parse(SOURCE).unwrap()).unwrap();
    let catalog = paradigms(&system);
    assert_eq!(catalog.entries.len(), 1);
    let paradigm = &catalog.entries[0];
    assert_eq!(paradigm.name, "BE");
    assert_eq!(paradigm.match_features, ["syn.person"]);
    assert!(paradigm.parameters.is_empty());
    assert_eq!(paradigm.branches[0].application, "is()");
    assert!(matches!(
        paradigm.branches[0].selector,
        ParadigmBranchSelector::Bundle { ref values }
            if values == &[Some("third".to_owned())]
    ));
    assert!(matches!(
        paradigm.branches[1].selector,
        ParadigmBranchSelector::Base
    ));
    assert_ne!(
        paradigm.id,
        paradigm.branches[0].member_id.as_deref().unwrap()
    );

    let entries = lexicon(&system, &LexiconFilter::all(), &ViewConfig::default())
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect::<Vec<_>>();
    assert_eq!(entries, ["be", "is"]);
    assert!(!entries.iter().any(|name| name == "BE"));
}
