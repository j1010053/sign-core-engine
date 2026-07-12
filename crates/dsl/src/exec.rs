//! executor:`Program` × `Word` → 推導(每規則一 commit,執行語意 §1)。
//!
//! 同一規則的多語句共享同一次 Parallel Match(B5):全部在凍結前狀態上
//! 求 Action,合為一個 commit。VerbClass 依語句組成歸類(讀韻律結構者 Query)。
//! `level` 標記(P3)在 M0 僅記錄,不改變執行(全部視為 word-level 逐序跑)。

use conlang_core::lifecycle::{run, Action, EngineError, VerbClass};
use conlang_core::repr::invariant::InvariantIssue;
use conlang_core::repr::word::Word;
use conlang_core::verbs;

use crate::lower::{LoweredRule, LoweredStmt, Program};

/// 一條規則 commit 後的紀錄(快照序列的一格)。
#[derive(Debug, Clone)]
pub struct StepRecord {
    pub rule: String,
    pub word: Word,
    pub issues: Vec<InvariantIssue>,
}

fn stmt_actions(p: &Program, w: &Word, s: &LoweredStmt) -> Result<Vec<Action>, EngineError> {
    match s {
        LoweredStmt::InsertFloatingNear {
            tier,
            val,
            onset_test,
        } => verbs::insert_floating_near(w, *tier, *val, *onset_test),
        LoweredStmt::Dock { tier, strategy } => verbs::dock(w, *tier, *strategy),
        LoweredStmt::Fill { tier, val } => verbs::fill(w, *tier, *val),
        LoweredStmt::MergeAdjacentEqual { tier } => verbs::merge_adjacent_equal(w, *tier),
        LoweredStmt::Spread {
            tier,
            val,
            ward,
            blocked_by,
            within,
            through,
            on_conflict,
        } => verbs::spread(
            w,
            *tier,
            *val,
            *ward,
            *blocked_by,
            *within,
            *through,
            *on_conflict,
        ),
        LoweredStmt::Shift { tier, n, ward } => verbs::shift(w, *tier, *n, *ward),
        LoweredStmt::DominateEmpty { level, class, ward } => {
            verbs::dominate_empty(w, *level, class, *ward)
        }
        LoweredStmt::Rewrite { m, out, env } => verbs::rewrite(w, &p.env.inv, m, out, env),
    }
}

fn class_of(rule: &LoweredRule) -> VerbClass {
    // repair 類(dominate/release)不觸發 reparse(A3);全 repair 規則 → Repair
    if !rule.stmts.is_empty()
        && rule
            .stmts
            .iter()
            .all(|s| matches!(s, LoweredStmt::DominateEmpty { .. }))
    {
        return VerbClass::Repair;
    }
    let reads_prosody = rule.stmts.iter().any(|s| {
        matches!(
            s,
            LoweredStmt::InsertFloatingNear { .. }
                | LoweredStmt::Dock { .. }
                | LoweredStmt::Fill { .. }
                | LoweredStmt::Spread { .. }
                | LoweredStmt::Shift { .. }
                | LoweredStmt::Rewrite { .. }
        )
    });
    if reads_prosody {
        VerbClass::Query
    } else {
        VerbClass::Melodic
    }
}

/// 對單詞跑整個規則序列;回傳每規則 commit 後的快照紀錄。
pub fn run_program(p: &Program, input: Word) -> Result<Vec<StepRecord>, EngineError> {
    let mut w = input;
    let mut records = Vec::with_capacity(p.rules.len());
    for rule in &p.rules {
        // B5:同規則各語句共享凍結前狀態
        let mut actions = Vec::new();
        for s in &rule.stmts {
            actions.extend(stmt_actions(p, &w, s)?);
        }
        let out = run(&w, &actions, class_of(rule))?;
        w = out.word;
        records.push(StepRecord {
            rule: rule.name.clone(),
            word: w.clone(),
            issues: out.issues,
        });
    }
    Ok(records)
}
