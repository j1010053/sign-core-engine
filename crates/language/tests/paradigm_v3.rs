use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::parser::parse_sign_application_source;
use conlang_language::system::{compile_system, DerivationContext, SignValue};
use conlang_language::{
    check_language, CaseCondition, Dim, Expression, Language, LanguageDocument, NodeKind,
    RefTargetV1,
};

const SOURCE: &str = r#"paradigm BE:
    case [syn.tense, syn.person, syn.number]:
        [present, first, singular]: am()
        [present, third, singular]: is()
        [present, _, _]: are()
        base: be()

sign am:
    syn:
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person = first
            number = enum(singular, plural)
            number = singular
    phon:
        /am/

sign are:
    syn:
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person = first | second | third
            number = enum(singular, plural)
            number = singular | plural
    phon:
        /are/

sign be:
    syn:
        feature:
            tense = enum(present, past)
            tense = present | past
            person = enum(first, second, third)
            person = first | second | third
            number = enum(singular, plural)
            number = singular | plural
    phon:
        /be/

sign is:
    syn:
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person = third
            number = enum(singular, plural)
            number = singular
    phon:
        /is/
"#;

fn deferred_clause_source() -> String {
    format!(
        "{SOURCE}\nsign Clause:\n    syn:\n        slots:\n            subject [*]\n            copula [*]\n        map copula autofill BE()\n        feature:\n            tense = enum(present, past)\n            tense = present\n            person = enum(first, second, third)\n            person => $slot.subject.syn.person\n            number = enum(singular, plural)\n            number => $slot.subject.syn.number\n    phon:\n        /{{$slot.subject}} {{$slot.copula}}/\n\nsign third_subject:\n    syn:\n        feature:\n            person = enum(first, second, third)\n            person = third\n            number = enum(singular, plural)\n            number = singular\n    phon:\n        /subject/\n"
    )
}

#[test]
fn paradigm_round_trips_as_a_distinct_top_level_container() {
    let language = Language::parse(SOURCE).expect("parse paradigm");
    assert_eq!(language.dump(), SOURCE);
    let paradigm = language.paradigm_named("BE").expect("BE paradigm");
    assert_eq!(
        paradigm.selector.match_features,
        ["syn.tense", "syn.person", "syn.number"]
    );
    assert!(matches!(
        paradigm.selector.branches.last(),
        Some(branch)
            if matches!(branch.condition, CaseCondition::Base)
                && matches!(branch.result, Expression::SignApplication(_))
    ));
    let report = check_language(&language);
    assert!(!report.has_errors(), "{:?}", report.diagnostics());
    compile_system(language).expect("compile paradigm");
}

#[test]
fn paradigm_requires_a_non_empty_final_base() {
    let missing = SOURCE.replace("        base: be()\n", "");
    let error = Language::parse(&missing).expect_err("missing base");
    assert!(error.msg.contains("final") && error.msg.contains("base"));

    let empty = SOURCE.replace("        base: be()", "        base:");
    let error = Language::parse(&empty).expect_err("empty base");
    assert!(error.msg.contains("Sign application"));
}

#[test]
fn paradigm_rejects_non_sign_members_and_incompatible_member_interfaces() {
    let unknown = SOURCE.replace("base: be()", "base: missing()");
    let report = check_language(&Language::parse(&unknown).expect("syntax"));
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "PARADIGM_UNKNOWN_MEMBER_SIGN"));

    let incompatible = SOURCE.replace(
        "sign is:\n    syn:",
        "sign is:\n    syn:\n        slots:\n            subject [*]",
    );
    let report = check_language(&Language::parse(&incompatible).expect("syntax"));
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "PARADIGM_MEMBER_SIGNATURE_MISMATCH"));
}

#[test]
fn paradigm_rejects_a_row_that_cannot_select_its_member() {
    let conflicting = SOURCE.replace(
        "[present, first, singular]: am()",
        "[present, first, singular]: is()",
    );
    let report = check_language(&Language::parse(&conflicting).expect("syntax"));
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "PARADIGM_MEMBER_PATTERN_CONFLICT"));
}

#[test]
fn callable_autofill_keeps_the_complete_application_after_whitespace() {
    let source = format!(
        "{SOURCE}\nsign consumer:\n    syn:\n        slots:\n            subject [*]\n            copula [*]\n        map copula autofill BE(subject: {{$slot.subject}}, object: be())\n"
    );
    let language = Language::parse(&source).expect("callable autofill parses");
    assert!(language
        .dump()
        .contains("map copula autofill BE(subject: {$slot.subject}, object: be())"));
}

#[test]
fn paradigm_selects_by_frozen_bundle_and_returns_the_member_identity() {
    let language = Language::parse(SOURCE).expect("parse");
    let expected = language.sign_named("is").unwrap().id.clone();
    let paradigm_id = language.paradigm_named("BE").unwrap().id.clone();
    let system = compile_system(language).expect("compile");
    let evaluation = system
        .evaluate_paradigm(
            "BE",
            &DerivationContext::new()
                .feature(Dim::Syn, "tense", "present")
                .feature(Dim::Syn, "person", "third")
                .feature(Dim::Syn, "number", "singular"),
        )
        .expect("select is");

    assert_eq!(evaluation.value.sign_id(), &expected);
    let SignValue::Stored(selected) = &evaluation.value else {
        panic!("nullary member must stay a stored Sign")
    };
    assert_eq!(selected.sign.name, "is");
    let selected_trace = evaluation
        .cases
        .iter()
        .find(|record| record.status == conlang_language::system::CaseBranchStatus::Matched)
        .expect("selection trace");
    let trace = selected_trace.paradigm.as_ref().expect("paradigm trace");
    assert_eq!(trace.paradigm_id, paradigm_id);
    assert_eq!(trace.member, "is");
    assert_eq!(trace.member_sign_id, expected);

    let fallback = system
        .evaluate_paradigm(
            "BE",
            &DerivationContext::new()
                .feature(Dim::Syn, "tense", "past")
                .feature(Dim::Syn, "person", "third")
                .feature(Dim::Syn, "number", "singular"),
        )
        .expect("select base");
    let SignValue::Stored(selected) = fallback.value else {
        panic!("base member must be stored")
    };
    assert_eq!(selected.sign.name, "be");
}

#[test]
fn public_application_api_resolves_sign_and_paradigm_callables() {
    let language = Language::parse(SOURCE).expect("parse");
    let expected = language.sign_named("is").unwrap().id.clone();
    let system = compile_system(language).expect("compile");
    let current = SignValue::Stored(system.evaluate_sign("is").expect("evaluate current sign"));

    let nullary = parse_sign_application_source("is()").expect("parse Sign call");
    let identity = system
        .evaluate_application(&current, &nullary)
        .expect("evaluate Sign call");
    assert_eq!(identity.value.sign_id(), &expected);

    let paradigm = parse_sign_application_source("BE()").expect("parse Paradigm call");
    let selected = system
        .evaluate_application(&current, &paradigm)
        .expect("evaluate Paradigm call");
    assert_eq!(selected.value.sign_id(), &expected);
    assert!(selected
        .cases
        .iter()
        .any(|record| record.paradigm.is_some()));
}

#[test]
fn selected_member_can_remain_partially_applied() {
    let source = r#"Symbol l
Symbol x
Class vowel {x}

marker trait V3Entity:

paradigm FORM:
    case [syn.variant]:
        [left]: left_form()
        base: right_form()

sign atom:
    belongs V3Entity
    phon:
        /x/

sign chooser:
    syn:
        slots:
            value [V3Entity]
        feature:
            variant = enum(left, right)
            variant = left
    phon:
        /{$slot.value}/
    case [syn.variant]:
        [left]: FORM(value: {$slot.value})
        base: FORM(value: {$slot.value})

sign left_form:
    syn:
        slots:
            value [V3Entity]
        feature:
            variant = enum(left, right)
            variant = left
    phon:
        /l{$slot.value}/

sign right_form:
    syn:
        slots:
            value [V3Entity]
        feature:
            variant = enum(left, right)
            variant = right
    phon:
        /r{$slot.value}/
"#;
    let system = compile_system(Language::parse(source).expect("parse")).expect("compile");
    let selected = system
        .evaluate_paradigm(
            "FORM",
            &DerivationContext::new().feature(Dim::Syn, "variant", "left"),
        )
        .expect("select partial member")
        .value;
    assert!(selected.has_free_variables());
    assert_eq!(selected.residual_parameters()[0].name, "value");

    let saturated = system
        .apply_arguments(&selected, &[SlotFiller::sign("value", "atom")])
        .expect("saturate selected member")
        .into_evaluated()
        .expect("applied value");
    assert_eq!(
        system.realize_phon(&saturated).unwrap().input.into_inner(),
        "lx"
    );
    // Keep the ordinary construction API in scope as a regression oracle:
    // Paradigm selection does not introduce a new filler or mapping type.
    let direct = system
        .derive(
            "left_form",
            &[SlotFiller::sign("value", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(direct.surface, "lx");

    let forwarded = system
        .derive_with_context(
            "chooser",
            &[SlotFiller::sign("value", "atom")],
            &SlotMap::identity(),
            DerivationContext::new(),
        )
        .expect("consumer forwards its argument through FORM")
        .surface;
    assert_eq!(forwarded, "lx");
}

#[test]
fn autofill_paradigm_is_resolved_after_the_consumer_bundle_exists() {
    let source = deferred_clause_source();
    let language = Language::parse(&source).expect("parse deferred autofill");
    assert!(language.dump().contains("map copula autofill BE()"));
    let system = compile_system(language).expect("compile deferred autofill");
    let token = system
        .apply_construction(
            "Clause",
            &[SlotFiller::sign("subject", "third_subject")],
            &SlotMap::identity(),
        )
        .expect("construct before deferred selection");
    assert!(token
        .residual_slots()
        .iter()
        .any(|slot| slot.name == "copula"));

    let evaluated = system
        .evaluate_token(token)
        .expect("evaluate and resolve BE autofill");
    assert!(evaluated.as_token().residual_slots().is_empty());
    assert_eq!(evaluated.as_token().phon_form().unwrap(), "subject is");
}

#[test]
fn explicit_filler_overrides_deferred_autofill() {
    let system = compile_system(
        Language::parse(&deferred_clause_source()).expect("parse deferred autofill"),
    )
    .expect("compile deferred autofill");
    let token = system
        .apply_construction(
            "Clause",
            &[
                SlotFiller::sign("subject", "third_subject"),
                SlotFiller::sign("copula", "be"),
            ],
            &SlotMap::identity(),
        )
        .expect("explicit filler wins");
    assert!(token.residual_slots().is_empty());

    let evaluated = system
        .evaluate_token(token)
        .expect("evaluate explicit filler");
    assert_eq!(evaluated.as_token().phon_form().unwrap(), "subject be");
}

#[test]
fn multiple_deferred_autofills_share_one_frozen_consumer_bundle() {
    let source = deferred_clause_source()
        .replace(
            "            copula [*]\n",
            "            copula [*]\n            auxiliary [*]\n",
        )
        .replace(
            "        map copula autofill BE()\n",
            "        map copula autofill BE()\n        map auxiliary autofill BE()\n",
        )
        .replace(
            "/{$slot.subject} {$slot.copula}/",
            "/{$slot.subject} {$slot.copula} {$slot.auxiliary}/",
        );
    let system = compile_system(Language::parse(&source).expect("parse two autofills"))
        .expect("compile two autofills");
    let token = system
        .apply_construction(
            "Clause",
            &[SlotFiller::sign("subject", "third_subject")],
            &SlotMap::identity(),
        )
        .expect("construct before both selections");
    let evaluated = system
        .evaluate_token(token)
        .expect("resolve both autofills");

    assert_eq!(evaluated.as_token().phon_form().unwrap(), "subject is is");
}

#[test]
fn paradigm_and_its_horizontal_member_links_have_stable_sidecar_identity() {
    let document = LanguageDocument::import_new_root(SOURCE, "paradigm-test").unwrap();
    let paradigm_ref = document.ref_for_paradigm("BE").expect("paradigm ref");
    assert_eq!(paradigm_ref.expected, NodeKind::Paradigm);
    assert_eq!(
        document
            .language()
            .paradigm_named("BE")
            .unwrap()
            .id
            .node_id(),
        &paradigm_ref.id
    );
    let branch_count = document
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.kind == NodeKind::CaseBranch)
        .count();
    assert_eq!(branch_count, 4);
    assert!(document.identities().refs.iter().any(|binding| {
        matches!(
            &binding.target,
            RefTargetV1::Local { target }
                if target.expected == NodeKind::Sign
                    && target.id == document.ref_for_sign("is").unwrap().id
        )
    }));

    let (source, manifest) = document.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&source, &manifest).unwrap();
    assert_eq!(reopened.ref_for_paradigm("BE"), Some(paradigm_ref));
}
