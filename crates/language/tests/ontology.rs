//! 步驟 12a 出口(修補07 P38 **v0.2:單一中立分類樹**)+ I22 語法:
//! belongs 閉包 + typed projection + 三類建構診斷。
//!
//! 獨立 oracle:round-trip 恆等(P21)、閉包語意(nearest-first/去重/循環安全)、
//! 三類診斷(未知/循環/重名)、projection 繼承(範疇預設由後代繼承、本地覆蓋)、
//! 維度正交(分類閉包中立、Def 按維過濾)。

use conlang_language::ontology::{self, OntologyDiag, OntologyRegistry};
use conlang_language::{Dim, Language};

fn parse(src: &str) -> Language {
    Language::parse(src).expect("parse")
}

/// 額外引用的 stdlib 本體必須 parse 且 round-trip 恆等(I20;parser 擴充自證)。
#[test]
fn std_ontology_parses_and_round_trips() {
    let l = ontology::std_ontology();
    let dump = l.dump();
    assert_eq!(parse(&dump).dump(), dump, "stdlib ontology.lang 必須 round-trip 恆等");
    let (reg, diags) = OntologyRegistry::build(&[&l]);
    assert!(diags.is_empty(), "stdlib 本體不得有建構診斷:{diags:?}");
    assert!(!reg.names().is_empty(), "分類樹不得為空");
    assert!(reg.has("Verb") && reg.has("Motion") && reg.has("Illocution"));
}

/// belongs 閉包:nearest-first、去重(單一中立樹)。
#[test]
fn belongs_closure_is_nearest_first() {
    let (reg, _) = OntologyRegistry::build(&[&ontology::std_ontology()]);
    assert_eq!(
        reg.closure("Ditransitive"),
        vec!["Ditransitive", "Transitive", "Verb", "Predicate"]
    );
    assert_eq!(reg.closure("Predicate"), vec!["Predicate"]);
    assert_eq!(reg.closure("Human"), vec!["Human", "Animate", "Physical", "Entity"]);
}

/// 使用者可擴充自定範疇,掛回本體某節點(docs/07 §9)。
#[test]
fn user_can_extend_ontology() {
    let user = parse("trait Ditransitive2:\n    belongs Ditransitive\n");
    let (reg, diags) = ontology::with_std(&user);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(
        reg.closure("Ditransitive2"),
        vec!["Ditransitive2", "Ditransitive", "Transitive", "Verb", "Predicate"]
    );
}

// ── 建構診斷(三類;分級資料,不 panic) ──

#[test]
fn unknown_belongs_target_is_diagnosed() {
    let user = parse("trait Foo:\n    belongs Ghost\n");
    let (_reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(
        diags.contains(&OntologyDiag::UnknownTrait {
            referrer: "Foo".into(),
            target: "Ghost".into(),
        }),
        "{diags:?}"
    );
}

#[test]
fn sign_belongs_to_unknown_category_is_diagnosed() {
    let user = parse("sign s:\n    belongs NoSuchCategory\n");
    let (_reg, diags) = ontology::with_std(&user);
    assert!(
        diags.contains(&OntologyDiag::UnknownTrait {
            referrer: "s".into(),
            target: "NoSuchCategory".into(),
        }),
        "{diags:?}"
    );
}

#[test]
fn belongs_cycle_is_diagnosed_and_closure_stays_safe() {
    let user = parse(
        "trait A:\n    belongs B\ntrait B:\n    belongs C\ntrait C:\n    belongs A\n",
    );
    let (reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(
        diags.iter().any(|d| matches!(d, OntologyDiag::Cycle { .. })),
        "應偵測環:{diags:?}"
    );
    let c = reg.closure("A");
    let mut sorted = c.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(c.len(), sorted.len(), "閉包去重、循環安全:{c:?}");
    assert!(c.contains(&"A".to_string()));
}

#[test]
fn duplicate_trait_is_diagnosed() {
    let user = parse("trait Dup:\ntrait Dup:\n");
    let (_reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(
        diags.contains(&OntologyDiag::DuplicateTrait { name: "Dup".into() }),
        "{diags:?}"
    );
}

// ── typed projection(對 Defs 的解讀 + 繼承;分類中立、Def 按維) ──

#[test]
fn projection_inherits_category_defaults_and_local_overrides() {
    let user = parse(
        "sign give:\n    belongs Transitive\n    belongs Transfer\n    phon:\n        /give/\n    sem:\n        gloss = GIVE\nsign proper:\n    belongs Noun\n    syn:\n        class = proper-noun\n",
    );
    let (reg, diags) = ontology::with_std(&user);
    assert!(diags.is_empty(), "with_std 建構不得有診斷:{diags:?}");

    let give = user.sign_named("give").unwrap();
    let syn = give.project(Dim::Syn, &reg);
    // 分類閉包**維度中立**:Transitive→Verb→Predicate 併 Transfer→Event
    assert_eq!(
        syn.categories,
        vec!["Transitive", "Verb", "Predicate", "Transfer", "Event"]
    );
    assert!(syn.is_a("Verb") && syn.is_a("Transfer"));
    // syn.class 由 Verb 範疇繼承(本地未覆蓋)
    assert_eq!(syn.get("syn.class"), Some("verb"));

    // sem 維:本地 sem.gloss;分類同上(中立)
    let sem = give.project(Dim::Sem, &reg);
    assert_eq!(sem.get("sem.gloss"), Some("GIVE"));
    assert_eq!(sem.categories, syn.categories, "分類跨維相同(單一樹)");

    // phon 維:本地 UR
    assert_eq!(give.project(Dim::Phon, &reg).get("phon"), Some("/give/"));

    // 本地覆蓋:proper 覆蓋 Nominal 繼承的 syn.class
    let proper = user.sign_named("proper").unwrap();
    let psyn = proper.project(Dim::Syn, &reg);
    assert_eq!(psyn.get("syn.class"), Some("proper-noun"), "本地勝(P6)");
    assert!(psyn.is_a("Nominal"));
}

/// 維度正交:syn projection 的 defs 不含 sem/phon 的 Def(P44)。
#[test]
fn projection_defs_are_dimension_orthogonal() {
    let user = parse(
        "sign w:\n    belongs Verb\n    phon:\n        /w/\n    sem:\n        gloss = W\n    prag:\n        register = formal\n",
    );
    let (reg, _) = ontology::with_std(&user);
    let w = user.sign_named("w").unwrap();
    assert_eq!(w.project(Dim::Syn, &reg).get("phon"), None);
    assert_eq!(w.project(Dim::Syn, &reg).get("sem.gloss"), None);
    assert_eq!(w.project(Dim::Prag, &reg).get("prag.register"), Some("formal"));
}

#[test]
fn registry_and_projection_are_deterministic() {
    let user = parse("sign s:\n    belongs Ditransitive\n");
    let (r1, _) = ontology::with_std(&user);
    let (r2, _) = ontology::with_std(&user);
    let s = user.sign_named("s").unwrap();
    assert_eq!(
        format!("{:?}", s.project_all(&r1)),
        format!("{:?}", s.project_all(&r2))
    );
}
