//! Public-path sealing tests for the complete P38–P44 runtime.

use conlang_language::construction::{CxgError, FillerProvenance, SlotFiller, SlotMap};
use conlang_language::synchronic::{self, RuleStatus};
use conlang_language::system::{CompileSystemError, SystemError};
use conlang_language::{compile_system, construction, ontology, Dim, Language, Severity};
use tshiatun_core::repr::word::MorphUnit;

const SYSTEM: &str = r#"Symbol d
Symbol o
Symbol g
Symbol r
Symbol u
Symbol n
Symbol x

Class vowel {o, u}

sign dog:
    belongs Noun
    sem:
        senses:
            core = DOG
        feature:
            sense_register = enum(neutral, elevated)
            sense_register => elevated
    phon:
        /dog/

sign run:
    belongs Verb
    sem:
        senses:
            core = RUN
    phon:
        /run/

sign Clause:
    belongs Verb
    syn:
        slots:
            subject [Noun]
            predicate [Verb]
        feature:
            state = enum(raw, ready)
            state = raw
            state => ready
    sem:
        roles:
            actor [Semantic]
            action [Semantic]?
            actor = {subject}
            action = {predicate}
        feature:
            frame = enum(event, occurrence)
            frame = event
            frame => occurrence / frame == event
    prag:
        feature:
            register = enum(neutral, formal)
            register => neutral
    phon:
        /{subject}{predicate}/
        o => u

sign Wrapper:
    belongs Verb
    syn:
        slots:
            clause [Verb]
    sem:
        roles:
            content [Semantic]
            content = {clause}
    phon:
        /x{clause}/
"#;

#[test]
fn public_runtime_connects_fillers_token_rules_and_phon_surface() {
    let system = compile_system(Language::parse(SYSTEM).unwrap()).unwrap();
    let fillers = [
        SlotFiller::sign("subject", "dog"),
        SlotFiller::sign("predicate", "run"),
    ];
    let first = system
        .derive("Clause", &fillers, &SlotMap::identity())
        .unwrap();
    let second = system
        .derive("Clause", &fillers, &SlotMap::identity())
        .unwrap();

    assert_eq!(
        first.surface, "dugrun",
        "local phon rule ran through Tshiatūn"
    );
    assert_eq!(first.token.phon_form().unwrap(), "dogrun");
    assert_eq!(
        first
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.state")
            .map(|(_, value)| value.as_str()),
        Some("ready")
    );
    assert_eq!(first.token.sem.field("frame"), Some("occurrence"));
    assert_eq!(
        first
            .token
            .sem
            .role("actor")
            .unwrap()
            .field("sense_register"),
        Some("elevated"),
        "filler rules run before recursive semantic composition"
    );
    assert_eq!(
        first
            .token
            .prag
            .iter()
            .find(|(path, _)| path == "prag.register")
            .map(|(_, value)| value.as_str()),
        Some("neutral")
    );
    assert!(first.rules.iter().all(|entry| entry.record.source.line > 0));
    assert!(first.diagnostics.is_empty());
    assert_eq!(first.surface, second.surface);
    assert_eq!(format!("{:?}", first.token), format!("{:?}", second.token));
    assert_eq!(format!("{:?}", first.rules), format!("{:?}", second.rules));
}

#[test]
fn public_surface_renders_structural_word_boundaries_as_spaces() {
    let source = r#"Symbol a
Symbol b
Symbol p
Class vowel {a}

sign left:
    phon:
        /ap/

sign right:
    phon:
        /ba/

sign Phrase:
    syn:
        slots:
            left [*]
            right [*]
    phon:
        /{left} {right}/
        p => b / _ ## b @stage phrase
"#;
    let system = compile_system(Language::parse(source).unwrap()).unwrap();
    let derivation = system
        .derive(
            "Phrase",
            &[
                SlotFiller::sign("left", "left"),
                SlotFiller::sign("right", "right"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();

    assert_eq!(derivation.realization.input.as_str(), "ap ba");
    assert_eq!(derivation.token.phon_form().unwrap(), "ap ba");
    assert_eq!(derivation.surface, "ab ba");
    let output = &derivation.phon_steps.last().expect("local phon rule").word;
    assert_eq!(
        output
            .morph
            .iter()
            .filter(|bracket| bracket.unit == MorphUnit::Word)
            .count(),
        2,
        "spaces in the phon template must become structural word brackets"
    );
}

#[test]
fn derived_token_can_fill_another_construction() {
    let system = compile_system(Language::parse(SYSTEM).unwrap()).unwrap();
    let clause = system
        .apply_construction(
            "Clause",
            &[
                SlotFiller::sign("subject", "dog"),
                SlotFiller::sign("predicate", "run"),
            ],
            &SlotMap::identity(),
        )
        .unwrap();
    let wrapped = system
        .apply_construction(
            "Wrapper",
            &[SlotFiller::token("clause", &clause)],
            &SlotMap::identity(),
        )
        .unwrap();

    assert_eq!(wrapped.phon_form().unwrap(), "xdogrun");
    assert_eq!(
        wrapped
            .sem
            .role("content")
            .unwrap()
            .role("actor")
            .unwrap()
            .field("sense_register"),
        Some("elevated")
    );
    assert!(matches!(
        wrapped.provenance.fillers[0].source,
        FillerProvenance::Derived(_)
    ));
}

#[test]
fn slot_map_supports_all_operations_and_validates_atomically() {
    let system = compile_system(Language::parse(SYSTEM).unwrap()).unwrap();
    let renamed = SlotMap::identity()
        .preserve("predicate")
        .rename("subject", "actor")
        .optional("predicate", true);
    let token = system
        .apply_construction("Clause", &[SlotFiller::sign("actor", "dog")], &renamed)
        .unwrap();
    assert!(
        token.is_saturated(),
        "unfilled predicate was overridden optional"
    );
    assert_eq!(token.phon_form().unwrap(), "dog");

    let internal = SlotMap::identity()
        .autofill("subject", "dog")
        .internalize("subject");
    let token = system
        .apply_construction("Clause", &[SlotFiller::sign("predicate", "run")], &internal)
        .unwrap();
    assert_eq!(token.phon_form().unwrap(), "dogrun");
    assert!(token
        .residual_slots()
        .iter()
        .all(|slot| slot.name != "subject"));

    let err = system
        .apply_construction("Clause", &[], &SlotMap::identity().internalize("subject"))
        .unwrap_err();
    assert!(matches!(
        err,
        SystemError::Construction(CxgError::InternalRequiredUnfilled(ref slot)) if slot == "subject"
    ));
    let err = system
        .apply_construction("Clause", &[], &SlotMap::identity().rename("ghost", "x"))
        .unwrap_err();
    assert!(matches!(
        err,
        SystemError::Construction(CxgError::SlotMapUnknown(ref slot)) if slot == "ghost"
    ));
    let err = system
        .apply_construction(
            "Clause",
            &[],
            &SlotMap::identity()
                .preserve("subject")
                .rename("subject", "actor"),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        SystemError::Construction(CxgError::SlotMapDuplicate { ref slot, .. }) if slot == "subject"
    ));
}

#[test]
fn validation_report_grades_resolved_conflicts_and_blocks_slot_conflicts() {
    // P71 §4.2:裸自造 Def 已不合法,故本例改用**封閉清單上**的套件座標。
    // 這條測的是 Def 衝突解析與 winner provenance,與路徑取哪一條無關。
    let warning_source = r#"trait Earlier:
    syn:
        tam.present = 0
trait Later:
    syn:
        tam.present = 1
sign s:
    belongs Earlier
    belongs Later
"#;
    let system = compile_system(Language::parse(warning_source).unwrap()).unwrap();
    let conflict = system
        .validation
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.code == "ONTOLOGY_DEF_CONFLICT_RESOLVED"
                && diagnostic
                    .sources
                    .iter()
                    .any(|source| source.path.as_deref() == Some("syn.tam.present"))
        })
        .unwrap();
    assert_eq!(conflict.severity, Severity::Warning);
    assert_eq!(
        conflict.sources[0].owner, "Later",
        "winner provenance first"
    );
    assert_eq!(
        system
            .evaluate_sign("s")
            .unwrap()
            .sign
            .project(Dim::Syn, &system.ontology)
            .get("syn.tam.present"),
        Some("1")
    );

    let invalid = r#"trait Left:
    syn:
        slots:
            arg [Noun]
trait Right:
    syn:
        slots:
            arg [Verb]
sign Bad:
    belongs Left
    belongs Right
"#;
    let error = compile_system(Language::parse(invalid).unwrap()).unwrap_err();
    let CompileSystemError::Validation(report) = error else {
        panic!("expected structured validation")
    };
    assert!(report
        .errors()
        .any(|diagnostic| diagnostic.code == "SLOT_CONFLICT"));
}

#[test]
fn compile_system_returns_coded_errors_for_duplicate_names_and_bad_guards() {
    let duplicate = Language::parse("sign same:\nsign same:\n").unwrap();
    let CompileSystemError::Validation(report) = compile_system(duplicate).unwrap_err() else {
        panic!("expected validation report")
    };
    assert!(report
        .errors()
        .any(|diagnostic| diagnostic.code == "SIGN_DUPLICATE"));

    let bad_guard = Language::parse(
        "sign s:\n    syn:\n        feature:\n            value = enum(x, y)\n            value => x / [Ghost]\n                else value => y\n",
    )
    .unwrap();
    let CompileSystemError::Validation(report) = compile_system(bad_guard).unwrap_err() else {
        panic!("expected validation report")
    };
    assert!(
        report.errors().any(|diagnostic| {
            diagnostic.code == "RULE_INVALID" && diagnostic.sources[0].location.line == 5
        }),
        "{:?}",
        report.errors().collect::<Vec<_>>()
    );
}

#[test]
fn phon_source_map_points_back_to_main_and_then_else_lines() {
    let source = "Symbol a\nSymbol b\nSymbol c\n\n\
                  global trait G:\n    phon:\n        a => b\n            then b => c\n";
    let language = Language::parse(source).unwrap();
    let artifacts = conlang_language::codegen::compile_full(&language).unwrap();
    let map = &artifacts.grammar.source_map;
    assert!(map
        .iter()
        .any(|entry| entry.branch == 0 && entry.source.line == 7));
    assert!(map
        .iter()
        .any(|entry| entry.branch == 1 && entry.source.line == 8));
}

#[test]
fn token_rule_units_distinguish_matched_unmatched_and_error() {
    let source = r#"Symbol a
Class vowel {a}
trait Known:
sign atom:
    belongs Noun
    phon:
        /a/
sign C:
    belongs Verb
    syn:
        slots:
            item [Noun]
        feature:
            state = enum(same)
            missed = enum(no, yes)
            broken = enum(no, yes)
            state = same
            state => same / [Verb]
            missed => no / [Known]
            broken => no / [Ghost]
    phon:
        /{item}/
"#;
    let language = Language::parse(source).unwrap();
    let (registry, ontology_diags) = ontology::with_std(&language);
    assert!(ontology_diags.is_empty());
    let token = construction::apply(&language, &registry, "C", &[("item", "atom")]).unwrap();
    let (token, records) = synchronic::run_token_dim_rules(&token, Dim::Syn, &registry);

    assert_eq!(records[0].status, RuleStatus::Matched);
    assert!(!records[0].changed, "identity still counts as Matched");
    assert_eq!(records[1].status, RuleStatus::Unmatched);
    assert_eq!(records[2].status, RuleStatus::Error);
    assert!(records[2].diag.as_deref().unwrap().contains("Ghost"));
    assert!(token
        .syn
        .iter()
        .all(|(path, _)| path != "syn.missed" && path != "syn.broken"));
}

#[test]
fn inherited_rules_are_diamond_deduplicated_and_keep_source_order() {
    let source = r#"trait Root:
    syn:
        feature:
            root = enum(yes)
            root => yes
trait Left:
    belongs Root
    syn:
        feature:
            left = enum(yes)
            left => yes / root == yes
trait Right:
    belongs Root
    syn:
        feature:
            right = enum(yes)
            right => yes / left == yes
sign s:
    belongs Left
    belongs Right
    syn:
        feature:
            local = enum(yes)
            local => yes / right == yes
"#;
    let system = compile_system(Language::parse(source).unwrap()).unwrap();
    let evaluated = system.evaluate_sign("s").unwrap();
    let syn = evaluated.sign.project(Dim::Syn, &system.ontology);
    for path in ["syn.root", "syn.left", "syn.right", "syn.local"] {
        assert_eq!(syn.get(path), Some("yes"), "{path}");
    }
    assert_eq!(evaluated.records.len(), 4, "diamond Root rule runs once");
    let lines: Vec<_> = evaluated
        .records
        .iter()
        .map(|record| record.source.line)
        .collect();
    assert_eq!(lines, vec![5, 11, 17, 24], "遠祖→近祖→本地的來源順序");
}

#[test]
fn inherited_slots_and_nearer_defaults_feed_construction_application() {
    let source = r#"Symbol a
Class vowel {a}
trait Base:
    syn:
        feature:
            level = enum(base, near)
            level = base
        slots:
            item [Noun]
trait Near:
    belongs Base
    syn:
        feature:
            level = enum(base, near)
            level = near
sign atom:
    belongs Noun
    phon:
        /a/
sign C:
    belongs Near
    phon:
        /{item}/
"#;
    let system = compile_system(Language::parse(source).unwrap()).unwrap();
    let token = system
        .apply_construction(
            "C",
            &[SlotFiller::sign("item", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(token.phon_form().unwrap(), "a");
    assert_eq!(
        token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.level")
            .map(|(_, value)| value.as_str()),
        Some("near")
    );
}

#[test]
fn local_rule_may_read_a_slot_inherited_from_its_trait_parent() {
    let source = r#"Symbol a
Class vowel {a}
trait BaseSlots:
    syn:
        slots:
            item [Noun]
trait ReadingChild:
    belongs BaseSlots
    syn:
        feature:
            observed = enum(yes)
            observed => yes / $slot.item == [Noun]
sign atom:
    belongs Noun
    phon:
        /a/
sign C:
    belongs ReadingChild
    phon:
        /{item}/
"#;
    let system = compile_system(Language::parse(source).unwrap()).unwrap();
    let derived = system
        .derive(
            "C",
            &[SlotFiller::sign("item", "atom")],
            &SlotMap::identity(),
        )
        .unwrap();
    assert_eq!(
        derived
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.observed")
            .map(|(_, value)| value.as_str()),
        Some("yes")
    );
}
