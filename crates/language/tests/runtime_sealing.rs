use conlang_language::construction::{FillerProvenance, SlotFiller, SlotMap};
use conlang_language::{
    compile_system, CaseBranchStatus, DerivationContext, Dim, Language, SignValue, SystemError,
};

#[test]
fn applied_residual_slot_keeps_its_name_when_passed_to_a_nested_application() {
    let language = Language::parse(
        r#"trait AliasAtom:

sign x:
    belongs AliasAtom
    phon:
        /x/

sign RenameTarget:
    syn:
        slots:
            renamed [AliasAtom]
    phon:
        /{renamed}/

sign AliasSource:
    syn:
        slots:
            original [AliasAtom]
    phon:
        /{original}/
    case:
        else:
            RenameTarget(renamed: {original})

sign seed:
    phon:
        /s/
    case:
        else:
            AliasSource()
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let value = system
        .evaluate_sign_expression("seed", &DerivationContext::new())
        .unwrap()
        .value;
    assert_eq!(
        value
            .residual_parameters()
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        ["original"],
        "the caller's free-variable name is the public alias"
    );

    let completed = system
        .apply_arguments(&value, &[SlotFiller::sign("original", "x")])
        .unwrap();
    let SignValue::Applied(completed) = completed else {
        panic!("supplying the aliased variable must return an applied Sign")
    };
    assert!(completed.is_saturated());
    assert_eq!(completed.phon_form().unwrap(), "x");
}

#[test]
fn resumed_construction_uses_internal_committed_and_autofilled_occurrences() {
    let language = Language::parse(
        r#"trait BoundAtom:
    syn:
        feature:
            mark = enum(plain, marked)?
    sem:
        feature:
            interpreted = enum(plain, marked)
            interpreted => $self.syn.mark

sign atom:
    belongs BoundAtom
    phon:
        /a/

sign automatic:
    belongs BoundAtom
    syn:
        feature:
            mark = plain
    phon:
        /u/

sign Wrapper:
    syn:
        slots:
            value [BoundAtom]
            helper [BoundAtom]
    phon:
        /{value}{helper}/

sign Outer:
    syn:
        slots:
            head [BoundAtom]
            tail [BoundAtom]
            auto [BoundAtom]
        map head rename public_head
        map auto autofill automatic
        slot_features:
            head.mark = marked
    phon:
        /{head}{tail}{auto}/
    case:
        else:
            Wrapper(value: {head}, helper: {auto})
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let partial = system
        .apply_construction(
            "Outer",
            &[SlotFiller::sign("public_head", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(partial.missing_required(), ["tail"]);

    let completed = system
        .apply_arguments(
            &SignValue::Applied(partial),
            &[SlotFiller::sign("tail", "atom")],
        )
        .unwrap();
    let SignValue::Applied(completed) = completed else {
        panic!("the outer Sign case must return Wrapper")
    };
    assert_eq!(completed.construction, "Wrapper");
    assert!(completed.is_saturated());
    assert_eq!(completed.fillers.len(), 2);
    let value = completed
        .fillers
        .iter()
        .find(|filler| filler.slot == "value")
        .unwrap();
    assert_eq!(value.scalar(Dim::Syn, "mark"), Some("marked"));
    assert_eq!(
        value.scalar(Dim::Sem, "interpreted"),
        Some("marked"),
        "the nested application receives the committed occurrence, not the raw source Sign"
    );
    let helper = completed
        .fillers
        .iter()
        .find(|filler| filler.slot == "helper")
        .unwrap();
    assert_eq!(helper.scalar(Dim::Syn, "mark"), Some("plain"));
}

#[test]
fn candidate_search_executes_typed_cases_but_never_hides_their_errors() {
    let language = Language::parse(
        r#"trait CandidateAtom:

trait CandidateConstruction:

sign x:
    belongs CandidateAtom
    phon:
        /x/

sign Faulty:
    belongs CandidateConstruction
    syn:
        slots:
            value [CandidateAtom]
        feature:
            trigger = enum(on, off)
    sem:
        feature:
            result = enum(yes, no)
            result =>
                case:
                    $self.syn.trigger == on:
                        yes
    phon:
        /{value}/
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let error = system
        .derive_candidates(
            "CandidateConstruction",
            &[SlotFiller::sign("value", "x")],
            &SlotMap::identity(),
            &DerivationContext::new().feature(Dim::Syn, "trigger", "off"),
        )
        .unwrap_err();
    assert!(matches!(error, SystemError::CaseDefaultMissing { .. }));
}

#[test]
fn candidate_context_misses_are_filtered_and_zero_is_not_ambiguity() {
    let language = Language::parse(
        r#"trait ExpectedAtom:

trait OtherAtom:

trait SelectableConstruction:

sign other:
    belongs OtherAtom
    phon:
        /o/

sign expected:
    belongs ExpectedAtom
    phon:
        /e/

sign Good:
    belongs SelectableConstruction
    syn:
        slots:
            value [ExpectedAtom]
        feature:
            mode = enum(on, off)
    phon:
        /{value}/

sign FixedOff:
    belongs SelectableConstruction
    syn:
        slots:
            value [ExpectedAtom]
        feature:
            mode = enum(on, off)
            mode = off
    phon:
        /{value}/
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let context_candidates = system
        .derive_candidates(
            "SelectableConstruction",
            &[SlotFiller::sign("value", "expected")],
            &SlotMap::identity(),
            &DerivationContext::new().feature(Dim::Syn, "mode", "on"),
        )
        .unwrap();
    assert_eq!(
        context_candidates
            .candidates
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>(),
        ["Good"]
    );

    let fillers = [SlotFiller::sign("value", "other")];
    let candidates = system
        .derive_candidates(
            "SelectableConstruction",
            &fillers,
            &SlotMap::identity(),
            &DerivationContext::new(),
        )
        .unwrap();
    assert!(candidates.candidates.is_empty());
    assert!(matches!(
        system.derive_category(
            "SelectableConstruction",
            &fillers,
            &SlotMap::identity(),
            DerivationContext::new()
        ),
        Err(SystemError::NoMatchingConstruction { .. })
    ));
}

#[test]
fn hard_occurrence_constraint_blocks_only_the_specific_case_branch() {
    let language = Language::parse(
        r#"Symbol x
Class vowel {x}

trait BlockingAtom:

trait CaseBearer:
    syn:
        feature:
            case = enum(plain, marked)

sign x:
    belongs BlockingAtom
    phon:
        /x/

sign NeedsMarked:
    syn:
        slots:
            target [CaseBearer]
        slot_features:
            target.case = marked
    phon:
        /{target}/

sign Current:
    belongs CaseBearer
    syn:
        slots:
            value [BlockingAtom]
        feature:
            case = plain
    phon:
        /{value}/
    case:
        $self == [CaseBearer]:
            NeedsMarked(target: {$self})
        else:
            $self
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let derivation = system
        .derive(
            "Current",
            &[SlotFiller::sign("value", "x")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(derivation.surface, "x");
    assert_eq!(derivation.token.construction, "Current");
    assert!(derivation.cases.iter().any(|record| {
        record.status == CaseBranchStatus::MoreSpecificBlocked
            && record.diagnostic_code == Some("CASE_MORE_SPECIFIC_BLOCKED")
    }));
    assert!(derivation
        .cases
        .iter()
        .any(|record| record.status == CaseBranchStatus::Matched));
}

#[test]
fn stored_self_application_does_not_replay_inherited_rules_or_replace_provenance() {
    let language = Language::parse(
        r#"Symbol s
Class vowel {s}

trait EvaluateOnce:
    syn:
        feature:
            state = enum(changed, committed)
            state => changed / state == committed
                else state => committed

sign Wrapper:
    syn:
        slots:
            value [*]
    phon:
        /{value}/

sign seed:
    belongs EvaluateOnce
    phon:
        /s/
    case:
        else:
            Wrapper({$self})
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let result = system
        .evaluate_sign_expression("seed", &DerivationContext::new())
        .unwrap();
    let SignValue::Applied(token) = result.value else {
        panic!("the wrapper application must return an applied Sign")
    };
    let value = token
        .fillers
        .iter()
        .find(|filler| filler.slot == "value")
        .unwrap();
    assert_eq!(
        value.scalar(Dim::Syn, "state"),
        Some("committed"),
        "the already committed inherited rule must not run a second time"
    );
    assert_eq!(
        value.provenance,
        FillerProvenance::StoredSign("seed".to_owned())
    );
    assert_eq!(value.sem.source.sign, "seed");
}

#[test]
fn stored_self_occurrence_constraints_rerun_local_rules_from_the_source_baseline() {
    let language = Language::parse(
        r#"Symbol s
Class vowel {s}

trait Contextual:
    syn:
        feature:
            mark = enum(plain, marked)?
    sem:
        feature:
            interpretation = enum(plain, marked)

sign Wrapper:
    syn:
        slots:
            value [Contextual]
        slot_features:
            value.mark = marked
    phon:
        /{value}/

sign seed:
    belongs Contextual
    sem:
        feature:
            interpretation => $self.syn.mark
    phon:
        /s/
    case:
        else:
            Wrapper({$self})
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let result = system
        .evaluate_sign_expression("seed", &DerivationContext::new())
        .unwrap();
    let SignValue::Applied(token) = result.value else {
        panic!("the wrapper application must return an applied Sign")
    };
    let value = token
        .fillers
        .iter()
        .find(|filler| filler.slot == "value")
        .unwrap();
    assert_eq!(value.scalar(Dim::Syn, "mark"), Some("marked"));
    assert_eq!(
        value.scalar(Dim::Sem, "interpretation"),
        Some("marked"),
        "the local Sem rule must run from the true source baseline after occurrence injection"
    );
    assert_eq!(
        value.provenance,
        FillerProvenance::StoredSign("seed".to_owned())
    );
    assert_eq!(value.sem.source.sign, "seed");
}
