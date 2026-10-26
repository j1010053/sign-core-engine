use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{compile_system_ref, CompileSystemError, Language, SemanticDocumentV1};

fn compile(source: &str) -> conlang_language::CompiledSystem {
    compile_system_ref(&Language::parse(source).unwrap()).unwrap_or_else(|error| match error {
        CompileSystemError::Validation(report) => panic!(
            "{:?}",
            report
                .diagnostics()
                .iter()
                .filter(|d| d.severity == conlang_language::Severity::Error)
                .collect::<Vec<_>>()
        ),
        error => panic!("{error:?}"),
    })
}

const BASE: &str = r#"Symbol a
Symbol u
Class vowel {a, u}
marker trait Verbal:
sign word:
    phon:
        /a/
sign other-word:
    phon:
        /u/
sign VP:
    belongs Verbal
    syn:
        slots:
            head [*]
    phon:
        /{$slot.head}/
sign Twin:
    belongs Verbal
    syn:
        slots:
            head [*]
    phon:
        /{$slot.head}/
sign Accept:
    syn:
        slots:
            comp [VP]
    sem:
        feature:
            kind = enum(vp, other)
            kind =
                case:
                    $slot.comp == [VP]:
                        vp
                    base:
                        other
        roles:
            comp [VP]
            comp = {$slot.comp}
    phon:
        /{$slot.comp}/
sign demo:
    syn:
        slots:
            phrase [Accept]
        map phrase autofill Accept(comp: VP(head: word()))
    phon:
        /{$slot.phrase}/
"#;

#[test]
fn derived_sign_instances_fill_slots_roles_and_guards_by_identity() {
    let system = compile(BASE);
    assert_eq!(
        system
            .derive("demo", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    let vp_id = system.evaluate_sign("VP").unwrap().sign.id;
    for word in ["word", "other-word"] {
        let vp = system
            .derive(
                "VP",
                &[SlotFiller::sign("head", word)],
                &SlotMap::identity(),
            )
            .unwrap();
        assert_eq!(vp.token.construction_id, vp_id);
        assert_eq!(vp.token.sem.construction_id.as_ref(), Some(&vp_id));
        let accepted = system
            .derive(
                "Accept",
                &[SlotFiller::token("comp", &vp.token)],
                &SlotMap::identity(),
            )
            .unwrap();
        assert_eq!(accepted.token.sem.features["kind"], "vp");
        assert_eq!(
            accepted
                .token
                .sem
                .role("comp")
                .unwrap()
                .construction_id
                .as_ref(),
            Some(&vp_id)
        );
        assert_eq!(
            accepted.token.fillers[0].sem.construction_id.as_ref(),
            Some(&vp_id)
        );
    }
}

#[test]
fn identical_surface_and_trait_membership_do_not_forge_instance_type() {
    let system = compile(BASE);
    let twin = system
        .derive(
            "Twin",
            &[SlotFiller::sign("head", "word")],
            &SlotMap::identity(),
        )
        .unwrap();
    let vp = system
        .derive(
            "VP",
            &[SlotFiller::sign("head", "word")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(twin.surface, vp.surface);
    assert_eq!(twin.token.syn_categories, vp.token.syn_categories);
    assert_ne!(twin.token.construction_id, vp.token.construction_id);
    assert!(system
        .derive(
            "Accept",
            &[SlotFiller::token("comp", &twin.token)],
            &SlotMap::identity()
        )
        .is_err());
    let outer = system
        .derive(
            "Accept",
            &[SlotFiller::token("comp", &vp.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    assert!(system
        .derive(
            "Accept",
            &[SlotFiller::token("comp", &outer.token)],
            &SlotMap::identity()
        )
        .is_err());
}

#[test]
fn sign_type_arguments_capture_through_nested_members_and_satisfy_trait_bounds() {
    let source = format!("{BASE}\ntrait Template<C: Verbal>:\n    export sign inner:\n        syn:\n            slots:\n                comp [C]\n        sem:\n            roles:\n                comp [C]\n                comp = {{$slot.comp}}\n        phon:\n            /{{$slot.comp}}/\nsign Owner:\n    belongs Template<VP>\n");
    let system = compile(&source);
    let vp = system
        .derive(
            "VP",
            &[SlotFiller::sign("head", "word")],
            &SlotMap::identity(),
        )
        .unwrap();
    let direct = system
        .derive(
            "Template<VP>.inner",
            &[SlotFiller::token("comp", &vp.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    let alias = system
        .derive(
            "Owner.inner",
            &[SlotFiller::token("comp", &vp.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(direct.token.construction_id, alias.token.construction_id);
    assert_eq!(
        alias.token.sem.role("comp").unwrap().construction_id,
        Some(vp.token.construction_id)
    );
    let invalid = source.replace("belongs Template<VP>", "belongs Template<word>");
    assert!(compile_system_ref(&Language::parse(&invalid).unwrap()).is_err());
}

#[test]
fn semantic_interchange_retains_nominal_identity() {
    let system = compile(BASE);
    let vp = system
        .derive(
            "VP",
            &[SlotFiller::sign("head", "word")],
            &SlotMap::identity(),
        )
        .unwrap();
    let document = SemanticDocumentV1::from_sem_node(&vp.token.sem);
    let json = document.to_json().unwrap();
    let decoded = SemanticDocumentV1::from_json(&json).unwrap();
    assert_eq!(
        decoded.root.construction_id.as_ref(),
        Some(&vp.token.construction_id)
    );
    assert_eq!(
        system.validate_semantic_document(&decoded).unwrap(),
        vp.token.sem
    );
    let accepted = system
        .derive(
            "Accept",
            &[SlotFiller::token("comp", &vp.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    let mut document = SemanticDocumentV1::from_sem_node(&accepted.token.sem);
    assert_eq!(
        system.validate_semantic_document(&document).unwrap(),
        accepted.token.sem
    );
    document.root.roles.get_mut("comp").unwrap().construction_id = None;
    assert!(system.validate_semantic_document(&document).is_err());
    document.root.construction_id = Some(conlang_language::SignId::document("absent", 999));
    assert!(system.validate_semantic_document(&document).is_err());
}

#[test]
fn three_parameter_xp_tutorial_derives_applied_vp_and_nullary_dp() {
    let source = include_str!("../../../tutorials/instance-types/phrases.lang");
    let system = compile(source);
    assert_eq!(
        system
            .derive("demo", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a u b"
    );
    for invalid in [
        source.replace("spec: DP()", "spec: verb()"),
        source.replace("comp: VP(head: verb())", "comp: DP()"),
    ] {
        let result = compile_system_ref(&Language::parse(&invalid).unwrap());
        assert!(
            result.is_err()
                || result
                    .unwrap()
                    .derive("demo", &[], &SlotMap::identity())
                    .is_err()
        );
    }
}

#[test]
fn specialization_identity_and_receiver_alias_arguments_are_canonical() {
    let source = format!("{BASE}\nmarker trait N:\nmarker trait V:\ntrait Family<T>:\n    export sign leaf:\n        syn:\n            slots:\n                head [*]\n        phon:\n            /{{$slot.head}}/\nsign NFamily:\n    belongs Family<N>\nsign VFamily:\n    belongs Family<V>\ntrait Box<C>:\n    export sign wrap:\n        syn:\n            slots:\n                item [C]\n        phon:\n            /{{$slot.item}}/\nsign AliasBox:\n    belongs Box<NFamily.leaf>\nsign FullBox:\n    belongs Box<Family<N>.leaf>\n");
    let system = compile(&source);
    let n = system
        .derive(
            "NFamily.leaf",
            &[SlotFiller::sign("head", "word")],
            &SlotMap::identity(),
        )
        .unwrap();
    let v = system
        .derive(
            "VFamily.leaf",
            &[SlotFiller::sign("head", "word")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(
        system.source_sign_id(&n.token.construction_id),
        system.source_sign_id(&v.token.construction_id)
    );
    assert_ne!(n.token.construction_id, v.token.construction_id);
    let a = system
        .derive(
            "AliasBox.wrap",
            &[SlotFiller::token("item", &n.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    let b = system
        .derive(
            "FullBox.wrap",
            &[SlotFiller::token("item", &n.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(a.token.construction_id, b.token.construction_id);
    assert!(system
        .derive(
            "AliasBox.wrap",
            &[SlotFiller::token("item", &v.token)],
            &SlotMap::identity()
        )
        .is_err());
}

#[test]
fn instance_types_do_not_enable_template_mounts_and_ambiguous_types_fail() {
    for extra in [
        "sign Bad:\n    belongs VP\n",
        "sign Bad:\n    VP[0]\n",
        "trait Mount<C>:\n    belongs C\nsign Bad:\n    belongs Mount<VP>\n",
        "marker trait VP:\n",
        "paradigm P:\n    case [syn.mode]:\n        base: word()\nsign Bad:\n    syn:\n        slots:\n            item [P]\n    phon:\n        /{$slot.item}/\n",
    ] {
        let language = Language::parse(&format!("{BASE}\n{extra}")).unwrap();
        assert!(compile_system_ref(&language).is_err(), "{extra}");
    }
}

#[test]
fn partial_applications_retain_type_and_forward_free_variables_until_derivation() {
    use conlang_language::{SignApplication, SignValue};
    let system = compile(&format!("{BASE}\nsign Through:\n    syn:\n        slots:\n            item [Accept]\n    phon:\n        /{{$slot.item}}/\n"));
    let current = SignValue::Stored(system.evaluate_sign("word").unwrap());
    let partial = system
        .evaluate_application(
            &current,
            &SignApplication {
                callee: "VP".into(),
                arguments: vec![],
                source: conlang_language::SourceLocation::unknown(),
            },
        )
        .unwrap()
        .value;
    assert!(!partial.is_saturated());
    assert_eq!(
        partial.sign_id(),
        &system.evaluate_sign("VP").unwrap().sign.id
    );
    let outer = system
        .apply_construction(
            "Accept",
            &[SlotFiller::token("comp", partial.token().unwrap())],
            &SlotMap::identity(),
        )
        .unwrap();
    assert!(!outer.is_saturated());
    assert_eq!(
        outer.fillers[0].sem.construction_id.as_ref(),
        Some(partial.sign_id())
    );
    let completed = system
        .apply_arguments(
            &SignValue::Applied(outer),
            &[SlotFiller::sign("head", "word")],
        )
        .unwrap();
    assert!(completed.is_saturated());
    // Continue through a full public derivation; this also exercises passing
    // the completed value as a nested filler after incremental application.
    assert_eq!(
        system
            .derive(
                "Through",
                &[SlotFiller::token("item", completed.token().unwrap())],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
}

#[test]
fn private_sign_types_remain_lexical_even_inside_materialized_type_arguments() {
    let source = format!("{BASE}\ntrait Box<C>:\n    export sign wrap:\n        syn:\n            slots:\n                item [C]\n        phon:\n            /{{$slot.item}}/\nsign Owner:\n    sign Hidden:\n        phon:\n            /a/\n    export sign Public:\n        belongs Box<Hidden>\n    syn:\n        slots:\n            item [Hidden]\n        map item autofill Hidden()\n    phon:\n        /{{$slot.item}}/\n");
    let system = compile(&source);
    assert_eq!(
        system
            .derive("Owner", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    assert!(system.evaluate_sign("Owner.Hidden").is_err());
    assert!(system.evaluate_sign("Box<Owner.Hidden>.wrap").is_err());
    for reference in ["Owner.Hidden", "Box<Owner.Hidden>.wrap"] {
        let invalid = format!("{source}\nsign Outside:\n    syn:\n        slots:\n            item [{reference}]\n    phon:\n        /{{$slot.item}}/\n");
        assert!(compile_system_ref(&Language::parse(&invalid).unwrap()).is_err());
    }
}

#[test]
fn package_qualified_sign_types_preserve_visibility_and_distinct_identity() {
    use conlang_language::{
        compile_with_packages_ref, LibraryCatalog, PackageResolver, PackageSources, PackageSpec,
    };
    let package = |id: &str, code: &str| {
        PackageSources {
        config: format!("schema = 2\nid = \"{id}\"\nversion = \"1\"\nlayer = \"overlay\"\ncapabilities = [\"traits\", \"signs\"]\ncode = [\"code/main.lang\"]\n"),
        code: code.into(), ..Default::default()
    }
    };
    let catalog = LibraryCatalog::with_packages([
        package("test:english", "Symbol a\nClass vowel {a}\nsign Hidden:\n    phon:\n        /a/\nexport sign VP:\n    syn:\n        slots:\n            item [Hidden]\n        map item autofill Hidden()\n    phon:\n        /{$slot.item}/\n"),
        package("test:french", "export sign VP:\n    phon:\n        /a/\n"),
        package("test:types", "export marker trait CategoryOrSign:\n"),
        package("test:values", "export sign CategoryOrSign:\n    phon:\n        /a/\n"),
    ]).unwrap();
    let mut spec = PackageSpec::default()
        .with_root(
            "test:english"
                .parse::<conlang_language::PackageId>()
                .unwrap(),
        )
        .with_root(
            "test:french"
                .parse::<conlang_language::PackageId>()
                .unwrap(),
        )
        .with_root("test:types".parse::<conlang_language::PackageId>().unwrap())
        .with_root(
            "test:values"
                .parse::<conlang_language::PackageId>()
                .unwrap(),
        );
    spec.aliases
        .insert("English".into(), "test:english".parse().unwrap());
    spec.aliases
        .insert("French".into(), "test:french".parse().unwrap());
    spec.aliases
        .insert("Types".into(), "test:types".parse().unwrap());
    spec.aliases
        .insert("Values".into(), "test:values".parse().unwrap());
    let packages = catalog.resolve(&spec).unwrap();
    let source = "trait Box<C>:\n    syn:\n        slots:\n            item [C]\n    phon:\n        /{$slot.item}/\nsign Accept:\n    belongs Box<English.VP>\n    Box[0]\n";
    let system = compile_with_packages_ref(&Language::parse(source).unwrap(), &packages).unwrap();
    let english = system
        .derive("English.VP", &[], &SlotMap::identity())
        .unwrap();
    let french = system
        .derive("French.VP", &[], &SlotMap::identity())
        .unwrap();
    assert_eq!(english.surface, french.surface);
    assert_ne!(english.token.construction_id, french.token.construction_id);
    assert_eq!(
        system
            .derive(
                "Accept",
                &[SlotFiller::token("item", &english.token)],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
    assert!(system
        .derive(
            "Accept",
            &[SlotFiller::token("item", &french.token)],
            &SlotMap::identity()
        )
        .is_err());
    for reference in ["VP", "English.Hidden"] {
        assert!(compile_with_packages_ref(
            &Language::parse(&source.replace("English.VP", reference)).unwrap(),
            &packages
        )
        .is_err());
    }
    // A project-local type keeps the established precedence over imported names.
    let local = format!(
        "{}\nsign VP:\n    phon:\n        /a/\n",
        source.replace("English.VP", "VP")
    );
    let system = compile_with_packages_ref(&Language::parse(&local).unwrap(), &packages).unwrap();
    assert_eq!(
        system
            .derive(
                "Accept",
                &[SlotFiller::sign("item", "VP")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
    assert!(system
        .derive(
            "Accept",
            &[SlotFiller::sign("item", "English.VP")],
            &SlotMap::identity()
        )
        .is_err());
    let source = "sign TraitHost:\n    syn:\n        slots:\n            item [Types.CategoryOrSign]\n    phon:\n        /{$slot.item}/\nsign SignHost:\n    syn:\n        slots:\n            item [Values.CategoryOrSign]\n    phon:\n        /{$slot.item}/\n";
    let system = compile_with_packages_ref(&Language::parse(source).unwrap(), &packages).unwrap();
    assert_eq!(
        system
            .derive(
                "SignHost",
                &[SlotFiller::sign("item", "Values.CategoryOrSign")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
    assert!(system
        .derive(
            "TraitHost",
            &[SlotFiller::sign("item", "Values.CategoryOrSign")],
            &SlotMap::identity()
        )
        .is_err());
    assert!(compile_with_packages_ref(
        &Language::parse(&source.replace("Values.CategoryOrSign", "CategoryOrSign")).unwrap(),
        &packages
    )
    .is_err());
}

#[test]
fn paradigm_results_and_role_expression_and_realization_guards_share_type_matching() {
    let source = format!("{BASE}\nparadigm Choose:\n    case [syn.mode]:\n        base: VP(head: word())\nsign Host:\n    syn:\n        slots:\n            item [VP]\n        map item autofill Choose()\n    sem:\n        roles:\n            item [VP]\n            item =\n                case:\n                    $slot.item == [VP]:\n                        {{$slot.item}}\n                    base:\n                        {{$slot.item}}\n    phon:\n        /{{$slot.item}}/\n        realization:\n            case:\n                $self == [Host] && $slot.item == [VP]:\n                    /u/\n                base:\n")
        .replace("    belongs Verbal\n    syn:", "    belongs Verbal\n    syn:\n        feature:\n            mode = enum(active, passive)\n            mode = active");
    let system = compile(&source);
    let result = system.derive("Host", &[], &SlotMap::identity()).unwrap();
    assert_eq!(result.surface, "u");
    assert_eq!(
        result.token.sem.role("item").unwrap().construction_id,
        Some(system.evaluate_sign("VP").unwrap().sign.id)
    );
    assert_eq!(result.cases.last().unwrap().branch, 0);
    let invalid = source.replace("base: VP(head: word())", "base: Twin(head: word())");
    assert!(compile_system_ref(&Language::parse(&invalid).unwrap()).is_err());
}

#[test]
fn token_rules_and_candidate_search_use_concrete_producer_identity() {
    let source = format!("{BASE}\nsign RuleHost:\n    syn:\n        slots:\n            item [*]\n        feature:\n            selected = enum(yes, no)\n            selected = no\n            selected => yes / $slot.item == [VP]\n    phon:\n        /{{$slot.item}}/\n");
    let system = compile(&source);
    for (name, expected) in [("VP", "yes"), ("Twin", "no")] {
        let value = system
            .derive(
                name,
                &[SlotFiller::sign("head", "word")],
                &SlotMap::identity(),
            )
            .unwrap();
        let result = system
            .derive(
                "RuleHost",
                &[SlotFiller::token("item", &value.token)],
                &SlotMap::identity(),
            )
            .unwrap();
        assert!(result
            .token
            .syn
            .iter()
            .any(|(key, value)| key == "syn.selected" && value == expected));
    }
    let candidates = system
        .derive_candidates(
            "VP",
            &[SlotFiller::sign("head", "word")],
            &SlotMap::identity(),
            &conlang_language::DerivationContext::new(),
        )
        .unwrap();
    assert_eq!(candidates.candidates.len(), 1);
    assert_eq!(
        candidates.candidates[0].id,
        system.evaluate_sign("VP").unwrap().sign.id
    );
}

#[test]
fn nominal_slot_narrowing_keeps_the_sign_feature_schema() {
    let source = format!("{BASE}\ntrait Broad:\n    syn:\n        slots:\n            comp [Verbal]\n    phon:\n        /{{$slot.comp}}/\nsign Narrow:\n    belongs Broad\n    syn:\n        slots:\n            comp [VP]\n        slot_features:\n            comp.mode = active\n")
        .replace("    belongs Verbal\n    syn:", "    belongs Verbal\n    syn:\n        feature:\n            mode = enum(active, passive)\n            mode = active");
    let system = compile(&source);
    let vp = system
        .derive(
            "VP",
            &[SlotFiller::sign("head", "word")],
            &SlotMap::identity(),
        )
        .unwrap();
    let result = system
        .derive(
            "Narrow",
            &[SlotFiller::token("comp", &vp.token)],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.surface, "a");
    assert_eq!(
        result.token.fillers[0].sem.construction_id.as_ref(),
        Some(&vp.token.construction_id)
    );
    assert!(result.token.fillers[0]
        .syn
        .iter()
        .any(|(key, value)| key == "syn.mode" && value == "active"));
    for invalid in [
        source.replace("comp.mode = active", "comp.mode = invalid"),
        source.replace(
            "comp [VP]\n        slot_features:",
            "comp [word]\n        slot_features:",
        ),
    ] {
        assert!(compile_system_ref(&Language::parse(&invalid).unwrap()).is_err());
    }
}
