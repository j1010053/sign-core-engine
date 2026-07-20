//! 步驟 12a 出口:四維 ontology + `belongs` 閉包 + typed projection(修補07 P38–P40)。
//!
//! 獨立 oracle 疊加:round-trip 恆等(P21,parser 擴充不破 step 8–9)、閉包語意
//! (self-first/ancestors-first/去重/循環安全)、四類建構診斷(未知/跨維/循環/重複)、
//! projection 繼承(範疇預設 Def 由後代繼承、本地覆蓋)、四維獨立(同名不同維)。

use conlang_language::ontology::{self, OntologyDiag, OntologyRegistry};
use conlang_language::{Dim, Language};

fn parse(src: &str) -> Language {
    Language::parse(src).expect("parse")
}

// ── stdlib 本體(I20:資料層 .lang) ──

/// 額外引用的 stdlib 本體必須 parse 且 round-trip 恆等(P21;parser 擴充自證)。
#[test]
fn std_ontology_parses_and_round_trips() {
    let l = ontology::std_ontology();
    let dump = l.dump();
    assert_eq!(
        parse(&dump).dump(),
        dump,
        "stdlib ontology.lang 必須 round-trip 恆等"
    );
    // 四棵樹都非空(四維獨立存在,P38)
    let (reg, diags) = OntologyRegistry::build(&[&l]);
    assert!(diags.is_empty(), "stdlib 本體不得有建構診斷:{diags:?}");
    for dim in Dim::all() {
        assert!(!reg.names(dim).is_empty(), "{dim:?} 樹不得為空");
    }
}

/// belongs 閉包:self-first、ancestors-first、去重、跨維不洩漏(P38)。
#[test]
fn belongs_closure_is_ancestors_first_and_dim_scoped() {
    let (reg, _) = OntologyRegistry::build(&[&ontology::std_ontology()]);
    // Ditransitive → Transitive → Verb → Predicate(同維鏈)
    assert_eq!(
        reg.closure(Dim::Syn, "Ditransitive"),
        vec!["Ditransitive", "Transitive", "Verb", "Predicate"],
        "self-first + ancestors-first"
    );
    // 葉節點自身
    assert_eq!(reg.closure(Dim::Syn, "Predicate"), vec!["Predicate"]);
    // 跨維:syn 的 Verb 不出現在 sem 樹
    assert!(reg.closure(Dim::Sem, "Verb").is_empty());
    // sem 鏈
    assert_eq!(
        reg.closure(Dim::Sem, "Human"),
        vec!["Human", "Animate", "Physical", "Entity"]
    );
}

/// 四維獨立:同名 `Motion` 可同時存在於 syn 與 sem,互不干涉(P38)。
#[test]
fn same_name_across_dims_is_independent() {
    let user = parse(
        "syn trait Motion {\n}\nsem trait Motion {\n    belongs Event\n}\nsem trait Event {\n}\n",
    );
    let (reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(diags.is_empty(), "同名不同維不得報衝突:{diags:?}");
    assert!(reg.has(Dim::Syn, "Motion"));
    assert!(reg.has(Dim::Sem, "Motion"));
    assert_eq!(reg.closure(Dim::Syn, "Motion"), vec!["Motion"]); // syn 無父
    assert_eq!(reg.closure(Dim::Sem, "Motion"), vec!["Motion", "Event"]);
}

// ── 建構診斷(四類;分級資料,不 panic) ──

#[test]
fn unknown_belongs_target_is_diagnosed() {
    let user = parse("syn trait Foo {\n    belongs Ghost\n}\n");
    let (_reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(
        diags.contains(&OntologyDiag::UnknownTrait {
            referrer: "Foo".into(),
            target: "Ghost".into(),
        }),
        "{diags:?}"
    );
}

/// 跨維 belongs:ontology trait 的父在別的維 → CrossDimBelongs(P38 閉包同維)。
#[test]
fn cross_dim_belongs_is_diagnosed() {
    let user = parse("syn trait Foo {\n    belongs Event\n}\nsem trait Event {\n}\n");
    let (_reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(
        diags.contains(&OntologyDiag::CrossDimBelongs {
            trait_name: "Foo".into(),
            dim: Dim::Syn,
            target: "Event".into(),
            target_dim: Dim::Sem,
        }),
        "{diags:?}"
    );
}

#[test]
fn belongs_cycle_is_diagnosed_and_closure_stays_safe() {
    let user = parse(
        "syn trait A {\n    belongs B\n}\nsyn trait B {\n    belongs C\n}\nsyn trait C {\n    belongs A\n}\n",
    );
    let (reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(
        diags.iter().any(|d| matches!(d, OntologyDiag::Cycle { dim: Dim::Syn, .. })),
        "應偵測環:{diags:?}"
    );
    // 閉包不得無限遞迴;每個名字至多出現一次
    let c = reg.closure(Dim::Syn, "A");
    let mut sorted = c.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(c.len(), sorted.len(), "閉包去重、循環安全:{c:?}");
    assert!(c.contains(&"A".to_string()));
}

#[test]
fn duplicate_trait_in_same_dim_is_diagnosed() {
    let user = parse("syn trait Dup {\n}\nsyn trait Dup {\n}\n");
    let (_reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(
        diags.contains(&OntologyDiag::DuplicateTrait {
            dim: Dim::Syn,
            name: "Dup".into(),
        }),
        "{diags:?}"
    );
}

/// sign 的 belongs 指向未知範疇 → 診斷(不靜默丟分類)。
#[test]
fn sign_belongs_to_unknown_category_is_diagnosed() {
    let user = parse("sign s {\n    belongs NoSuchCategory\n}\n");
    let (_reg, diags) = ontology::with_std(&user);
    assert!(
        diags.contains(&OntologyDiag::UnknownTrait {
            referrer: "s".into(),
            target: "NoSuchCategory".into(),
        }),
        "{diags:?}"
    );
}

// ── typed projection(對 Defs 的解讀 + 繼承) ──

/// sign 經 belongs 繼承範疇預設 Def;本地覆蓋(P6/P39)。
#[test]
fn projection_inherits_category_defaults_and_local_overrides() {
    // 使用者 sign:一個及物動詞、一個覆蓋 syn.class 的
    let user = parse(
        "sign give {\n    belongs Transitive\n    belongs Transfer\n    phon = /give/\n    sem.gloss = GIVE\n}\nsign proper {\n    belongs Noun\n    syn.class = proper-noun\n}\n",
    );
    let (reg, diags) = ontology::with_std(&user);
    assert!(diags.is_empty(), "with_std 建構不得有診斷:{diags:?}");

    let give = user.sign_named("give").unwrap();
    let syn = give.project(Dim::Syn, &reg);
    // 分類閉包:Transitive → Verb → Predicate
    assert_eq!(syn.categories, vec!["Transitive", "Verb", "Predicate"]);
    assert!(syn.is_a("Verb"));
    // syn.class 由 Verb 範疇繼承(本地未覆蓋)
    assert_eq!(syn.get("syn.class"), Some("verb"));

    // sem 維:Transfer → Event;本地 sem.gloss
    let sem = give.project(Dim::Sem, &reg);
    assert_eq!(sem.categories, vec!["Transfer", "Event"]);
    assert_eq!(sem.get("sem.gloss"), Some("GIVE"));

    // phon 維:本地 UR;無 phon 範疇
    let phon = give.project(Dim::Phon, &reg);
    assert_eq!(phon.get("phon"), Some("/give/"));
    assert!(phon.categories.is_empty());

    // 本地覆蓋:proper 覆蓋 Nominal 繼承的 syn.class
    let proper = user.sign_named("proper").unwrap();
    let psyn = proper.project(Dim::Syn, &reg);
    assert_eq!(psyn.get("syn.class"), Some("proper-noun"), "本地勝(P6)");
    assert!(psyn.is_a("Nominal"));
}

/// 維度正交:syn projection 不含 sem/phon 的 Def(P44 每維各自)。
#[test]
fn projections_are_dimension_orthogonal() {
    let user = parse(
        "sign w {\n    belongs Verb\n    phon = /w/\n    sem.gloss = W\n    prag.register = formal\n}\n",
    );
    let (reg, _) = ontology::with_std(&user);
    let w = user.sign_named("w").unwrap();
    assert_eq!(w.project(Dim::Syn, &reg).get("phon"), None);
    assert_eq!(w.project(Dim::Syn, &reg).get("sem.gloss"), None);
    assert_eq!(w.project(Dim::Sem, &reg).get("prag.register"), None);
    assert_eq!(w.project(Dim::Prag, &reg).get("prag.register"), Some("formal"));
}

/// 決定性:同輸入兩次建構 registry + projection 逐位元相同(P26)。
#[test]
fn registry_and_projection_are_deterministic() {
    let user = parse("sign s {\n    belongs Ditransitive\n}\n");
    let (r1, _) = ontology::with_std(&user);
    let (r2, _) = ontology::with_std(&user);
    let s = user.sign_named("s").unwrap();
    assert_eq!(
        format!("{:?}", s.project_all(&r1)),
        format!("{:?}", s.project_all(&r2))
    );
}
