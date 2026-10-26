use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{
    check_language_with_packages, compile_with_packages_ref, Language, LanguageDocument,
    LibraryCatalog, PackageResolver, PackageSources, PackageSpec,
};

fn package(id: &str, code: &str) -> PackageSources {
    PackageSources {
        config: format!("schema = 2\nid = \"{id}\"\nversion = \"1\"\nlayer = \"overlay\"\ncapabilities = [\"traits\", \"signs\", \"paradigms\"]\ncode = [\"code/main.lang\"]\n"),
        code: code.into(), ..Default::default()
    }
}

fn resolved(code: &str) -> conlang_language::ResolvedPackages {
    let catalog = LibraryCatalog::with_packages([package("natural:english", code)]).unwrap();
    let mut spec = PackageSpec::default().with_root(
        "natural:english"
            .parse::<conlang_language::PackageId>()
            .unwrap(),
    );
    spec.aliases
        .insert("English".into(), "natural:english".parse().unwrap());
    catalog.resolve(&spec).unwrap()
}

#[test]
fn export_and_qualified_references_survive_source_and_identity_round_trips() {
    let source = "export marker trait Noun:\n\nexport trait Count<T: English.Noun>:\n    belongs English.Noun\n\nexport sign word:\n    belongs English.Noun\n    English.Noun[0]\n\nexport paradigm Words:\n    case [syn.number]:\n        base: English.word()\n";
    let document = LanguageDocument::import_new_root(source, "exports").unwrap();
    let (source, identities) = document.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&source, &identities).unwrap();
    assert_eq!(document, reopened);
    assert!(reopened.language().traits.iter().all(|v| v.exported));
    assert!(reopened.language().signs[0].exported);
    assert!(reopened.language().paradigms[0].exported);
    assert!(source.contains("English.Noun"));
    assert!(Language::parse("export distribution:\n").is_err());
}

#[test]
fn exported_sign_can_use_private_trait_and_sign_without_exposing_them() {
    let packages = resolved("Symbol a\nClass vowel {a}\n\nmarker trait Hidden:\n\nsign inner:\n    belongs Hidden\n    phon:\n        /a/\n\nexport sign word:\n    syn:\n        slots:\n            item [Hidden]\n        map item autofill inner\n        map item internalize\n    phon:\n        /{$slot.item}/\n");
    let system = compile_with_packages_ref(&Language::new(), &packages).unwrap();
    assert_eq!(
        system
            .derive("English.word", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    for name in ["inner", "English.inner"] {
        assert!(system.evaluate_sign(name).is_err(), "{name}");
    }
    for reference in ["Hidden", "English.Hidden"] {
        let source = Language::parse(&format!("sign bad:\n    belongs {reference}\n")).unwrap();
        assert!(check_language_with_packages(&source, &packages).has_errors());
        assert!(compile_with_packages_ref(&source, &packages).is_err());
    }
    let hidden = system
        .effective_language()
        .signs
        .iter()
        .find(|v| !v.exported)
        .unwrap();
    assert!(system
        .derive(&hidden.name, &[], &SlotMap::identity())
        .is_err());
}

#[test]
fn same_export_names_coexist_and_ambiguous_bare_names_fail() {
    let catalog = LibraryCatalog::with_packages([
        package("natural:english", "Symbol a\nSymbol b\nClass vowel {a, b}\n\nexport marker trait Noun:\n\nexport sign word:\n    belongs Noun\n    phon:\n        /a/\n"),
        package("natural:french", "export marker trait Noun:\n\nexport sign word:\n    belongs Noun\n    phon:\n        /b/\n"),
    ]).unwrap();
    let mut spec = PackageSpec::default()
        .with_root(
            "natural:english"
                .parse::<conlang_language::PackageId>()
                .unwrap(),
        )
        .with_root(
            "natural:french"
                .parse::<conlang_language::PackageId>()
                .unwrap(),
        );
    spec.aliases
        .insert("English".into(), "natural:english".parse().unwrap());
    spec.aliases
        .insert("French".into(), "natural:french".parse().unwrap());
    let packages = catalog.resolve(&spec).unwrap();
    let system = compile_with_packages_ref(&Language::new(), &packages).unwrap();
    assert_eq!(
        system
            .derive("English.word", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    assert_eq!(
        system
            .derive("French.word", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "b"
    );
    assert!(system.derive("word", &[], &SlotMap::identity()).is_err());
    let ambiguous = Language::parse("sign bad:\n    belongs Noun\n").unwrap();
    assert!(compile_with_packages_ref(&ambiguous, &packages).is_err());
}

#[test]
fn only_exported_declarations_can_be_overridden() {
    let packages = resolved("Symbol a\nSymbol b\nClass vowel {a, b}\n\nmarker trait Secret:\n\nexport marker trait Noun:\n\nexport sign word:\n    belongs Noun\n    phon:\n        /a/\n");
    let caller =
        Language::parse("sign English.word:\n    belongs English.Noun\n    phon:\n        /b/\n")
            .unwrap();
    let system = compile_with_packages_ref(&caller, &packages).unwrap();
    assert_eq!(
        system
            .derive("English.word", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "b"
    );
    assert!(packages.packages()[0].code.contains("/a/"));
    let illegal = Language::parse("marker trait English.Secret:\n").unwrap();
    assert!(compile_with_packages_ref(&illegal, &packages).is_err());
}

#[test]
fn export_table_cannot_promote_a_private_declaration() {
    let mut source = package("natural:english", "marker trait Noun:\n");
    source.exports = "stable_id\tkind\talias\nnatural:english:Noun\ttrait\tNoun\n".into();
    let error = LibraryCatalog::with_packages([source]).unwrap_err();
    assert!(error.to_string().contains("PACKAGE_PRIVATE_EXPORT"));
}

#[test]
fn qualified_slot_constraints_and_runtime_fillers_resolve() {
    let packages = resolved("Symbol a\nClass vowel {a}\n\nexport trait Noun:\n    syn:\n        feature:\n            number = enum(singular, plural)\n\nexport sign word:\n    belongs Noun\n    Noun[0]\n    syn:\n        feature:\n            number = singular\n    phon:\n        /a/\n");
    let source = Language::parse("sign wrapper:\n    syn:\n        slots:\n            item [English.Noun]\n    phon:\n        /{$slot.item}/\n").unwrap();
    let system = compile_with_packages_ref(&source, &packages).unwrap();
    assert_eq!(
        system
            .derive(
                "wrapper",
                &[SlotFiller::sign("item", "English.word")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
}

#[test]
fn public_paradigm_preserves_private_member_identity() {
    let packages = resolved(
        r#"Symbol a
Class vowel {a}

sign member:
    syn:
        feature:
            number = enum(sg, pl)
            number = sg
    phon:
        /a/

export paradigm Forms:
    case [syn.number]:
        [sg]: member()
        base: member()

paradigm Private:
    case [syn.number]:
        base: member()
"#,
    );
    let caller = Language::parse("sign wrapper:\n    syn:\n        slots:\n            item [*]\n        map item autofill English.Forms()\n        feature:\n            number = enum(sg, pl)\n            number = sg\n    phon:\n        /{$slot.item}/\n").unwrap();
    let system = compile_with_packages_ref(&caller, &packages).unwrap();
    let context = conlang_language::DerivationContext::new().feature(
        conlang_language::Dim::Syn,
        "number",
        "sg",
    );
    let member = system
        .effective_language()
        .signs
        .iter()
        .find(|s| s.name.ends_with(".member"))
        .unwrap();
    let selected = system.evaluate_paradigm("English.Forms", &context).unwrap();
    assert_eq!(selected.value.sign_id(), &member.id);
    assert_eq!(
        system
            .derive_with_context("wrapper", &[], &SlotMap::identity(), context.clone())
            .unwrap()
            .surface,
        "a"
    );
    for name in ["Private", "English.Private"] {
        assert!(system.evaluate_paradigm(name, &context).is_err());
        assert!(system.evaluate_sign_expression(name, &context).is_err());
    }
    let hidden = &system
        .effective_language()
        .paradigms
        .iter()
        .find(|p| !p.exported)
        .unwrap()
        .name;
    assert!(system.evaluate_paradigm(hidden, &context).is_err());
    assert!(system.evaluate_sign("English.member").is_err());
}

#[test]
fn package_aliases_and_dependency_bindings_ignore_project_alias_overrides() {
    let base = package(
        "natural:base",
        "Symbol a\nClass vowel {a}\nexport marker trait Noun:\n",
    );
    let other = package("natural:other", "export marker trait Noun:\n");
    let mut consumer = package(
        "natural:consumer",
        "export sign word:\n    belongs Grammar.Noun\n    phon:\n        /a/\n",
    );
    consumer
        .config
        .push_str("requires = [\"natural:base\"]\n[aliases]\nGrammar = \"natural:base\"\n");
    let catalog = LibraryCatalog::with_packages([base, other, consumer]).unwrap();
    let mut spec = PackageSpec::default()
        .with_root(
            "natural:consumer"
                .parse::<conlang_language::PackageId>()
                .unwrap(),
        )
        .with_root(
            "natural:other"
                .parse::<conlang_language::PackageId>()
                .unwrap(),
        );
    spec.aliases
        .insert("Grammar".into(), "natural:other".parse().unwrap());
    let packages = catalog.resolve(&spec).unwrap();
    let caller =
        Language::parse("sign local:\n    belongs Grammar.Noun\n    phon:\n        /a/\n").unwrap();
    let system = compile_with_packages_ref(&caller, &packages).unwrap();
    let word = system.effective_language().sign_named("word").unwrap();
    let local = system.effective_language().sign_named("local").unwrap();
    assert_ne!(
        system.ontology.sign_categories(word),
        system.ontology.sign_categories(local)
    );
    assert_eq!(
        system
            .derive("consumer.word", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    assert!(catalog
        .resolve_export(conlang_language::LibraryKind::Natural, "Noun")
        .is_err());
    assert!(catalog
        .resolve_export(conlang_language::LibraryKind::Natural, "base.Noun")
        .is_ok());
    assert!(!catalog.export_index().contains_key("Noun"));
}

#[test]
fn packages_cannot_capture_undeclared_dependencies_or_private_exports() {
    for (reference, requires) in [
        ("Noun", ""),
        ("base.Noun", ""),
        ("base.Secret", "requires = [\"natural:base\"]\n"),
    ] {
        let base = package(
            "natural:base",
            "export marker trait Noun:\nmarker trait Secret:\n",
        );
        let mut consumer = package(
            "natural:consumer",
            &format!("export sign word:\n    belongs {reference}\n"),
        );
        consumer.config.push_str(requires);
        let catalog = LibraryCatalog::with_packages([base, consumer]).unwrap();
        let spec = PackageSpec::default()
            .with_root(
                "natural:base"
                    .parse::<conlang_language::PackageId>()
                    .unwrap(),
            )
            .with_root(
                "natural:consumer"
                    .parse::<conlang_language::PackageId>()
                    .unwrap(),
            );
        assert!(catalog.resolve(&spec).is_err(), "{reference}");
    }
}

#[test]
fn private_globals_stay_inside_their_package() {
    let packages = resolved(
        r#"Symbol a
Symbol b
Class vowel {a, b}

global trait Sound:
    phon:
        a => b

export sign word:
    phon:
        /a/
"#,
    );
    let caller = Language::parse("sign local:\n    phon:\n        /a/\n").unwrap();
    let system = compile_with_packages_ref(&caller, &packages).unwrap();
    assert_eq!(
        system
            .derive("English.word", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "b"
    );
    assert_eq!(
        system
            .derive("local", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
}

#[test]
fn qualified_bounds_arguments_mounts_and_category_guards_execute() {
    let packages = resolved(
        r#"Symbol a
Symbol b
Class vowel {a, b}

export trait Noun:
    syn:
        feature:
            number = enum(sg, pl)

export sign word:
    belongs Noun
    Noun[0]
    syn:
        feature:
            number = sg
    phon:
        /a/
"#,
    );
    let caller = Language::parse(
        r#"trait Wrapper<T: English.Noun>:
    pass

sign wrapped:
    belongs Wrapper<English.Noun>
    Wrapper[0]
    belongs English.Noun
    English.Noun[0]
    syn:
        slots:
            item [English.Noun]
        feature:
            number = sg
            number => pl / [English.Noun]
    phon:
        /{$slot.item}/
        realization:
            case:
                $slot.item == [English.Noun] && $self.syn.number == pl:
                    /b/
                base:
"#,
    )
    .unwrap();
    let system = compile_with_packages_ref(&caller, &packages).unwrap();
    let result = system
        .derive(
            "wrapped",
            &[SlotFiller::sign("item", "English.word")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.surface, "b");
    assert_eq!(result.cases.last().unwrap().branch, 0);
}

#[test]
fn malformed_export_headers_and_qualified_names_are_rejected() {
    for source in [
        "export\n",
        "export sign English.:\n",
        "export trait .Noun:\n",
        "export trait English..Noun:\n",
    ] {
        assert!(Language::parse(source).is_err(), "{source}");
    }
}

#[test]
fn tutorial_package_is_executable() {
    let sources = PackageSources {
        config: include_str!(
            "../../../tutorials/package-exports/packages/natural/english/package.toml"
        )
        .into(),
        code: include_str!(
            "../../../tutorials/package-exports/packages/natural/english/code/grammar.lang"
        )
        .into(),
        ..Default::default()
    };
    let catalog = LibraryCatalog::with_packages([sources]).unwrap();
    let mut spec = PackageSpec::default().with_root(
        "natural:english"
            .parse::<conlang_language::PackageId>()
            .unwrap(),
    );
    spec.aliases
        .insert("English".into(), "natural:english".parse().unwrap());
    let packages = catalog.resolve(&spec).unwrap();
    let language = Language::parse(include_str!(
        "../../../tutorials/package-exports/language.lang"
    ))
    .unwrap();
    let system = compile_with_packages_ref(&language, &packages).unwrap();
    assert_eq!(
        system
            .derive(
                "NP",
                &[SlotFiller::sign("head", "English.cat")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "kat"
    );
    assert_eq!(
        system
            .derive("plural_cat", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "kats"
    );
    assert!(system.evaluate_sign("English.cats").is_err());
}

#[test]
fn public_trait_and_paradigm_can_be_overridden_locally() {
    let packages = resolved("export trait Noun:\n    syn:\n        feature:\n            number = enum(sg, pl)\n\nexport sign first:\n    belongs Noun\n    syn:\n        feature:\n            number = sg | pl\n\nexport sign second:\n    belongs Noun\n    syn:\n        feature:\n            number = sg | pl\n\nexport paradigm Forms:\n    case [syn.number]:\n        base: first()\n");
    let dual = "sign dual:\n    belongs English.Noun\n    syn:\n        feature:\n            number = dual\n";
    assert!(compile_with_packages_ref(&Language::parse(dual).unwrap(), &packages).is_err());
    let source = format!("trait English.Noun:\n    syn:\n        feature:\n            number = enum(sg, pl, dual)\n\nparadigm English.Forms:\n    case [syn.number]:\n        base: English.second()\n\n{dual}");
    let system = compile_with_packages_ref(&Language::parse(&source).unwrap(), &packages).unwrap();
    let selected = system
        .evaluate_paradigm(
            "English.Forms",
            &conlang_language::DerivationContext::new().feature(
                conlang_language::Dim::Syn,
                "number",
                "sg",
            ),
        )
        .unwrap();
    assert_eq!(
        selected.value.sign_id(),
        &system.effective_language().sign_named("second").unwrap().id
    );
    assert!(packages.packages()[0].code.contains("base: first()"));
}

#[test]
fn private_names_cannot_enter_through_runtime_arguments_and_maps() {
    let packages = resolved("Symbol a\nSymbol b\nClass vowel {a, b}\nsign inner:\n    phon:\n        /a/\n\nexport sign wrapper:\n    syn:\n        slots:\n            item [*]\n        map item autofill inner\n        map item internalize\n    phon:\n        /{$slot.item}/\n");
    let caller = Language::parse("sign inner:\n    phon:\n        /b/\n\nsign use:\n    syn:\n        slots:\n            item [*]\n    phon:\n        /{$slot.item}/\n").unwrap();
    let system = compile_with_packages_ref(&caller, &packages).unwrap();
    assert_eq!(
        system
            .derive("English.wrapper", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    assert_eq!(
        system
            .derive(
                "use",
                &[SlotFiller::sign("item", "inner")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "b"
    );
    let private = "English.inner";
    assert!(system
        .derive(
            "use",
            &[SlotFiller::sign("item", private)],
            &SlotMap::identity()
        )
        .is_err());
    let mapping = SlotMap::from_ops(vec![conlang_language::SlotMapOp::AutoFill {
        slot: "item".into(),
        filler: private.into(),
    }]);
    assert!(system.derive("use", &[], &mapping).is_err());
    let current = system
        .evaluate_sign_expression("inner", &Default::default())
        .unwrap();
    let app = conlang_language::parser::parse_sign_application_source("use(item: English.inner())")
        .unwrap();
    assert!(system.evaluate_application(&current.value, &app).is_err());
}
