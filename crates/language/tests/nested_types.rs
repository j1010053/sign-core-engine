use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{compile_system_ref, CompileSystemError, Language, SignItem};

#[test]
fn tutorial_file_derives_and_enforces_captured_types() {
    let source = include_str!("../../../tutorials/nested-types/phrases.lang");
    let system = compiled(source);
    let mapping = SlotMap::identity();
    assert_eq!(system.derive("demo", &[], &mapping).unwrap().surface, "ba");
    assert_eq!(
        system
            .derive(
                "XP<N>.bar-comp",
                &[SlotFiller::sign("head", "noun")],
                &mapping
            )
            .unwrap()
            .surface,
        "a"
    );
    assert!(system
        .derive(
            "XP<V>.bar-comp",
            &[SlotFiller::sign("head", "noun")],
            &mapping
        )
        .is_err());
    let changed = compiled(&source.replace("/a/", "/u/"));
    assert_eq!(changed.derive("demo", &[], &mapping).unwrap().surface, "bu");
}

const PHRASES: &str = r#"Symbol a
Symbol b
Class vowel {a, b}

marker trait N:
marker trait V:
export marker trait X-bar<T>:

export trait XP<T>:
    export sign bar-comp:
        belongs X-bar<T>
        syn:
            slots:
                head [T]
        phon:
            /{$slot.head}/
    export sign bar-adjunct:
        belongs X-bar<T>
        syn:
            slots:
                adjunct [*]?
                head [X-bar<T>]
        phon:
            /{$slot.head}/
    syn:
        slots:
            spec [*]?
            head [X-bar<T>]

sign NP:
    belongs XP<N>
    XP[0]
    phon:
        /{$slot.head}/
sign VP:
    belongs XP<V>
    XP[0]
    phon:
        /{$slot.head}/
sign noun:
    belongs N
    phon:
        /a/
sign verb:
    belongs V
    phon:
        /b/
"#;

fn compiled(source: &str) -> conlang_language::CompiledSystem {
    let language = Language::parse(source).unwrap();
    compile_system_ref(&language).unwrap_or_else(|error| match error {
        CompileSystemError::Validation(report) => panic!(
            "{:?}",
            report
                .diagnostics()
                .iter()
                .filter(|d| d.severity == conlang_language::Severity::Error)
                .collect::<Vec<_>>()
        ),
        other => panic!("{other:?}"),
    })
}

#[test]
fn nested_source_is_separate_from_macro_blocks_and_roundtrips() {
    let source = Language::parse(PHRASES).unwrap();
    let xp = source.trait_named("XP").unwrap();
    assert_eq!(xp.blocks.len(), 1);
    assert_eq!(xp.members.signs.len(), 2);
    assert_eq!(
        xp.blocks[0]
            .items
            .iter()
            .filter(|i| matches!(i, SignItem::Slot(_)))
            .count(),
        2
    );
    let canonical = source.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    let document =
        conlang_language::LanguageDocument::import_new_root(PHRASES, "nested:source").unwrap();
    let leaf = document.ref_for_sign("XP.bar-comp").unwrap();
    let owner = document.ref_for_trait("XP").unwrap();
    let entry = document
        .identities()
        .nodes
        .iter()
        .find(|entry| entry.id == leaf.id)
        .unwrap();
    assert_eq!(entry.parent.as_ref(), Some(&owner.id));
    let (source, ids) = document.dump_pair().unwrap();
    let reopened = conlang_language::LanguageDocument::open(&source, &ids).unwrap();
    assert_eq!(document, reopened);
    let system = compiled(&source);
    let outcome = system
        .derive(
            "XP<N>.bar-comp",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(outcome.surface, "a");
}

#[test]
fn captured_parameters_reach_leaf_and_invariant_family_survives_derive() {
    let system = compiled(PHRASES);
    let leaf = system
        .derive(
            "XP<N>.bar-comp",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(leaf.surface, "a");
    let bar = system
        .derive(
            "XP<N>.bar-adjunct",
            &[SlotFiller::token("head", &leaf.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    let np = system
        .derive(
            "NP",
            &[SlotFiller::token("head", &bar.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(np.surface, "a");
    assert!(!np
        .diagnostics
        .iter()
        .any(|d| d.severity == conlang_language::Severity::Error));
    assert!(!np.phon_steps.iter().any(|step| step
        .diags
        .iter()
        .any(|d| d.level == tshiatun_core::diag::DiagLevel::Error)));
    assert!(system
        .derive(
            "VP",
            &[SlotFiller::token("head", &bar.token)],
            &SlotMap::identity()
        )
        .is_err());
    assert!(system
        .derive(
            "XP<N>.bar-comp",
            &[SlotFiller::sign("head", "verb")],
            &SlotMap::identity()
        )
        .is_err());
    let other = system
        .derive(
            "XP<V>.bar-comp",
            &[SlotFiller::sign("head", "verb")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_ne!(leaf.token.construction_id, other.token.construction_id);
    assert_eq!(
        system.source_sign_id(&leaf.token.construction_id),
        system.source_sign_id(&other.token.construction_id)
    );
    assert_eq!(
        system
            .language()
            .trait_named("XP")
            .unwrap()
            .members
            .signs
            .len(),
        2
    );
}

#[test]
fn private_members_do_not_leak_to_siblings_or_runtime() {
    let source = "trait Owner<T>:\n    marker trait Hidden:\n    sign helper:\n        belongs Hidden\n    syn:\n        slots:\n            item [Hidden]\nmarker trait N:\nsign use:\n    belongs Owner<N>\n    phon:\n        /{$slot.item}/\n";
    let system = compiled(source);
    assert!(system.evaluate_sign("Owner<N>.helper").is_err());
    assert!(compile_system_ref(
        &Language::parse(&format!(
            "{source}\nsign bad:\n    belongs Owner<N>.Hidden\n"
        ))
        .unwrap()
    )
    .is_err());
}

#[test]
fn recursive_arguments_arity_bounds_and_capture_errors() {
    let valid = "marker trait N:\nmarker trait Box<T>:\ntrait Outer<T>:\n    export trait Inner<U>:\n        syn:\n            slots:\n                left [T]\n                right [U]\nsign pair:\n    belongs Outer<Box<N>>.Inner<N>\n    Outer<Box<N>>.Inner<N>[0]\n    phon:\n        /{$slot.left}{$slot.right}/\n";
    compiled(valid);
    for source in [
        "marker trait N:\nmarker trait Tag<T>:\nsign bad:\n    belongs Tag\n",
        "marker trait N:\nsign bad:\n    belongs N<N>\n",
        "trait Outer<T>:\n    trait Inner<T>:\n        pass\n",
        "marker trait N:\nmarker trait V:\nmarker trait Box<T: N>:\nsign bad:\n    belongs Box<V>\n",
    ] {
        assert!(conlang_language::check_language(&Language::parse(source).unwrap()).has_errors(), "{source}");
    }
}

#[test]
fn arguments_do_not_capture_the_templates_own_parameter_names() {
    let system = compiled("marker trait N:\nmarker trait T:\ntrait Pair<T, U>:\n    syn:\n        slots:\n            first [T]\n            second [U]\nsign pair:\n    belongs Pair<N, T>\n    Pair<N, T>[0]\n    phon:\n        /{$slot.first}{$slot.second}/\n");
    let parameters = system.callable_parameters("pair").unwrap();
    assert_eq!(parameters[0].constraint.category(), Some("N"));
    assert_eq!(parameters[1].constraint.category(), Some("T"));
    assert!(system.language().dump().contains("Pair<N, T>[0]"));
}

#[test]
fn member_call_activates_owner_and_public_paradigm_can_use_private_member() {
    let system = compiled("Symbol a\nClass vowel {a}\nmarker trait N:\ntrait Factory<T>:\n    sign inner:\n        belongs T\n        syn:\n            feature:\n                choice = enum(a, b)\n                choice = a\n        phon:\n            /a/\n    export paradigm Pick:\n        case [syn.choice]:\n            base: inner()\n    export sign other:\n        belongs T\n        phon:\n            /a/\nsign result:\n    syn:\n        slots:\n            head [N]\n        map head autofill Factory<N>.Pick()\n    phon:\n        /{$slot.head}/\n");
    assert_eq!(
        system
            .derive("result", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    assert!(system.evaluate_sign("Factory<N>.other").is_ok());
    assert!(system.evaluate_sign("Factory<N>.inner").is_err());
}

#[test]
fn generic_guards_and_case_memberships_retain_arguments() {
    let source = "Symbol a\nClass vowel {a}\nmarker trait N:\nmarker trait V:\nmarker trait Box<T>:\nsign noun:\n    belongs Box<N>\n    phon:\n        /a/\nsign verb:\n    belongs Box<V>\n    phon:\n        /a/\nsign choose:\n    syn:\n        slots:\n            head [*]\n        feature:\n            selected = enum(yes, no)\n            selected =\n                case:\n                    $slot.head == [Box<N>]:\n                        yes\n                    base:\n                        no\n    phon:\n        /{$slot.head}/\n";
    let system = compiled(source);
    let noun = system
        .derive(
            "choose",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity(),
        )
        .unwrap();
    let verb = system
        .derive(
            "choose",
            &[SlotFiller::sign("head", "verb")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(noun.surface, "a");
    assert_eq!(verb.surface, "a");
    assert!(noun
        .token
        .syn
        .iter()
        .any(|(key, value)| key.ends_with("selected") && value == "yes"));
    assert!(verb
        .token
        .syn
        .iter()
        .any(|(key, value)| key.ends_with("selected") && value == "no"));
    let source = Language::parse("marker trait N:\nmarker trait Box<T>:\nsign sample:\n    case:\n        base:\n            $self\n            belongs Box<N>\n").unwrap();
    assert!(source.dump().contains("belongs Box<N>"));
}

#[test]
fn expanding_type_cycles_and_incomplete_or_ambiguous_mounts_fail() {
    for source in [
        "marker trait N:\nmarker trait Box<T>:\ntrait Loop<T>:\n    syn:\n        slots:\n            next [Loop<Box<T>>]\nsign use:\n    belongs Loop<N>\n",
        "marker trait N:\nmarker trait V:\ntrait Pair<T>:\n    pass\nsign use:\n    belongs Pair<N>\n    belongs Pair<V>\n    Pair[0]\n",
        "marker trait N:\ntrait Blocks<T>:\n    pass\n    ==\n    pass\nsign use:\n    belongs Blocks<N>\n    Blocks<N>[0]\n",
    ] {
        assert!(compile_system_ref(&Language::parse(source).unwrap()).is_err(), "{source}");
    }
}

#[test]
fn package_qualifiers_bind_owner_and_arguments_and_hide_private_helpers() {
    use conlang_language::{
        LibraryCatalog, PackageId, PackageResolver, PackageSources, PackageSpec,
    };
    let package = |id: &str, code: &str| {
        PackageSources {
        config: format!("schema = 2\nid = \"{id}\"\nversion = \"1\"\nlayer = \"overlay\"\ncapabilities = [\"traits\", \"signs\", \"paradigms\"]\ncode = [\"main.lang\"]\n"),
        code: code.into(), ..Default::default()
    }
    };
    let catalog = LibraryCatalog::with_packages([
        package("natural:phrases", "Symbol a\nClass vowel {a}\nexport trait XP<T>:\n    marker trait Hidden:\n    export sign leaf:\n        belongs Hidden\n        syn:\n            slots:\n                head [T]\n        phon:\n            /{$slot.head}/\n"),
        package("natural:lexicon", "export marker trait N:\nexport sign word:\n    belongs N\n    phon:\n        /a/\n"),
    ]).unwrap();
    let mut spec = PackageSpec::default()
        .with_root("natural:phrases".parse::<PackageId>().unwrap())
        .with_root("natural:lexicon".parse::<PackageId>().unwrap());
    spec.aliases
        .insert("P".into(), "natural:phrases".parse().unwrap());
    spec.aliases
        .insert("L".into(), "natural:lexicon".parse().unwrap());
    let packages = catalog.resolve(&spec).unwrap();
    let source = Language::parse("sign NP:\n    belongs P.XP<L.N>\n").unwrap();
    let system = conlang_language::compile_with_packages_ref(&source, &packages).unwrap();
    let result = system
        .derive(
            "P.XP<L.N>.leaf",
            &[SlotFiller::sign("head", "L.word")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.surface, "a");
    let public = system
        .public_symbol_name(
            &result.token.construction,
            conlang_language::LibraryExportKind::Sign,
        )
        .unwrap();
    assert!(
        public.ends_with(".leaf") && !public.contains("__"),
        "{public}"
    );
    let illegal = Language::parse("sign bad:\n    belongs P.XP<L.N>.Hidden\n").unwrap();
    assert!(conlang_language::compile_with_packages_ref(&illegal, &packages).is_err());
}

#[test]
fn bound_dependencies_are_ordered_and_constructed_bounds_are_invariant() {
    compiled("marker trait N:\ntrait Child:\n    belongs N\ntrait Pair<T, U: T>:\n    pass\nsign pair:\n    belongs Pair<N, Child>\n");
    for source in [
        "marker trait U:\ntrait Bad<T: U, U>:\n    pass\n",
        "marker trait T:\ntrait Bad<T: T>:\n    pass\n",
        "marker trait N:\nmarker trait V:\nmarker trait Box<T>:\ntrait Child:\n    belongs Box<N>\ntrait Bound<T: Box<V>>:\n    pass\nsign bad:\n    belongs Bound<Child>\n",
    ] { assert!(conlang_language::check_language(&Language::parse(source).unwrap()).has_errors(), "{source}"); }
}
