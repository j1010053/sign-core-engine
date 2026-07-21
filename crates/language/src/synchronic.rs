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

/// 對一個 sign 求值一條規則(主 + else,第一匹配)。回傳紀錄 + 可選 patch。
fn eval_rule(
    id: RuleId,
    body: &str,
    else_chain: &[String],
    sign: &SignDef,
    dim: Dim,
    reg: &OntologyRegistry,
) -> (RuleRecord, Option<DimPatch>) {
    let branches = std::iter::once(body).chain(else_chain.iter().map(String::as_str));
    for (bi, branch) in branches.enumerate() {
        let dr = match parse_dim_rule(branch) {
            Ok(d) => d,
            Err(e) => {
                // Error:直接診斷,不進後續分支(P43)
                return (
                    RuleRecord {
                        rule_id: id,
                        dim,
                        status: RuleStatus::Error,
                        changed: false,
                        branch: None,
                        diag: Some(format!("branch {bi}: {e}")),
                    },
                    None,
                );
            }
        };
        let matched = dr
            .guard
            .as_ref()
            .map_or(true, |g| guard_matches(g, sign, dim, reg));
        if matched {
            let path = format!("{}.{}", dim.keyword(), dr.field);
            let old = sign.project(dim, reg).get(&path).map(str::to_owned);
            let changed = old.as_deref() != Some(dr.value.as_str());
            let patch = DimPatch {
                dim,
                sets: vec![(path, dr.value)],
            };
            return (
                RuleRecord {
                    rule_id: id,
                    dim,
                    status: RuleStatus::Matched,
                    changed,
                    branch: Some(bi),
                    diag: None,
                },
                Some(patch),
            );
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
    let rules: Vec<(RuleId, String, Vec<String>)> = sign
        .items
        .iter()
        .filter_map(|i| match i {
            SignItem::Rule(r) if r.dim == dim => {
                Some((r.id, r.body.clone(), r.else_chain.clone()))
            }
            _ => None,
        })
        .collect();
    let mut cur = sign.clone();
    let mut records = Vec::with_capacity(rules.len());
    for (id, body, else_chain) in rules {
        let (rec, patch) = eval_rule(id, &body, &else_chain, &cur, dim, reg);
        if let Some(p) = patch {
            cur = apply_patch(&cur, &p);
        }
        records.push(rec);
    }
    (cur, records)
}
