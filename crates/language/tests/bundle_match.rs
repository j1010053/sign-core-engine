use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::system::{compile_system, DerivationContext, SignValue};
use conlang_language::{check_language, CompileSystemError, Dim, Expression, Language, SignItem};

const CASE_SOURCE: &str = r#"sign chooser:
    syn:
        feature:
            tense = enum(present, past)
            person = enum(first, second, third)
            number = enum(singular, plural)
            form = enum(am, is, are, be)
            form = be
            form =
                case [syn.tense, syn.person, syn.number]:
                    [present, first, singular]: am
                    [present, third, singular]: is
                    [present, _, _]: are
                    base:
    phon:
        /be/
"#;

fn syn_value(system: &conlang_language::CompiledSystem, context: DerivationContext) -> String {
    system
        .evaluate_sign_with_context("chooser", &context)
        .expect("evaluate")
        .sign
        .project(Dim::Syn, &system.ontology)
        .get("syn.form")
        .expect("form")
        .to_owned()
}

#[test]
fn bundle_case_round_trips_and_uses_first_match_wildcards_and_empty_base() {
    let language = Language::parse(CASE_SOURCE).expect("parse");
    assert_eq!(language.dump(), CASE_SOURCE);
    let case = language
        .sign_named("chooser")
        .unwrap()
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::FeatureExpression(expression) => match &expression.expression {
                Expression::Case(case) => Some(case.as_ref()),
                _ => None,
            },
            _ => None,
        })
        .expect("feature case");
    assert_eq!(
        case.match_features,
        ["syn.tense", "syn.person", "syn.number"]
    );
    assert!(matches!(
        case.branches.last().map(|branch| &branch.result),
        Some(Expression::ImplicitBase)
    ));

    let system = compile_system(language).expect("compile");
    let context = |tense, person, number| {
        DerivationContext::new()
            .feature(Dim::Syn, "tense", tense)
            .feature(Dim::Syn, "person", person)
            .feature(Dim::Syn, "number", number)
    };
    assert_eq!(
        syn_value(&system, context("present", "first", "singular")),
        "am"
    );
    assert_eq!(
        syn_value(&system, context("present", "third", "singular")),
        "is"
    );
    assert_eq!(
        syn_value(&system, context("present", "second", "plural")),
        "are"
    );
    assert_eq!(
        syn_value(&system, context("past", "third", "singular")),
        "be"
    );
}

#[test]
fn case_uses_base_and_when_uses_else() {
    let case_else =
        CASE_SOURCE.replace("                    base:\n", "                    else:\n");
    let error = Language::parse(&case_else).expect_err("case else must be rejected");
    assert!(error.msg.contains("case") && error.msg.contains("base"));

    let when_base = r#"sign invalid:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = off
        when [syn.trigger]:
            [on]:
                feature:
                    trigger = off
            base:
                feature:
                    trigger = on
"#;
    let error = Language::parse(when_base).expect_err("when base must be rejected");
    assert!(error.msg.contains("when") && error.msg.contains("else"));
}

#[test]
fn bundle_header_rows_are_shape_checked_by_the_parser() {
    let wrong_arity = CASE_SOURCE.replace("[present, first, singular]: am", "[present, first]: am");
    let error = Language::parse(&wrong_arity).expect_err("wrong row arity");
    assert!(error.msg.contains("3 feature"), "{}", error.msg);

    let duplicate = CASE_SOURCE.replace(
        "case [syn.tense, syn.person, syn.number]",
        "case [syn.tense, syn.person, syn.tense]",
    );
    let error = Language::parse(&duplicate).expect_err("duplicate projection");
    assert!(error.msg.contains("more than once"), "{}", error.msg);
}

#[test]
fn bundle_fields_and_values_are_checked_against_typed_feature_domains() {
    let unknown = CASE_SOURCE.replace("syn.person", "syn.unknown");
    let error = compile_system(Language::parse(&unknown).expect("syntax"))
        .expect_err("unknown feature must fail");
    let CompileSystemError::Validation(report) = error else {
        panic!("expected validation error")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "CASE_BUNDLE_UNKNOWN_FEATURE"));

    let outside = CASE_SOURCE.replace(
        "[present, first, singular]: am",
        "[present, fourth, singular]: am",
    );
    let error = compile_system(Language::parse(&outside).expect("syntax"))
        .expect_err("out-of-domain value must fail");
    let CompileSystemError::Validation(report) = error else {
        panic!("expected validation error")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "CASE_BUNDLE_VALUE_OUT_OF_DOMAIN"));
}

#[test]
fn a_broader_earlier_bundle_row_warns_that_it_shadows_later_rows() {
    let source = CASE_SOURCE.replace(
        "                    [present, first, singular]: am\n                    [present, third, singular]: is\n                    [present, _, _]: are",
        "                    [present, _, _]: are\n                    [present, first, singular]: am\n                    [present, third, singular]: is",
    );
    let language = Language::parse(&source).expect("parse");
    let report = check_language(&language);
    assert!(!report.has_errors(), "{:?}", report.diagnostics());
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "CASE_BUNDLE_BRANCH_SHADOWED"));
}

#[test]
fn when_bundle_matching_reads_one_frozen_snapshot_and_else_is_fallback_only() {
    let source = r#"sign cumulative:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = on
            outcome = enum(base, first, leaked, fallback)
            outcome = base
        when [syn.trigger, syn.outcome]:
            [on, base]:
                feature:
                    outcome = first
            [_, first]:
                feature:
                    outcome = leaked
            else:
                feature:
                    outcome = fallback
    phon:
        /x/
"#;
    let language = Language::parse(source).expect("parse");
    assert_eq!(language.dump(), source);
    let system = compile_system(language).expect("compile");
    let evaluated = system
        .evaluate_sign_expression("cumulative", &DerivationContext::new())
        .expect("evaluate");
    let SignValue::Stored(stored) = evaluated.value else {
        panic!("when bundle should preserve the stored Sign")
    };
    assert_eq!(
        stored
            .sign
            .project(Dim::Syn, &system.ontology)
            .get("syn.outcome"),
        Some("first")
    );
}

#[test]
fn bundle_case_selects_an_applied_tokens_phon_realization_and_base_keeps_default() {
    let source = r#"trait BundleVerb:

sign go:
    belongs BundleVerb
    phon:
        /go/

sign present_third:
    syn:
        slots:
            stem [BundleVerb]
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person = third
            number = enum(singular, plural)
            number = singular
    phon:
        /{$slot.stem}/
        realization:
            case [syn.tense, syn.person, syn.number]:
                [present, third, singular]: /goes/
                base:

sign past:
    syn:
        slots:
            stem [BundleVerb]
        feature:
            tense = enum(present, past)
            tense = past
            person = enum(first, second, third)
            person = third
            number = enum(singular, plural)
            number = singular
    phon:
        /{$slot.stem}/
        realization:
            case [syn.tense, syn.person, syn.number]:
                [present, third, singular]: /goes/
                base:
"#;
    let language = Language::parse(source).expect("parse");
    let canonical = language.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    let system = compile_system(language).expect("compile");

    let realize = |construction| {
        let token = system
            .apply_construction(
                construction,
                &[SlotFiller::sign("stem", "go")],
                &SlotMap::identity(),
            )
            .and_then(|token| system.evaluate_token(token).map_err(Into::into))
            .expect("apply and evaluate");
        system
            .realize_phon(&token)
            .expect("realize")
            .input
            .into_inner()
    };

    assert_eq!(realize("present_third"), "goes");
    assert_eq!(realize("past"), "go");
}

#[test]
fn empty_base_preserves_existing_sign_and_role_context_values() {
    let source = r#"trait BundleEntity:

trait BundleMarked:

sign alpha:
    belongs BundleEntity
    phon:
        /alpha/

sign beta:
    belongs BundleEntity
    phon:
        /beta/

sign chooser_with_base:
    syn:
        slots:
            first [BundleEntity]
            second [BundleEntity]
        feature:
            trigger = enum(on, off)
            trigger = off
    sem:
        roles:
            agent [BundleEntity]
            agent = {$slot.first}
            agent =
                case [syn.trigger]:
                    [on]: {$slot.second}
                    base:
    phon:
        /{$slot.first}/
    case [syn.trigger]:
        [on]:
            belongs BundleMarked
        base:
"#;
    let system = compile_system(Language::parse(source).expect("parse")).expect("compile");
    let token = system
        .apply_construction(
            "chooser_with_base",
            &[
                SlotFiller::sign("first", "alpha"),
                SlotFiller::sign("second", "beta"),
            ],
            &SlotMap::identity(),
        )
        .and_then(|token| system.evaluate_token(token).map_err(Into::into))
        .expect("apply and evaluate");

    assert_eq!(
        token.as_token().sem.role("agent").unwrap().source.sign,
        "alpha"
    );
    assert!(!token
        .as_token()
        .syn_categories
        .iter()
        .any(|category| category == "BundleMarked"));
}
