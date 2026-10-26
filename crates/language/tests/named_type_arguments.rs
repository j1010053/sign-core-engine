use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{compile_system_ref, CompileSystemError, Language, LanguageDocument};

const PHRASES: &str = include_str!("../../../tutorials/named-type-arguments/phrases.lang");

fn compile(source: &str) -> conlang_language::CompiledSystem {
    compile_system_ref(&Language::parse(source).unwrap())
        .unwrap_or_else(|error| panic!("{}", diagnostics(error)))
}

fn diagnostics(error: CompileSystemError) -> String {
    match error {
        CompileSystemError::Validation(report) => format!(
            "{:?}",
            report
                .diagnostics()
                .iter()
                .filter(|d| d.severity == conlang_language::Severity::Error)
                .collect::<Vec<_>>()
        ),
        other => format!("{other:?}"),
    }
}

#[test]
fn named_source_roundtrips_without_turning_labels_into_type_references() {
    let document = LanguageDocument::import_new_root(PHRASES, "named:source").unwrap();
    let (source, ids) = document.dump_pair().unwrap();
    assert!(source.contains("XP<TenseHead, Comp=VP, Spec=DP>"));
    assert!(source.contains("XP<Comp=VP, Spec=DP, Head=TenseHead>.bar-comp"));
    assert_eq!(LanguageDocument::open(&source, &ids).unwrap(), document);
    assert_eq!(Language::parse(&source).unwrap().dump(), source);
    let system = compile_system_ref(document.language()).unwrap();
    assert_eq!(
        system
            .derive("demo", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a u b"
    );
}

#[test]
fn reordered_named_and_mixed_runtime_references_share_nested_instance_identity() {
    let system = compile(PHRASES);
    let vp = system
        .derive(
            "VP",
            &[SlotFiller::sign("head", "verb")],
            &SlotMap::identity(),
        )
        .unwrap();
    let inputs = [
        SlotFiller::sign("head", "tense"),
        SlotFiller::token("comp", &vp.token),
    ];
    let expected = system
        .derive("TP.bar-comp", &inputs, &SlotMap::identity())
        .unwrap();
    for name in [
        "XP<TenseHead, DP, VP>.bar-comp",
        "XP<Head=TenseHead, Spec=DP, Comp=VP>.bar-comp",
        "XP<Spec=DP, Comp=VP, Head=TenseHead>.bar-comp",
        "XP<TenseHead, Spec=DP, Comp=VP>.bar-comp",
    ] {
        let actual = system.derive(name, &inputs, &SlotMap::identity()).unwrap();
        assert_eq!(actual.surface, "u b");
        assert_eq!(actual.token.construction_id, expected.token.construction_id);
        assert_eq!(actual.token.sem, expected.token.sem);
        assert_eq!(
            actual.token.sem.role("comp").unwrap().construction_id,
            Some(vp.token.construction_id.clone())
        );
        let phrase = system
            .derive(
                "TP",
                &[SlotFiller::token("head", &actual.token)],
                &SlotMap::identity(),
            )
            .unwrap();
        assert_eq!(phrase.surface, "u b");
    }
    assert!(system
        .derive("XP<Comp=VP>.bar-comp", &inputs, &SlotMap::identity())
        .is_err());
    assert!(system
        .derive(
            "XP<TenseHead, Head=TenseHead, Spec=DP, Comp=VP>.bar-comp",
            &inputs,
            &SlotMap::identity()
        )
        .is_err());
}

#[test]
fn constraints_guards_roles_and_positional_value_calls_accept_named_types() {
    let source = format!("{PHRASES}\nsign Check:\n    syn:\n        slots:\n            value [XP<Spec=DP, Head=TenseHead, Comp=VP>.bar-comp]\n        map value autofill XP<Comp=VP, Head=TenseHead, Spec=DP>.bar-comp(head: tense(), comp: VP(verb()))\n        feature:\n            selected = enum(yes, no)\n            selected = no\n            selected => yes / $slot.value == [XP<Comp=VP, Spec=DP, Head=TenseHead>.bar-comp]\n    sem:\n        roles:\n            value [XP<TenseHead, Comp=VP, Spec=DP>.bar-comp]\n            value = {{$slot.value}}\n    phon:\n        /{{$slot.value}}/\nsign Outer:\n    syn:\n        slots:\n            item [Check]\n        map item autofill Check(XP<Head=TenseHead, Comp=VP, Spec=DP>.bar-comp(head: tense(), comp: VP(verb())))\n    phon:\n        /{{$slot.item}}/\n");
    let system = compile(&source);
    let result = system.derive("Check", &[], &SlotMap::identity()).unwrap();
    assert_eq!(result.surface, "u b");
    assert!(result
        .token
        .syn
        .iter()
        .any(|(k, v)| k == "syn.selected" && v == "yes"));
    assert_eq!(
        result.token.sem.role("value").unwrap().construction_id,
        Some(system.evaluate_sign("TP.bar-comp").unwrap().sign.id)
    );
    assert_eq!(
        system
            .derive("Outer", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "u b"
    );
}

#[test]
fn invalid_bindings_are_diagnosed_at_declarations_and_calls() {
    for (args, code) in [
        (
            "Head=TenseHead, Spec=DP, Extra=VP",
            "TYPE_ARGUMENT_UNKNOWN_NAME",
        ),
        (
            "head=TenseHead, Spec=DP, Comp=VP",
            "TYPE_ARGUMENT_UNKNOWN_NAME",
        ),
        (
            "Head=TenseHead, Head=TenseHead, Comp=VP",
            "TYPE_ARGUMENT_DUPLICATE",
        ),
        (
            "TenseHead, Head=TenseHead, Comp=VP",
            "TYPE_ARGUMENT_DUPLICATE",
        ),
        ("Head=TenseHead, DP, Comp=VP", "TYPE_ARGUMENT_ORDER"),
        ("Comp=VP", "TYPE_PARAM_ARITY_MISMATCH"),
        ("TenseHead, DP, VP, DP", "TYPE_PARAM_ARITY_MISMATCH"),
    ] {
        let invalid = PHRASES.replace("XP<TenseHead, Comp=VP, Spec=DP>", &format!("XP<{args}>"));
        let error =
            diagnostics(compile_system_ref(&Language::parse(&invalid).unwrap()).unwrap_err());
        assert!(error.contains(code), "{args}: {error}");
        let invalid = PHRASES.replace(
            "XP<Comp=VP, Spec=DP, Head=TenseHead>.bar-comp",
            &format!("XP<{args}>.bar-comp"),
        );
        let error =
            diagnostics(compile_system_ref(&Language::parse(&invalid).unwrap()).unwrap_err());
        assert!(error.contains(code), "{args}: {error}");
    }
    for invalid in [
        "XP<Head=>",
        "XP<Head=VP()>",
        "XP<X.Head=VP>",
        "XP<Head: VP>",
        "XP<Head=VP,>",
    ] {
        assert!(
            Language::parse(&format!("sign S:\n    belongs {invalid}\n")).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn named_binding_preserves_bound_order_and_nested_parameter_capture() {
    let source = "Symbol a\nClass vowel {a}\nmarker trait Category:\nsign noun:\n    belongs Category\n    phon:\n        /a/\nsign other:\n    phon:\n        /a/\nmarker trait Pair<T, U: T>:\ntrait Outer<Root>:\n    export trait Inner<Value: Root>:\n        export sign leaf:\n            syn:\n                slots:\n                    head [Value]\n            phon:\n                /{$slot.head}/\nsign Owner:\n    belongs Outer<Root=Category>.Inner<Value=noun>\nsign Bound:\n    belongs Pair<U=noun, T=Category>\n";
    let system = compile(source);
    let left = system
        .derive(
            "Outer<Root=Category>.Inner<Value=noun>.leaf",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity(),
        )
        .unwrap();
    let right = system
        .derive(
            "Owner.leaf",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(left.token, right.token);
    for target in ["Pair<U=other, T=Category>", "Pair<Category, U=other>"] {
        let bad = source.replace("Pair<U=noun, T=Category>", target);
        let error = diagnostics(compile_system_ref(&Language::parse(&bad).unwrap()).unwrap_err());
        assert!(error.contains("BOUND"), "{error}");
    }
}

#[test]
fn recursive_arguments_and_receiver_aliases_canonicalize_before_family_matching() {
    let source = "Symbol a\nClass vowel {a}\nmarker trait N:\nmarker trait V:\nmarker trait Pair<A, B>:\ntrait Box<T>:\n    export sign leaf:\n        syn:\n            slots:\n                head [T]\n        phon:\n            /{$slot.head}/\nsign Bind:\n    belongs Box<T=Pair<B=V, A=N>>\nsign noun:\n    belongs Pair<N, B=V>\n    phon:\n        /a/\nsign Other:\n    belongs Box<Pair<V, N>>\n";
    let system = compile(source);
    let expected = system
        .derive(
            "Bind.leaf",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity(),
        )
        .unwrap();
    for name in [
        "Box<Pair<N, V>>.leaf",
        "Box<T=Pair<A=N, B=V>>.leaf",
        "Box<Pair<B=V, A=N>>.leaf",
    ] {
        let actual = system
            .derive(
                name,
                &[SlotFiller::sign("head", "noun")],
                &SlotMap::identity(),
            )
            .unwrap();
        assert_eq!(actual.token.construction_id, expected.token.construction_id);
    }
    assert!(system
        .derive(
            "Other.leaf",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity()
        )
        .is_err());
}

#[test]
fn equivalent_belongs_and_mount_spellings_introduce_members_once() {
    let source = "Symbol a\nClass vowel {a}\nmarker trait N:\ntrait Box<A, B>:\n    export sign leaf:\n        phon:\n            /a/\n    syn:\n        slots:\n            head [A]\n    phon:\n        /{$slot.head}/\nsign Owner:\n    belongs Box<N, N>\n    belongs Box<B=N, A=N>\n    Box[0]\nsign word:\n    belongs N\n    phon:\n        /a/\n";
    let system = compile(source);
    assert_eq!(
        system
            .derive(
                "Owner",
                &[SlotFiller::sign("head", "word")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
    assert_eq!(
        system
            .derive("Owner.leaf", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    let explicit = source.replace("    Box[0]", "    Box<A=N, B=N>[0]");
    assert_eq!(
        compile(&explicit)
            .derive("Owner.leaf", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
}

#[test]
fn labels_and_captured_parameters_do_not_resolve_as_global_types() {
    let source = "Symbol a\nClass vowel {a}\ntrait T<X>:\nmarker trait N:\ntrait Box<T>:\n    export sign leaf:\n        syn:\n            slots:\n                item [T]\n        phon:\n            /{$slot.item}/\ntrait Forward<T>:\n    belongs Box<T=T>\nsign Owner:\n    belongs Forward<T=N>\nsign word:\n    belongs N\n    phon:\n        /a/\n";
    let system = compile(source);
    assert_eq!(
        system
            .derive(
                "Owner.leaf",
                &[SlotFiller::sign("item", "word")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
}

#[test]
fn named_arguments_preserve_private_capture_without_granting_external_access() {
    let source = "Symbol a\nClass vowel {a}\ntrait Box<C>:\n    export sign wrap:\n        syn:\n            slots:\n                item [C]\n        phon:\n            /{$slot.item}/\nsign Owner:\n    sign Hidden:\n        phon:\n            /a/\n    export sign Public:\n        belongs Box<C=Hidden>\n    syn:\n        slots:\n            item [Public.wrap]\n        map item autofill Box<C=Hidden>.wrap(item: Hidden())\n    phon:\n        /{$slot.item}/\n";
    let system = compile(source);
    assert_eq!(
        system
            .derive("Owner", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    for name in [
        "Owner.Hidden",
        "Box<C=Owner.Hidden>.wrap",
        "Box<Owner.Hidden>.wrap",
    ] {
        assert!(system.evaluate_sign(name).is_err(), "{name}");
        let bad = format!("{source}\nsign Outside:\n    syn:\n        slots:\n            item [{name}]\n    phon:\n        /{{$slot.item}}/\n");
        let error = diagnostics(compile_system_ref(&Language::parse(&bad).unwrap()).unwrap_err());
        assert!(error.contains("PRIVATE"), "{error}");
    }
    let invalid = format!("{PHRASES}\nsign Outside:\n    syn:\n        slots:\n            item [XP<Spec=DP, Comp=VP, Head=TenseHead>.Bar]\n");
    assert!(
        diagnostics(compile_system_ref(&Language::parse(&invalid).unwrap()).unwrap_err())
            .contains("PRIVATE")
    );
}

#[test]
fn package_qualified_named_arguments_use_parameter_labels_and_public_type_values() {
    use conlang_language::{
        compile_with_packages_ref, LibraryCatalog, PackageResolver, PackageSources, PackageSpec,
    };
    let catalog = LibraryCatalog::with_packages([PackageSources {
        config: "schema = 2\nid = \"test:grammar\"\nversion = \"1\"\nlayer = \"overlay\"\ncapabilities = [\"traits\", \"signs\"]\ncode = [\"code/main.lang\"]\n".into(),
        code: "Symbol a\nClass vowel {a}\nexport trait Box<Value>:\n    sign hidden:\n        phon:\n            /a/\n    export sign leaf:\n        syn:\n            slots:\n                head [Value]\n        phon:\n            /{$slot.head}/\nexport sign Word:\n    phon:\n        /a/\nsign Secret:\n    phon:\n        /a/\n".into(),
        ..Default::default()
    }]).unwrap();
    let mut spec = PackageSpec::default().with_root(
        "test:grammar"
            .parse::<conlang_language::PackageId>()
            .unwrap(),
    );
    spec.aliases
        .insert("Grammar".into(), "test:grammar".parse().unwrap());
    let packages = catalog.resolve(&spec).unwrap();
    let source = "sign Owner:\n    belongs Grammar.Box<Value=Grammar.Word>\nsign demo:\n    syn:\n        slots:\n            item [Owner.leaf]\n        map item autofill Grammar.Box<Value=Grammar.Word>.leaf(head: Grammar.Word())\n    phon:\n        /{$slot.item}/\n";
    let system = compile_with_packages_ref(&Language::parse(source).unwrap(), &packages).unwrap();
    assert_eq!(
        system
            .derive("demo", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    let id = system.evaluate_sign("Owner.leaf").unwrap().sign.id;
    for name in [
        "Grammar.Box<Grammar.Word>.leaf",
        "Grammar.Box<Value=Grammar.Word>.leaf",
    ] {
        assert_eq!(system.evaluate_sign(name).unwrap().sign.id, id);
    }
    for name in [
        "Grammar.Box<Value=Grammar.Secret>.leaf",
        "Grammar.Box<Value=Grammar.Word>.hidden",
    ] {
        assert!(system.evaluate_sign(name).is_err(), "{name}");
    }
    let bad = source.replace("Value=Grammar.Word", "Value=Grammar.Secret");
    assert!(compile_with_packages_ref(&Language::parse(&bad).unwrap(), &packages).is_err());
}

// This public API intentionally stops at macro expansion; runtime assertions
// above go through compile_system_ref + derive instead.
#[test]
fn expansion_and_trait_view_share_the_named_argument_binder() {
    use conlang_language::{compile, SignItem};
    let source = "trait Pair<Left, Right>:\n    syn:\n        slots:\n            left [Left]\n            right [Right]\ntrait Forward<T>:\n    belongs Pair<Right=T, Left=N>\n    Pair[0]\nsign Owner:\n    belongs Forward<T=V>\n    Forward[0]\n";
    let language = Language::parse(source).unwrap();
    let expanded = compile::expand_traits(&language).unwrap();
    let constraints = |items: &[SignItem]| {
        items
            .iter()
            .filter_map(|item| match item {
                SignItem::Slot(slot) => slot.constraint.category().map(str::to_owned),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        constraints(&expanded.sign_named("Owner").unwrap().items),
        vec!["N", "V"]
    );
    let view = compile::trait_view(&language, &[], "Forward").unwrap();
    assert!(constraints(&view)
        .iter()
        .all(|value| value == "N" || value == "T"));
    assert!(constraints(&view).contains(&"T".into()));
    let invalid = Language::parse(&source.replace("Right=T, Left=N", "Right=T, Right=N")).unwrap();
    assert!(compile::expand_traits(&invalid)
        .unwrap_err()
        .to_string()
        .contains("TYPE_ARGUMENT_DUPLICATE"));
    assert!(compile::trait_view(&invalid, &[], "Forward")
        .unwrap_err()
        .to_string()
        .contains("TYPE_ARGUMENT_DUPLICATE"));
}

#[test]
fn nested_aliases_and_implicit_mounts_normalize_each_owners_arguments() {
    let source = "Symbol a\nClass vowel {a}\nmarker trait N:\nmarker trait V:\ntrait Outer<Left, Right>:\n    export trait Inner<Value>:\n        export sign leaf:\n            syn:\n                slots:\n                    item [Value]\n            phon:\n                /{$slot.item}/\n        syn:\n            slots:\n                item [Value]\n        phon:\n            /{$slot.item}/\nsign Receiver:\n    belongs Outer<Right=V, Left=N>\nsign InnerOwner:\n    belongs Outer<Right=V, Left=N>.Inner<Value=N>\n    Outer<Left=N, Right=V>.Inner[0]\nsign word:\n    belongs N\n    phon:\n        /a/\n";
    let system = compile(source);
    let expected = system.evaluate_sign("InnerOwner.leaf").unwrap().sign.id;
    for reference in [
        "Receiver.Inner<Value=N>.leaf",
        "Outer<Left=N, Right=V>.Inner<Value=N>.leaf",
        "Outer<N, V>.Inner<N>.leaf",
    ] {
        assert_eq!(system.evaluate_sign(reference).unwrap().sign.id, expected);
        assert_eq!(
            system
                .derive(
                    reference,
                    &[SlotFiller::sign("item", "word")],
                    &SlotMap::identity()
                )
                .unwrap()
                .surface,
            "a"
        );
    }
    assert_eq!(
        system
            .derive(
                "InnerOwner",
                &[SlotFiller::sign("item", "word")],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
}

#[test]
fn named_sign_arguments_keep_nominal_types_and_generic_families_distinct() {
    let source = format!("{PHRASES}\nsign Twin:\n    syn:\n        slots:\n            head [V]\n    phon:\n        /{{$slot.head}}/\nsign OtherTP:\n    belongs XP<Spec=DP, Comp=Twin, Head=TenseHead>\n    XP[0]\n");
    let system = compile(&source);
    let twin = system
        .derive(
            "Twin",
            &[SlotFiller::sign("head", "verb")],
            &SlotMap::identity(),
        )
        .unwrap();
    let vp = system
        .derive(
            "VP",
            &[SlotFiller::sign("head", "verb")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(twin.surface, vp.surface);
    assert_ne!(twin.token.construction_id, vp.token.construction_id);
    let other = system
        .derive(
            "XP<Head=TenseHead, Comp=Twin, Spec=DP>.bar-comp",
            &[
                SlotFiller::sign("head", "tense"),
                SlotFiller::token("comp", &twin.token),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_ne!(
        other.token.construction_id,
        system.evaluate_sign("TP.bar-comp").unwrap().sign.id
    );
    assert!(system
        .derive(
            "TP.bar-comp",
            &[
                SlotFiller::sign("head", "tense"),
                SlotFiller::token("comp", &twin.token)
            ],
            &SlotMap::identity()
        )
        .is_err());
    assert!(system
        .derive(
            "TP",
            &[SlotFiller::token("head", &other.token)],
            &SlotMap::identity()
        )
        .is_err());
    assert_eq!(
        system
            .derive(
                "OtherTP",
                &[SlotFiller::token("head", &other.token)],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "u b"
    );
}
