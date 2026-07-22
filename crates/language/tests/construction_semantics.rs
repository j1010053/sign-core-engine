//! 步驟 12c 出口:construction semantics(form-meaning pair;修補07 §12c,P42)。
//! 🔑 derived 語意 = frame,role 綁 filler 的**語意節點**(非字串替換);
//! 可擴充接口(SemNode 遞迴)容納未來複雜語意模型;polysemy/synonymy 合法;
//! 語意引用無法解析 → 診斷。

use conlang_language::codegen::{compile_full, Artifacts};
use conlang_language::construction::{self, CxgError};
use conlang_language::ontology::{self, OntologyRegistry};
use conlang_language::Language;

fn setup() -> (Language, Artifacts, OntologyRegistry) {
    let src = include_str!("fixtures/semantics.lang");
    let lang = Language::parse(src).expect("parse");
    let art = compile_full(&lang).expect("①–⑤");
    let (reg, diags) = ontology::with_std(&lang);
    assert!(diags.is_empty(), "ontology 建構不得有診斷:{diags:?}");
    (lang, art, reg)
}

/// 🔑 derived 語意組合:role giver/gift 綁 filler 的語意**節點**(含其全部欄位),
/// 而非把 gloss 字串塞進模板。construction 自身純量欄位(frame)保留。
#[test]
fn derived_sem_composes_filler_meaning_nodes() {
    let (lang, _art, reg) = setup();
    let tok = construction::apply(
        &lang,
        &reg,
        "GiveEvent",
        &[("agent", "john"), ("theme", "book")],
    )
    .unwrap();

    // construction 純量欄位
    assert_eq!(tok.sem.field("frame"), Some("giving"));
    // role giver = john 的**語意節點**(非字串;帶 john 全部 sem 欄位)
    let giver = tok.sem.role("giver").expect("giver role");
    assert_eq!(giver.field("gloss"), Some("JOHN"));
    assert_eq!(
        giver.field("ref"),
        Some("individual"),
        "整個節點,非僅 gloss"
    );
    // role gift = book 的語意節點
    let gift = tok.sem.role("gift").expect("gift role");
    assert_eq!(gift.field("gloss"), Some("BOOK"));
    // 節點非字串:giver 是有多欄位的 SemNode
    assert!(!giver.is_atomic() || giver.fields.len() >= 2);
}

/// form-meaning pair 同時導出:表層(form)+ 語意(meaning)。
#[test]
fn form_and_meaning_derived_together() {
    let (lang, art, reg) = setup();
    let tok = construction::apply(
        &lang,
        &reg,
        "GiveEvent",
        &[("agent", "john"), ("theme", "book")],
    )
    .unwrap();
    // form 極:表層 + syn 範疇
    assert_eq!(
        construction::surface(&art.grammar.program, &tok).unwrap(),
        "gijobo"
    );
    assert!(tok.syn_categories.contains(&"Verb".to_string()));
    // meaning 極:frame 綁 filler 語意
    assert_eq!(tok.sem.field("frame"), Some("giving"));
    assert_eq!(tok.sem.role("giver").unwrap().field("gloss"), Some("JOHN"));
}

/// polysemy(多義):一 filler 帶多個 sense 欄位,全數保留於其語意節點。
#[test]
fn polysemy_filler_keeps_multiple_senses() {
    let (lang, _art, reg) = setup();
    let tok = construction::apply(
        &lang,
        &reg,
        "GiveEvent",
        &[("agent", "john"), ("theme", "book")],
    )
    .unwrap();
    let gift = tok.sem.role("gift").unwrap();
    assert_eq!(gift.field("gloss"), Some("BOOK"));
    assert_eq!(gift.field("sense2"), Some("LOGBOOK"), "多義合法、不去重");
}

/// synonymy(同義):sofa/couch 同 gloss → 建構無診斷、兩者皆可填。
#[test]
fn synonymy_is_legal() {
    let (lang, _art, reg) = setup();
    for filler in ["sofa", "couch"] {
        let tok = construction::apply(
            &lang,
            &reg,
            "GiveEvent",
            &[("agent", "john"), ("theme", filler)],
        )
        .unwrap();
        assert_eq!(tok.sem.role("gift").unwrap().field("gloss"), Some("SEAT"));
    }
}

/// 部分套用:未填 slot 的 sem role 暫略(giver 有、gift 無);frame 仍在。
#[test]
fn partial_application_omits_unfilled_sem_roles() {
    let (lang, _art, reg) = setup();
    let tok = construction::apply(&lang, &reg, "GiveEvent", &[("agent", "john")]).unwrap();
    assert!(!tok.is_saturated());
    assert_eq!(tok.sem.field("frame"), Some("giving"));
    assert!(tok.sem.role("giver").is_some(), "已填 agent → giver 有");
    assert!(tok.sem.role("gift").is_none(), "未填 theme → gift 暫略");
}

/// sem role 引用不存在的 slot → SemRefUnknown(不默默近似)。
#[test]
fn unresolvable_sem_reference_is_diagnosed() {
    let (lang, _art, reg) = setup();
    let e = construction::apply(&lang, &reg, "BadSem", &[("agent", "john")]).unwrap_err();
    assert!(
        matches!(e, CxgError::SemRefUnknown { ref role, ref slot, .. }
            if role == "agent-role" && slot == "nonexistent"),
        "{e:?}"
    );
}

/// P42:語意組合**不就地改**來源 sign(filler 語意節點是副本)。
#[test]
fn semantic_composition_does_not_mutate_source() {
    let (lang, _art, reg) = setup();
    let before = format!("{:?}", lang.sign_named("john").unwrap());
    let _ = construction::apply(
        &lang,
        &reg,
        "GiveEvent",
        &[("agent", "john"), ("theme", "book")],
    )
    .unwrap();
    let after = format!("{:?}", lang.sign_named("john").unwrap());
    assert_eq!(before, after);
}

/// 決定性:同輸入兩次組合語意逐位元相同(P26)。
#[test]
fn semantic_composition_is_deterministic() {
    let (lang, _art, reg) = setup();
    let mk = || {
        construction::apply(
            &lang,
            &reg,
            "GiveEvent",
            &[("agent", "john"), ("theme", "book")],
        )
        .unwrap()
        .sem
    };
    assert_eq!(format!("{:?}", mk()), format!("{:?}", mk()));
}
