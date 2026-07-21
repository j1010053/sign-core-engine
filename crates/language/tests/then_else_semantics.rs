//! Lexurgy `Else` / `Then` 語意一致性功能測試(I26;修補07 P43 + tshiatun 協作規範 §2/§3)。
//!
//! 對照 Lexurgy 定義:
//! - **`Else`** = first-matching fallback:各分支從**同一輸入**依序試,第一個匹配勝出、
//!   其餘不跑(identity 算匹配)。**至多一個分支生效**。
//! - **`Then`** = 順序組合:前分支 match/apply/commit 後,下一分支讀**更新後**狀態;
//!   **全分支依序皆跑**(非條件分支)。
//! - 平坦層不得混用 `then`/`else`(混用 = 定位錯誤)。

use conlang_language::ontology::{self, OntologyRegistry};
use conlang_language::synchronic::{self, RuleStatus};
use conlang_language::{Dim, Language};

fn setup() -> (Language, OntologyRegistry) {
    let lang = Language::parse(include_str!("fixtures/synchronic.lang")).expect("parse");
    let (reg, diags) = ontology::with_std(&lang);
    assert!(diags.is_empty(), "{diags:?}");
    (lang, reg)
}

fn run(lang: &Language, reg: &OntologyRegistry, sign: &str) -> conlang_language::SignDef {
    synchronic::run_sign_dim_rules(lang.sign_named(sign).unwrap(), Dim::Syn, reg).0
}

/// 🔑 同兩分支 `x => a` / `x => b`:**Else 只跑第一(x=a)**,**Then 兩者依序跑(x=b)**。
/// 這是 Else(至多一分支)與 Then(全分支)行為的判別點。
#[test]
fn else_runs_one_branch_then_runs_all() {
    let (lang, reg) = setup();
    let e = run(&lang, &reg, "else_same");
    assert_eq!(e.project(Dim::Syn, &reg).get("syn.x"), Some("a"), "Else:只第一分支");

    let t = run(&lang, &reg, "then_same");
    assert_eq!(t.project(Dim::Syn, &reg).get("syn.x"), Some("b"), "Then:兩分支依序,後者勝");
}

/// feeding:Then 第二分支見第一分支的 commit(守衛 `x == 1` 成立 → y=2);
/// Else 第二分支根本不執行(主分支已匹配)→ y 未設。
#[test]
fn then_feeds_next_branch_else_does_not() {
    let (lang, reg) = setup();
    let e = run(&lang, &reg, "else_feed");
    let es = e.project(Dim::Syn, &reg);
    assert_eq!(es.get("syn.x"), Some("1"));
    assert_eq!(es.get("syn.y"), None, "Else:第二分支不跑,y 未設");

    let t = run(&lang, &reg, "then_feed");
    let ts = t.project(Dim::Syn, &reg);
    assert_eq!(ts.get("syn.x"), Some("1"));
    assert_eq!(ts.get("syn.y"), Some("2"), "Then:第二分支見 x=1 → y=2(feeding)");
}

/// Then 首分支未匹配([Nope]),後分支**仍跑**(順序組合,非條件 fallback)。
#[test]
fn then_runs_later_branch_even_if_earlier_unmatched() {
    let (lang, reg) = setup();
    let t = run(&lang, &reg, "then_noop_first");
    let ts = t.project(Dim::Syn, &reg);
    assert_eq!(ts.get("syn.x"), None, "首分支守衛不成立 → noop");
    assert_eq!(ts.get("syn.y"), Some("2"), "Then 後分支照跑");
}

/// Then 每分支各一筆紀錄(全分支皆求值);Else 至多一筆有效結果。
#[test]
fn then_records_every_branch_else_records_first_match() {
    let (lang, reg) = setup();
    let (_, then_recs) =
        synchronic::run_sign_dim_rules(lang.sign_named("then_same").unwrap(), Dim::Syn, &reg);
    assert_eq!(then_recs.len(), 2, "Then:主+then 各一筆");
    assert!(then_recs.iter().all(|r| r.status == RuleStatus::Matched));
    assert_eq!(then_recs[0].branch, Some(0));
    assert_eq!(then_recs[1].branch, Some(1));

    let (_, else_recs) =
        synchronic::run_sign_dim_rules(lang.sign_named("else_same").unwrap(), Dim::Syn, &reg);
    assert_eq!(else_recs.len(), 1, "Else:整條規則一筆");
    assert_eq!(else_recs[0].status, RuleStatus::Matched);
    assert_eq!(else_recs[0].branch, Some(0), "第一分支匹配即止");
}

/// 平坦層混用 `then`/`else` = 定位錯誤(巢狀括號屬後續)。
#[test]
fn mixing_then_and_else_is_a_located_error() {
    let e = Language::parse(
        "sign m:\n    syn:\n        x => a\n            else x => b\n            then x => c\n",
    )
    .unwrap_err();
    assert!(e.msg.contains("mix"), "{e}");
    // 反向亦然
    let e2 = Language::parse(
        "sign m:\n    syn:\n        x => a\n            then x => b\n            else x => c\n",
    )
    .unwrap_err();
    assert!(e2.msg.contains("mix"), "{e2}");
}

/// round-trip:then 鏈正規化為不動點,印於維度區塊。
#[test]
fn then_chain_round_trips() {
    let src = "sign s:\n    syn:\n        x => a\n            then x => b\n";
    let d1 = Language::parse(src).unwrap().dump();
    assert!(d1.contains("then x => b"), "then 鏈保留:\n{d1}");
    assert_eq!(Language::parse(&d1).unwrap().dump(), d1, "不動點");
}
