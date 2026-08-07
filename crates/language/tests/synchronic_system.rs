//! 共時語法系統**整合總檢查**(12a–12e;audit「共時語法功能總檢查」)。
//! 一份 mini-grammar 串起全部層:分類樹 → 四維投影 → construction(form-meaning)
//! → 同步規則(Else)→ patch → entrenchment → Flow A(compile→codegen→surface)。
//! 證明各層**組合**正確,非僅各自孤立。

use conlang_language::codegen::compile_full;
use conlang_language::construction;
use conlang_language::ontology::{self, OntologyRegistry};
use conlang_language::patch::Patch;
use conlang_language::synchronic::{self, RuleStatus};
use conlang_language::{Dim, Language};

fn load() -> (Language, OntologyRegistry) {
    let lang = Language::parse(include_str!("fixtures/synchronic_system.lang")).expect("parse");
    let (reg, diags) = ontology::with_std(&lang);
    assert!(diags.is_empty(), "整合本體不得有診斷:{diags:?}");
    (lang, reg)
}

/// 層 1–2:分類樹(單一中立)+ 四維投影 + 繼承。
#[test]
fn ontology_and_projection_layers() {
    let (lang, reg) = load();
    let run = lang.sign_named("run").unwrap();
    // belongs 閉包:run → Verb→Predicate 併 Motion→Event(維度中立)
    let cats = reg.sign_categories(run);
    for c in ["Verb", "Predicate", "Motion", "Event"] {
        assert!(cats.contains(&c.to_string()), "缺範疇 {c}:{cats:?}");
    }
    // 投影按維:syn 繼承 Verb 的 class=verb(sign 未本地覆寫該 Def,只有規則)
    assert_eq!(
        run.project(Dim::Syn, &reg).get("syn.valence"),
        Some("base"),
        "the declared feature inherits its base value before the rule runs"
    );
    // P71 §4.1:gloss 住 `senses:`,Def 投影已無此鍵
    assert_eq!(run.project(Dim::Sem, &reg).get("sem.gloss"), None);
    assert_eq!(
        conlang_language::sem::SemNode::of_sign(run, &reg).field("gloss"),
        Some("RUN")
    );
    assert_eq!(
        run.project(Dim::Sem, &reg).get("sem.manner"),
        Some("neutral"),
        "已宣告的 sem feature 仍是 Def 路徑"
    );
    assert_eq!(run.project(Dim::Phon, &reg).get("phon"), Some("/run/"));
}

/// 層 4–5:construction application = form-meaning pair。
/// form:表層經引擎導出;meaning:frame 綁 filler 的語意**節點**。
#[test]
fn construction_form_and_meaning_layer() {
    let (lang, reg) = load();
    let art = compile_full(&lang).expect("Flow A: ①–⑤");
    let tok = construction::apply(
        &lang,
        &reg,
        "Clause",
        &[("subject", "dog"), ("predicate", "run")],
    )
    .expect("apply");
    // form 極:表層 + syn 範疇
    assert_eq!(
        construction::surface(&art.grammar.program, &tok).unwrap(),
        "dogrun"
    );
    assert!(tok.syn_categories.contains(&"Verb".to_string()));
    // meaning 極:frame + role 綁 filler 語意節點(非字串)
    assert_eq!(tok.sem.field("frame"), Some("event"));
    assert_eq!(tok.sem.role("actor").unwrap().field("gloss"), Some("DOG"));
    assert_eq!(tok.sem.role("action").unwrap().field("gloss"), Some("RUN"));
}

/// 層 6:同步規則求值於 Sign projection(syn 無守衛式 + prag Else)。
#[test]
fn synchronic_rules_layer() {
    let (lang, reg) = load();
    // syn:run 的 `class => intransitive / [Verb]` → 守衛成立 → 覆寫繼承的 verb
    let run = lang.sign_named("run").unwrap();
    let (run2, recs) = synchronic::run_sign_dim_rules(run, Dim::Syn, &reg);
    assert_eq!(recs[0].status, RuleStatus::Matched);
    assert_eq!(
        run2.project(Dim::Syn, &reg).get("syn.valence"),
        Some("intransitive")
    );

    // prag:lord 屬 Honorific → Else 主分支 → formal
    let lord = lang.sign_named("lord").unwrap();
    let (lord2, _) = synchronic::run_sign_dim_rules(lord, Dim::Prag, &reg);
    assert_eq!(
        lord2.project(Dim::Prag, &reg).get("prag.register"),
        Some("formal")
    );
}

/// 層 6 續:維度隔離——syn 規則跑完不生 sem/prag/phon Def。
#[test]
fn dimension_isolation_across_system() {
    let (lang, reg) = load();
    let run = lang.sign_named("run").unwrap();
    let (run2, _) = synchronic::run_sign_dim_rules(run, Dim::Syn, &reg);
    // sem/phon 仍為原值(未被 syn 規則污染)
    assert_eq!(
        run2.project(Dim::Sem, &reg).get("sem.manner"),
        Some("neutral")
    );
    assert_eq!(
        conlang_language::sem::SemNode::of_sign(&run2, &reg).field("gloss"),
        Some("RUN"),
        "義項亦未被 syn 規則污染"
    );
    assert_eq!(run2.project(Dim::Phon, &reg).get("phon"), Some("/run/"));
    assert!(run2.project(Dim::Prag, &reg).defs.is_empty());
}

/// 層 7:typed patch + entrenchment 資料欄位。
#[test]
fn patch_and_entrenchment_layer() {
    let (lang, reg) = load();
    let run = lang.sign_named("run").unwrap();
    assert_eq!(run.entrenchment(), Some(0.9));
    let run2 = Patch::sem().set("manner", "rapid").apply(run);
    assert_eq!(
        run.project(Dim::Sem, &reg).get("sem.manner"),
        Some("neutral"),
        "原不變"
    );
    assert_eq!(
        run2.project(Dim::Sem, &reg).get("sem.manner"),
        Some("rapid")
    );
    assert_eq!(run2.entrenchment(), Some(0.9), "patch 未動 entrenchment");
}

/// 層 8:Flow A 端到端(compile→codegen→引擎)仍成立;整條鏈決定性。
#[test]
fn flow_a_and_whole_chain_deterministic() {
    let (lang, reg) = load();
    let art1 = compile_full(&lang).unwrap();
    let art2 = compile_full(&lang).unwrap();
    assert_eq!(
        art1.grammar.phon_source, art2.grammar.phon_source,
        "codegen 決定性"
    );

    let mk = || {
        let tok = construction::apply(
            &lang,
            &reg,
            "Clause",
            &[("subject", "dog"), ("predicate", "run")],
        )
        .unwrap();
        format!("{:?}", tok.sem)
    };
    assert_eq!(mk(), mk(), "construction 語意決定性");
}

/// 整份 fixture round-trip 不動點(全語法特徵:belongs/slots/維度規則/Else/entrenchment)。
#[test]
fn whole_fixture_round_trips() {
    let d1 = Language::parse(include_str!("fixtures/synchronic_system.lang"))
        .unwrap()
        .dump();
    let d2 = Language::parse(&d1).unwrap().dump();
    assert_eq!(d1, d2);
}
