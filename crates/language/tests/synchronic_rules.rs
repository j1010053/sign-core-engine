//! 步驟 12d 出口:四維同步規則 + Lexurgy 式 `Else` 三分(修補07 P43/P44,I25)。
//!
//! 語意迴歸矩陣:最小正例(Matched)、近失敗負例(Unmatched→else)、identity
//! (值未變仍 Matched、不落 else)、Error(畸形→診斷,不進 else)、多轉換
//! (順序求值,後見前 patch)、維度隔離(syn 規則只寫 syn)。內部狀態斷言
//! (projection/patch/RuleRecord)非僅表層。

use conlang_language::ontology::{self, OntologyRegistry};
use conlang_language::synchronic::{self, RuleStatus};
use conlang_language::{Dim, Language};

fn setup() -> (Language, OntologyRegistry) {
    let lang = Language::parse(include_str!("fixtures/synchronic.lang")).expect("parse");
    let (reg, diags) = ontology::with_std(&lang);
    assert!(diags.is_empty(), "{diags:?}");
    (lang, reg)
}

/// 規則帶維度標記(parser 依區塊):prag 規則 dim=Prag、syn 規則 dim=Syn。
#[test]
fn rules_are_tagged_with_their_dimension() {
    let (lang, _) = setup();
    let teacher = lang.sign_named("teacher").unwrap();
    // P71-C:prag 亦支援 typed feature,故 register 規則現為 `FeatureRule`。
    let dims: Vec<_> = teacher
        .items
        .iter()
        .filter_map(|i| match i {
            conlang_language::SignItem::Rule(r) | conlang_language::SignItem::FeatureRule(r) => {
                Some(r.dim)
            }
            _ => None,
        })
        .collect();
    assert_eq!(dims, vec![Dim::Prag]);
    let chain = lang.sign_named("chain").unwrap();
    // P71 §4.3:`chain` 的目標 a/b 現為已宣告特徵,故其規則節點是 `FeatureRule`。
    // 維度標記(P44 隔離的依據)對兩種規則節點一致。
    let dims: Vec<_> = chain
        .items
        .iter()
        .filter_map(|i| match i {
            conlang_language::SignItem::Rule(r) | conlang_language::SignItem::FeatureRule(r) => {
                Some(r.dim)
            }
            _ => None,
        })
        .collect();
    assert_eq!(dims, vec![Dim::Syn, Dim::Syn]);
}

/// 最小正例:守衛 `[Honorific]` 成立 → register:=formal(Matched,branch 0,changed)。
#[test]
fn matched_main_branch_sets_value() {
    let (lang, reg) = setup();
    let teacher = lang.sign_named("teacher").unwrap();
    let (out, recs) = synchronic::run_sign_dim_rules(teacher, Dim::Prag, &reg);
    assert_eq!(
        out.project(Dim::Prag, &reg).get("prag.register"),
        Some("formal")
    );
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].status, RuleStatus::Matched);
    assert_eq!(recs[0].branch, Some(0), "命中主分支");
    assert!(recs[0].changed, "teacher 原無 register → 值實變");
}

/// 近失敗負例:守衛不成立 → 落 else 分支(Matched via else,branch 1)。
#[test]
fn unmatched_main_falls_to_else() {
    let (lang, reg) = setup();
    let dog = lang.sign_named("dog").unwrap();
    let (out, recs) = synchronic::run_sign_dim_rules(dog, Dim::Prag, &reg);
    assert_eq!(
        out.project(Dim::Prag, &reg).get("prag.register"),
        Some("neutral")
    );
    assert_eq!(recs[0].status, RuleStatus::Matched);
    assert_eq!(recs[0].branch, Some(1), "命中 else 分支");
}

/// 🔑 identity:king 已 register=formal 且 belongs Honorific → 主分支 Matched
/// (changed=false),**不落 else**(否則會誤變 neutral)。P43 identity=Matched。
#[test]
fn identity_match_is_matched_and_blocks_else() {
    let (lang, reg) = setup();
    let king = lang.sign_named("king").unwrap();
    let (out, recs) = synchronic::run_sign_dim_rules(king, Dim::Prag, &reg);
    assert_eq!(
        out.project(Dim::Prag, &reg).get("prag.register"),
        Some("formal"),
        "identity 命中主分支,未落 else 的 neutral"
    );
    assert_eq!(recs[0].status, RuleStatus::Matched);
    assert_eq!(recs[0].branch, Some(0));
    assert!(
        !recs[0].changed,
        "值本已 formal → changed=false 但仍 Matched"
    );
}

/// Error:畸形規則(無 `=>`)→ Error 診斷,不進 else、不改 sign。
#[test]
fn malformed_rule_is_error_not_else() {
    let (lang, reg) = setup();
    let broken = lang.sign_named("broken").unwrap();
    let (out, recs) = synchronic::run_sign_dim_rules(broken, Dim::Syn, &reg);
    assert_eq!(recs[0].status, RuleStatus::Error);
    assert!(recs[0].diag.is_some());
    assert_eq!(
        out.project(Dim::Syn, &reg).defs,
        Vec::new(),
        "Error 不改 sign"
    );
}

/// 多轉換 + 順序求值:rule2 的守衛 `a == 1` 依賴 rule1 先前的 patch。
#[test]
fn sequential_rules_see_prior_patch() {
    let (lang, reg) = setup();
    let chain = lang.sign_named("chain").unwrap();
    let (out, recs) = synchronic::run_sign_dim_rules(chain, Dim::Syn, &reg);
    let syn = out.project(Dim::Syn, &reg);
    assert_eq!(syn.get("syn.a"), Some("1"));
    assert_eq!(
        syn.get("syn.b"),
        Some("2"),
        "rule2 見 rule1 的 a=1 → 守衛成立"
    );
    assert!(recs.iter().all(|r| r.status == RuleStatus::Matched));
}

/// 維度隔離(P44):syn 規則只寫 syn projection,不生 sem/phon/prag Def。
#[test]
fn dimension_isolation_syn_rule_touches_only_syn() {
    let (lang, reg) = setup();
    let chain = lang.sign_named("chain").unwrap();
    let (out, _) = synchronic::run_sign_dim_rules(chain, Dim::Syn, &reg);
    assert!(out.project(Dim::Sem, &reg).defs.is_empty());
    assert!(out.project(Dim::Prag, &reg).defs.is_empty());
    assert!(out.project(Dim::Phon, &reg).defs.is_empty());
    assert!(out
        .project(Dim::Syn, &reg)
        .defs
        .iter()
        .all(|(p, _)| p.starts_with("syn.")));
}

/// 逐求值單元:每 sign 各自判定(teacher→formal、dog→neutral 同規則不同結果)。
#[test]
fn per_sign_evaluation_is_independent() {
    let (lang, reg) = setup();
    let t = synchronic::run_sign_dim_rules(lang.sign_named("teacher").unwrap(), Dim::Prag, &reg).0;
    let d = synchronic::run_sign_dim_rules(lang.sign_named("dog").unwrap(), Dim::Prag, &reg).0;
    assert_eq!(
        t.project(Dim::Prag, &reg).get("prag.register"),
        Some("formal")
    );
    assert_eq!(
        d.project(Dim::Prag, &reg).get("prag.register"),
        Some("neutral")
    );
}

/// typed patch `Sign × Patch → Sign'` 保留原 sign(不就地改)+ 決定性。
#[test]
fn patch_application_is_immutable_and_deterministic() {
    let (lang, reg) = setup();
    let teacher = lang.sign_named("teacher").unwrap();
    let before = format!("{teacher:?}");
    let a = synchronic::run_sign_dim_rules(teacher, Dim::Prag, &reg).0;
    let b = synchronic::run_sign_dim_rules(teacher, Dim::Prag, &reg).0;
    assert_eq!(format!("{teacher:?}"), before, "原 sign 不變");
    assert_eq!(format!("{a:?}"), format!("{b:?}"), "決定性");
}

/// round-trip:含 syn/sem/prag 規則 + else 的 fixture 正規化為不動點(P21)。
#[test]
fn fixture_round_trips() {
    let d1 = Language::parse(include_str!("fixtures/synchronic.lang"))
        .unwrap()
        .dump();
    let d2 = Language::parse(&d1).unwrap().dump();
    assert_eq!(d1, d2);
    // prag 規則印在 prag: 區塊(維度標記正確)
    assert!(d1.contains("    prag:\n"), "{d1}");
    assert!(d1.contains("register => formal / [Honorific]"), "{d1}");
}
