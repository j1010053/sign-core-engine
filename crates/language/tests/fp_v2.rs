use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::system::{
    compile_system, CandidateSelector, DerivationContext, SignValue, SystemError,
};
use conlang_language::{
    CaseBranchStatus, CaseSelection, CompileSystemError, Dim, FeatureDecl, Language, SignItem,
    SourceLocation,
};

const FP_SOURCE: &str = r#"trait TestVerb:

trait ThirdSingular:
    syn:
        feature:
            number = enum(singular, plural)
            number = singular
            person = enum(first, second, third)
            person = third

trait FiniteVerb:

trait SibilantFinal:

sign en_3sg:
    belongs ThirdSingular
    syn:
        slots:
            stem [TestVerb]
    phon:
        /{$slot.stem}+s/
        realization:
            case:
                $slot.stem == [SibilantFinal]:
                    /{$slot.stem}+es/
                else:
                    /{$slot.stem}+s/

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
    let canonical = parsed.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    assert!(canonical.contains("$slot.stem == [SibilantFinal]:"));
    assert!(canonical.contains("en_3sg({$self})"));
}

#[test]
fn sign_context_case_merges_anonymous_trait_fragment_into_the_same_sign() {
    let source = r#"trait FragmentTestVerb:

trait FragmentThirdSingular:

trait ThirdPersonFragment:
    belongs FragmentThirdSingular
    syn:
        feature:
            inflection = enum(base, third)
            inflection = third
    sem:
        feature:
            exponent = enum(third_singular)
            exponent = third_singular
    phon:
        /walks/

sign walk:
    belongs FragmentTestVerb
    syn:
        feature:
            trigger = enum(on, off)
    phon:
        /walk/
    prag:
        feature:
            selected = enum(yes, no)
    case:
        $self.syn.trigger == on:
            ThirdPersonFragment
            prag:
                feature:
                    selected = yes
"#;
    let parsed = Language::parse(source).expect("SignContext fragment parses");
    let canonical = parsed.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    assert!(canonical.contains("ThirdPersonFragment"));
    assert!(canonical.contains("selected = yes"));

    let system = compile_system(parsed).expect("trait fragment compiles");
    let base = system.evaluate_sign("walk").unwrap();
    let base_id = base.sign.id.clone();
    let evaluated = system
        .evaluate_sign_expression(
            "walk",
            &DerivationContext::new().feature(Dim::Syn, "trigger", "on"),
        )
        .unwrap();
    let SignValue::Stored(stored) = evaluated.value else {
        panic!("a SignContext fragment must not create an Applied Sign entity")
    };
    assert_eq!(stored.sign.id, base_id, "fragment merge preserves SignId");
    assert_eq!(stored.sign.name, "walk");
    assert!(stored
        .sign
        .items
        .iter()
        .any(|item| matches!(item, SignItem::Belongs(name) if name == "FragmentThirdSingular")));
    assert!(stored.sign.items.iter().any(|item| {
        matches!(item, SignItem::FeatureValue(value)
            if value.dim == Dim::Syn
                && value.name == "inflection"
                && value.value == "third")
    }));
    assert!(stored.sign.items.iter().any(
        |item| matches!(item, SignItem::Def(def) if def.path == "phon" && def.value == "/walks/")
    ));
    assert!(stored.sign.items.iter().any(|item| {
        matches!(item, SignItem::FeatureValue(value)
            if value.dim == Dim::Sem && value.name == "exponent" && value.value == "third_singular")
    }));
    assert!(stored.sign.items.iter().any(|item| {
        matches!(item, SignItem::FeatureValue(value)
            if value.dim == Dim::Prag && value.name == "selected" && value.value == "yes")
    }));

    let unmatched = system
        .evaluate_sign_expression(
            "walk",
            &DerivationContext::new().feature(Dim::Syn, "trigger", "off"),
        )
        .unwrap();
    let SignValue::Stored(unmatched) = unmatched.value else {
        panic!("unmatched case returns the base Sign")
    };
    assert_eq!(unmatched.sign.id, base_id);
    assert!(unmatched.sign.items.iter().any(
        |item| matches!(item, SignItem::Def(def) if def.path == "phon" && def.value == "/walk/")
    ));
    assert!(!unmatched
        .sign
        .items
        .iter()
        .any(|item| matches!(item, SignItem::Belongs(name) if name == "FragmentThirdSingular")));
}

#[test]
fn when_guards_share_one_frozen_pre_merge_snapshot() {
    let source = r#"sign cumulative:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = on
            outcome = enum(base, first, second, fallback)
            outcome = base
            leaked = enum(no, yes)
            leaked = no
        when:
            $self.syn.trigger == on:
                feature:
                    outcome = first
            $self.syn.outcome == first:
                feature:
                    leaked = yes
            $self.syn.trigger == on:
                feature:
                    outcome = second
            else:
                feature:
                    outcome = fallback
    sem:
        feature:
            selected = enum(semantic, none)
        when:
            $self.syn.trigger == on:
                feature:
                    selected = semantic
    prag:
        feature:
            licensed = enum(yes, no)
        when:
            $self.syn.trigger == on:
                feature:
                    licensed = yes
    phon:
        /x/
"#;
    let parsed = Language::parse(source).expect("dimension contexts and when parse");
    let canonical = parsed.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    assert!(canonical.contains("        when:\n            $self.syn.trigger == on:"));
    // P71 §4.3 後各維先有 `feature:` 宣告,`when:` 不再是維度的第一個子項;
    // 這裡要釘的是「每一維各有自己的 when: 分支」,故直接看分支內容。
    assert!(canonical
        .contains("            $self.syn.trigger == on:\n                feature:\n                    selected = semantic"));
    assert!(canonical
        .contains("            $self.syn.trigger == on:\n                feature:\n                    licensed = yes"));

    let system = compile_system(parsed).expect("closed dimension fragments compile");
    let evaluated = system
        .evaluate_sign_expression("cumulative", &DerivationContext::new())
        .unwrap();
    let SignValue::Stored(stored) = evaluated.value else {
        panic!("when fragments merge into the existing Sign")
    };
    let feature = |name: &str| {
        stored.sign.items.iter().rev().find_map(|item| match item {
            SignItem::FeatureValue(value) if value.name == name => Some(value.value.as_str()),
            _ => None,
        })
    };
    assert_eq!(
        feature("outcome"),
        Some("second"),
        "independently matched fragments merge in source order with later-wins"
    );
    assert_eq!(
        feature("leaked"),
        Some("no"),
        "the first fragment must not make the second guard match"
    );
    assert!(stored.sign.items.iter().any(|item| {
        matches!(item, SignItem::FeatureValue(value)
            if value.dim == Dim::Sem && value.name == "selected" && value.value == "semantic")
    }));
    assert!(stored.sign.items.iter().any(|item| {
        matches!(item, SignItem::FeatureValue(value)
            if value.dim == Dim::Prag && value.name == "licensed" && value.value == "yes")
    }));

    let syn_records = evaluated
        .cases
        .iter()
        .filter(|record| record.selection == CaseSelection::Accumulate)
        .take(4)
        .collect::<Vec<_>>();
    assert_eq!(syn_records.len(), 4);
    assert_eq!(syn_records[0].status, CaseBranchStatus::Matched);
    assert_eq!(syn_records[1].status, CaseBranchStatus::Unmatched);
    assert_eq!(syn_records[2].status, CaseBranchStatus::Matched);
    assert_eq!(
        syn_records[3].status,
        CaseBranchStatus::Unmatched,
        "`else` is inactive when any ordinary when branch matched"
    );
}

#[test]
fn when_else_uses_the_same_external_default_policy() {
    let system = compile_system(
        Language::parse(
            r#"sign fallback:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = off
            result = enum(base, matched, fallback)
            result = base
        when:
            $self.syn.trigger == on:
                feature:
                    result = matched
            else:
                feature:
                    result = fallback
    phon:
        /x/
"#,
        )
        .unwrap(),
    )
    .unwrap();
    let evaluated = system
        .evaluate_sign_expression("fallback", &DerivationContext::new())
        .unwrap();
    let SignValue::Stored(stored) = evaluated.value else {
        panic!("dimension when produces a stored Sign")
    };
    assert!(stored.sign.items.iter().rev().any(|item| {
        matches!(item, SignItem::FeatureValue(value)
            if value.name == "result" && value.value == "fallback")
    }));
}

#[test]
fn when_guard_error_aborts_before_any_fragment_commit() {
    let system = compile_system(
        Language::parse(
            r#"trait AtomicNominal:
    syn:
        feature:
            number = enum(singular, plural)

sign atomic:
    syn:
        slots:
            subject [AtomicNominal]
        feature:
            trigger = enum(on, off)
            trigger = on
            outcome = enum(base, first, second)
            outcome = base
        when:
            $self.syn.trigger == on:
                feature:
                    outcome = first
            $slot.subject.syn.number == singular:
                feature:
                    outcome = second
    phon:
        /{$slot.subject}/
"#,
        )
        .unwrap(),
    )
    .unwrap();
    let error = system
        .evaluate_sign_expression("atomic", &DerivationContext::new())
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("required slot \"subject\" has no value for syn.number"),
        "{error}"
    );

    let unchanged = system.evaluate_sign("atomic").unwrap();
    assert!(unchanged.sign.items.iter().rev().any(|item| {
        matches!(item, SignItem::FeatureValue(value)
            if value.name == "outcome" && value.value == "base")
    }));
}

#[test]
fn trait_expansion_is_closed_to_sign_context() {
    let error = Language::parse(
        r#"trait SuffixFragment:
    phon:
        /s/

sign word:
    phon:
        /word/
        realization:
            case:
                else:
                    SuffixFragment
"#,
    )
    .unwrap_err();
    assert!(
        error.msg.contains("PhonContext"),
        "a trait cannot be coerced into a phon fragment: {}",
        error.msg
    );
}

#[test]
fn sign_context_fragment_is_checked_as_a_typed_sign_body() {
    let error = compile_system(
        Language::parse(
            r#"sign fragment_schema:
    syn:
        feature:
            mode = enum(a, b)
    phon:
        /x/
    case:
        else:
            syn:
                feature:
                    mode = outside
"#,
        )
        .unwrap(),
    )
    .unwrap_err();
    let CompileSystemError::Validation(report) = error else {
        panic!("invalid fragment must fail static validation")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "FEATURE_VALUE_OUT_OF_DOMAIN"));

    let system = compile_system(
        Language::parse(
            r#"sign nested_fragment_schema:
    phon:
        /x/
    case:
        else:
            syn:
                feature:
                    mode = enum(a, b)
                    mode = a
            case:
                else:
                    syn:
                        feature:
                            mode = b
"#,
        )
        .unwrap(),
    )
    .expect("a nested fragment sees declarations in its enclosing SignContext");
    let evaluated = system
        .evaluate_sign_expression("nested_fragment_schema", &DerivationContext::new())
        .unwrap();
    let SignValue::Stored(evaluated) = evaluated.value else {
        panic!("fragment merging keeps the stored Sign identity")
    };
    assert!(evaluated.sign.items.iter().rev().any(|item| {
        matches!(item, SignItem::FeatureValue(value)
            if value.name == "mode" && value.value == "b")
    }));
}

/// v1 淘汰後(2026-07-24):FP `case` 語法為預設,**無需 schema 標頭**即可解析;
/// canonical dump 不再輸出標頭(printer 已移除);那行現已**被拒絕**,
/// 見 `the_legacy_v2_header_is_now_rejected`。
#[test]
fn case_syntax_parses_without_any_schema_header() {
    let language = Language::parse(
        r#"sign walk:
    phon:
        /walk/
    case:
        $self == [Verb]:
            inflect({$self})
"#,
    )
    .expect("headerless FP case syntax must parse after v1 removal");
    let canonical = language.dump();
    assert!(
        !canonical.contains("conlang.lang/v2"),
        "printer 不再輸出 schema 標頭:\n{canonical}"
    );
    assert!(canonical.contains("case"), "case 保留於 canonical");
    assert_eq!(
        Language::parse(&canonical).unwrap().dump(),
        canonical,
        "不動點"
    );
}

/// 🔑 舊 `schema conlang.lang/v2` 標頭現在**被拒絕**。
///
/// v1 於 2026-07-24 硬移除後,那行成了 parser 特別認得、然後丟掉的 no-op
/// ——不進 dump、不影響 identity digest,對任何行為零貢獻(常數名字就叫
/// `LEGACY_V2_HEADER`)。留著等於保留一條「認得但無意義」的語法。
///
/// **拒絕而非忽略**:無意義的輸入靜默通過,會讓作者以為那行有作用。
/// 本專案未曾發布,沒有既有檔案需要相容;repo 內三個 `.lang` 已一併清掉。
#[test]
fn the_legacy_v2_header_is_now_rejected() {
    let body = "sign walk:\n    phon:\n        /walk/\n";
    Language::parse(body).expect("無標頭的來源照常解析");

    let error = Language::parse(&format!("schema conlang.lang/v2\n\n{body}"))
        .expect_err("舊標頭應被拒,而不是靜默忽略");
    assert!(
        format!("{error:?}").contains("line: 1"),
        "錯誤要指得出是第一行:{error:?}"
    );
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
        r#"trait TestNominal:
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
        /{$slot.subject} {$slot.predicate}/
    constraints:
        equal($slot.subject.syn.number, $slot.predicate.syn.number)
        before($slot.subject, $slot.predicate)
        adjacent($slot.subject, $slot.predicate)
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
fn unsaturated_sign_can_receive_arguments_without_becoming_another_entity() {
    let language = Language::parse(
        r#"trait Piece:
    syn:
        feature:
            mark = enum(left, right)

trait Pair:
    syn:
        feature:
            selected = enum(left, right)

sign left:
    belongs Piece
    syn:
        feature:
            mark = left
    phon:
        /L/

sign right:
    belongs Piece
    syn:
        feature:
            mark = right
    phon:
        /R/

sign Pairing:
    belongs Pair
    syn:
        slots:
            first [Piece]
            second [Piece]
        feature:
            selected => $slot.second.syn.mark
    phon:
        /{$slot.first} {$slot.second}/

sign seed:
    belongs Piece
    phon:
        /S/
    case:
        $self == [Piece]:
            Pairing(first: {$self})
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let stored = system
        .evaluate_sign_expression("Pairing", &DerivationContext::new())
        .unwrap()
        .value;
    assert!(matches!(stored, SignValue::Stored(_)));
    assert!(stored.has_free_variables());
    assert_eq!(stored.residual_parameters().len(), 2);
    let value = system
        .evaluate_sign_expression("seed", &DerivationContext::new())
        .unwrap()
        .value;
    assert!(value.has_free_variables());
    assert_eq!(value.residual_parameters()[0].name, "second");
    let original_id = value.sign_id().clone();
    let resumed = system
        .apply_arguments(&value, &[SlotFiller::sign("second", "right")])
        .unwrap();
    let SignValue::Applied(token) = resumed else {
        panic!("resume returns the completed Sign")
    };
    assert!(token.is_saturated());
    assert_eq!(token.phon_form().unwrap(), "S R");
    assert_eq!(
        token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.selected")
            .map(|(_, value)| value.as_str()),
        Some("right"),
        "rules that were Unmatched while the Sign was partial must rerun after saturation"
    );
    assert_eq!(value.residual_parameters()[0].name, "second");
    assert_eq!(value.sign_id(), &original_id);
}

#[test]
fn nested_unsaturated_signs_export_named_variables_and_resume_immutably() {
    let language = Language::parse(
        r#"trait NestedPiece:

sign value:
    belongs NestedPiece
    phon:
        /V/

sign Inner:
    syn:
        slots:
            stem [NestedPiece]
    phon:
        /{$slot.stem}/

sign Pair:
    syn:
        slots:
            left [*]
            right [*]
    phon:
        /{$slot.left} {$slot.right}/

sign seed:
    phon:
        /S/
    case:
        else:
            Pair(left = Inner(), right = Inner())
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let value = system
        .evaluate_sign_expression("seed", &DerivationContext::new())
        .unwrap()
        .value;
    assert!(value.has_free_variables());
    assert_eq!(
        value
            .residual_parameters()
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        ["stem"]
    );
    let completed = system
        .apply_arguments(&value, &[SlotFiller::sign("stem", "value")])
        .unwrap();
    let SignValue::Applied(completed) = completed else {
        panic!("resuming the same Sign value must produce an applied Sign")
    };
    assert_eq!(completed.phon_form().unwrap(), "V V");
    assert!(
        value.has_free_variables(),
        "the input Sign remains immutable"
    );
}

#[test]
fn competition_returns_all_candidates_and_samples_deterministically() {
    let language = Language::parse(
        r#"trait Atom:

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
        /a {$slot.value}/

sign B:
    belongs CompetingConstruction
    entrenchment = 0.75
    syn:
        slots:
            value [Atom]
    phon:
        /b {$slot.value}/
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
fn entrenchment_sampling_normalizes_finite_weights_before_they_overflow() {
    let system = compile_system(Language::parse(FP_SOURCE).unwrap()).unwrap();
    let first_id = conlang_language::SignId::local(10_000);
    let second_id = conlang_language::SignId::local(10_001);
    let candidates = conlang_language::CandidateSet {
        category: "OverflowingCompetition".to_owned(),
        candidates: vec![
            conlang_language::ConstructionCandidate {
                id: first_id.clone(),
                name: "HugeFirst".to_owned(),
                entrenchment: 1.7e308,
            },
            conlang_language::ConstructionCandidate {
                id: second_id.clone(),
                name: "HugeSecond".to_owned(),
                entrenchment: 8.5e307,
            },
        ],
    };
    let raw_total: f64 = candidates
        .candidates
        .iter()
        .map(|candidate| candidate.entrenchment)
        .sum();
    assert!(
        raw_total.is_infinite(),
        "the regression requires raw overflow"
    );

    let first_draw = system
        .select_candidate(
            &candidates,
            CandidateSelector::SampleEntrenchment { seed: 3 },
            None,
        )
        .unwrap();
    let second_draw = system
        .select_candidate(
            &candidates,
            CandidateSelector::SampleEntrenchment { seed: 11 },
            None,
        )
        .unwrap();

    assert_eq!(first_draw.selected, first_id);
    assert_eq!(second_draw.selected, second_id);
    assert_eq!(
        first_draw,
        system
            .select_candidate(
                &candidates,
                CandidateSelector::SampleEntrenchment { seed: 3 },
                None,
            )
            .unwrap(),
        "the same seed must remain replayable",
    );
}

#[test]
fn phon_projection_evaluates_the_full_sign_before_extracting_phon() {
    let language = Language::parse(
        r#"trait ProjectionAtom:

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
        /{$slot.base}!/

sign Outer:
    belongs OuterCategory
    syn:
        slots:
            stem [ProjectionAtom]
    phon:
        /{$slot.stem}/
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
        r#"trait Atom:

sign Wrapper:
    syn:
        slots:
            value [*]
    phon:
        /{$slot.value}/

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
        r#"sign A:
    syn:
        slots:
            value [*]
    phon:
        /{$slot.value}/
    case:
        else:
            B({$self})

sign B:
    syn:
        slots:
            value [*]
    phon:
        /{$slot.value}/
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

#[test]
fn feature_and_role_cases_execute_in_the_dimension_pipeline() {
    let language = Language::parse(
        r#"Symbol a
Symbol l
Symbol p
Symbol h
Symbol b
Symbol e
Symbol t
Class vowel {a, e}

trait ChoiceEntity:
    belongs Semantic

trait ChoiceAtom:
    belongs ChoiceEntity

trait ChoiceFrame:
    belongs SemanticFrame

sign alpha:
    belongs ChoiceAtom
    phon:
        /alpha/

sign beta:
    belongs ChoiceAtom
    phon:
        /beta/

sign Chooser:
    belongs ChoiceFrame
    syn:
        slots:
            first [ChoiceAtom]
            second [ChoiceAtom]
        feature:
            number = enum(singular, plural)
            selection = enum(first, second)
    sem:
        feature:
            boundedness = enum(bounded, unbounded)
            boundedness =>
                case:
                    $self.syn.number == plural:
                        unbounded
                    else:
                        bounded
        roles:
            agent [ChoiceEntity]
            agent =
                case:
                    $self.syn.selection == first:
                        {$slot.first}
                    else:
                        {$slot.second}
    phon:
        /{$slot.first} {$slot.second}/
"#,
    )
    .unwrap();
    let canonical = language.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    let system = compile_system(language).unwrap();
    let derivation = system
        .derive_with_context(
            "Chooser",
            &[
                SlotFiller::sign("first", "alpha"),
                SlotFiller::sign("second", "beta"),
            ],
            &SlotMap::identity(),
            DerivationContext::new()
                .feature(Dim::Syn, "number", "plural")
                .feature(Dim::Syn, "selection", "first"),
        )
        .unwrap();
    assert_eq!(
        derivation.token.sem.features.get("boundedness"),
        Some(&"unbounded".to_owned())
    );
    assert_eq!(
        derivation.token.sem.role("agent").unwrap().source.sign,
        "alpha"
    );
    assert!(derivation
        .cases
        .iter()
        .any(|record| record.status == conlang_language::CaseBranchStatus::Matched));
}

#[test]
fn feature_case_without_branch_or_base_reports_the_typed_default_error() {
    let language = Language::parse(
        r#"Symbol x

trait Unit:

sign x:
    belongs Unit
    phon:
        /x/

sign MissingDefault:
    syn:
        slots:
            value [Unit]
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
        /{$slot.value}/
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let error = system
        .derive_with_context(
            "MissingDefault",
            &[SlotFiller::sign("value", "x")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "trigger", "off"),
        )
        .unwrap_err();
    assert!(matches!(error, SystemError::CaseDefaultMissing { .. }));
}

#[test]
fn public_derive_executes_sign_level_case_and_nested_sign_rules() {
    let language = Language::parse(
        r#"Symbol p
Symbol r
Symbol e
Symbol x
Class vowel {e, x}

trait Unit:

trait Wrapped:

sign x:
    belongs Unit
    phon:
        /x/

sign Prefix:
    belongs Wrapped
    syn:
        slots:
            base [*]
        feature:
            apply = enum(no, yes)
            committed = enum(no, yes)
            committed => yes
    phon:
        /pre {$slot.base}/

sign Root:
    syn:
        slots:
            value [Unit]
        feature:
            apply = enum(no, yes)
    phon:
        /{$slot.value}/
    case:
        $self.syn.apply == yes:
            Prefix({$self})
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let derivation = system
        .derive_with_context(
            "Root",
            &[SlotFiller::sign("value", "x")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "apply", "yes"),
        )
        .unwrap();
    assert_eq!(derivation.surface, "pre x");
    assert_eq!(
        derivation
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.apply")
            .map(|(_, value)| value.as_str()),
        Some("yes"),
        "a Sign application must preserve its derivation constraints"
    );
    assert_eq!(
        derivation
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.committed")
            .map(|(_, value)| value.as_str()),
        Some("yes")
    );
    assert!(derivation
        .token
        .syn_categories
        .iter()
        .any(|category| category == "Wrapped"));
    assert_eq!(
        derivation
            .cases
            .iter()
            .filter(|record| record.status == conlang_language::CaseBranchStatus::Matched)
            .count(),
        1
    );
}

#[test]
fn feature_case_rejects_a_sign_valued_branch_during_compile() {
    let language = Language::parse(
        r#"sign Helper:
    phon:
        /h/

sign Invalid:
    syn:
        feature:
            value = enum(one, two)
            value =>
                case:
                    else:
                        Helper()
    phon:
        /x/
"#,
    )
    .unwrap();
    let CompileSystemError::Validation(report) = compile_system(language).unwrap_err() else {
        panic!("a Sign-valued feature branch must fail static type checking")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "CASE_BRANCH_TYPE_MISMATCH"));
}

/// P71-C:typed feature 支援 syn/sem/**prag**;`phon` 仍不支援。
///
/// prag 於 P71-C 開放,是因為 R2 要求自造欄位一律先宣告值域,而 prag 原本沒有
/// 這個出口。`phon` 維持不支援——其內容是 UR/模板與 DSL 音變規則,不是 enum 欄位。
#[test]
fn typed_features_are_supported_in_prag_but_not_phon() {
    let decl = |dim| {
        vec![SignItem::FeatureDecl(FeatureDecl {
            dim,
            name: "register".to_owned(),
            values: vec!["formal".to_owned(), "informal".to_owned()],
            source: SourceLocation::unknown(),
        })]
    };

    // phon:仍是 FEATURE_DIMENSION_UNSUPPORTED
    let mut language = Language::new();
    language.add_sign("invalid_phon_feature", decl(Dim::Phon));
    let CompileSystemError::Validation(report) = compile_system(language).unwrap_err() else {
        panic!("phon 仍是不支援的 feature 維度")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "FEATURE_DIMENSION_UNSUPPORTED"));

    // prag:正向控制組——必須通過,且不得留下該診斷碼
    let mut language = Language::new();
    language.add_sign("valid_prag_feature", decl(Dim::Prag));
    let compiled = compile_system(language).expect("prag typed feature 於 P71-C 後合法");
    assert!(
        !compiled
            .validation
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "FEATURE_DIMENSION_UNSUPPORTED"),
        "prag 不該再有維度不支援診斷"
    );
}

#[test]
fn nested_case_leaves_are_statically_type_checked() {
    let language = Language::parse(
        r#"sign invalid:
    syn:
        feature:
            trigger = enum(on, off)
            result = enum(yes, no)
            result =>
                case:
                    $self.syn.trigger == on:
                        case:
                            else:
                                outside_domain
                    else:
                        no
    phon:
        /x/
"#,
    )
    .unwrap();
    let CompileSystemError::Validation(report) = compile_system(language).unwrap_err() else {
        panic!("a nested enum leaf outside its declared domain must fail at compile time")
    };
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code == "FEATURE_EXPRESSION_VALUE_OUT_OF_DOMAIN"));
}

#[test]
fn nested_cases_round_trip_and_execute_in_feature_role_and_phon_positions() {
    let language = Language::parse(
        r#"Symbol x
Class vowel {x}

trait NestedEntity:
    belongs Semantic

trait NestedFrame:
    belongs SemanticFrame

trait NestedUnit:
    belongs NestedEntity

sign x:
    belongs NestedUnit
    phon:
        /x/

sign NestedChoice:
    belongs NestedFrame
    syn:
        slots:
            value [NestedUnit]
        feature:
            trigger = enum(on, off)
    sem:
        feature:
            outcome = enum(yes, no)
            outcome =>
                case:
                    $self.syn.trigger == on:
                        case:
                            $self.syn.trigger == on:
                                yes
                            else:
                                no
                    else:
                        no
        roles:
            agent [NestedEntity]
            agent =
                case:
                    $self.syn.trigger == on:
                        case:
                            else:
                                {$slot.value}
                    else:
                        {$slot.value}
    phon:
        /{$slot.value}/
        realization:
            case:
                $self.syn.trigger == on:
                    case:
                        $self.syn.trigger == on:
                            /x {$slot.value}/
                        else:
                            /never/
                else:
                    /{$slot.value}/
"#,
    )
    .unwrap();
    let canonical = language.dump();
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
    assert!(!canonical.contains("<nested-case>"));

    let system = compile_system(language).unwrap();
    let derivation = system
        .derive_with_context(
            "NestedChoice",
            &[SlotFiller::sign("value", "x")],
            &SlotMap::identity(),
            DerivationContext::new().feature(Dim::Syn, "trigger", "on"),
        )
        .unwrap();
    assert_eq!(derivation.surface, "x x");
    assert_eq!(
        derivation
            .token
            .sem
            .features
            .get("outcome")
            .map(String::as_str),
        Some("yes")
    );
    assert_eq!(derivation.token.sem.role("agent").unwrap().source.sign, "x");
    assert!(
        derivation.cases.len() >= 6,
        "nested branch trace is retained"
    );
    assert!(derivation.realization.cases.len() >= 2);
}

#[test]
fn nested_sign_case_returns_the_inner_sign_expression() {
    let language = Language::parse(
        r#"Symbol w
Symbol r
Symbol a
Symbol p
Symbol e
Symbol d
Symbol x
Class vowel {a, e, x}

trait NestedAtom:

sign x:
    belongs NestedAtom
    phon:
        /x/

sign Wrapper:
    syn:
        slots:
            base [*]
    phon:
        /wrapped {$slot.base}/

sign Root:
    syn:
        slots:
            value [NestedAtom]
    phon:
        /{$slot.value}/
    case:
        else:
            case:
                else:
                    Wrapper({$self})
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let derivation = system
        .derive(
            "Root",
            &[SlotFiller::sign("value", "x")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(derivation.surface, "wrapped x");
    assert!(derivation.cases.len() >= 2);
    assert!(derivation
        .occurrences
        .iter()
        .any(|occurrence| occurrence.slot_path == "base"));
}

#[test]
fn stored_filler_runs_the_same_feature_case_pipeline_before_composition() {
    let language = Language::parse(
        r#"Symbol m
Class vowel {m}

trait CaseUnit:
    belongs Semantic

sign marked:
    belongs CaseUnit
    syn:
        feature:
            trigger = enum(on, off)
            trigger = on
    sem:
        feature:
            outcome = enum(yes, no)
            outcome =>
                case:
                    $self.syn.trigger == on:
                        yes
                    else:
                        no
    phon:
        /m/

sign Holder:
    syn:
        slots:
            item [CaseUnit]
    phon:
        /{$slot.item}/
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let derivation = system
        .derive(
            "Holder",
            &[SlotFiller::sign("item", "marked")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(
        derivation.token.fillers[0].sem.features.get("outcome"),
        Some(&"yes".to_owned())
    );
}

#[test]
fn sign_case_membership_materializes_the_complete_trait_contract() {
    let language = Language::parse(
        r#"Symbol s
Symbol a
Class vowel {a}

trait ContractEntity:
    belongs Semantic

trait ContractAtom:
    belongs ContractEntity

trait EnrichedContract:
    belongs SemanticFrame
    syn:
        slots:
            adjunct [ContractAtom]
        tam.present = yes
        feature:
            committed = enum(no, yes)
            committed => yes
    sem:
        roles:
            theme [ContractEntity]
            theme = {$slot.adjunct}
    phon:
        realization:
            case:
                $self.syn.committed == yes:
                    /{$slot.adjunct}/

sign seed:
    belongs ContractAtom
    phon:
        /s/

sign adjunct:
    belongs ContractAtom
    phon:
        /a/

sign Wrapper:
    syn:
        slots:
            stem [ContractAtom]
    phon:
        /{$slot.stem}/

sign stored_root:
    belongs ContractAtom
    phon:
        /s/
    case:
        else:
            $self
            belongs EnrichedContract

sign root:
    belongs ContractAtom
    phon:
        /s/
    case:
        else:
            Wrapper({$self})
            belongs EnrichedContract
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let stored = system
        .evaluate_sign_expression("stored_root", &DerivationContext::new())
        .unwrap()
        .value;
    let SignValue::Stored(stored) = &stored else {
        panic!("a self branch must remain a stored Sign")
    };
    assert_eq!(
        stored
            .sign
            .project(Dim::Syn, &system.ontology)
            .defs
            .iter()
            .find(|(path, _)| path == "syn.tam.present")
            .map(|(_, value)| value.as_str()),
        Some("yes")
    );
    assert_eq!(
        stored
            .sign
            .project(Dim::Syn, &system.ontology)
            .defs
            .iter()
            .find(|(path, _)| path == "syn.committed")
            .map(|(_, value)| value.as_str()),
        Some("yes")
    );
    assert_eq!(
        stored.sign.id,
        system.language().sign_named("stored_root").unwrap().id
    );
    assert_eq!(
        stored
            .sign
            .items
            .iter()
            .filter(|item| matches!(item, conlang_language::SignItem::RoleDecl(_)))
            .count(),
        1
    );
    assert_eq!(
        stored
            .sign
            .items
            .iter()
            .filter(|item| matches!(item, conlang_language::SignItem::Realization(_)))
            .count(),
        1
    );
    assert_eq!(
        conlang_language::construction::parameters_of(&stored.sign)
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        ["adjunct"]
    );

    let value = system
        .evaluate_sign_expression("root", &DerivationContext::new())
        .unwrap()
        .value;
    let SignValue::Applied(partial) = &value else {
        panic!("the case branch must return the refined applied Sign")
    };
    let wrapper_id = system.language().sign_named("Wrapper").unwrap().id.clone();
    assert_eq!(partial.construction_id, wrapper_id);
    assert_eq!(
        partial
            .syn
            .iter()
            .find(|(path, _)| path == "syn.tam.present")
            .map(|(_, value)| value.as_str()),
        Some("yes"),
        "the inherited Syn Def must enter the rebuilt deep state"
    );
    assert_eq!(
        partial
            .syn
            .iter()
            .find(|(path, _)| path == "syn.committed")
            .map(|(_, value)| value.as_str()),
        Some("yes"),
        "the inherited rule must execute exactly on the rebuilt baseline"
    );
    assert_eq!(
        value
            .residual_parameters()
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        ["adjunct"],
        "a required slot introduced by branch membership remains a Sign variable"
    );
    assert!(partial
        .sem
        .types
        .iter()
        .any(|category| category == "EnrichedContract"));

    let completed = system
        .apply_arguments(&value, &[SlotFiller::sign("adjunct", "adjunct")])
        .unwrap();
    let SignValue::Applied(completed) = completed else {
        panic!("supplying the inherited slot must saturate the same Sign")
    };
    assert_eq!(completed.construction_id, wrapper_id);
    assert_eq!(completed.provenance.construction, "Wrapper");
    assert!(completed.is_saturated());
    assert_eq!(
        completed.sem.role("theme").unwrap().source.sign,
        "adjunct",
        "the inherited role schema and binding must execute after saturation"
    );
    assert_eq!(
        system.realize_phon(&completed).unwrap().input.as_str(),
        "a",
        "the inherited realization must remain executable after resume"
    );
}
