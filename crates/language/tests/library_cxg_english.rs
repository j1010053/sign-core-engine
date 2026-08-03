use conlang_language::construction::{CxgError, FillerProvenance, SlotFiller, SlotMap};
use conlang_language::library::{embedded_catalog, LibraryId, LibraryKind, LibrarySpec};
use conlang_language::synchronic::RuleStatus;
use conlang_language::{compile_system, compile_with_libraries, DerivationContext, Dim, Language};

type DerivationCase<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);

fn english_spec() -> LibrarySpec {
    LibrarySpec::natural(LibraryId::new(LibraryKind::Natural, "en-standard"))
}

#[test]
fn catalog_selection_is_explicit_and_deterministic() {
    let catalog = embedded_catalog().unwrap();
    let ids = catalog
        .packages()
        .iter()
        .map(|package| package.id.to_string())
        .collect::<Vec<_>>();
    // 序 = kind → priority → name(std:grammaticalization 與 std:core 同為
    // priority 0,故排在 grambank(10)/cxg(20)之前)。
    assert_eq!(
        ids,
        [
            "std:core",
            "std:grammaticalization",
            "std:grambank",
            "std:cxg",
            "natural:en-standard"
        ]
    );

    let default = compile_system(Language::new()).unwrap();
    assert!(default
        .effective_language()
        .sign_named("EnglishDefiniteNP")
        .is_none());
    assert!(default.language().signs.is_empty());

    let first = compile_with_libraries(Language::new(), english_spec()).unwrap();
    let second = compile_with_libraries(Language::new(), english_spec()).unwrap();
    assert!(first
        .effective_language()
        .sign_named("EnglishDefiniteNP")
        .is_some());
    assert!(first.language().signs.is_empty());
    assert_eq!(first.libraries(), second.libraries());
    assert_eq!(
        first.effective_language().dump(),
        second.effective_language().dump()
    );
    assert_eq!(
        first
            .libraries()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "std:core",
            "std:grammaticalization",
            "std:grambank",
            "std:cxg",
            "natural:en-standard"
        ]
    );
}

#[test]
fn predicate_nominal_schemas_separate_required_optional_and_zero_copula() {
    let source = r#"Symbol a
Symbol b
Symbol c
Class vowel {a}

sign alice:
    belongs Noun
    sem:
        feature:
            ref = enum(ALICE, BAKER)
            ref = ALICE
    phon:
        /a/
sign baker:
    belongs Noun
    sem:
        feature:
            ref = enum(ALICE, BAKER)
            ref = BAKER
    phon:
        /b/
sign cop:
    belongs Copula
    sem:
        feature:
            relation = enum(EQUATIVE)
            relation = EQUATIVE
    phon:
        /c/

sign RequiredNominalEquation:
    belongs RequiredCopulaPredicateNominal
    phon:
        /{subject}{copula}{predicate}/
sign OptionalNominalEquation:
    belongs OptionalCopulaPredicateNominal
    phon:
        /{subject}{copula}{predicate}/
sign ZeroNominalEquation:
    belongs ZeroCopulaPredicateNominal
    phon:
        /{subject}{predicate}/
"#;
    let system = compile_system(Language::parse(source).unwrap()).unwrap();

    let required_error = system
        .derive(
            "RequiredNominalEquation",
            &[
                SlotFiller::sign("subject", "alice"),
                SlotFiller::sign("predicate", "baker"),
            ],
            &SlotMap::identity(),
        )
        .unwrap_err();
    assert!(matches!(
        required_error,
        conlang_language::SystemError::Construction(CxgError::Unsaturated(ref slots))
            if slots == &["copula"]
    ));

    let optional = system
        .derive(
            "OptionalNominalEquation",
            &[
                SlotFiller::sign("subject", "alice"),
                SlotFiller::sign("predicate", "baker"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(optional.surface, "ab");

    let required = system
        .derive(
            "RequiredNominalEquation",
            &[
                SlotFiller::sign("subject", "alice"),
                SlotFiller::sign("copula", "cop"),
                SlotFiller::sign("predicate", "baker"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(required.surface, "acb");

    let zero = system
        .derive(
            "ZeroNominalEquation",
            &[
                SlotFiller::sign("subject", "alice"),
                SlotFiller::sign("predicate", "baker"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(zero.surface, "ab");
}

#[test]
fn standard_english_profile_pins_the_25_official_rows() {
    let catalog = embedded_catalog().unwrap();
    let package = catalog
        .packages()
        .iter()
        .find(|package| package.id.to_string() == "natural:en-standard")
        .unwrap();
    let rows = package
        .data
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 25);
    assert!(rows.iter().all(|row| row.contains("\tstan1293\t")));
    assert!(rows.iter().any(|row| row.starts_with("GB082\t0\tGB082-0")));
    assert!(rows.iter().any(|row| row.starts_with("GB107\t1\tGB107-1")));
    assert!(rows.iter().any(|row| row.starts_with("GB118\t1\tGB118-1")));
}

#[test]
fn regular_english_count_inflection_is_one_deep_realization_and_seen_is_lexicalized() {
    let system = compile_with_libraries(Language::new(), english_spec()).unwrap();
    let deep = system
        .effective_language()
        .sign_named("EnglishCountNounForm")
        .expect("the one deep count-noun sign");
    for removed_surface_sign in ["dogs", "runs", "sees", "is", "does"] {
        assert!(
            system
                .effective_language()
                .sign_named(removed_surface_sign)
                .is_none(),
            "{removed_surface_sign} must be selected by realization, not stored as a sibling sign"
        );
    }
    assert_eq!(
        system
            .effective_language()
            .sign_named("seen")
            .unwrap()
            .lexicalized(),
        Some(true),
        "the irregular participle remains an explicit lexicalized workaround"
    );
    assert_eq!(
        system
            .effective_language()
            .sign_named("dog")
            .unwrap()
            .lexicalized(),
        None,
        "the base form is not mislabeled as generated morphology"
    );

    let singular = system
        .derive_with_context(
            "EnglishCountNounForm",
            &[SlotFiller::sign("stem", "dog")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "singular"),
        )
        .unwrap();
    let plural = system
        .derive_with_context(
            "EnglishCountNounForm",
            &[SlotFiller::sign("stem", "dog")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "number", "plural"),
        )
        .unwrap();
    assert_eq!(singular.token.construction_id, deep.id);
    assert_eq!(plural.token.construction_id, deep.id);
    assert_eq!(singular.realization.input.as_str(), "dog");
    assert_eq!(plural.realization.input.as_str(), "dogs");
    assert_eq!(singular.surface, "dog");
    assert_eq!(plural.surface, "dogs");
    assert_eq!(
        plural.token.sem.features.get("number"),
        Some(&"plural".to_owned())
    );
    assert_eq!(
        plural
            .token
            .sem
            .role("referent")
            .map(|node| node.source.sign.as_str()),
        Some("dog")
    );
}

#[test]
fn english_verbs_assign_case_and_nominal_occurrences_realize_it() {
    let system = compile_with_libraries(Language::new(), english_spec()).unwrap();
    let predicate = system.evaluate_sign("see").unwrap();
    let predicate_syn = predicate.sign.project(Dim::Syn, &system.ontology);
    assert_eq!(predicate_syn.get("syn.subject_case"), Some("nominative"));
    assert_eq!(predicate_syn.get("syn.object_case"), Some("accusative"));

    let source_before = system
        .effective_language()
        .sign_named("she")
        .unwrap()
        .project(Dim::Syn, &system.ontology)
        .defs;
    assert!(source_before.iter().all(|(path, _)| path != "syn.case"));

    let fillers = [
        SlotFiller::sign("subject", "she"),
        SlotFiller::sign("predicate", "see"),
        SlotFiller::sign("object", "she"),
    ];
    let clause = system
        .derive("EnglishSVOTransitiveClause", &fillers, &SlotMap::identity())
        .unwrap();
    let repeated = system
        .derive("EnglishSVOTransitiveClause", &fillers, &SlotMap::identity())
        .unwrap();
    assert_eq!(clause.surface, "she sees her");
    assert_eq!(clause.surface, repeated.surface);
    assert_eq!(clause.token, repeated.token);
    assert_eq!(clause.rules, repeated.rules);
    assert_eq!(clause.realization, repeated.realization);
    assert!(clause.diagnostics.is_empty());
    assert!(clause.rules.iter().any(|entry| {
        entry.unit == "EnglishSVOTransitiveClause"
            && entry.record.status == RuleStatus::Matched
            && entry.record.slot_reads.len() == 2
            && entry
                .record
                .slot_reads
                .iter()
                .all(|read| read.path.ends_with("case"))
    }));
    assert_eq!(
        clause
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.subject_case")
            .map(|(_, value)| value.as_str()),
        Some("nominative")
    );
    assert_eq!(
        clause
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.object_case")
            .map(|(_, value)| value.as_str()),
        Some("accusative")
    );
    let subject = clause
        .token
        .fillers
        .iter()
        .find(|snapshot| snapshot.slot == "subject")
        .unwrap();
    let object = clause
        .token
        .fillers
        .iter()
        .find(|snapshot| snapshot.slot == "object")
        .unwrap();
    assert_eq!(subject.scalar(Dim::Syn, "case"), Some("nominative"));
    assert_eq!(object.scalar(Dim::Syn, "case"), Some("accusative"));
    assert_eq!(
        subject
            .phon
            .iter()
            .find(|(path, _)| path == "phon")
            .map(|(_, value)| value.as_str()),
        Some("/she/")
    );
    assert_eq!(
        object
            .phon
            .iter()
            .find(|(path, _)| path == "phon")
            .map(|(_, value)| value.as_str()),
        Some("/her/")
    );
    assert_eq!(
        subject.provenance,
        FillerProvenance::StoredSign("she".to_owned())
    );
    assert_eq!(
        object.provenance,
        FillerProvenance::StoredSign("she".to_owned())
    );
    assert_eq!(
        system
            .effective_language()
            .sign_named("she")
            .unwrap()
            .project(Dim::Syn, &system.ontology)
            .defs,
        source_before,
        "per-occurrence case must never mutate the lexical sign"
    );

    let renamed = system
        .derive(
            "EnglishSVOTransitiveClause",
            &[
                SlotFiller::sign("actor", "she"),
                SlotFiller::sign("predicate", "see"),
                SlotFiller::sign("patient", "she"),
            ],
            &SlotMap::identity()
                .rename("subject", "actor")
                .rename("object", "patient"),
        )
        .unwrap();
    assert_eq!(renamed.surface, "she sees her");
    assert_eq!(
        renamed.token.fillers[0].scalar(Dim::Syn, "case"),
        Some("nominative"),
        "slot feature assignment is keyed by internal slot after SlotMap"
    );
    assert_eq!(
        renamed.token.fillers[2].scalar(Dim::Syn, "case"),
        Some("accusative")
    );

    let fixed = Language::parse(
        r#"sign fixed_accusative:
    belongs Pronoun
    belongs EnglishCaseBearer
    belongs EnglishThirdSingular
    syn:
        feature:
            case = accusative
    phon:
        /her/
"#,
    )
    .unwrap();
    let conflict_system = compile_with_libraries(fixed, english_spec()).unwrap();
    let conflict = conflict_system
        .derive(
            "EnglishSVOTransitiveClause",
            &[
                SlotFiller::sign("subject", "fixed_accusative"),
                SlotFiller::sign("predicate", "see"),
                SlotFiller::sign("object", "she"),
            ],
            &SlotMap::identity(),
        )
        .unwrap_err();
    assert!(matches!(
        conflict,
        conlang_language::SystemError::Construction(CxgError::SlotFeatureConflict {
            ref slot,
            ref feature,
            ref expected,
            ref actual,
        }) if slot == "subject"
            && feature == "case"
            && expected == "nominative"
            && actual == "accusative"
    ));
}

#[test]
fn twelve_english_constructions_execute_through_the_public_runtime() {
    let system = compile_with_libraries(Language::new(), english_spec()).unwrap();
    let cases: &[DerivationCase<'_>] = &[
        (
            "EnglishDefiniteNP",
            &[("determiner", "the"), ("nominal", "dog")],
            "the dog",
        ),
        (
            "EnglishIndefiniteNP",
            &[("determiner", "a"), ("nominal", "book")],
            "a book",
        ),
        (
            "EnglishPluralNP",
            &[("nominal", "dog"), ("marker", "plural_s")],
            "dogs",
        ),
        (
            "EnglishPossessiveNP",
            &[("possessor", "john"), ("possessed", "book")],
            "johns book",
        ),
        (
            "EnglishAttributiveNP",
            &[("attribute", "big"), ("nominal", "dog")],
            "big dog",
        ),
        (
            "EnglishPrepositionalPhrase",
            &[("marker", "in"), ("complement", "house")],
            "in house",
        ),
        (
            "EnglishCopularPredication",
            &[("subject", "john"), ("copula", "be"), ("predicate", "big")],
            "john is big",
        ),
        (
            "EnglishDoNegation",
            &[
                ("subject", "john"),
                ("auxiliary", "do"),
                ("negator", "not"),
                ("predicate", "run"),
            ],
            "john does not run",
        ),
        (
            "EnglishPolarQuestion",
            &[("auxiliary", "do"), ("subject", "john"), ("clause", "run")],
            "does john run",
        ),
        (
            "EnglishPeriphrasticPassive",
            &[
                ("patient", "mary"),
                ("auxiliary", "be"),
                ("predicate", "seen"),
                ("agent", "john"),
            ],
            "mary is seen by john",
        ),
    ];

    for (construction, raw_fillers, expected) in cases {
        let fillers = raw_fillers
            .iter()
            .map(|(slot, filler)| SlotFiller::sign(slot, filler))
            .collect::<Vec<_>>();
        let first = system
            .derive(construction, &fillers, &SlotMap::identity())
            .unwrap_or_else(|error| panic!("{construction}: {error}"));
        let second = system
            .derive(construction, &fillers, &SlotMap::identity())
            .unwrap();
        assert_eq!(&first.surface, expected, "{construction}");
        assert_eq!(first.surface, second.surface, "{construction}");
        assert_eq!(first.token, second.token, "{construction}");
        assert_eq!(first.rules, second.rules, "{construction}");
        assert!(
            first.diagnostics.is_empty(),
            "{construction}: {:?}",
            first.diagnostics
        );
        assert!(!first.token.phon.is_empty(), "{construction}: phon pole");
        assert!(!first.token.syn.is_empty(), "{construction}: syn pole");
        assert!(
            !first.token.sem.types.is_empty(),
            "{construction}: semantic/frame type"
        );
        assert!(
            !first.token.sem.roles.is_empty(),
            "{construction}: recursive sem roles"
        );
        assert!(
            first.token.sem.roles.iter().all(|(_, role)| {
                !role.types.is_empty() || !role.features.is_empty() || !role.roles.is_empty()
            }),
            "{construction}: every semantic role resolves to filler meaning"
        );
        assert_eq!(first.token.provenance.construction, *construction);
        assert_eq!(first.token.provenance.fillers.len(), raw_fillers.len());
        assert!(first
            .rules
            .iter()
            .all(|record| record.record.source_package.is_some()));
        assert_eq!(first.token.fillers.len(), raw_fillers.len());
        assert!(first.token.is_saturated());

        let expected_prag = match *construction {
            "EnglishDefiniteNP" => Some(("prag.identifiability", "identifiable")),
            "EnglishIndefiniteNP" => Some(("prag.identifiability", "non-identifiable")),
            "EnglishIntransitiveClause"
            | "EnglishSVOTransitiveClause"
            | "EnglishCopularPredication"
            | "EnglishDoNegation" => Some(("prag.clause-type", "declarative")),
            "EnglishPolarQuestion" => Some(("prag.illocution", "polar-question")),
            "EnglishPeriphrasticPassive" => Some(("prag.perspective", "patient-prominent")),
            _ => None,
        };
        if let Some((path, value)) = expected_prag {
            assert!(first
                .token
                .prag
                .iter()
                .any(|(candidate, actual)| candidate == path && actual == value));
        }
    }

    // The two verbal clauses assign case internally from their predicate.
    let intransitive = system
        .derive(
            "EnglishIntransitiveClause",
            &[
                SlotFiller::sign("subject", "john"),
                SlotFiller::sign("predicate", "run"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(intransitive.surface, "john runs");
    assert!(intransitive.diagnostics.is_empty());

    let transitive = system
        .derive(
            "EnglishSVOTransitiveClause",
            &[
                SlotFiller::sign("subject", "john"),
                SlotFiller::sign("predicate", "see"),
                SlotFiller::sign("object", "mary"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(transitive.surface, "john sees mary");
    assert!(transitive.diagnostics.is_empty());
    assert_eq!(transitive.token.provenance.fillers.len(), 3);
}

#[test]
fn slot_rules_read_frozen_full_dimension_snapshots_and_report_provenance() {
    let system = compile_with_libraries(Language::new(), english_spec()).unwrap();
    let john_before = system
        .effective_language()
        .sign_named("john")
        .unwrap()
        .project(Dim::Syn, &system.ontology)
        .defs;
    let derivation = system
        .derive(
            "EnglishSVOTransitiveClause",
            &[
                SlotFiller::sign("subject", "john"),
                SlotFiller::sign("predicate", "see"),
                SlotFiller::sign("object", "mary"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    let agreement = derivation
        .rules
        .iter()
        .find(|entry| {
            entry.unit == "EnglishSVOTransitiveClause"
                && entry.record.dim == Dim::Syn
                && !entry.record.slot_reads.is_empty()
        })
        .expect("agreement rule is observable");
    assert_eq!(agreement.record.status, RuleStatus::Matched);
    assert_eq!(
        agreement.record.source_package.as_deref(),
        Some("natural:en-standard")
    );
    assert_eq!(agreement.record.slot_reads.len(), 1);
    assert_eq!(agreement.record.slot_reads[0].slot, "subject");
    assert_eq!(agreement.record.slot_reads[0].dim, Dim::Syn);
    assert_eq!(agreement.record.slot_reads[0].path, "number");
    assert_eq!(
        agreement.record.slot_reads[0].value.as_deref(),
        Some("singular")
    );
    assert_eq!(
        derivation
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.number")
            .map(|(_, value)| value.as_str()),
        Some("singular")
    );
    assert_eq!(
        derivation
            .token
            .fillers
            .iter()
            .find(|snapshot| snapshot.slot == "subject")
            .unwrap()
            .scalar(Dim::Syn, "number"),
        Some("singular")
    );
    assert_eq!(
        system
            .effective_language()
            .sign_named("john")
            .unwrap()
            .project(Dim::Syn, &system.ontology)
            .defs,
        john_before,
        "token rules must not mutate the frozen filler sign"
    );
}

#[test]
fn typed_unify_conflict_is_generic_runtime_error_and_any_sign_accepts_non_category_fillers() {
    let source = r#"Symbol a
Symbol b
Class vowel {a}

trait LocalNumber:
    syn:
        feature:
            number = enum(singular, plural)

sign singular_controller:
    belongs Noun
    belongs LocalNumber
    syn:
        feature:
            number = singular
    phon:
        /a/
sign singular_target:
    belongs Predicate
    belongs LocalNumber
    syn:
        feature:
            number = singular
    phon:
        /a/
sign plural_target:
    belongs Predicate
    belongs LocalNumber
    syn:
        feature:
            number = plural
    phon:
        /b/
sign Agreement:
    belongs ControllerTargetAgreement
    phon:
        /{controller}{target}/
"#;
    let generic = compile_system(Language::parse(source).unwrap()).unwrap();
    let matched = generic
        .derive(
            "Agreement",
            &[
                SlotFiller::sign("controller", "singular_controller"),
                SlotFiller::sign("target", "singular_target"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(
        matched
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.agreement_number")
            .map(|(_, value)| value.as_str()),
        Some("singular")
    );
    assert!(matched
        .rules
        .iter()
        .any(|entry| entry.record.status == RuleStatus::Matched));

    let mismatch = generic
        .derive(
            "Agreement",
            &[
                SlotFiller::sign("controller", "singular_controller"),
                SlotFiller::sign("target", "plural_target"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert!(mismatch
        .rules
        .iter()
        .any(|entry| entry.record.status == RuleStatus::Error
            && entry
                .record
                .diag
                .as_deref()
                .is_some_and(|diag| diag.contains("unify conflict"))));
    assert!(!mismatch.diagnostics.is_empty());

    let system = compile_with_libraries(Language::new(), english_spec()).unwrap();
    let any = system
        .derive(
            "EnglishAttributiveNP",
            &[
                SlotFiller::sign("attribute", "big"),
                SlotFiller::sign("nominal", "dog"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(any.surface, "big dog");
}

#[test]
fn slot_read_then_feeding_optional_fallback_and_static_errors_are_distinct() {
    let source = r#"Symbol x
Symbol y
Class vowel {x, y}

sign filler:
    syn:
        feature:
            number = enum(singular, plural)
            number = singular
    sem:
        feature:
            ref = enum(X)
            ref = X
    phon:
        /x/

sign CopyConstruction:
    syn:
        slots:
            head [*]
            optional [*]?
        feature:
            copied = enum(singular, plural)
            fed = enum(yes, no)
            optional-value = enum(singular, plural, absent)
            copied => $slot.head.syn.number
                then fed => yes / copied == singular
            optional-value => $slot.optional.syn.number
                else optional-value => absent
    sem:
        roles:
            referent [Semantic]
            referent = {head}
    phon:
        /{head}{optional}/
"#;
    let system = compile_system(Language::parse(source).unwrap()).unwrap();
    let derivation = system
        .derive(
            "CopyConstruction",
            &[SlotFiller::sign("head", "filler")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(derivation.surface, "x");
    let syn = |path: &str| {
        derivation
            .token
            .syn
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(syn("syn.copied"), Some("singular"));
    assert_eq!(syn("syn.fed"), Some("yes"));
    assert_eq!(syn("syn.optional-value"), Some("absent"));
    assert!(derivation.rules.iter().any(|entry| {
        entry.record.status == RuleStatus::Matched
            && entry.record.branch == Some(1)
            && entry.record.slot_reads.is_empty()
    }));

    let invalid = r#"sign Bad:
    syn:
        slots:
            head [*]
        feature:
            copied = enum(singular, plural)
            copied => $slot.typo.syn.number
    sem:
        roles:
            referent [Semantic]
            referent = {head}
    phon:
        /{head}/
"#;
    let error = compile_system(Language::parse(invalid).unwrap()).unwrap_err();
    let conlang_language::CompileSystemError::Validation(report) = error else {
        panic!("expected validation report")
    };
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == "RULE_INVALID" && diagnostic.message.contains("unknown slot")
    }));
}

#[test]
fn all_eight_std_cxg_realizations_have_executable_positive_cases() {
    let source = r#"Symbol a
Symbol i
Symbol m
Symbol n
Symbol p
Symbol q
Symbol s
Symbol v
Class vowel {a, i}

sign marker:
    belongs Adposition
    sem:
        feature:
            relation = enum(AT)
            relation = AT
    phon:
        /pa/
sign nominal:
    belongs Noun
    sem:
        feature:
            ref = enum(N, S, O)
            ref = N
    phon:
        /na/
sign subject:
    belongs Noun
    sem:
        feature:
            ref = enum(N, S, O)
            ref = S
    phon:
        /sa/
sign object:
    belongs Noun
    sem:
        feature:
            ref = enum(N, S, O)
            ref = O
    phon:
        /ma/
sign transitive:
    belongs Transitive
    sem:
        feature:
            event = enum(V, I)
            event = V
    phon:
        /vi/
sign predicate:
    belongs Intransitive
    sem:
        feature:
            event = enum(V, I)
            event = I
    phon:
        /i/
sign negator:
    sem:
        feature:
            polarity = enum(negative)
            polarity = negative
    phon:
        /m/
sign particle:
    prag:
        feature:
            illocution = enum(polar-question)
            illocution = polar-question
    phon:
        /qa/

sign TestPre:
    belongs PrepositionalPhrase
sign TestPost:
    belongs PostpositionalPhrase
sign TestSVO:
    belongs SVOTransitiveClause
sign TestOVS:
    belongs OVSTransitiveClause
sign TestPrefixNeg:
    belongs PrefixNegation
sign TestSuffixNeg:
    belongs SuffixNegation
sign TestInitialQuestion:
    belongs InitialPolarQuestion
sign TestSerial:
    belongs SerialPredicate
"#;
    let system = compile_system(Language::parse(source).unwrap()).unwrap();
    let cases: &[DerivationCase<'_>] = &[
        (
            "TestPre",
            &[("marker", "marker"), ("complement", "nominal")],
            "pa na",
        ),
        (
            "TestPost",
            &[("marker", "marker"), ("complement", "nominal")],
            "na pa",
        ),
        (
            "TestSVO",
            &[
                ("subject", "subject"),
                ("predicate", "transitive"),
                ("object", "object"),
            ],
            "sa vi ma",
        ),
        (
            "TestOVS",
            &[
                ("subject", "subject"),
                ("predicate", "transitive"),
                ("object", "object"),
            ],
            "ma vi sa",
        ),
        (
            "TestPrefixNeg",
            &[("negator", "negator"), ("predicate", "predicate")],
            "mi",
        ),
        (
            "TestSuffixNeg",
            &[("negator", "negator"), ("predicate", "predicate")],
            "im",
        ),
        (
            "TestInitialQuestion",
            &[("particle", "particle"), ("clause", "predicate")],
            "qa i",
        ),
        (
            "TestSerial",
            &[("first", "predicate"), ("second", "transitive")],
            "i vi",
        ),
    ];
    for (construction, raw, expected) in cases {
        let fillers = raw
            .iter()
            .map(|(slot, filler)| SlotFiller::sign(slot, filler))
            .collect::<Vec<_>>();
        let derivation = system
            .derive(construction, &fillers, &SlotMap::identity())
            .unwrap_or_else(|error| panic!("{construction}: {error}"));
        assert_eq!(derivation.surface, *expected, "{construction}");
    }
}

#[test]
fn pole_and_unused_slot_diagnostics_are_coded_and_gb132_does_not_choose_order() {
    let warnings = r#"Symbol a
Class vowel {a}
sign filler:
    phon:
        /a/
sign Meaningless:
    syn:
        slots:
            head [*]
    phon:
        /{head}/
sign Unused:
    syn:
        slots:
            head [*]
    sem:
        feature:
            constant = enum(X)
            constant = X
    phon:
        /a/
sign VerbMedialOnly:
    belongs GB132_Present
    phon:
        /a/
"#;
    let system = compile_system(Language::parse(warnings).unwrap()).unwrap();
    let codes = system
        .validation
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&"CONSTRUCTION_MEANING_MISSING"));
    assert!(codes.contains(&"CONSTRUCTION_SLOT_UNUSED"));
    let verb_medial = system
        .effective_language()
        .sign_named("VerbMedialOnly")
        .unwrap();
    let categories = system.ontology.sign_categories(verb_medial);
    assert!(!categories
        .iter()
        .any(|category| category == "SVOTransitiveClause"));
    assert!(!categories
        .iter()
        .any(|category| category == "OVSTransitiveClause"));

    let missing_form = r#"sign BadForm:
    syn:
        slots:
            head [*]
    sem:
        roles:
            referent [Semantic]
            referent = {head}
"#;
    let error = compile_system(Language::parse(missing_form).unwrap()).unwrap_err();
    let conlang_language::CompileSystemError::Validation(report) = error else {
        panic!("expected validation report")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "CONSTRUCTION_PHON_MISSING"));
}
