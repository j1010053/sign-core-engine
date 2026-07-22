use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::system::{
    compile_system, CandidateSelector, DerivationContext, SignValue, SystemError,
};
use conlang_language::{CompileSystemError, Dim, Language, LanguageDocument, LanguageSchema};

const FP_SOURCE: &str = r#"schema conlang.lang/v2

trait TestVerb:

trait ThirdSingular:

trait FiniteVerb:

trait SibilantFinal:

sign en_3sg:
    belongs ThirdSingular
    syn:
        slots:
            stem [TestVerb]
    phon:
        /{stem}+s/
        realization:
            case stem.phon:
                == SibilantFinal:
                    /{stem}+es/
                else:
                    /{stem}+s/

sign walk:
    belongs TestVerb
    syn:
        feature:
            number = enum(singular, plural)
            person = enum(first, second, third)
    phon:
        /walk/
    case:
        $self.syn.number == singular && $self.syn.person == third:
            en_3sg({$self})
            belongs FiniteVerb

sign hiss:
    belongs TestVerb
    belongs SibilantFinal
    phon:
        /hiss/
"#;

#[test]
fn v2_round_trip_keeps_context_typed_case() {
    let parsed = Language::parse(FP_SOURCE).expect("V2 source parses");
    assert_eq!(parsed.schema(), LanguageSchema::V2);
    let canonical = parsed.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    assert!(canonical.contains("case stem.phon:"));
    assert!(canonical.contains("en_3sg({$self})"));
}

#[test]
fn v1_does_not_accept_v2_case_syntax() {
    let error = Language::parse(
        r#"sign walk:
    phon:
        /walk/
    case:
        $self == [Verb]:
            inflect({$self})
"#,
    )
    .unwrap_err();
    assert!(error.msg.contains("conlang.lang/v2"));
}

#[test]
fn sign_application_returns_a_full_typed_sign() {
    let language = Language::parse(FP_SOURCE).unwrap();
    let system = compile_system(language).unwrap();
    let context = DerivationContext::new()
        .feature(Dim::Syn, "number", "singular")
        .feature(Dim::Syn, "person", "third");
    let evaluated = system.evaluate_sign_expression("walk", &context).unwrap();
    let SignValue::Applied(token) = evaluated.value else {
        panic!("3sg case should apply the en_3sg Sign function")
    };
    assert!(token
        .syn_categories
        .iter()
        .any(|item| item == "ThirdSingular"));
    assert!(token.syn_categories.iter().any(|item| item == "FiniteVerb"));
    assert!(token.is_saturated());
    assert_eq!(
        system.realize_phon(&token).unwrap().input.as_str(),
        "walk+s"
    );
    assert_eq!(evaluated.cases.len(), 1);

    let plural = DerivationContext::new()
        .feature(Dim::Syn, "number", "plural")
        .feature(Dim::Syn, "person", "third");
    assert!(matches!(
        system
            .evaluate_sign_expression("walk", &plural)
            .unwrap()
            .value,
        SignValue::Stored(_)
    ));

    let sibilant = system
        .apply_construction(
            "en_3sg",
            &[SlotFiller::sign("stem", "hiss")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(
        system.realize_phon(&sibilant).unwrap().input.as_str(),
        "hiss+es"
    );
}

#[test]
fn binary_constraints_execute_at_application() {
    let language = Language::parse(
        r#"schema conlang.lang/v2

trait TestNominal:
    syn:
        feature:
            number = enum(singular, plural)

trait TestClause:

sign one:
    belongs TestNominal
    syn:
        feature:
            number = singular
    phon:
        /one/

sign many:
    belongs TestNominal
    syn:
        feature:
            number = plural
    phon:
        /many/

sign Agreement:
    belongs TestClause
    syn:
        slots:
            subject [TestNominal]
            predicate [TestNominal]
    phon:
        /{subject} {predicate}/
    constraints:
        equal(subject.syn.number, predicate.syn.number)
        before(subject, predicate)
        adjacent(subject, predicate)
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let error = system
        .apply_construction(
            "Agreement",
            &[
                SlotFiller::sign("subject", "one"),
                SlotFiller::sign("predicate", "many"),
            ],
            &SlotMap::identity(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("equal constraint conflicts"));
}

#[test]
fn partial_sign_can_be_resumed_without_mutating_the_original() {
    let language = Language::parse(
        r#"schema conlang.lang/v2

trait Piece:

trait Pair:

sign left:
    belongs Piece
    phon:
        /L/

sign right:
    belongs Piece
    phon:
        /R/

sign Pairing:
    belongs Pair
    syn:
        slots:
            first [Piece]
            second [Piece]
    phon:
        /{first} {second}/

sign seed:
    belongs Piece
    phon:
        /S/
    case:
        $self == [Piece]:
            Pairing(first = {$self})
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let value = system
        .evaluate_sign_expression("seed", &DerivationContext::new())
        .unwrap()
        .value;
    let partial = value.partial().expect("one required parameter remains");
    assert_eq!(partial.parameters()[0].name, "second");
    let resumed = system
        .resume_partial(&partial, &[SlotFiller::sign("second", "right")])
        .unwrap();
    let SignValue::Applied(token) = resumed else {
        panic!("resume returns the completed Sign")
    };
    assert!(token.is_saturated());
    assert_eq!(token.phon_form().unwrap(), "S R");
    assert_eq!(partial.parameters()[0].name, "second");
}

#[test]
fn competition_returns_all_candidates_and_samples_deterministically() {
    let language = Language::parse(
        r#"schema conlang.lang/v2

trait Atom:

trait CompetingConstruction:

sign x:
    belongs Atom
    phon:
        /x/

sign A:
    belongs CompetingConstruction
    entrenchment = 0.25
    syn:
        slots:
            value [Atom]
    phon:
        /a {value}/

sign B:
    belongs CompetingConstruction
    entrenchment = 0.75
    syn:
        slots:
            value [Atom]
    phon:
        /b {value}/
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let fillers = [SlotFiller::sign("value", "x")];
    let candidates = system
        .derive_candidates(
            "CompetingConstruction",
            &fillers,
            &SlotMap::identity(),
            &DerivationContext::new(),
        )
        .unwrap();
    assert_eq!(
        candidates
            .candidates
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "B"]
    );
    assert!(matches!(
        system.derive_category(
            "CompetingConstruction",
            &fillers,
            &SlotMap::identity(),
            DerivationContext::new()
        ),
        Err(SystemError::AmbiguousConstruction { .. })
    ));
    let first = system
        .select_candidate(
            &candidates,
            CandidateSelector::SampleEntrenchment { seed: 42 },
            None,
        )
        .unwrap();
    let second = system
        .select_candidate(
            &candidates,
            CandidateSelector::SampleEntrenchment { seed: 42 },
            None,
        )
        .unwrap();
    assert_eq!(first, second);
}

#[test]
fn explicit_document_migration_preserves_existing_node_ids() {
    let source = r#"trait Root:

sign item:
    belongs Root
    phon:
        /x/
"#;
    let document = LanguageDocument::import_new_root(source, "project:root").unwrap();
    let before = document
        .identities()
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.kind, node.address.clone()))
        .collect::<Vec<_>>();
    let migrated = document.migrate_to_v2().unwrap();
    let after = migrated
        .identities()
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.kind, node.address.clone()))
        .collect::<Vec<_>>();
    assert_eq!(before, after);
    assert!(migrated.source().starts_with("schema conlang.lang/v2\n"));
    let (source, sidecar) = migrated.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &sidecar).unwrap(), migrated);
}

#[test]
fn phon_projection_evaluates_the_full_sign_before_extracting_phon() {
    let language = Language::parse(
        r#"schema conlang.lang/v2

trait ProjectionAtom:

trait OuterCategory:

sign x:
    belongs ProjectionAtom
    phon:
        /x/

sign Suffix:
    syn:
        slots:
            base [OuterCategory]
    phon:
        /{base}!/

sign Outer:
    belongs OuterCategory
    syn:
        slots:
            stem [ProjectionAtom]
    phon:
        /{stem}/
        realization:
            case:
                $self == [OuterCategory]:
                    /{Suffix({$self}).phon.ret}/
"#,
    )
    .unwrap();
    let canonical = language.dump();
    assert!(canonical.contains("/{Suffix({$self}).phon.ret}/"));
    let system = compile_system(language).unwrap();
    let token = system
        .apply_construction(
            "Outer",
            &[SlotFiller::sign("stem", "x")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(system.realize_phon(&token).unwrap().input.as_str(), "x!");
}

#[test]
fn nested_applications_participate_in_static_resolution_and_cycle_checks() {
    let unknown = Language::parse(
        r#"schema conlang.lang/v2

trait Atom:

sign Wrapper:
    syn:
        slots:
            value [*]
    phon:
        /{value}/

sign root:
    belongs Atom
    phon:
        /r/
    case:
        else:
            Wrapper(value = Missing({$self}))
"#,
    )
    .unwrap();
    let CompileSystemError::Validation(report) = compile_system(unknown).unwrap_err() else {
        panic!("unknown nested application must be a validation error")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "APPLICATION_UNKNOWN_SIGN"));

    let cyclic = Language::parse(
        r#"schema conlang.lang/v2

sign A:
    syn:
        slots:
            value [*]
    phon:
        /{value}/
    case:
        else:
            B({$self})

sign B:
    syn:
        slots:
            value [*]
    phon:
        /{value}/
    case:
        else:
            A({$self})
"#,
    )
    .unwrap();
    let CompileSystemError::Validation(report) = compile_system(cyclic).unwrap_err() else {
        panic!("application cycle must be a validation error")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "APPLICATION_CYCLE"));
}
