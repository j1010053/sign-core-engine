//! Functional evidence for embedded `lib/std/{core,grambank,cxg}` packages.

use std::collections::BTreeSet;

use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::ontology::{self, OntologyRegistry};
use conlang_language::stdlib::{self, StdLoadError};
use conlang_language::synchronic::RuleStatus;
use conlang_language::{compile_system, Dim, Language, SignItem};

#[derive(Debug)]
struct FeatureRow<'a> {
    id: &'a str,
    question: &'a str,
    values: &'a str,
    root: &'a str,
    present: &'a str,
    absent: &'a str,
    source: &'a str,
}

fn feature_rows(data: &str) -> Vec<FeatureRow<'_>> {
    let mut lines = data.lines();
    assert_eq!(
        lines.next(),
        Some("id\tquestion\tdomain\tvalues\troot_trait\tpresent_trait\tabsent_trait\tsource")
    );
    lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 8, "bad feature row: {line:?}");
            FeatureRow {
                id: fields[0],
                question: fields[1],
                values: fields[3],
                root: fields[4],
                present: fields[5],
                absent: fields[6],
                source: fields[7],
            }
        })
        .collect()
}

fn trait_value<'a>(language: &'a Language, name: &str, path: &str) -> Option<&'a str> {
    language
        .trait_named(name)?
        .blocks
        .iter()
        .flat_map(|block| &block.items)
        .find_map(|item| match item {
            SignItem::Def(def) if def.path == path => Some(def.value.as_str()),
            _ => None,
        })
}

#[test]
fn packages_exports_and_combined_ontology_are_deterministic() {
    let first = stdlib::packages().expect("embedded packages validate");
    let second = stdlib::packages().expect("second load validates");
    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .map(|package| (package.name.as_str(), package.priority))
            .collect::<Vec<_>>(),
        [
            ("core", 0),
            ("grammaticalization", 0),
            ("grambank", 10),
            ("cxg", 20)
        ]
    );
    assert!(first.iter().all(|package| package.enabled));
    assert_eq!(
        first
            .iter()
            .map(|package| package.rule_namespace.as_str())
            .collect::<Vec<_>>(),
        [
            "std:core",
            "std:grammaticalization",
            "std:grambank",
            "std:cxg"
        ]
    );
    // 依**名字**查,不用位置索引——加新套件不該弄壞這些斷言。
    let by_name = |namespace: &str| {
        first
            .iter()
            .find(|package| package.rule_namespace == namespace)
            .unwrap_or_else(|| panic!("missing {namespace}"))
    };
    assert_eq!(by_name("std:core").exports.len(), 30);
    assert_eq!(by_name("std:grambank").exports.len(), 76);
    assert_eq!(by_name("std:cxg").exports.len(), 27);
    // P52 路徑庫:一個參數化 Recipe 加功能 Goal；路徑與權重留在 data。
    let grammaticalization = by_name("std:grammaticalization");
    assert_eq!(
        grammaticalization
            .exports
            .iter()
            .map(|export| export.alias.as_str())
            .collect::<Vec<_>>(),
        ["VerbToTense", "Future", "Perfect"]
    );
    assert_eq!(
        grammaticalization.data_paths,
        ["data/paths.tsv", "data/weights.tsv"]
    );
    assert_eq!(
        by_name("std:cxg").code_paths,
        ["code/schema.lang", "code/realizations.lang"]
    );
    assert_eq!(
        by_name("std:cxg")
            .requires
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["std:core"]
    );

    let stable_ids: BTreeSet<_> = first
        .iter()
        .flat_map(|package| package.exports.iter())
        .map(|export| export.stable_id.as_str())
        .collect();
    let aliases: BTreeSet<_> = first
        .iter()
        .flat_map(|package| package.exports.iter())
        .map(|export| export.alias.as_str())
        .collect();
    // Step 17 adds the Future and Perfect Goal exports to the prior 134.
    assert_eq!(stable_ids.len(), 136);
    assert_eq!(aliases.len(), 136);

    for alias in [
        "Semantic",
        "SemanticFrame",
        "EventFrame",
        "Relation",
        "AgreementBearer",
        "TransferFrame",
    ] {
        let export = stdlib::resolve_export(alias).expect("new core primitive is exported");
        assert_eq!(export.package, "core");
        assert_eq!(export.stable_id, format!("std:core:{alias}"));
    }

    let present = stdlib::resolve_export("GB107_Present").unwrap();
    assert_eq!(present.stable_id, "std:grambank:GB107:1");
    assert_eq!(present.package, "grambank");
    assert!(matches!(
        stdlib::resolve_export("GB999_Typo"),
        Err(StdLoadError::UnknownAlias { .. })
    ));

    let combined_a = stdlib::load_default().unwrap();
    let combined_b = stdlib::load_default().unwrap();
    assert_eq!(combined_a.dump(), combined_b.dump());
    assert_eq!(
        Language::parse(&combined_a.dump()).unwrap().dump(),
        combined_a.dump()
    );
    assert_eq!(combined_a.traits.len(), 133);
    for core_semantic_trait in [
        "Semantic",
        "SemanticFrame",
        "EventFrame",
        "Relation",
        "AgreementBearer",
        "TransferFrame",
    ] {
        assert!(
            combined_a.trait_named(core_semantic_trait).is_some(),
            "missing exported core ontology trait {core_semantic_trait}"
        );
    }
    let (_registry, diagnostics) = OntologyRegistry::build(&[&combined_a]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn catalog_has_25_binary_parameters_and_three_resolvable_knowledge_states() {
    let packages = stdlib::packages().unwrap();
    let package = packages
        .iter()
        .find(|package| package.name == "grambank")
        .unwrap();
    let rows = feature_rows(package.data);
    assert_eq!(rows.len(), 25);
    let ids: BTreeSet<_> = rows.iter().map(|row| row.id).collect();
    assert_eq!(ids.len(), 25);

    let std = stdlib::load_default().unwrap();
    for row in rows {
        assert_eq!(row.values, "0|1|?");
        assert!(!row.question.is_empty());
        assert_eq!(
            row.source,
            format!("https://grambank.clld.org/parameters/{}", row.id)
        );
        for alias in [row.root, row.present, row.absent] {
            assert_eq!(stdlib::resolve_export(alias).unwrap().package, "grambank");
            assert!(std.trait_named(alias).is_some(), "missing trait {alias}");
        }
        let path = format!("syn.typology.grambank.{}", row.id);
        assert_eq!(trait_value(&std, row.root, &path), Some("?"));
        assert_eq!(trait_value(&std, row.present, &path), Some("1"));
        assert_eq!(trait_value(&std, row.absent, &path), Some("0"));
    }
}

#[test]
fn missing_unknown_absent_and_present_are_not_collapsed() {
    let language = Language::parse(
        r#"sign unrecorded:
sign unknown:
    belongs GB020_DefiniteOrSpecificArticles
sign absent:
    belongs GB020_Absent
sign present:
    belongs GB020_Present
"#,
    )
    .unwrap();
    let (registry, diagnostics) = ontology::with_std(&language);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let value = |name: &str| {
        language
            .sign_named(name)
            .unwrap()
            .project(Dim::Syn, &registry)
            .get("syn.typology.grambank.GB020")
            .map(str::to_owned)
    };
    assert_eq!(value("unrecorded"), None);
    assert_eq!(value("unknown").as_deref(), Some("?"));
    assert_eq!(value("absent").as_deref(), Some("0"));
    assert_eq!(value("present").as_deref(), Some("1"));
}

#[test]
fn public_runtime_inherits_behavior_and_uses_value_traits_as_guards() {
    let language = Language::parse(
        r#"sign profiled_clause:
    belongs GB020_Present
    belongs GB083_Absent
    belongs GB107_Present
    belongs GB132_Present
    belongs GB322_Present
    belongs GB522_Present
    syn:
        category-guard => matched / [GB107_Present]
        field-guard => matched / typology.grambank.GB107 == 1
    prag:
        evidence-guard => matched / [GB322_Present]
sign near_miss:
    belongs GB107_Absent
    syn:
        category-guard => matched / [GB107_Present]
        field-guard => matched / typology.grambank.GB107 == 1
"#,
    )
    .unwrap();
    let system = compile_system(language).expect("public std path compiles");
    assert!(system.ontology.has("GB107_Present"));
    assert!(!system
        .language()
        .dump()
        .contains("trait GrambankSyntaxFeature"));

    let profiled = system.evaluate_sign("profiled_clause").unwrap();
    let syn = profiled.sign.project(Dim::Syn, &system.ontology);
    let sem = profiled.sign.project(Dim::Sem, &system.ontology);
    let prag = profiled.sign.project(Dim::Prag, &system.ontology);
    assert_eq!(syn.get("syn.typology.grambank.GB020"), Some("1"));
    assert_eq!(syn.get("syn.typology.grambank.GB083"), Some("0"));
    assert_eq!(
        syn.get("syn.negation.standard.bound-verb"),
        Some("available")
    );
    assert_eq!(
        syn.get("syn.word-order.transitive.verb-medial"),
        Some("unmarked")
    );
    assert_eq!(
        syn.get("syn.argument.subject.omission"),
        Some("context-licensed")
    );
    assert_eq!(syn.get("syn.category-guard"), Some("matched"));
    assert_eq!(syn.get("syn.field-guard"), Some("matched"));
    assert_eq!(
        sem.get("sem.reference.identifiability"),
        Some("article-marked")
    );
    assert_eq!(sem.get("sem.time.past"), Some("not-morphologically-marked"));
    assert_eq!(
        prag.get("prag.information-structure.transitive-order"),
        Some("verb-medial-default")
    );
    assert_eq!(
        prag.get("prag.evidence.direct"),
        Some("grammatically-marked")
    );
    assert_eq!(prag.get("prag.evidence-guard"), Some("matched"));
    assert_eq!(profiled.records.len(), 3);
    assert!(profiled
        .records
        .iter()
        .all(|record| record.status == RuleStatus::Matched));

    let near_miss = system.evaluate_sign("near_miss").unwrap();
    let syn = near_miss.sign.project(Dim::Syn, &system.ontology);
    assert_eq!(syn.get("syn.typology.grambank.GB107"), Some("0"));
    assert_eq!(syn.get("syn.category-guard"), None);
    assert_eq!(syn.get("syn.field-guard"), None);
    assert_eq!(near_miss.records.len(), 2);
    assert!(near_miss
        .records
        .iter()
        .all(|record| record.status == RuleStatus::Unmatched));
}

#[test]
fn positive_feature_can_classify_a_real_construction_without_owning_its_rules() {
    let language = Language::parse(
        r#"Symbol q
Symbol a
Class vowel {a}

sign proposition:
    belongs Verb
    phon:
        /a/
sign PolarQuestion:
    belongs GB262_Present
    syn:
        slots:
            clause [Verb]
    phon:
        /q{clause}/
"#,
    )
    .unwrap();
    let system = compile_system(language).unwrap();
    let derivation = system
        .derive(
            "PolarQuestion",
            &[SlotFiller::sign("clause", "proposition")],
            &SlotMap::identity(),
        )
        .unwrap();

    assert_eq!(
        derivation.surface, "qa",
        "the local construction owns order"
    );
    assert_eq!(
        derivation
            .token
            .syn
            .iter()
            .find(|(path, _)| path == "syn.interrogative.polar-particle.initial")
            .map(|(_, value)| value.as_str()),
        Some("available")
    );
    assert_eq!(
        derivation
            .token
            .prag
            .iter()
            .find(|(path, _)| path == "prag.illocution.polar-question")
            .map(|(_, value)| value.as_str()),
        Some("initial-particle-strategy")
    );
}

#[test]
fn local_override_and_conflicting_value_traits_follow_ontology_precedence() {
    let language = Language::parse(
        r#"sign local_override:
    belongs GB107_Present
    syn:
        negation.standard.bound-verb = construction-specific
sign conflict:
    belongs GB020_Present
    belongs GB020_Absent
"#,
    )
    .unwrap();
    let system = compile_system(language).expect("resolved Def conflicts remain warnings");
    let local = system.evaluate_sign("local_override").unwrap();
    assert_eq!(
        local
            .sign
            .project(Dim::Syn, &system.ontology)
            .get("syn.negation.standard.bound-verb"),
        Some("construction-specific")
    );
    let conflict = system.evaluate_sign("conflict").unwrap();
    assert_eq!(
        conflict
            .sign
            .project(Dim::Syn, &system.ontology)
            .get("syn.typology.grambank.GB020"),
        Some("0"),
        "later equal-distance belongs wins"
    );
    let warning = system
        .validation
        .warnings()
        .find(|diagnostic| {
            diagnostic.code == "ONTOLOGY_DEF_CONFLICT_RESOLVED"
                && diagnostic.message.contains("GB020")
                && diagnostic.message.starts_with("conflict ")
        })
        .expect("value conflict must retain provenance");
    assert_eq!(warning.sources[0].owner, "GB020_Absent");
    assert!(warning
        .sources
        .iter()
        .any(|source| source.owner == "GB020_Present"));
}
