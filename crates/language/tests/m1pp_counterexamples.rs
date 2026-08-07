//! M1++ sealing counterexamples for P38–P44.
//!
//! Keep these cases deliberately small: each one isolates a semantic branch
//! that used to compile and pass broad integration tests while producing the
//! wrong runtime state.

use conlang_language::ontology::OntologyRegistry;
use conlang_language::patch::Patch;
use conlang_language::synchronic::{self, RuleStatus};
use conlang_language::{Dim, Language};

#[test]
fn later_same_depth_belongs_wins() {
    let lang = Language::parse(
        "trait Earlier:\n    syn:\n        tam.present = earlier\n\
         trait Later:\n    syn:\n        tam.present = later\n\
         sign s:\n    belongs Earlier\n    belongs Later\n",
    )
    .unwrap();
    let (reg, diags) = OntologyRegistry::build(&[&lang]);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(
        lang.sign_named("s")
            .unwrap()
            .project(Dim::Syn, &reg)
            .get("syn.tam.present"),
        Some("later")
    );
}

#[test]
fn unknown_category_guard_is_error_and_never_falls_into_else() {
    let lang = Language::parse(
        "sign s:\n    syn:\n        feature:\n            value = enum(main, fallback)\n            value => main / [Ghost]\n                else value => fallback\n",
    )
    .unwrap();
    let (reg, _) = OntologyRegistry::build(&[&lang]);
    let sign = lang.sign_named("s").unwrap();
    let (out, records) = synchronic::run_sign_dim_rules(sign, Dim::Syn, &reg);

    assert_eq!(records[0].status, RuleStatus::Error);
    assert_eq!(out.project(Dim::Syn, &reg).get("syn.value"), None);
}

#[test]
fn then_stops_after_error_but_keeps_prior_commits() {
    let lang = Language::parse(
        "sign s:\n    syn:\n        feature:\n            first = enum(committed)\n            late = enum(forbidden)\n            first => committed\n                then malformed branch\n                then late => forbidden\n",
    )
    .unwrap();
    let (reg, _) = OntologyRegistry::build(&[&lang]);
    let sign = lang.sign_named("s").unwrap();
    let (out, records) = synchronic::run_sign_dim_rules(sign, Dim::Syn, &reg);

    assert_eq!(records.len(), 2, "the branch after Error must not run");
    assert_eq!(records[1].status, RuleStatus::Error);
    let syn = out.project(Dim::Syn, &reg);
    assert_eq!(syn.get("syn.first"), Some("committed"));
    assert_eq!(syn.get("syn.late"), None);
}

#[test]
fn patch_set_replaces_all_duplicate_local_defs() {
    let lang = Language::parse(
        "sign s:\n    syn:\n        tam.past = stale-1\n        tam.past = stale-2\n",
    )
    .unwrap();
    let (reg, _) = OntologyRegistry::build(&[&lang]);
    let sign = lang.sign_named("s").unwrap();
    let out = Patch::syn().set("tam.past", "fresh").apply(sign);

    assert_eq!(
        out.project(Dim::Syn, &reg).get("syn.tam.past"),
        Some("fresh")
    );
    assert_eq!(
        out.items
            .iter()
            .filter(
                |item| matches!(item, conlang_language::SignItem::Def(d) if d.path == "syn.tam.past")
            )
            .count(),
        1
    );
}
