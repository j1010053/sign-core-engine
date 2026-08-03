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
    assert_eq!(
        parse(&dump).dump(),
        dump,
        "stdlib ontology.lang 必須 round-trip 恆等"
    );
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
        // P71-S:`Verb belongs Event` 之後,語意型別真的在閉包裡
        //(單一中立樹的直接後果,與既有的 `belongs Transfer` 同形)。
        vec![
            "Ditransitive",
            "Transitive",
            "Verb",
            "Predicate",
            "Event",
            "EventFrame",
            "SemanticFrame",
            "Semantic"
        ]
    );
    assert_eq!(reg.closure("Predicate"), vec!["Predicate"]);
    assert_eq!(
        reg.closure("Human"),
        vec!["Human", "Animate", "Physical", "Entity", "Semantic"],
        "Entity is now explicitly a semantic type, while nearest-first order remains stable"
    );
}

/// 使用者可擴充自定範疇,掛回本體某節點(docs/07 §9)。
#[test]
fn user_can_extend_ontology() {
    let user = parse("trait Ditransitive2:\n    belongs Ditransitive\n");
    let (reg, diags) = ontology::with_std(&user);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(
        reg.closure("Ditransitive2"),
        vec![
            "Ditransitive2",
            "Ditransitive",
            "Transitive",
            "Verb",
            "Predicate",
            "Event",
            "EventFrame",
            "SemanticFrame",
            "Semantic"
        ]
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
    let user = parse("trait A:\n    belongs B\ntrait B:\n    belongs C\ntrait C:\n    belongs A\n");
    let (reg, diags) = OntologyRegistry::build(&[&user]);
    assert!(
        diags
            .iter()
            .any(|d| matches!(d, OntologyDiag::Cycle { .. })),
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
        "trait LocalNominalStatus:\n    syn:\n        feature:\n            nominal_status = enum(common, proper)\nsign give:\n    belongs Transitive\n    belongs Transfer\n    phon:\n        /give/\n    sem:\n        senses:\n            core = GIVE\nsign frame_only:\n    belongs Transfer\nsign proper:\n    belongs Noun\n    belongs LocalNominalStatus\n    syn:\n        feature:\n            nominal_status = proper\n",
    );
    let (reg, diags) = ontology::with_std(&user);
    assert!(diags.is_empty(), "with_std 建構不得有診斷:{diags:?}");

    let give = user.sign_named("give").unwrap();
    let syn = give.project(Dim::Syn, &reg);
    // 分類閉包**維度中立**:Transitive→Verb→Predicate 併 Transfer→Event
    assert_eq!(
        syn.categories,
        // P71-S:`Verb belongs Event` 使 Event/EventFrame 由 Verb 這條路徑
        // 更早進入閉包(nearest-first),Transfer 分支隨後併入。
        vec![
            "Transitive",
            "Verb",
            "Predicate",
            "Event",
            "EventFrame",
            "SemanticFrame",
            "Semantic",
            "Transfer",
            "TransferFrame",
        ]
    );
    assert!(
        syn.is_a("Verb")
            && syn.is_a("Transfer")
            && syn.is_a("TransferFrame")
            && syn.is_a("EventFrame")
            && syn.is_a("SemanticFrame")
            && syn.is_a("Semantic")
    );
    // `give` 現在**是** Event —— 但那是經 Transitive→Verb→Event(P71-S 的裁定),
    // 不是經 Transfer。原不變式(Transfer 是 frame 契約,不蘊含 Event 範疇)因此
    // 改在一個不是 Verb 的 Transfer sign 上觀察,否則被 Verb 那條路徑遮蔽。
    assert!(syn.is_a("Event"), "give 經 Verb 取得 Event");
    let frame_only = user.sign_named("frame_only").unwrap().project(Dim::Syn, &reg);
    assert!(frame_only.is_a("TransferFrame") && frame_only.is_a("EventFrame"));
    assert!(
        !frame_only.is_a("Event"),
        "Transfer is a frame contract; it does not silently assert the Event category"
    );
    // Ontology membership is carried by `belongs`; it no longer writes a
    // mutable `syn.class` default.
    assert_eq!(
        syn.get("syn.class"),
        None,
        "category identity is in belongs closure, not a mutable syn.class default"
    );

    // sem 維:本地義項;分類同上(中立)
    let sem = give.project(Dim::Sem, &reg);
    // P71 §4.1:gloss 住 `senses:`,不再是 Def 路徑——投影層已無此鍵
    assert_eq!(sem.get("sem.gloss"), None, "gloss 已非 Def 路徑");
    assert_eq!(
        conlang_language::sem::SemNode::of_sign(give, &reg).field("gloss"),
        Some("GIVE"),
        "本地義項仍隨 sign 投影可見"
    );
    assert_eq!(sem.categories, syn.categories, "分類跨維相同(單一樹)");

    // phon 維:本地 UR
    assert_eq!(give.project(Dim::Phon, &reg).get("phon"), Some("/give/"));

    // A locally declared enum feature remains available on the Syn projection.
    let proper = user.sign_named("proper").unwrap();
    let psyn = proper.project(Dim::Syn, &reg);
    assert_eq!(psyn.get("syn.nominal_status"), Some("proper"));
    assert!(psyn.is_a("Nominal"));
}

#[test]
fn generic_and_typed_feature_values_share_one_precedence_stream() {
    let language = parse(
        // P71 §4.2:此例需要**同一路徑**同時能是裸 Def 與宣告過的 feature,
        // 故用封閉清單上的單段座標 `prag.illocution`(自造的 `syn.state` 已不合法)。
        r#"trait BaseState:
    prag:
        feature:
            illocution = enum(base, local)
            illocution = base
sign item:
    belongs BaseState
    prag:
        illocution = local
"#,
    );
    let (registry, diagnostics) = ontology::with_std(&language);
    assert!(diagnostics.is_empty());
    let effective = registry.effective_sign(language.sign_named("item").unwrap());
    assert_eq!(
        effective
            .project(Dim::Prag, &registry)
            .get("prag.illocution"),
        Some("local"),
        "a local generic Def must beat an inherited typed FeatureValue"
    );
}

/// 維度正交:syn projection 的 defs 不含 sem/phon 的 Def(P44)。
#[test]
fn projection_defs_are_dimension_orthogonal() {
    let user = parse(
        "sign w:\n    belongs Verb\n    phon:\n        /w/\n    sem:\n        senses:\n            core = W\n    prag:\n        feature:\n            register = enum(formal, neutral)\n            register = formal\n",
    );
    let (reg, _) = ontology::with_std(&user);
    let w = user.sign_named("w").unwrap();
    assert_eq!(w.project(Dim::Syn, &reg).get("phon"), None);
    assert_eq!(w.project(Dim::Syn, &reg).get("sem.gloss"), None);
    assert_eq!(
        w.project(Dim::Prag, &reg).get("prag.register"),
        Some("formal")
    );
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
