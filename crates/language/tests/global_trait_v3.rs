//! V3 global-trait contract: exactly one selected template is applied to every
//! Sign at the lowest inheritance priority.  Global phon rules remain grammar
//! rules and must not be replayed as per-Sign cophonology.

use conlang_language::ontology::OntologyRegistry;
use conlang_language::{codegen, compile};
use conlang_language::{compile_system, Dim, Language, SignItem};

#[test]
fn global_content_is_automatic_and_has_the_lowest_value_priority() {
    let language = Language::parse(
        r#"global trait Defaults:
    syn:
        slots:
            context [*]?
        feature:
            mode = enum(base, specific, local)
            mode = base

trait Specific:
    syn:
        feature:
            mode = specific

sign bare:

sign inherited:
    belongs Specific

sign local:
    belongs Specific
    syn:
        feature:
            mode = local
"#,
    )
    .expect("V3 global sample parses");
    let (registry, diagnostics) = OntologyRegistry::build(&[&language]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(registry.global_trait_name(), Some("Defaults"));

    let bare = language.sign_named("bare").unwrap();
    assert_eq!(
        bare.project(Dim::Syn, &registry).get("syn.mode"),
        Some("base"),
        "a Sign without `belongs` still receives the global default"
    );
    assert!(registry
        .sign_categories(bare)
        .contains(&"Defaults".to_owned()));
    assert!(registry.effective_sign(bare).items.iter().any(
        |item| matches!(item, SignItem::Slot(slot) if slot.name == "context" && slot.optional)
    ));

    let inherited = language.sign_named("inherited").unwrap();
    assert_eq!(
        inherited.project(Dim::Syn, &registry).get("syn.mode"),
        Some("specific"),
        "a specifically mounted trait overrides the global package"
    );
    assert_eq!(
        registry.sign_categories(inherited),
        vec!["Specific", "Defaults"],
        "the explicit category stays first while the automatic category remains visible"
    );

    let local = language.sign_named("local").unwrap();
    assert_eq!(
        local.project(Dim::Syn, &registry).get("syn.mode"),
        Some("local"),
        "a local Sign value remains the highest priority"
    );
}

#[test]
fn global_syn_behavior_runs_per_sign_while_global_phon_runs_only_in_the_grammar() {
    let language = Language::parse(
        r#"Symbol a
Symbol b

Class vowel {a, b}

global trait Defaults:
    syn:
        feature:
            state = enum(base, changed)
            state = base
            state => changed / state == base
    phon:
        a => b

sign x:
    phon:
        /a/
"#,
    )
    .expect("V3 executable global sample parses");
    let system = compile_system(language).expect("global content compiles");
    let source = system.effective_language().sign_named("x").unwrap();
    let effective = system.ontology.effective_sign(source);
    let dimensions = effective
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::Rule(rule) | SignItem::FeatureRule(rule) => Some(rule.dim),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        dimensions,
        vec![Dim::Syn],
        "the global phon rule must not be duplicated as local cophonology"
    );

    assert_eq!(
        system
            .artifacts
            .grammar
            .phon_source
            .matches("a => b")
            .count(),
        1,
        "the global phon rule enters Compiled Grammar exactly once"
    );
    let evaluated = system.evaluate_sign("x").expect("evaluate stored Sign");
    assert_eq!(
        evaluated
            .sign
            .project(Dim::Syn, &system.ontology)
            .get("syn.state"),
        Some("changed"),
        "the non-phon global rule ran through the Sign pipeline"
    );
}

#[test]
fn a_specific_trait_rule_can_read_a_slot_supplied_by_the_global_template() {
    let language = Language::parse(
        r#"Symbol x

global trait Defaults:
    syn:
        slots:
            context [*]?
        feature:
            state = enum(base, changed)
            state = base

trait Specific:
    syn:
        feature:
            state => changed / $slot.context == [Noun]

sign x:
    belongs Specific
    phon:
        /x/
"#,
    )
    .expect("cross-layer global slot sample parses");
    compile_system(language).expect("specific behavior sees the automatic global contract");
}

#[test]
fn inherited_global_phonology_is_compiled_once_as_grammar_behavior() {
    let language = Language::parse(
        r#"Symbol a
Symbol b

trait ParentPhonology:
    phon:
        a => b

global trait Defaults:
    belongs ParentPhonology
    ParentPhonology

sign x:
    phon:
        /a/
"#,
    )
    .expect("global inheritance sample parses");
    let pipeline = compile::compile(&language).expect("low-level pipeline compiles");
    let (low_level_grammar, _) =
        codegen::codegen(&pipeline.ordered).expect("low-level codegen expands the same chain");
    assert_eq!(
        low_level_grammar.phon_source.matches("a => b").count(),
        1,
        "belongs projection and its explicit expansion point denote one inherited RuleId"
    );

    let system = compile_system(language).expect("global inheritance compiles");
    assert_eq!(
        system
            .artifacts
            .grammar
            .phon_source
            .matches("a => b")
            .count(),
        1
    );

    let source = system.effective_language().sign_named("x").unwrap();
    let effective = system.ontology.effective_sign(source);
    assert!(effective.items.iter().all(|item| {
        !matches!(item, SignItem::Rule(rule) | SignItem::FeatureRule(rule) if rule.dim == Dim::Phon)
    }));
}
