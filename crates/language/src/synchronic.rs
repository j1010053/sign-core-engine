//! 四維同步規則求值(步驟 12d;修補07 P43/P44,I25)。
//!
//! syn/sem/prag 規則求值於 **Sign 的維度 projection**(phon 規則求值於 Word,dsl,
//! 不在此)。**維度隔離(P44)**:一條 dim 規則只讀寫自己那維(產出該維 [`DimPatch`]),
//! 不碰他維——結構保證(規則在其維度區塊內,寫入 path 自帶維度前綴)。
//!
//! **Lexurgy 式 `Else` 三分(P43)**:一條規則 = 主分支 + else 鏈,**第一匹配勝出**:
//! - **Matched**:某分支守衛成立(含 identity:值未變仍算 Matched)→ 套用、跳過其餘;
//! - **Unmatched**:無分支匹配(`matched == 0`)→(整條規則 noop);
//! - **Error**:分支語法/守衛畸形 → 診斷,**不得偷偷進後續分支**。
//!
//! **逐求值單元**:每個 Sign 各自判定;規則序內**順序求值**(後規則見前 patch,P9)。
//! 保留 `matched`(status)/`changed`(值實變)/`diag`/source-location(RuleId)/
//! 決定性書寫序。

use crate::ontology::OntologyRegistry;
use crate::{Def, Dim, RuleId, SignDef, SignItem};

/// 求值三分(P43)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleStatus {
    Matched,
    Unmatched,
    Error,
}

/// 某維 typed patch(P30/P39;12e 形式化,12d 產出):dim-scoped Def upsert。
/// `Sign × Patch → Sign'`([`apply_patch`],保留原 Sign)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimPatch {
    pub dim: Dim,
    /// full path(含維度前綴)→ 新值 upsert。
    pub sets: Vec<(String, String)>,
}

/// 一條規則對一個 sign 的求值紀錄(trace)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleRecord {
    pub rule_id: RuleId,
    pub dim: Dim,
    pub status: RuleStatus,
    /// 值是否實變(identity 命中時 false,但 status 仍 Matched)。
    pub changed: bool,
    /// 命中分支索引(0 = 主,1.. = else_chain);Unmatched/Error 為 None。
    pub branch: Option<usize>,
    /// Error 診斷 / 資訊。
    pub diag: Option<String>,
}

// ── 結構化 dim 規則(自 raw body 解析) ──

struct DimRule {
    field: String,
    value: String,
    guard: Option<Guard>,
}
enum Guard {
    /// `[Category]`:sign 的 belongs 閉包含 Category(維度中立,P38 v0.2)。
    IsA(String),
    /// `field == value`:本維 projection 該欄等於 value。
    FieldEq(String, String),
}

fn parse_dim_rule(body: &str) -> Result<DimRule, String> {
    let (lhs, rhs) = body.split_once("=>").ok_or("rule must contain `=>`")?;
    let field = lhs.trim().to_owned();
    if field.is_empty() || field.contains(char::is_whitespace) {
        return Err(format!("rule LHS must be a single field, got {field:?}"));
    }
    let (value, guard) = match rhs.split_once(" / ") {
        Some((v, g)) => (v.trim().to_owned(), Some(parse_guard(g.trim())?)),
        None => (rhs.trim().to_owned(), None),
    };
    if value.is_empty() {
        return Err("rule RHS value is empty".to_owned());
    }
    Ok(DimRule {
        field,
        value,
        guard,
    })
}

fn parse_guard(g: &str) -> Result<Guard, String> {
    if let Some(inner) = g.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let cat = inner.trim();
        if cat.is_empty() {
            return Err("empty category guard `[]`".to_owned());
        }
        return Ok(Guard::IsA(cat.to_owned()));
    }
    if let Some((f, v)) = g.split_once("==") {
        return Ok(Guard::FieldEq(f.trim().to_owned(), v.trim().to_owned()));
    }
    Err(format!("malformed guard {g:?}"))
}

fn guard_matches(guard: &Guard, sign: &SignDef, dim: Dim, reg: &OntologyRegistry) -> bool {
    match guard {
        Guard::IsA(cat) => reg.sign_categories(sign).iter().any(|c| c == cat),
        Guard::FieldEq(f, v) => {
            let path = format!("{}.{}", dim.keyword(), f);
            sign.project(dim, reg).get(&path) == Some(v.as_str())
        }
    }
}

/// 求值**單一分支**(parse + guard + 可選 patch),記其 branch 索引。
fn eval_one_branch(
    id: RuleId,
    bi: usize,
    branch: &str,
    sign: &SignDef,
    dim: Dim,
    reg: &OntologyRegistry,
) -> (RuleRecord, Option<DimPatch>) {
    let mk = |status, changed, branch, diag| RuleRecord {
        rule_id: id,
        dim,
        status,
        changed,
        branch,
        diag,
    };
    let dr = match parse_dim_rule(branch) {
        Ok(d) => d,
        Err(e) => {
            return (
                mk(RuleStatus::Error, false, None, Some(format!("branch {bi}: {e}"))),
                None,
            )
        }
    };
    let matched = dr
        .guard
        .as_ref()
        .map_or(true, |g| guard_matches(g, sign, dim, reg));
    if !matched {
        return (mk(RuleStatus::Unmatched, false, Some(bi), None), None);
    }
    let path = format!("{}.{}", dim.keyword(), dr.field);
    let old = sign.project(dim, reg).get(&path).map(str::to_owned);
    let changed = old.as_deref() != Some(dr.value.as_str());
    (
        mk(RuleStatus::Matched, changed, Some(bi), None),
        Some(DimPatch {
            dim,
            sets: vec![(path, dr.value)],
        }),
    )
}

/// **Lexurgy `Else`**(P43):主 + else 鏈,**第一匹配 fallback**——第一個 Matched
/// 勝出(其餘不跑);identity(值未變)算 Matched;Error 立即中止,**不進後續分支**;
/// 全 Unmatched → 整條 Unmatched。回傳單一紀錄 + 可選 patch(至多套一次)。
fn eval_else(
    id: RuleId,
    body: &str,
    else_chain: &[String],
    sign: &SignDef,
    dim: Dim,
    reg: &OntologyRegistry,
) -> (RuleRecord, Option<DimPatch>) {
    for (bi, branch) in std::iter::once(body)
        .chain(else_chain.iter().map(String::as_str))
        .enumerate()
    {
        let (rec, patch) = eval_one_branch(id, bi, branch, sign, dim, reg);
        match rec.status {
            RuleStatus::Matched | RuleStatus::Error => return (rec, patch),
            RuleStatus::Unmatched => continue, // 試下一分支
        }
    }
    (
        RuleRecord {
            rule_id: id,
            dim,
            status: RuleStatus::Unmatched,
            changed: false,
            branch: None,
            diag: None,
        },
        None,
    )
}

/// **Lexurgy `Then`**(I26):主 + then 鏈,**順序組合**——每分支依序在**更新後**狀態
/// 上跑其 match/apply(前分支 commit 後,下一分支讀新狀態);**全分支皆跑**(非條件
/// 分支)。回傳演化後 sign + 每分支一筆紀錄(含未匹配分支)。
fn eval_then(
    id: RuleId,
    body: &str,
    then_chain: &[String],
    sign: &SignDef,
    dim: Dim,
    reg: &OntologyRegistry,
) -> (SignDef, Vec<RuleRecord>) {
    let mut cur = sign.clone();
    let mut records = Vec::new();
    for (bi, branch) in std::iter::once(body)
        .chain(then_chain.iter().map(String::as_str))
        .enumerate()
    {
        let (rec, patch) = eval_one_branch(id, bi, branch, &cur, dim, reg);
        if let Some(p) = patch {
            cur = apply_patch(&cur, &p);
        }
        records.push(rec);
    }
    (cur, records)
}

/// typed patch 套用(P30):`Sign × Patch → Sign'`,**保留原 Sign**;
/// upsert 同 path Def(有則改值,無則附加本地 Def)。
pub fn apply_patch(sign: &SignDef, patch: &DimPatch) -> SignDef {
    let mut s = sign.clone();
    for (path, value) in &patch.sets {
        let existing = s.items.iter_mut().find_map(|it| match it {
            SignItem::Def(d) if &d.path == path => Some(d),
            _ => None,
        });
        match existing {
            Some(d) => d.value = value.clone(),
            None => s.items.push(SignItem::Def(Def {
                path: path.clone(),
                value: value.clone(),
            })),
        }
    }
    s
}

/// 對一個 sign 跑其**某維**的全部同步規則(書寫序、順序求值,後見前 patch)。
/// 回傳(演化後 sign,逐規則紀錄)。**不就地改**輸入 sign。
pub fn run_sign_dim_rules(
    sign: &SignDef,
    dim: Dim,
    reg: &OntologyRegistry,
) -> (SignDef, Vec<RuleRecord>) {
    let rules: Vec<crate::Rule> = sign
        .items
        .iter()
        .filter_map(|i| match i {
            SignItem::Rule(r) if r.dim == dim => Some(r.clone()),
            _ => None,
        })
        .collect();
    let mut cur = sign.clone();
    let mut records = Vec::new();
    for r in rules {
        if !r.then_chain.is_empty() {
            // Lexurgy Then:順序組合,全分支皆跑
            let (next, recs) = eval_then(r.id, &r.body, &r.then_chain, &cur, dim, reg);
            cur = next;
            records.extend(recs);
        } else {
            // Lexurgy Else(或無鏈單分支):第一匹配
            let (rec, patch) = eval_else(r.id, &r.body, &r.else_chain, &cur, dim, reg);
            if let Some(p) = patch {
                cur = apply_patch(&cur, &p);
            }
            records.push(rec);
        }
    }
    (cur, records)
}
