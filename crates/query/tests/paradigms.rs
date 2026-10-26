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

#[test]
fn package_catalogs_expose_usable_qualified_names_and_filter_private_declarations() {
    use conlang_language::{
        compile_with_packages_ref, LibraryCatalog, PackageId, PackageResolver, PackageSources,
        PackageSpec,
    };
    let code = "export marker trait Noun:\n\nexport sign word:\n    belongs Noun\n\nsign member:\n    syn:\n        feature:\n            number = enum(sg, pl)\n            number = sg\n\nexport paradigm Forms:\n    case [syn.number]:\n        base: member()\n\nparadigm Private:\n    case [syn.number]:\n        base: member()\n";
    let sources = ["english", "french"].map(|name| PackageSources {
        config: format!("schema = 2\nid = \"natural:{name}\"\nversion = \"1\"\nlayer = \"overlay\"\ncapabilities = [\"traits\", \"signs\", \"paradigms\"]\ncode = [\"main.lang\"]\n"),
        code: code.into(), ..Default::default()
    });
    let packages = LibraryCatalog::with_packages(sources)
        .unwrap()
        .resolve(
            &PackageSpec::default()
                .with_root("natural:english".parse::<PackageId>().unwrap())
                .with_root("natural:french".parse::<PackageId>().unwrap()),
        )
        .unwrap();
    let system = compile_with_packages_ref(&Language::new(), &packages).unwrap();
    let all = lexicon(&system, &LexiconFilter::all(), &ViewConfig::default());
    assert_eq!(all.total_before_filter, 2);
    assert_eq!(
        all.entries
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["english.word", "french.word"]
    );
    assert!(all
        .entries
        .iter()
        .all(|entry| system.evaluate_sign(&entry.name).is_ok()));
    let filtered = lexicon(
        &system,
        &LexiconFilter::all().with_category("english.Noun"),
        &ViewConfig::default(),
    );
    assert_eq!(
        filtered
            .entries
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["english.word"]
    );
    assert!(lexicon(
        &system,
        &LexiconFilter::all().with_category("Noun"),
        &ViewConfig::default()
    )
    .entries
    .is_empty());
    let catalog = paradigms(&system);
    assert_eq!(
        catalog
            .entries
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["english.Forms", "french.Forms"]
    );
    for entry in catalog.entries {
        assert!(system.callable_parameters(&entry.name).is_some());
        let selected = system
            .evaluate_paradigm(&entry.name, &Default::default())
            .unwrap();
        assert_eq!(
            entry.branches[0].member_id.as_deref(),
            Some(selected.value.sign_id().to_string().as_str())
        );
    }
}
