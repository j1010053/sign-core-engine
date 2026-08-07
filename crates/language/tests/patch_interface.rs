//! 步驟 12e 出口:typed patch 接口 + entrenchment 資料欄位(修補07 P30/P39,I27)。
//! **僅介面 + 資料**——無 entrenchment/固化動力學。

use conlang_language::ontology;
use conlang_language::patch::{diff, Patch, PatchOp};
use conlang_language::{Dim, Language, Severity, ValidationReport};

fn sign(src: &str) -> conlang_language::SignDef {
    Language::parse(src)
        .unwrap()
        .signs
        .into_iter()
        .next()
        .unwrap()
}

/// `Sign × Patch → Sign'`:Set upsert、**保留原 Sign**(不就地破壞,P30)。
#[test]
fn set_upserts_and_preserves_original() {
    let s = sign("sign a:\n    syn:\n        tam.present = 0\n");
    let before = format!("{s:?}");
    let p = Patch::syn().set("tam.present", "1").set("tam.past", "1");
    let s2 = p.apply(&s);
    assert_eq!(format!("{s:?}"), before, "原 Sign 不變(P30)");

    let (reg, _) = ontology::with_std(&Language::new());
    let syn = s2.project(Dim::Syn, &reg);
    assert_eq!(syn.get("syn.tam.present"), Some("1"), "upsert 既有欄位");
    assert_eq!(syn.get("syn.tam.past"), Some("1"), "新增欄位");
}

/// Unset 移除本地 Def;繼承值(範疇預設)由 projection 重新浮現。
#[test]
fn unset_removes_local_and_inheritance_resurfaces() {
    // Verb 範疇預設 syn.class=verb(stdlib);sign 本地覆寫為 proper,unset 後回繼承值。
    let lang =
        Language::parse("trait InheritedSynFeature:\n    syn:\n        tam.past = base\nsign p:\n    belongs Verb\n    belongs InheritedSynFeature\n    syn:\n        tam.past = local\n").unwrap();
    let (reg, _) = ontology::with_std(&lang);
    let p = lang.sign_named("p").unwrap();
    assert_eq!(
        p.project(Dim::Syn, &reg).get("syn.tam.past"),
        Some("local"),
        "本地覆寫"
    );

    let s2 = Patch::syn().unset("tam.past").apply(p);
    assert_eq!(
        s2.project(Dim::Syn, &reg).get("syn.tam.past"),
        Some("base"),
        "unset 本地後,範疇繼承值 verb 重新浮現"
    );
}

/// 維度隔離(型別層):builder 只產本維前綴 path;一個 syn patch 不可能碰 sem/phon。
#[test]
fn patch_is_dimension_isolated_by_construction() {
    let p = Patch::syn().set("class", "verb").unset("old");
    assert_eq!(p.dim(), Dim::Syn);
    assert!(p.ops().iter().all(|op| match op {
        PatchOp::Set { path, .. } | PatchOp::Unset { path } => path.starts_with("syn."),
    }));
}

/// 序列化 round-trip:`parse(render(p)) == p`。
#[test]
fn patch_serialization_round_trips() {
    let p = Patch::sem()
        .set("time.past", "1")
        .unset("stale")
        .set("causation", "direct");
    let text = p.render();
    assert_eq!(
        text,
        "sem: set time.past=1; unset stale; set causation=direct"
    );
    assert_eq!(Patch::parse(&text).unwrap(), p, "round-trip");
    // 空 patch
    let e = Patch::phon();
    assert_eq!(Patch::parse(&e.render()).unwrap(), e);
}

#[test]
fn patch_parse_rejects_malformed() {
    assert!(Patch::parse("no-colon").is_err());
    assert!(Patch::parse("xyz: set a=1").is_err(), "未知維度");
    assert!(Patch::parse("syn: set noequals").is_err());
    assert!(Patch::parse("syn: frobnicate a").is_err(), "未知 op");
    let cross_dimension = Patch::parse("syn: set sem.value=x").unwrap_err();
    assert_eq!(cross_dimension.code(), "PATCH_CROSS_DIMENSION");
    let mut report = ValidationReport::new();
    report.push(cross_dimension.diagnostic());
    assert!(report.has_errors());
    assert_eq!(report.diagnostics()[0].severity, Severity::Error);
    assert!(Patch::syn().try_set("", "x").is_err(), "空 path");
}

#[test]
fn diff_apply_matches_target_in_each_dimension() {
    let before = sign(
        "sign w:\n    phon:\n        /a/\n    syn:\n        tam.present = 1\n    sem:\n        senses:\n            core = A\n",
    );
    let after = sign(
        "sign w:\n    phon:\n        /b/\n    syn:\n        tam.past = 2\n    sem:\n        senses:\n            core = B\n    prag:\n        evidence.direct = 1\n",
    );
    let empty = Language::new();
    let (reg, _) = ontology::with_std(&empty);
    let mut current = before;
    for dim in Dim::all() {
        let patch = diff(&current, &after, dim);
        assert_eq!(
            Patch::parse(&patch.render()).unwrap(),
            patch,
            "{dim:?} diff round-trip"
        );
        current = patch.apply(&current);
        assert_eq!(
            current.project(dim, &reg).defs,
            after.project(dim, &reg).defs
        );
    }
}

// ── entrenchment(資料欄位;無動力學) ──

/// 讀 entrenchment 資料欄位;不可變 setter;無固化動力學(僅欄位)。
#[test]
fn entrenchment_is_data_field_only() {
    let s = sign("sign w:\n    entrenchment = 0.42\n    phon:\n        /w/\n");
    assert_eq!(s.entrenchment(), Some(0.42));

    let s2 = s.with_entrenchment(0.9);
    assert_eq!(s.entrenchment(), Some(0.42), "原 Sign 不變(不可變 setter)");
    assert_eq!(s2.entrenchment(), Some(0.9), "upsert");

    // 未宣告 → None(不預設、不推導:動力學留 M2/B)
    let bare = sign("sign x:\n    phon:\n        /x/\n");
    assert_eq!(bare.entrenchment(), None);
    assert_eq!(bare.with_entrenchment(0.1).entrenchment(), Some(0.1));
}

/// entrenchment 是**非維度** meta 欄位:不落任一維 projection(維度正交)。
#[test]
fn entrenchment_is_non_dimensional() {
    let lang =
        Language::parse("sign w:\n    entrenchment = 0.5\n    syn:\n        tam.present = 1\n")
            .unwrap();
    let (reg, _) = ontology::with_std(&lang);
    let w = lang.sign_named("w").unwrap();
    for dim in Dim::all() {
        assert!(
            w.project(dim, &reg).get("entrenchment").is_none(),
            "{dim:?} projection 不含 entrenchment"
        );
        assert!(w
            .project(dim, &reg)
            .get(&format!("{}.entrenchment", dim.keyword()))
            .is_none());
    }
    assert_eq!(w.entrenchment(), Some(0.5), "但 meta accessor 可讀");
}

#[test]
fn lexicalized_is_data_only_and_non_dimensional() {
    let s = sign("sign w:\n    lexicalized = false\n    phon:\n        /w/\n");
    assert_eq!(s.lexicalized(), Some(false));
    let changed = s.with_lexicalized(true);
    assert_eq!(s.lexicalized(), Some(false));
    assert_eq!(changed.lexicalized(), Some(true));
    let empty = Language::new();
    let (reg, _) = ontology::with_std(&empty);
    for dim in Dim::all() {
        assert!(changed.project(dim, &reg).get("lexicalized").is_none());
    }
}
