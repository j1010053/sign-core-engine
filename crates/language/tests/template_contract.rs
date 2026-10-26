use conlang_language::construction::SlotMap;
use conlang_language::{compile_system_ref, CompileSystemError, Language};

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

const PRELUDE: &str = "Symbol a\nClass vowel {a}\nmarker trait N:\nsign noun:\n    belongs N\n    phon:\n        /a/\n";

#[test]
fn direct_and_template_members_derive_with_the_same_content() {
    let body = "    sign helper:\n        syn:\n            slots:\n                head [N]\n        phon:\n            /{$slot.head}/\n    syn:\n        slots:\n            item [*]\n        map item autofill helper(head: noun())\n    phon:\n        /{$slot.item}/\n";
    let direct = compile(&format!("{PRELUDE}sign Direct:\n{body}"));
    let templated = compile(&format!(
        "{PRELUDE}trait Template:\n{body}sign Receiver:\n    belongs Template\n    Template\n"
    ));
    let a = direct.derive("Direct", &[], &SlotMap::identity()).unwrap();
    let b = templated
        .derive("Receiver", &[], &SlotMap::identity())
        .unwrap();
    assert_eq!(a.surface, "a");
    assert_eq!(a.surface, b.surface);
    assert_eq!(
        direct.callable_parameters("Direct").unwrap(),
        templated.callable_parameters("Receiver").unwrap()
    );
    assert!(direct.evaluate_sign("Direct.helper").is_err());
    assert!(templated.evaluate_sign("Receiver.helper").is_err());
}

#[test]
fn receiver_can_use_private_members_and_exported_aliases() {
    let source = format!("{PRELUDE}trait XP<T>:\n    sign hidden:\n        syn:\n            slots:\n                head [T]\n        phon:\n            /{{$slot.head}}/\n    export sign leaf:\n        belongs T\n        phon:\n            /a/\nsign NP:\n    belongs XP<N>\n    syn:\n        slots:\n            value [*]\n        map value autofill hidden(head: leaf())\n    phon:\n        /{{$slot.value}}/\n");
    for mount in ["belongs XP<N>", "XP<N>", "XP<N>[0]"] {
        let source = source.replace("belongs XP<N>", mount);
        let system = compile(&source);
        assert_eq!(
            system
                .derive("NP", &[], &SlotMap::identity())
                .unwrap()
                .surface,
            "a"
        );
        let alias = system.derive("NP.leaf", &[], &SlotMap::identity()).unwrap();
        let original = system
            .derive("XP<N>.leaf", &[], &SlotMap::identity())
            .unwrap();
        assert_eq!(alias.token.construction_id, original.token.construction_id);
        assert!(system.evaluate_sign("NP.hidden").is_err());
        assert!(system.evaluate_sign("XP<N>.hidden").is_err());
    }
}

#[test]
fn all_application_forms_preserve_features_roles_optional_slots_and_rule_order() {
    use conlang_language::construction::SlotFiller;
    let prelude = "Symbol a\nSymbol b\nSymbol c\nClass vowel {a, b, c}\nmarker trait N:\nsign noun:\n    belongs N\n    phon:\n        /a/\n";
    let content = "    syn:\n        slots:\n            head [N]\n            tail [*]?\n        feature:\n            number = enum(sg, pl)\n            number = sg\n    sem:\n        roles:\n            head [N]\n            head = {$slot.head}\n    phon:\n        /{$slot.head}{$slot.tail}/\n        b => c\n        a => b\n";
    // Handwritten direct Sign is the oracle; no expansion function builds expectations.
    let direct = compile(&format!("{prelude}sign Direct:\n{content}"));
    let expected = direct
        .derive(
            "Direct",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(expected.surface, "b");
    assert!(expected
        .token
        .syn
        .contains(&("syn.number".into(), "sg".into())));
    assert_eq!(expected.token.sem.roles.len(), 1);
    for mount in ["", "    Template\n", "    Template[0]\n"] {
        let system = compile(&format!(
            "{prelude}trait Template:\n{content}sign Receiver:\n    belongs Template\n{mount}"
        ));
        let actual = system
            .derive(
                "Receiver",
                &[SlotFiller::sign("head", "noun")],
                &SlotMap::identity(),
            )
            .unwrap();
        assert_eq!(actual.surface, expected.surface, "{mount}");
        assert_eq!(actual.token.syn, expected.token.syn);
        assert_eq!(actual.token.sem.roles, expected.token.sem.roles);
        assert_eq!(actual.token.sem.features, expected.token.sem.features);
        assert_eq!(
            actual.token.residual_slots(),
            expected.token.residual_slots()
        );
        assert!(actual
            .token
            .syn_categories
            .iter()
            .any(|name| name == "Template"));
        assert!(!expected
            .token
            .syn_categories
            .iter()
            .any(|name| name == "Template"));
        assert_eq!(actual.token.fillers.len(), 1);
    }
}

fn errors(source: &str) -> Vec<String> {
    conlang_language::check_language(&Language::parse(source).unwrap())
        .diagnostics()
        .iter()
        .filter(|d| d.severity == conlang_language::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn members_conflict_without_implicit_overrides_and_cycles_are_bounded() {
    for source in [
        "sign A:\n    sign helper:\n    sign helper:\n",
        "trait A:\n    sign helper:\ntrait B:\n    sign helper:\nsign Receiver:\n    belongs A\n    belongs B\n",
        "trait A:\n    sign helper:\nsign Receiver:\n    belongs A\n    sign helper:\n",
        "trait A:\n    sign helper:\n    marker trait helper:\n",
    ] { assert!(errors(source).iter().any(|code| code == "TEMPLATE_MEMBER_CONFLICT"), "{source}"); }
    for source in [
        "trait A:\n    belongs A.missing\n",
        "trait A:\n    belongs B\ntrait B:\n    belongs A\n",
    ] {
        assert!(!errors(source).is_empty(), "{source}");
    }
}

#[test]
fn caller_arguments_stay_in_caller_scope_and_template_bindings_are_hygienic() {
    let source = "Symbol a\nSymbol b\nClass vowel {a, b}\ntrait Template:\n    sign helper:\n        phon:\n            /a/\n    syn:\n        slots:\n            item [*]\n        map item autofill helper()\n    phon:\n        /{$slot.item}/\nsign Receiver:\n    belongs Template\nsign helper:\n    phon:\n        /b/\n";
    let system = compile(source);
    assert_eq!(
        system
            .derive("Receiver", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    assert_eq!(
        system
            .derive("helper", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "b"
    );
    let bad = format!("{source}sign outside:\n    syn:\n        slots:\n            value [*]\n        map value autofill Receiver(item: Receiver.helper())\n    phon:\n        /{{$slot.value}}/\n");
    assert!(errors(&bad)
        .iter()
        .any(|code| code == "NESTED_PRIVATE_MEMBER"));
}

#[test]
fn arbitrary_sign_and_trait_scopes_capture_parameters_and_preserve_source_ids() {
    use conlang_language::construction::SlotFiller;
    let source = format!("{PRELUDE}sign Factory:\n    export trait Schema<T>:\n        export sign Builder:\n            export sign leaf:\n                belongs T\n                syn:\n                    slots:\n                        head [T]\n                phon:\n                    /{{$slot.head}}/\nsign Receiver:\n    belongs Factory.Schema<N>\n");
    let doc =
        conlang_language::LanguageDocument::import_new_root(&source, "template:nested").unwrap();
    let leaf = doc.ref_for_sign("Factory.Schema.Builder.leaf").unwrap();
    let parent = doc.ref_for_sign("Factory.Schema.Builder").unwrap();
    let entry = doc
        .identities()
        .nodes
        .iter()
        .find(|entry| entry.id == leaf.id)
        .unwrap();
    assert_eq!(entry.parent.as_ref(), Some(&parent.id));
    let (text, ids) = doc.dump_pair().unwrap();
    let reopened = conlang_language::LanguageDocument::open(&text, &ids).unwrap();
    assert_eq!(reopened, doc);
    let system = conlang_language::compile_document(&reopened, &Default::default()).unwrap();
    let result = system
        .derive(
            "Receiver.Builder.leaf",
            &[SlotFiller::sign("head", "noun")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(result.surface, "a");
    assert_eq!(
        system.source_sign_id(&result.token.construction_id).0,
        leaf.id
    );
    assert!(
        system.derive("Factory", &[], &SlotMap::identity()).is_err(),
        "member declarations do not supply the owner's phon"
    );
}

#[test]
fn package_receivers_use_private_members_but_only_exported_paths_escape() {
    use conlang_language::{LibraryCatalog, PackageResolver, PackageSources, PackageSpec};
    let package = PackageSources {
        config: "schema = 2\nid = \"natural:template\"\nversion = \"1\"\nlayer = \"overlay\"\ncapabilities = [\"traits\", \"signs\", \"paradigms\"]\ncode = [\"main.lang\"]\n".into(),
        code: "Symbol a\nClass vowel {a}\nexport trait Template:\n    sign hidden:\n        phon:\n            /a/\n    export sign public:\n        syn:\n            slots:\n                item [*]\n            map item autofill hidden()\n        phon:\n            /{$slot.item}/\nexport sign Built:\n    belongs Template\n    syn:\n        slots:\n            item [*]\n        map item autofill hidden()\n    phon:\n        /{$slot.item}/\n".into(),
        ..Default::default()
    };
    let catalog = LibraryCatalog::with_packages([package.clone()]).unwrap();
    let mut spec = PackageSpec::default().with_root(
        "natural:template"
            .parse::<conlang_language::PackageId>()
            .unwrap(),
    );
    spec.aliases
        .insert("P".into(), "natural:template".parse().unwrap());
    let packages = catalog.resolve(&spec).unwrap();
    let caller = Language::parse("sign Receiver:\n    belongs P.Template\n    syn:\n        slots:\n            item [*]\n        map item autofill hidden()\n    phon:\n        /{$slot.item}/\n").unwrap();
    let system = conlang_language::compile_with_packages_ref(&caller, &packages).unwrap();
    for name in ["Receiver", "Receiver.public", "P.Built", "P.Built.public"] {
        assert_eq!(
            system
                .derive(name, &[], &SlotMap::identity())
                .unwrap()
                .surface,
            "a",
            "{name}"
        );
    }
    for name in ["Receiver.hidden", "P.Built.hidden", "P.Template.hidden"] {
        assert!(
            system.derive(name, &[], &SlotMap::identity()).is_err(),
            "{name}"
        );
    }
    let private_template = PackageSources {
        code: package
            .code
            .replace("export trait Template", "trait Template"),
        ..package
    };
    let catalog = LibraryCatalog::with_packages([private_template]).unwrap();
    let packages = catalog.resolve(&spec).unwrap();
    let system = conlang_language::compile_with_packages_ref(&Language::new(), &packages).unwrap();
    assert_eq!(
        system
            .derive("P.Built.public", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "a"
    );
    assert!(system
        .derive("P.Template.public", &[], &SlotMap::identity())
        .is_err());
}

#[test]
fn receiver_aliases_keep_concrete_families_and_private_access_separate() {
    use conlang_language::construction::SlotFiller;
    let source = "Symbol a\nClass vowel {a}\nmarker trait N:\nmarker trait V:\ntrait XP<T>:\n    marker trait Bar:\n    export sign leaf:\n        belongs Bar\n        phon:\n            /a/\n    sign hidden:\n        phon:\n            /a/\n    syn:\n        slots:\n            head [Bar]\n    phon:\n        /{$slot.head}/\nsign NP:\n    belongs XP<N>\nsign VP:\n    belongs XP<V>\n";
    let system = compile(source);
    let noun = system.derive("NP.leaf", &[], &SlotMap::identity()).unwrap();
    let verb = system.derive("VP.leaf", &[], &SlotMap::identity()).unwrap();
    assert_ne!(noun.token.construction_id, verb.token.construction_id);
    assert_eq!(
        system.source_sign_id(&noun.token.construction_id),
        system.source_sign_id(&verb.token.construction_id)
    );
    assert_eq!(
        system
            .derive(
                "NP",
                &[SlotFiller::token("head", &noun.token)],
                &SlotMap::identity()
            )
            .unwrap()
            .surface,
        "a"
    );
    assert!(system
        .derive(
            "NP",
            &[SlotFiller::token("head", &verb.token)],
            &SlotMap::identity()
        )
        .is_err());
    let cross_family = format!("{source}sign Intruder:\n    belongs XP<N>\n    sign local:\n        origin = sign(XP<V>.hidden)\n");
    assert!(errors(&cross_family)
        .iter()
        .any(|c| c == "NESTED_PRIVATE_MEMBER"));
}

#[test]
fn realization_cases_preserve_meaning_trace_and_context_across_template_forms() {
    use conlang_language::{DerivationContext, Dim};
    let prelude = "Symbol a\nSymbol e\nClass vowel {a, e}\n";
    let content = "    syn:\n        feature:\n            number = enum(singular, plural)\n    sem:\n        feature:\n            count = enum(singular, plural)\n            count => $self.syn.number\n    phon:\n        /a/\n        realization:\n            case [syn.number]:\n                [plural]:\n                    a => e\n                base:\n";
    let direct = compile(&format!("{prelude}sign Direct:\n{content}"));
    for mount in ["", "    Template\n", "    Template[0]\n"] {
        let templated = compile(&format!(
            "{prelude}trait Template:\n{content}sign Receiver:\n    belongs Template\n{mount}"
        ));
        for (number, surface) in [("plural", "e"), ("singular", "a"), ("plural", "e")] {
            let context = DerivationContext::new().feature(Dim::Syn, "number", number);
            let expected = direct
                .derive_with_context("Direct", &[], &SlotMap::identity(), context.clone())
                .unwrap();
            let actual = templated
                .derive_with_context("Receiver", &[], &SlotMap::identity(), context)
                .unwrap();
            assert_eq!(actual.surface, surface, "{mount}");
            assert_eq!(actual.surface, expected.surface);
            assert_eq!(actual.token.sem.field("count"), Some(number));
            assert_eq!(actual.token.sem.features, expected.token.sem.features);
            assert!(!actual.cases.is_empty());
            assert_eq!(
                actual
                    .cases
                    .iter()
                    .map(|c| (&c.selection, c.branch, &c.status))
                    .collect::<Vec<_>>(),
                expected
                    .cases
                    .iter()
                    .map(|c| (&c.selection, c.branch, &c.status))
                    .collect::<Vec<_>>()
            );
            assert!(!actual
                .diagnostics
                .iter()
                .any(|d| d.severity == conlang_language::Severity::Error));
            assert!(!actual.phon_steps.iter().any(|step| step
                .diags
                .iter()
                .any(|d| d.level == tshiatun_core::diag::DiagLevel::Error)));
        }
        assert!(templated
            .derive("Receiver", &[], &SlotMap::identity())
            .is_err());
    }
}

#[test]
fn nested_declarations_do_not_change_block_indices_or_completeness() {
    let source = "Symbol a\nSymbol b\nSymbol c\nClass vowel {a, b, c}\ntrait Template:\n    sign helper:\n        phon:\n            /a/\n    phon:\n        /a/\n        b => c\n    ==\n    phon:\n        a => b\nsign Receiver:\n    belongs Template\n    Template[0]\n    Template[1]\n";
    let system = compile(source);
    assert_eq!(
        system
            .derive("Receiver", &[], &SlotMap::identity())
            .unwrap()
            .surface,
        "b"
    );
    let incomplete = Language::parse(&source.replace("    Template[1]\n", "")).unwrap();
    assert!(compile_system_ref(&incomplete).is_err());
    let language = Language::parse(source).unwrap();
    assert_eq!(language.trait_named("Template").unwrap().blocks.len(), 2);
}

#[test]
fn metadata_member_references_bind_to_source_and_accept_nested_type_arguments() {
    let source = format!("{PRELUDE}trait Family<T, U>:\n    sign helper:\n        belongs T\n        phon:\n            /a/\n    export sign derived:\n        origin = sign(helper)\n        components = sign(helper), sign(noun)\n        phon:\n            /a/\nsign Receiver:\n    belongs Family<N, N>\n");
    let system = compile(&source);
    let derived = system
        .derive("Receiver.derived", &[], &SlotMap::identity())
        .unwrap();
    assert_eq!(derived.surface, "a");
    let effective = system
        .effective_language()
        .signs
        .iter()
        .find(|s| s.id == derived.token.construction_id)
        .unwrap();
    let origin = effective.origin().unwrap();
    let helper = system.effective_language().sign_named(&origin.0).unwrap();
    let components = effective.components().unwrap();
    assert_eq!(
        components,
        vec![origin, conlang_language::SignRef("noun".into())]
    );
    let source = system.language().sign_named("Family.helper").unwrap();
    assert_eq!(system.source_sign_id(&helper.id), &source.id);
    let explicit = "sign Origin:\n    origin = sign(Family<N, N>.helper)\n    components = sign(Family<N, N>.helper), sign(noun)\n";
    // Parentheses delimit component separators even when a type has multiple arguments.
    let parsed = Language::parse(explicit).unwrap();
    assert_eq!(parsed.signs[0].components().unwrap().len(), 2);
}
#[test]
fn tutorial_file_keeps_receiver_aliases_and_private_members() {
    use conlang_language::construction::{SlotFiller, SlotMap};
    let source = include_str!("../../../tutorials/template-scopes/example.lang");
    let system = compile(source);
    let mapping = SlotMap::identity();
    assert_eq!(system.derive("NP", &[], &mapping).unwrap().surface, "a");
    assert_eq!(
        system
            .derive("NP.head", &[SlotFiller::sign("head", "noun")], &mapping)
            .unwrap()
            .surface,
        "a"
    );
    assert_eq!(
        system.derive("Container", &[], &mapping).unwrap().surface,
        "ba"
    );
    assert!(system.derive("NP.private-head", &[], &mapping).is_err());
    assert!(system
        .derive("NP.head", &[SlotFiller::sign("head", "verb")], &mapping)
        .is_err());
}
