//! Deterministic syn/sem/prag rule evaluation for stored signs and derived
//! construction tokens. Token rules may read immutable filler snapshots with
//! `$slot.<name>.<dim>.<path>` and `unify(...)`.

use std::collections::BTreeSet;

use crate::construction::{slots_of, FillerSnapshot};
use crate::ontology::OntologyRegistry;
use crate::path::parse_path;
use crate::reference::{self, DimPolicy, PathPolicy, RefError, RefSpec};
use crate::{
    CaseCondition, Def, Dim, Expression, RuleId, RuleNamespace, SignDef, SignItem, Slot,
    SourceLocation,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleStatus {
    Matched,
    Unmatched,
    Error,
}

pub use crate::patch::Patch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotRead {
    pub slot: String,
    pub dim: Dim,
    pub path: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfRead {
    pub dim: Dim,
    pub path: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleRecord {
    pub rule_id: RuleId,
    pub dim: Dim,
    pub status: RuleStatus,
    pub changed: bool,
    pub branch: Option<usize>,
    pub diag: Option<String>,
    pub source: SourceLocation,
    pub source_package: Option<String>,
    pub slot_reads: Vec<SlotRead>,
    pub self_reads: Vec<SelfRead>,
}

#[derive(Debug, Clone)]
struct SlotAccess {
    slot: String,
    dim: Dim,
    path: String,
}

#[derive(Debug, Clone)]
struct SelfAccess {
    dim: Dim,
    path: String,
}

#[derive(Debug, Clone)]
enum Access {
    Slot(SlotAccess),
    Self_(SelfAccess),
}

#[derive(Debug, Clone)]
enum ValueExpr {
    Literal(String),
    Access(Access),
    Unify(Vec<Access>),
    /// Like `unify`, but an absent operand is a runtime Error rather than an
    /// Unmatched result.  This is used for obligatory selection contracts
    /// such as case government while preserving optional-feature semantics
    /// for ordinary `unify`.
    Require(Vec<Access>),
}

#[derive(Debug, Clone)]
struct DimRule {
    field: String,
    value: ValueExpr,
    guard: Option<Guard>,
}

#[derive(Debug, Clone)]
enum Guard {
    IsA(String),
    FieldEq(String, String),
    SlotFieldEq(SlotAccess, String),
    SlotIsA(String, String),
    SelfIsA(String),
    SelfFieldEq(SelfAccess, String),
    /// `$x.<dim>.<path> == <值或另一個引用>`(P91):主體來自求值環境的具名
    /// 綁定。右端可以是字面值,也可以是**另一個綁定的欄位**——那正是跨參數
    /// guard 的核心用途(比較兩個參數的同一個欄位)。
    BindingFieldEq(BindingAccess, BindingOperand),
    /// `$x == [Trait]`(P91)。
    BindingIsA(String, String),
    /// `A && B && …`(P91):連言收進文法本身。
    ///
    /// 先前 `&&` 由**六個消費端各自 split**,guard 文法只認單一比較。
    /// function 層要支援多主體就會出現第七份;收進這裡讓它只有一份。
    All(Vec<Guard>),
}

/// `$<name>.<dim>.<path>`——具名綁定的欄位讀取。
#[derive(Debug, Clone)]
struct BindingAccess {
    name: String,
    dim: Dim,
    path: String,
}

/// guard 右端:字面值或另一個綁定欄位。
#[derive(Debug, Clone)]
enum BindingOperand {
    Literal(String),
    /// `$y`——純量參數(function 層的參數可綁 sign,也可綁字面值)。
    Scalar(String),
    Access(BindingAccess),
}

/// Slots a realization/phon-case guard reads (so slot-usage validation counts a
/// slot referenced only from a guard, not just from a template).
pub(crate) fn realization_guard_slot_references(source: &str) -> Vec<String> {
    match parse_guard(source) {
        Ok(Guard::SlotFieldEq(access, _)) => vec![access.slot],
        Ok(Guard::SlotIsA(slot, _)) => vec![slot],
        _ => Vec::new(),
    }
}

/// `$self.<dim>.<path>`——維度必填、路徑必填且走 Path 文法驗證。
const SELF_ACCESS: RefSpec = RefSpec {
    allow_self: true,
    allow_slot: false,
    allow_binding: false,
    dim: DimPolicy::Required,
    path: PathPolicy::RequiredValidated,
};

/// `$slot.<name>.<dim>.<path>`——同上,主體換成 slot。
const SLOT_ACCESS: RefSpec = RefSpec {
    allow_self: false,
    allow_slot: true,
    allow_binding: false,
    dim: DimPolicy::Required,
    path: PathPolicy::RequiredValidated,
};

fn parse_self_access(value: &str) -> Result<SelfAccess, String> {
    let reference = reference::parse(&SELF_ACCESS, value).map_err(|error| match error {
        RefError::MissingSigil | RefError::SubjectNotAllowed => {
            format!("self reference must begin with `$self.`, got {value:?}")
        }
        RefError::BadPath(message) => message,
        _ => format!("self reference must be `$self.phon|syn|sem|prag.PATH`, got {value:?}"),
    })?;
    Ok(SelfAccess {
        dim: reference.dim.expect("SELF_ACCESS requires a dimension"),
        path: reference.path.expect("SELF_ACCESS requires a path"),
    })
}

fn parse_access(value: &str) -> Result<Access, String> {
    if value.trim().starts_with("$slot.") {
        parse_slot_access(value).map(Access::Slot)
    } else if value.trim().starts_with("$self.") {
        parse_self_access(value).map(Access::Self_)
    } else {
        Err(format!(
            "expected `$self` or `$slot` reference, got {value:?}"
        ))
    }
}

fn parse_slot_access(value: &str) -> Result<SlotAccess, String> {
    let reference = reference::parse(&SLOT_ACCESS, value).map_err(|error| match error {
        RefError::MissingSigil | RefError::SubjectNotAllowed => {
            format!("slot reference must begin with `$slot.`, got {value:?}")
        }
        RefError::BadPath(message) => message,
        _ => format!("slot reference must be `$slot.NAME.phon|syn|sem|prag.PATH`, got {value:?}"),
    })?;
    Ok(SlotAccess {
        slot: reference
            .slot()
            .expect("SLOT_ACCESS forbids the self subject")
            .to_owned(),
        dim: reference.dim.expect("SLOT_ACCESS requires a dimension"),
        path: reference.path.expect("SLOT_ACCESS requires a path"),
    })
}

fn parse_value(value: &str) -> Result<ValueExpr, String> {
    let value = value.trim();
    if let Some(inner) = value
        .strip_prefix("require(")
        .and_then(|inner| inner.strip_suffix(')'))
    {
        let accesses = inner
            .split(',')
            .map(parse_access)
            .collect::<Result<Vec<_>, _>>()?;
        if accesses.is_empty() {
            return Err("require needs at least one typed reference".to_owned());
        }
        let dimensions = accesses
            .iter()
            .map(|access| match access {
                Access::Slot(access) => access.dim,
                Access::Self_(access) => access.dim,
            })
            .collect::<std::collections::BTreeSet<_>>();
        if dimensions.len() != 1 {
            return Err("require operands must belong to one dimension".to_owned());
        }
        return Ok(ValueExpr::Require(accesses));
    }
    if let Some(inner) = value
        .strip_prefix("unify(")
        .and_then(|inner| inner.strip_suffix(')'))
    {
        let accesses = inner
            .split(',')
            .map(parse_access)
            .collect::<Result<Vec<_>, _>>()?;
        if accesses.len() < 2 {
            return Err("unify requires at least two typed references".to_owned());
        }
        let dimensions = accesses
            .iter()
            .map(|access| match access {
                Access::Slot(access) => access.dim,
                Access::Self_(access) => access.dim,
            })
            .collect::<std::collections::BTreeSet<_>>();
        if dimensions.len() != 1 {
            return Err("unify operands must belong to one dimension".to_owned());
        }
        return Ok(ValueExpr::Unify(accesses));
    }
    if value.starts_with("$slot.") || value.starts_with("$self.") {
        return parse_access(value).map(ValueExpr::Access);
    }
    if value.is_empty() {
        return Err("rule RHS value is empty".to_owned());
    }
    Ok(ValueExpr::Literal(value.to_owned()))
}

fn parse_dim_rule(body: &str) -> Result<DimRule, String> {
    let (lhs, rhs) = body.split_once("=>").ok_or("rule must contain `=>`")?;
    let field = lhs.trim().to_owned();
    if field.is_empty()
        || field.contains(char::is_whitespace)
        || parse_path(&field).is_err()
        || Dim::parse(field.split(['.', '[', '~']).next().unwrap_or_default()).is_some()
    {
        return Err(format!("rule LHS must be a single field, got {field:?}"));
    }
    let (value, guard) = match rhs.split_once(" / ") {
        Some((value, guard)) => (parse_value(value)?, Some(parse_guard(guard.trim())?)),
        None => (parse_value(rhs)?, None),
    };
    Ok(DimRule {
        field,
        value,
        guard,
    })
}

fn parse_guard(value: &str) -> Result<Guard, String> {
    // 連言先切。單一條件時不包 `All`,避免改變既有結構。
    if value.contains("&&") {
        let parts = value
            .split("&&")
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(parse_atomic_guard)
            .collect::<Result<Vec<_>, _>>()?;
        if parts.len() < 2 {
            return Err(format!("malformed conjunction {value:?}"));
        }
        return Ok(Guard::All(parts));
    }
    parse_atomic_guard(value)
}

fn binding_access(source: &str) -> Result<BindingAccess, String> {
    let read = reference::parse(&reference::BINDING_FIELD, source)
        .map_err(|error| format!("binding reference {source:?}: {error}"))?;
    Ok(BindingAccess {
        name: read
            .binding()
            .expect("BINDING_FIELD yields a binding")
            .to_owned(),
        dim: read.dim.expect("BINDING_FIELD requires a dimension"),
        path: read.path.clone().expect("BINDING_FIELD requires a path"),
    })
}

fn parse_atomic_guard(value: &str) -> Result<Guard, String> {
    if let Some(inner) = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        let category = inner.trim();
        if category.is_empty() {
            return Err("empty category guard `[]`".to_owned());
        }
        return Ok(Guard::IsA(category.to_owned()));
    }
    let Some((field, expected)) = value.split_once("==") else {
        return Err(format!("malformed guard {value:?}"));
    };
    let field = field.trim();
    let expected = expected.trim();
    if expected.is_empty() {
        return Err(format!("malformed guard {value:?}"));
    }
    if field == "$self" {
        let Some(category) = expected.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
            return Err("`$self ==` requires a `[Trait]` value".to_owned());
        };
        return Ok(Guard::SelfIsA(category.trim().to_owned()));
    }
    if let Some(slot) = field
        .strip_prefix("$slot.")
        .filter(|name| !name.contains('.'))
    {
        let Some(category) = expected.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
            return Err("`$slot.NAME ==` requires a `[Trait]` value".to_owned());
        };
        return Ok(Guard::SlotIsA(slot.to_owned(), category.trim().to_owned()));
    }
    if field.starts_with("$slot.") {
        return Ok(Guard::SlotFieldEq(
            parse_slot_access(field)?,
            expected.to_owned(),
        ));
    }
    if field.starts_with("$self.") {
        return Ok(Guard::SelfFieldEq(
            parse_self_access(field)?,
            expected.to_owned(),
        ));
    }
    // `$x == [Trait]` / `$x.<dim>.<path> == 值`(P91)。`$self`/`$slot.` 已在
    // 上面攔掉,故此處的 `$` 開頭必是具名綁定。
    if field.starts_with('$') {
        if let Ok(read) = reference::parse(&reference::BINDING_ONLY, field) {
            let name = read.binding().expect("BINDING_ONLY yields a binding");
            let Some(category) = expected.strip_prefix('[').and_then(|v| v.strip_suffix(']'))
            else {
                return Err(format!("`{field} ==` requires a `[Trait]` value"));
            };
            return Ok(Guard::BindingIsA(
                name.to_owned(),
                category.trim().to_owned(),
            ));
        }
        let left = binding_access(field)?;
        let right = if let Ok(read) = reference::parse(&reference::BINDING_ONLY, expected) {
            BindingOperand::Scalar(read.binding().expect("binding").to_owned())
        } else if expected.starts_with('$') {
            BindingOperand::Access(binding_access(expected)?)
        } else {
            BindingOperand::Literal(expected.to_owned())
        };
        return Ok(Guard::BindingFieldEq(left, right));
    }
    if field.is_empty()
        || parse_path(field).is_err()
        || Dim::parse(field.split(['.', '[', '~']).next().unwrap_or_default()).is_some()
    {
        return Err(format!("malformed field guard {value:?}"));
    }
    Ok(Guard::FieldEq(field.to_owned(), expected.to_owned()))
}

fn accesses(rule: &DimRule) -> Vec<&SlotAccess> {
    let mut result = Vec::new();
    match &rule.value {
        ValueExpr::Literal(_) => {}
        ValueExpr::Access(Access::Slot(access)) => result.push(access),
        ValueExpr::Access(Access::Self_(_)) => {}
        ValueExpr::Unify(values) | ValueExpr::Require(values) => {
            result.extend(values.iter().filter_map(|access| match access {
                Access::Slot(access) => Some(access),
                Access::Self_(_) => None,
            }))
        }
    }
    if let Some(Guard::SlotFieldEq(access, _)) = &rule.guard {
        result.push(access);
    }
    result
}

pub(crate) fn rule_slot_references(rule: &crate::Rule) -> Vec<String> {
    std::iter::once(rule.body.as_str())
        .chain(rule.else_chain.iter().map(String::as_str))
        .chain(rule.then_chain.iter().map(String::as_str))
        .filter_map(|branch| parse_dim_rule(branch).ok())
        .flat_map(|parsed| {
            accesses(&parsed)
                .into_iter()
                .map(|access| access.slot.clone())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// P71 §7 A1:**普通規則的目標路徑亦受封閉清單約束**。
///
/// 規則寫入的是 `Patch` 的 Def 路徑(見本檔 `Patch::for_dim(dim).set(...)`),
/// 與 `Def` 同一個路徑空間。只關 `Def` 而不關這裡,等於前門上鎖、側門敞開:
/// `syn: category => noun` 照樣寫得進去,而那正是 P69 才剛修掉的路徑。
///
/// **`FeatureRule` 不走這條**——其目標是宣告過的 feature,已有
/// `FEATURE_RULE_UNDECLARED` 與 `FEATURE_RULE_VALUE_OUT_OF_DOMAIN` 兩道檢查,
/// 那正是 R2 給作者的正解出口。
pub(crate) fn rule_target_violations(rule: &crate::Rule) -> Vec<String> {
    std::iter::once(rule.body.as_str())
        .chain(rule.else_chain.iter().map(String::as_str))
        .chain(rule.then_chain.iter().map(String::as_str))
        .enumerate()
        .filter_map(|(index, branch)| {
            let parsed = parse_dim_rule(branch).ok()?;
            let path = format!("{}.{}", rule.dim.keyword(), parsed.field);
            (!crate::system::def_path_allowed(&path))
                .then(|| format!("branch {index}: {}", crate::system::closed_list_hint(&path)))
        })
        .collect()
}

/// P71 增修 D:**guard 讀的欄位路徑亦受封閉清單約束**。
///
/// guard 與 Def／規則目標是同一個路徑空間(都經 `sign.project(dim).get(path)`),
/// 差別只在方向。§7 A1 關掉了寫入側門後,讀取端仍是 §2.1 記載的原狀:欄位名
/// 打錯回 `Unmatched`,不發任何診斷——規則於是永遠不觸發,而作者看不到訊號。
///
/// 白名單 = 封閉清單 ∪ 主體可見的 typed feature。兩半缺一不可:只有封閉清單
/// 的話連 R2 的正解出口都會擋掉——`feature:` 的值投影進的正是同一個扁平路徑
/// 空間(`projection.rs` 的 `<dim>.<feature.name>`)。
///
/// 「主體可見」按主體是否**靜態已知**分兩檔(呼叫端決定傳哪一份):
/// - 具體 sign 的 `$self`(含裸欄位 = 本規則維度上的 `$self`):feature 集合封閉,
///   用它**有效**(含繼承)的宣告嚴查,與寫入側 `FEATURE_RULE_UNDECLARED` 同判準。
/// - `$slot.NAME` 的 filler、以及 **trait 裡的 `$self`**:靜態未知——`[*]` 槽可填
///   任何 sign、filler 能自帶本地 feature、trait 是模板(菱形下可合法 guard 在
///   兄弟 trait 的 feature 上)。改用**語言全域**的 feature 宣告集:這是不會誤擋
///   的最強上界,全語言沒有任何一處宣告過的名字,沒有任何主體能有它。
///
/// **`FeatureRule` 也走這條**(與 `rule_target_violations` 相反)——豁免 `FeatureRule`
/// 的理由是它的**目標**已有兩道檢查,那個理由不及於它的 guard。
pub(crate) fn rule_guard_violations(
    rule: &crate::Rule,
    self_features: &BTreeSet<(Dim, String)>,
    filler_features: &BTreeSet<(Dim, String)>,
) -> Vec<String> {
    std::iter::once(rule.body.as_str())
        .chain(rule.else_chain.iter().map(String::as_str))
        .chain(rule.then_chain.iter().map(String::as_str))
        .enumerate()
        .filter_map(|(index, branch)| {
            let parsed = parse_dim_rule(branch).ok()?;
            let violation = guard_read_violation(
                parsed.guard.as_ref()?,
                rule.dim,
                self_features,
                filler_features,
            )?;
            Some(format!("branch {index}: {violation}"))
        })
        .collect()
}

/// 單一 guard 的讀取路徑檢查。範疇守衛(`[Cat]`／`$self == [Cat]`／
/// `$slot.x == [Cat]`)讀的是本體樹不是路徑空間,已有 unknown category 檢查,
/// 不在此處重複。
fn guard_read_violation(
    guard: &Guard,
    dim: Dim,
    self_features: &BTreeSet<(Dim, String)>,
    filler_features: &BTreeSet<(Dim, String)>,
) -> Option<String> {
    match guard {
        // 裸欄位 = 本規則所在維度上的 `$self` 讀取(見 `guard_matches` 的 `FieldEq`)。
        Guard::FieldEq(field, _) => read_path_violation(dim, field, "$self", self_features),
        Guard::SelfFieldEq(access, _) => {
            read_path_violation(access.dim, &access.path, "$self", self_features)
        }
        Guard::SlotFieldEq(access, _) => read_path_violation(
            access.dim,
            &access.path,
            &format!("$slot.{}", access.slot),
            filler_features,
        ),
        Guard::IsA(_) | Guard::SelfIsA(_) | Guard::SlotIsA(_, _) => None,
        Guard::BindingFieldEq(_, _) | Guard::BindingIsA(_, _) => None,
        Guard::All(parts) => parts
            .iter()
            .find_map(|part| guard_read_violation(part, dim, self_features, filler_features)),
    }
}

/// P71 增修 E:**值表達式讀的欄位路徑亦受封閉清單約束**。
///
/// 與增修 D 是同一個洞的兩半:`field => $self.<dim>.<path>` /
/// `$slot.N.<dim>.<path>` / `unify(…)` / `require(…)` 走的是同一組 `read_self`
/// `read_slot`,路徑打錯同樣回 `Unmatched` 且不發診斷。判準與可見範圍完全沿用
/// D2/D3(白名單 = 封閉清單 ∪ 主體可見的 typed feature;主體靜態已知才嚴查)。
///
/// **後果比 guard 重**:guard 打錯只是規則不觸發(no-op);值打錯讓規則
/// `Unmatched`,依 P43 的 Else 三分**落進 `else` 分支**——靜默失敗在這裡是有輸出的,
/// 產出的是一個錯的值而不是不動作。
///
/// 一個分支可能有多個違規讀取(`unify` 兩個運算元都打錯),故逐個回報而非只回第一個。
pub(crate) fn rule_value_violations(
    rule: &crate::Rule,
    self_features: &BTreeSet<(Dim, String)>,
    filler_features: &BTreeSet<(Dim, String)>,
) -> Vec<String> {
    std::iter::once(rule.body.as_str())
        .chain(rule.else_chain.iter().map(String::as_str))
        .chain(rule.then_chain.iter().map(String::as_str))
        .enumerate()
        .flat_map(|(index, branch)| {
            let Ok(parsed) = parse_dim_rule(branch) else {
                return Vec::new();
            };
            value_accesses(&parsed.value)
                .into_iter()
                .filter_map(|access| {
                    let violation = access_read_violation(access, self_features, filler_features)?;
                    Some(format!("branch {index}: {violation}"))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn value_accesses(value: &ValueExpr) -> Vec<&Access> {
    match value {
        ValueExpr::Literal(_) => Vec::new(),
        ValueExpr::Access(access) => vec![access],
        ValueExpr::Unify(values) | ValueExpr::Require(values) => values.iter().collect(),
    }
}

fn access_read_violation(
    access: &Access,
    self_features: &BTreeSet<(Dim, String)>,
    filler_features: &BTreeSet<(Dim, String)>,
) -> Option<String> {
    match access {
        Access::Self_(access) => {
            read_path_violation(access.dim, &access.path, "$self", self_features)
        }
        Access::Slot(access) => read_path_violation(
            access.dim,
            &access.path,
            &format!("$slot.{}", access.slot),
            filler_features,
        ),
    }
}

fn read_path_violation(
    dim: Dim,
    tail: &str,
    subject: &str,
    declared: &BTreeSet<(Dim, String)>,
) -> Option<String> {
    let path = format!("{}.{}", dim.keyword(), tail);
    let allowed =
        crate::system::def_path_allowed(&path) || declared.contains(&(dim, tail.to_owned()));
    (!allowed).then(|| crate::system::read_path_hint(&path, subject))
}

pub(crate) fn validate_rule(
    rule: &crate::Rule,
    registry: &OntologyRegistry,
    slots: &[Slot],
) -> Vec<String> {
    std::iter::once(rule.body.as_str())
        .chain(rule.else_chain.iter().map(String::as_str))
        .chain(rule.then_chain.iter().map(String::as_str))
        .enumerate()
        .flat_map(|(index, branch)| match parse_dim_rule(branch) {
            Err(error) => vec![format!("branch {index}: {error}")],
            Ok(parsed) => {
                let mut errors = Vec::new();
                if let Some(Guard::IsA(category)) = &parsed.guard {
                    if !registry.has(category) {
                        errors.push(format!(
                            "branch {index}: unknown category guard [{category}]"
                        ));
                    }
                }
                if let Some(Guard::SelfIsA(category) | Guard::SlotIsA(_, category)) = &parsed.guard
                {
                    if !registry.has(category) {
                        errors.push(format!(
                            "branch {index}: unknown category guard [{category}]"
                        ));
                    }
                }
                if let Some(Guard::SlotIsA(slot, _)) = &parsed.guard {
                    if !slots.iter().any(|decl| decl.name == *slot) {
                        errors.push(format!("branch {index}: unknown slot reference {slot:?}"));
                    }
                }
                for access in accesses(&parsed) {
                    if !slots.iter().any(|slot| slot.name == access.slot) {
                        errors.push(format!(
                            "branch {index}: unknown slot reference {:?}",
                            access.slot
                        ));
                    }
                }
                errors
            }
        })
        .collect()
}

struct EvalContext<'a> {
    fillers: &'a [FillerSnapshot],
    slots: &'a [Slot],
}

enum ReadResult {
    Value(String),
    Unmatched,
    Error(String),
}

fn read_slot(
    access: &SlotAccess,
    context: &EvalContext<'_>,
    reads: &mut Vec<SlotRead>,
) -> ReadResult {
    let Some(slot) = context.slots.iter().find(|slot| slot.name == access.slot) else {
        return ReadResult::Error(format!("unknown slot reference {:?}", access.slot));
    };
    let filler = context
        .fillers
        .iter()
        .find(|filler| filler.slot == access.slot);
    let value = filler
        .and_then(|snapshot| snapshot.scalar(access.dim, &access.path))
        .map(str::to_owned);
    reads.push(SlotRead {
        slot: access.slot.clone(),
        dim: access.dim,
        path: access.path.clone(),
        value: value.clone(),
    });
    match (filler, value) {
        (_, Some(value)) => ReadResult::Value(value),
        // P75:filler 在,但它宣告過(且沒有 `?`)的 feature 沒有值 → Error。
        // 宣告住在 filler 上,所以判斷依據是 snapshot 帶過來的 `required_features`。
        (Some(snapshot), None)
            if snapshot
                .required_features
                .contains(&(access.dim, access.path.clone())) =>
        {
            ReadResult::Error(format!(
                "$slot.{}.{}.{} has no value on filler {:?}; \
                 assign it, or declare it `{} = enum(...)?` if absence is expected (P75)",
                access.slot,
                access.dim.keyword(),
                access.path,
                snapshot.name,
                access.path,
            ))
        }
        (Some(_), None) => ReadResult::Unmatched,
        (None, None) if slot.optional => ReadResult::Unmatched,
        (None, None) => ReadResult::Error(format!(
            "required slot {:?} has no value for {}.{}",
            access.slot,
            access.dim.keyword(),
            access.path
        )),
    }
}

fn resolve_value(
    expression: &ValueExpr,
    sign: &SignDef,
    registry: &OntologyRegistry,
    context: &EvalContext<'_>,
    reads: &mut Vec<SlotRead>,
    self_reads: &mut Vec<SelfRead>,
) -> ReadResult {
    match expression {
        ValueExpr::Literal(value) => ReadResult::Value(value.clone()),
        ValueExpr::Access(Access::Slot(access)) => read_slot(access, context, reads),
        ValueExpr::Access(Access::Self_(access)) => read_self(access, sign, registry, self_reads),
        ValueExpr::Unify(accesses) => {
            let mut value: Option<String> = None;
            for access in accesses {
                let read = match access {
                    Access::Slot(access) => read_slot(access, context, reads),
                    Access::Self_(access) => read_self(access, sign, registry, self_reads),
                };
                match read {
                    ReadResult::Value(candidate) => {
                        if let Some(expected) = &value {
                            if expected != &candidate {
                                return ReadResult::Error(format!(
                                    "unify conflict: {expected:?} != {candidate:?}"
                                ));
                            }
                        } else {
                            value = Some(candidate);
                        }
                    }
                    ReadResult::Unmatched => return ReadResult::Unmatched,
                    ReadResult::Error(error) => return ReadResult::Error(error),
                }
            }
            ReadResult::Value(value.expect("unify arity validated"))
        }
        ValueExpr::Require(accesses) => {
            let mut value: Option<String> = None;
            for access in accesses {
                let read = match access {
                    Access::Slot(access) => read_slot(access, context, reads),
                    Access::Self_(access) => read_self(access, sign, registry, self_reads),
                };
                match read {
                    ReadResult::Value(candidate) => {
                        if let Some(expected) = &value {
                            if expected != &candidate {
                                return ReadResult::Error(format!(
                                    "required value conflict: {expected:?} != {candidate:?}"
                                ));
                            }
                        } else {
                            value = Some(candidate);
                        }
                    }
                    ReadResult::Unmatched => {
                        return ReadResult::Error(
                            "required typed reference has no value".to_owned(),
                        );
                    }
                    ReadResult::Error(error) => return ReadResult::Error(error),
                }
            }
            ReadResult::Value(value.expect("require arity validated"))
        }
    }
}

fn read_self(
    access: &SelfAccess,
    sign: &SignDef,
    registry: &OntologyRegistry,
    reads: &mut Vec<SelfRead>,
) -> ReadResult {
    let path = format!("{}.{}", access.dim.keyword(), access.path);
    let value = sign
        .project(access.dim, registry)
        .get(&path)
        .map(str::to_owned);
    reads.push(SelfRead {
        dim: access.dim,
        path: access.path.clone(),
        value: value.clone(),
    });
    match value {
        Some(value) => ReadResult::Value(value),
        // P75:宣告過、但**沒有** `?` 的 feature,缺席是 Error 而非靜默 `Unmatched`。
        // 封閉清單座標(找不到宣告)不在此列——P75 §3 a 裁定範圍限 typed feature。
        None => match required_feature(sign.items.iter(), access.dim, &access.path) {
            Some(declaration) => ReadResult::Error(absent_feature_message(
                "$self",
                access.dim,
                &access.path,
                &sign.name,
                declaration,
            )),
            None => ReadResult::Unmatched,
        },
    }
}

/// 找出 `(dim, name)` 的 feature 宣告,**且它沒有 `?`**(即缺席為 Error 的那種)。
///
/// 由後往前找,與 `system.rs` 既有的 declaration winner 慣例一致(後寫者勝)。
/// 傳入的 items 必須來自**已 `effective_sign` 解析**的視野,否則看不到繼承來的宣告。
fn required_feature<'a>(
    items: impl Iterator<Item = &'a SignItem>,
    dim: Dim,
    name: &str,
) -> Option<&'a crate::FeatureDecl> {
    items
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find_map(|item| match item {
            SignItem::FeatureDecl(declaration)
                if declaration.dim == dim && declaration.name == name =>
            {
                Some(declaration)
            }
            _ => None,
        })
        .filter(|declaration| !declaration.optional)
}

/// 訊息**必須指出正解**(與 P71 §4.2 同一條原則):作者要嘛給它值,要嘛把宣告
/// 標成 `?`。只說「沒有值」會讓人以為是資料錯,而不知道 `?` 才是表達「本來就可能沒有」的寫法。
fn absent_feature_message(
    subject: &str,
    dim: Dim,
    name: &str,
    owner: &str,
    declaration: &crate::FeatureDecl,
) -> String {
    format!(
        "{subject}.{}.{name} has no value on {owner:?}; \
         assign it, or declare `{name} = enum({})?` if absence is expected (P75)",
        dim.keyword(),
        declaration.values.join(", ")
    )
}

fn source_package(id: &RuleId) -> Option<String> {
    match id.namespace() {
        RuleNamespace::Local | RuleNamespace::Document(_) => None,
        RuleNamespace::Package(package) => Some(package.clone()),
    }
}

#[allow(clippy::too_many_arguments)]
fn record(
    id: &RuleId,
    dim: Dim,
    status: RuleStatus,
    changed: bool,
    branch: Option<usize>,
    diag: Option<String>,
    source: SourceLocation,
    slot_reads: Vec<SlotRead>,
    self_reads: Vec<SelfRead>,
) -> RuleRecord {
    RuleRecord {
        rule_id: id.clone(),
        dim,
        status,
        changed,
        branch,
        diag,
        source,
        source_package: source_package(id),
        slot_reads,
        self_reads,
    }
}

enum GuardResult {
    Matched,
    Unmatched,
    Error(String),
}

/// realization 與 typed `case:` 分支的 guard(system.rs 的 `CASE_INVALID_GUARD` 站)。
/// P71 增修 D 同樣適用:這裡讀的是同一個路徑空間,只是語法位置不同。
pub(crate) fn validate_realization_guard(
    source: &str,
    registry: &OntologyRegistry,
    slots: &[Slot],
    self_features: &BTreeSet<(Dim, String)>,
    filler_features: &BTreeSet<(Dim, String)>,
) -> Result<(), String> {
    match parse_guard(source)? {
        Guard::IsA(category) | Guard::SelfIsA(category) => registry
            .has(&category)
            .then_some(())
            .ok_or_else(|| format!("unknown category guard [{category}]")),
        Guard::SlotIsA(slot, category) => {
            if !slots.iter().any(|item| item.name == slot) {
                return Err(format!("unknown slot reference {slot:?}"));
            }
            registry
                .has(&category)
                .then_some(())
                .ok_or_else(|| format!("unknown category guard [{category}]"))
        }
        Guard::SlotFieldEq(access, _) => {
            if !slots.iter().any(|item| item.name == access.slot) {
                return Err(format!("unknown slot reference {:?}", access.slot));
            }
            match read_path_violation(
                access.dim,
                &access.path,
                &format!("$slot.{}", access.slot),
                filler_features,
            ) {
                Some(violation) => Err(violation),
                None => Ok(()),
            }
        }
        Guard::SelfFieldEq(access, _) => {
            match read_path_violation(access.dim, &access.path, "$self", self_features) {
                Some(violation) => Err(violation),
                None => Ok(()),
            }
        }
        Guard::All(_) => {
            validate_realization_conjuncts(source, registry, slots, self_features, filler_features)
        }
        Guard::BindingFieldEq(_, _) | Guard::BindingIsA(_, _) => {
            Err("`$<name>` bindings exist only in `.chg` function guards".to_owned())
        }
        Guard::FieldEq(_, _) => {
            Err("realization guards require explicit `$self` or `$slot` reads".to_owned())
        }
    }
}

fn validate_realization_conjuncts(
    source: &str,
    registry: &OntologyRegistry,
    slots: &[Slot],
    self_features: &BTreeSet<(Dim, String)>,
    filler_features: &BTreeSet<(Dim, String)>,
) -> Result<(), String> {
    source
        .split("&&")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .try_for_each(|part| {
            validate_realization_guard(part, registry, slots, self_features, filler_features)
        })
}

/// 求值環境裡的具名綁定(P91):`$x` → 一個 sign。`.lang` 側恆為空。
pub type GuardBindings<'a> = std::collections::BTreeMap<String, &'a SignDef>;

#[allow(clippy::too_many_arguments)]
fn guard_matches(
    guard: &Guard,
    sign: &SignDef,
    dim: Dim,
    registry: &OntologyRegistry,
    context: &EvalContext<'_>,
    reads: &mut Vec<SlotRead>,
    self_reads: &mut Vec<SelfRead>,
    bindings: &GuardBindings<'_>,
    scalars: &std::collections::BTreeMap<String, String>,
) -> GuardResult {
    match guard {
        Guard::All(parts) => {
            for part in parts {
                match guard_matches(
                    part, sign, dim, registry, context, reads, self_reads, bindings, scalars,
                ) {
                    GuardResult::Matched => {}
                    other => return other,
                }
            }
            GuardResult::Matched
        }
        Guard::BindingIsA(name, category) => {
            let Some(bound) = bindings.get(name) else {
                return GuardResult::Error(format!("guard references unbound `${name}`"));
            };
            if !registry.has(category) {
                return GuardResult::Error(format!("unknown category guard [{category}]"));
            }
            if registry
                .sign_categories(bound)
                .iter()
                .any(|item| item == category)
            {
                GuardResult::Matched
            } else {
                GuardResult::Unmatched
            }
        }
        Guard::BindingFieldEq(access, expected) => {
            let read = |access: &BindingAccess| -> Result<Option<String>, String> {
                let bound = bindings
                    .get(&access.name)
                    .ok_or_else(|| format!("guard references unbound `${}`", access.name))?;
                let path = format!("{}.{}", access.dim.keyword(), access.path);
                Ok(bound
                    .project(access.dim, registry)
                    .get(&path)
                    .map(str::to_owned))
            };
            let left = match read(access) {
                Ok(value) => value,
                Err(error) => return GuardResult::Error(error),
            };
            let right = match expected {
                BindingOperand::Literal(value) => Some(value.clone()),
                BindingOperand::Scalar(name) => match scalars.get(name) {
                    Some(value) => Some(value.clone()),
                    None => {
                        return GuardResult::Error(format!("guard references unbound `${name}`"))
                    }
                },
                BindingOperand::Access(access) => match read(access) {
                    Ok(value) => value,
                    Err(error) => return GuardResult::Error(error),
                },
            };
            // 兩端都讀不到不算相等——缺席不是一種值。
            match (left, right) {
                (Some(left), Some(right)) if left == right => GuardResult::Matched,
                _ => GuardResult::Unmatched,
            }
        }
        Guard::IsA(category) => {
            if !registry.has(category) {
                return GuardResult::Error(format!("unknown category guard [{category}]"));
            }
            if registry
                .sign_categories(sign)
                .iter()
                .any(|candidate| candidate == category)
            {
                GuardResult::Matched
            } else {
                GuardResult::Unmatched
            }
        }
        Guard::FieldEq(field, expected) => {
            let path = format!("{}.{}", dim.keyword(), field);
            if sign.project(dim, registry).get(&path) == Some(expected.as_str()) {
                GuardResult::Matched
            } else {
                GuardResult::Unmatched
            }
        }
        Guard::SlotFieldEq(access, expected) => match read_slot(access, context, reads) {
            ReadResult::Value(value) if value == *expected => GuardResult::Matched,
            ReadResult::Value(_) | ReadResult::Unmatched => GuardResult::Unmatched,
            ReadResult::Error(error) => GuardResult::Error(error),
        },
        Guard::SelfFieldEq(access, expected) => {
            match read_self(access, sign, registry, self_reads) {
                ReadResult::Value(value) if value == *expected => GuardResult::Matched,
                ReadResult::Value(_) | ReadResult::Unmatched => GuardResult::Unmatched,
                ReadResult::Error(error) => GuardResult::Error(error),
            }
        }
        Guard::SelfIsA(category) => {
            if !registry.has(category) {
                GuardResult::Error(format!("unknown category guard [{category}]"))
            } else if registry
                .sign_categories(sign)
                .iter()
                .any(|item| item == category)
            {
                GuardResult::Matched
            } else {
                GuardResult::Unmatched
            }
        }
        Guard::SlotIsA(slot, category) => {
            if !registry.has(category) {
                return GuardResult::Error(format!("unknown category guard [{category}]"));
            }
            let Some(declaration) = context.slots.iter().find(|item| item.name == *slot) else {
                return GuardResult::Error(format!("unknown slot reference {slot:?}"));
            };
            let filler = context.fillers.iter().find(|item| item.slot == *slot);
            match filler {
                Some(filler) if filler.categories.iter().any(|item| item == category) => {
                    GuardResult::Matched
                }
                Some(_) => GuardResult::Unmatched,
                None if declaration.optional || context.fillers.is_empty() => {
                    GuardResult::Unmatched
                }
                None => GuardResult::Error(format!("required slot {slot:?} is unfilled")),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn eval_one_branch(
    id: &RuleId,
    branch_index: usize,
    branch: &str,
    sign: &SignDef,
    dim: Dim,
    registry: &OntologyRegistry,
    source: SourceLocation,
    context: &EvalContext<'_>,
) -> (RuleRecord, Option<Patch>) {
    let parsed = match parse_dim_rule(branch) {
        Ok(parsed) => parsed,
        Err(error) => {
            return (
                record(
                    id,
                    dim,
                    RuleStatus::Error,
                    false,
                    None,
                    Some(format!("branch {branch_index}: {error}")),
                    source,
                    Vec::new(),
                    Vec::new(),
                ),
                None,
            );
        }
    };
    let mut reads = Vec::new();
    let mut self_reads = Vec::new();
    if let Some(guard) = &parsed.guard {
        match guard_matches(
            guard,
            sign,
            dim,
            registry,
            context,
            &mut reads,
            &mut self_reads,
            &GuardBindings::new(),
            &std::collections::BTreeMap::new(),
        ) {
            GuardResult::Matched => {}
            GuardResult::Unmatched => {
                return (
                    record(
                        id,
                        dim,
                        RuleStatus::Unmatched,
                        false,
                        Some(branch_index),
                        None,
                        source,
                        reads,
                        self_reads,
                    ),
                    None,
                );
            }
            GuardResult::Error(error) => {
                return (
                    record(
                        id,
                        dim,
                        RuleStatus::Error,
                        false,
                        None,
                        Some(format!("branch {branch_index}: {error}")),
                        source,
                        reads,
                        self_reads,
                    ),
                    None,
                );
            }
        }
    }
    let value = match resolve_value(
        &parsed.value,
        sign,
        registry,
        context,
        &mut reads,
        &mut self_reads,
    ) {
        ReadResult::Value(value) => value,
        ReadResult::Unmatched => {
            return (
                record(
                    id,
                    dim,
                    RuleStatus::Unmatched,
                    false,
                    Some(branch_index),
                    None,
                    source,
                    reads,
                    self_reads,
                ),
                None,
            );
        }
        ReadResult::Error(error) => {
            return (
                record(
                    id,
                    dim,
                    RuleStatus::Error,
                    false,
                    None,
                    Some(format!("branch {branch_index}: {error}")),
                    source,
                    reads,
                    self_reads,
                ),
                None,
            );
        }
    };
    if let Some(declaration) = sign.items.iter().find_map(|item| match item {
        SignItem::FeatureDecl(feature) if feature.dim == dim && feature.name == parsed.field => {
            Some(feature)
        }
        _ => None,
    }) {
        if !declaration.values.contains(&value) {
            return (
                record(
                    id,
                    dim,
                    RuleStatus::Error,
                    false,
                    None,
                    Some(format!(
                        "branch {branch_index}: value {value:?} is outside enum({}) for {}.{}",
                        declaration.values.join(", "),
                        dim.keyword(),
                        parsed.field
                    )),
                    source,
                    reads,
                    self_reads,
                ),
                None,
            );
        }
    }
    let path = format!("{}.{}", dim.keyword(), parsed.field);
    let changed = sign.project(dim, registry).get(&path) != Some(value.as_str());
    (
        record(
            id,
            dim,
            RuleStatus::Matched,
            changed,
            Some(branch_index),
            None,
            source,
            reads,
            self_reads,
        ),
        Some(Patch::for_dim(dim).set(&parsed.field, &value)),
    )
}

fn eval_else(
    rule: &crate::Rule,
    sign: &SignDef,
    registry: &OntologyRegistry,
    sources: &[SourceLocation],
    context: &EvalContext<'_>,
) -> (RuleRecord, Option<Patch>) {
    for (index, branch) in std::iter::once(rule.body.as_str())
        .chain(rule.else_chain.iter().map(String::as_str))
        .enumerate()
    {
        let (record, patch) = eval_one_branch(
            &rule.id,
            index,
            branch,
            sign,
            rule.dim,
            registry,
            sources.get(index).copied().unwrap_or_default(),
            context,
        );
        match record.status {
            RuleStatus::Matched | RuleStatus::Error => return (record, patch),
            RuleStatus::Unmatched => {}
        }
    }
    (
        record(
            &rule.id,
            rule.dim,
            RuleStatus::Unmatched,
            false,
            None,
            None,
            sources.first().copied().unwrap_or_default(),
            Vec::new(),
            Vec::new(),
        ),
        None,
    )
}

fn eval_then(
    rule: &crate::Rule,
    sign: &SignDef,
    registry: &OntologyRegistry,
    sources: &[SourceLocation],
    context: &EvalContext<'_>,
) -> (SignDef, Vec<RuleRecord>) {
    let mut current = sign.clone();
    let mut records = Vec::new();
    for (index, branch) in std::iter::once(rule.body.as_str())
        .chain(rule.then_chain.iter().map(String::as_str))
        .enumerate()
    {
        let (record, patch) = eval_one_branch(
            &rule.id,
            index,
            branch,
            &current,
            rule.dim,
            registry,
            sources.get(index).copied().unwrap_or_default(),
            context,
        );
        if let Some(patch) = patch {
            current = patch.apply(&current);
        }
        let stop = record.status == RuleStatus::Error;
        records.push(record);
        if stop {
            break;
        }
    }
    (current, records)
}

fn run_dim_rules(
    sign: &SignDef,
    dim: Dim,
    registry: &OntologyRegistry,
    fillers: &[FillerSnapshot],
    slots: &[Slot],
) -> (SignDef, Vec<RuleRecord>) {
    let rules = sign
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::Rule(rule) | SignItem::FeatureRule(rule) if rule.dim == dim => {
                Some(rule.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let context = EvalContext { fillers, slots };
    let mut current = sign.clone();
    let mut records = Vec::new();
    for rule in rules {
        let sources = std::iter::once(rule.source)
            .chain(rule.branch_sources.iter().copied())
            .collect::<Vec<_>>();
        if rule.then_chain.is_empty() {
            let (record, patch) = eval_else(&rule, &current, registry, &sources, &context);
            if let Some(patch) = patch {
                current = patch.apply(&current);
            }
            records.push(record);
        } else {
            let (next, branch_records) = eval_then(&rule, &current, registry, &sources, &context);
            current = next;
            records.extend(branch_records);
        }
    }
    (current, records)
}

pub fn run_sign_dim_rules(
    sign: &SignDef,
    dim: Dim,
    registry: &OntologyRegistry,
) -> (SignDef, Vec<RuleRecord>) {
    let slots = slots_of(sign);
    run_dim_rules(sign, dim, registry, &[], &slots)
}

/// Evaluate V2 enum-valued cases on a stored Sign.  Construction occurrence
/// evaluation uses this same transition after each dimension's ordinary
/// rules, so a Sign behaves identically whether evaluated directly or used as
/// a filler.  The function is deliberately dimension-local and never writes
/// another pole.
pub(crate) fn run_sign_feature_expressions(
    sign: &SignDef,
    dim: Dim,
    registry: &OntologyRegistry,
) -> Result<SignDef, String> {
    fn scalar(sign: &SignDef, path: &str, registry: &OntologyRegistry) -> Option<String> {
        let path = path.strip_prefix("$self.").unwrap_or(path);
        let (dimension, _) = path.split_once('.')?;
        let dimension = Dim::parse(dimension)?;
        sign.project(dimension, registry)
            .defs
            .into_iter()
            .rev()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, value)| value)
    }

    let expressions = sign
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::FeatureExpression(expression) if expression.dim == dim => {
                Some(expression.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut current = sign.clone();
    for expression in expressions {
        let declaration = current.items.iter().rev().find_map(|item| match item {
            SignItem::FeatureDecl(declaration)
                if declaration.dim == dim && declaration.name == expression.name =>
            {
                Some(declaration)
            }
            _ => None,
        });
        let Some(declaration) = declaration else {
            return Err(format!(
                "FEATURE_EXPRESSION_UNDECLARED: {}.{}",
                dim.keyword(),
                expression.name
            ));
        };
        let Expression::Case(case) = &expression.expression else {
            return Err(format!(
                "FEATURE_EXPRESSION_NOT_CASE: {}.{}",
                dim.keyword(),
                expression.name
            ));
        };
        let mut selected = None;
        for branch in &case.branches {
            let matched = match &branch.condition {
                CaseCondition::Else => true,
                CaseCondition::Guard(guard) => {
                    let (status, _, _, error) = evaluate_sign_guard(&current, guard, registry);
                    match status {
                        RuleStatus::Matched => true,
                        RuleStatus::Unmatched => false,
                        RuleStatus::Error => {
                            return Err(error.unwrap_or_else(|| {
                                format!("CASE_GUARD_ERROR: guard {guard:?} failed")
                            }))
                        }
                    }
                }
                CaseCondition::Equals(expected) => {
                    let scrutinee = case.scrutinee.as_deref().ok_or_else(|| {
                        "CASE_SCRUTINEE_MISSING: equality case has no scrutinee".to_owned()
                    })?;
                    scalar(&current, scrutinee, registry).as_deref() == Some(expected.as_str())
                }
            };
            if !matched {
                continue;
            }
            let Expression::EnumValue(value) = &branch.result else {
                return Err(format!(
                    "CASE_BRANCH_TYPE_MISMATCH: {}.{} must return an enum value",
                    dim.keyword(),
                    expression.name
                ));
            };
            if !declaration.values.contains(value) {
                return Err(format!(
                    "FEATURE_EXPRESSION_VALUE_OUT_OF_DOMAIN: {value:?} is outside enum({})",
                    declaration.values.join(", ")
                ));
            }
            selected = Some(value.clone());
            break;
        }
        if let Some(value) = selected {
            current = Patch::for_dim(dim)
                .set(&expression.name, &value)
                .apply(&current);
        } else if scalar(
            &current,
            &format!("{}.{}", dim.keyword(), expression.name),
            registry,
        )
        .is_none()
        {
            return Err(format!(
                "CASE_DEFAULT_MISSING: {}.{} has no matching branch and no base value",
                dim.keyword(),
                expression.name
            ));
        }
    }
    Ok(current)
}

fn token_sign(token: &crate::construction::DerivedToken) -> SignDef {
    let mut items = token
        .rule_sign
        .items
        .iter()
        .filter(|item| {
            matches!(
                item,
                SignItem::TraitMount {
                    name: _,
                    kind: crate::TraitMountKind::Declaration,
                    ..
                } | SignItem::Slot(_)
                    | SignItem::FeatureDecl(_)
                    | SignItem::RoleDecl(_)
                    | SignItem::RoleBinding(_)
                    | SignItem::Realization(_)
                    | SignItem::Rule(_)
                    | SignItem::FeatureRule(_)
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    for (path, value) in token.phon.iter().chain(&token.syn).chain(&token.prag) {
        items.push(SignItem::Def(Def {
            path: path.clone(),
            value: value.clone(),
        }));
    }
    for (field, value) in &token.sem.fields {
        items.push(SignItem::Def(Def {
            path: format!("sem.{field}"),
            value: value.clone(),
        }));
    }
    for (field, value) in &token.sem.features {
        items.push(SignItem::Def(Def {
            path: format!("sem.{field}"),
            value: value.clone(),
        }));
    }
    SignDef {
        id: token.rule_sign.id.clone(),
        name: format!("{}#token", token.construction),
        items,
    }
}

pub(crate) fn evaluate_token_guard(
    token: &crate::construction::DerivedToken,
    source: &str,
    registry: &OntologyRegistry,
) -> (RuleStatus, Vec<SlotRead>, Vec<SelfRead>, Option<String>) {
    let guard = match parse_guard(source) {
        Ok(guard) => guard,
        Err(error) => return (RuleStatus::Error, Vec::new(), Vec::new(), Some(error)),
    };
    let sign = token_sign(token);
    let slots = slots_of(&token.rule_sign);
    let context = EvalContext {
        fillers: &token.fillers,
        slots: &slots,
    };
    let mut slot_reads = Vec::new();
    let mut self_reads = Vec::new();
    let result = guard_matches(
        &guard,
        &sign,
        Dim::Phon,
        registry,
        &context,
        &mut slot_reads,
        &mut self_reads,
        &GuardBindings::new(),
        &std::collections::BTreeMap::new(),
    );
    match result {
        GuardResult::Matched => (RuleStatus::Matched, slot_reads, self_reads, None),
        GuardResult::Unmatched => (RuleStatus::Unmatched, slot_reads, self_reads, None),
        GuardResult::Error(error) => (RuleStatus::Error, slot_reads, self_reads, Some(error)),
    }
}

/// Evaluate a realization guard for a stored sign after occurrence-local
/// constraints have been applied.  This is intentionally read-only: it lets
/// a nominal choose an allomorph such as `she`/`her` without mutating the
/// lexicon or exposing the rest of the construction token.
/// Evaluate a `.lang` guard against one sign.
///
/// Public so the **diachronic function layer can reuse the same guard language**
/// (P48: 「body 的執行語意由既有的 `case`/`when` 承載」). Exposing this instead of
/// letting `changeset` grow its own predicate evaluator keeps a single source of
/// truth for what a guard means; two evaluators would drift and the drift would be
/// silent (a guard that means one thing synchronically and another diachronically).
///
/// The subject is always `$self`. A function guard such as `verb.syn.category == verb`
/// is rewritten by the caller to `$self.…` after binding its parameter, so this
/// function stays ignorant of function parameters.
///
/// `Err` carries the evaluator's own diagnostic; an unparseable or ill-typed guard is
/// **never** reported as "unmatched".
/// 在**具名綁定環境**上求值一句 guard(P91)。
///
/// `.chg` 的 function guard 走這個入口:主體是 `$<參數名>`,由 `bindings`
/// 解析。先前的作法是把參數名**文字代換成 `$self`** 再呼叫
/// [`guard_matches_sign`]——那條路徑只有一個隱含主體,所以跨參數的 guard
/// 表達不出來(`FUNCTION_GUARD_MULTI_SUBJECT`)。改成環境之後限制消失,而
/// 兩邊仍是**同一個求值器**(不得另寫第二套述詞語意)。
pub fn guard_matches_bindings(
    guard: &str,
    bindings: &GuardBindings<'_>,
    scalars: &std::collections::BTreeMap<String, String>,
    registry: &OntologyRegistry,
) -> Result<bool, String> {
    let parsed = parse_guard(guard)?;
    // function 層沒有 ambient sign;無主體形(`[Trait]`、裸 `field ==`)因此
    // 無所依附,交給呼叫端當「沒有主體」處理。
    let placeholder = SignDef {
        id: crate::SignId::synthetic(),
        name: String::new(),
        items: Vec::new(),
    };
    let context = EvalContext {
        fillers: &[],
        slots: &[],
    };
    let (mut reads, mut self_reads) = (Vec::new(), Vec::new());
    match guard_matches(
        &parsed,
        &placeholder,
        Dim::Syn,
        registry,
        &context,
        &mut reads,
        &mut self_reads,
        bindings,
        scalars,
    ) {
        GuardResult::Matched => Ok(true),
        GuardResult::Unmatched => Ok(false),
        GuardResult::Error(error) => Err(error),
    }
}

/// 一句 guard 引用到的具名綁定(P91),依**角色**分開。
///
/// 角色決定型別要求:當**主體**用的(`$x.<dim>.<path>`、`$x == [Trait]`)必須
/// 綁到一個 sign;當**純量值**用的(`… == $y`)綁的是字面值。呼叫端據此決定
/// 該報「不是 sign」還是直接取值。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuardBindingRoles {
    /// 路徑頭與範疇測試的主體。
    pub subjects: Vec<String>,
    /// 右端的純量參數。
    pub scalars: Vec<String>,
}

pub fn guard_binding_roles(guard: &str) -> GuardBindingRoles {
    fn walk(guard: &Guard, out: &mut GuardBindingRoles) {
        match guard {
            Guard::All(parts) => parts.iter().for_each(|part| walk(part, out)),
            Guard::BindingIsA(name, _) => out.subjects.push(name.clone()),
            Guard::BindingFieldEq(access, operand) => {
                out.subjects.push(access.name.clone());
                match operand {
                    BindingOperand::Access(right) => out.subjects.push(right.name.clone()),
                    BindingOperand::Scalar(name) => out.scalars.push(name.clone()),
                    BindingOperand::Literal(_) => {}
                }
            }
            _ => {}
        }
    }
    let mut out = GuardBindingRoles::default();
    if let Ok(parsed) = parse_guard(guard) {
        walk(&parsed, &mut out);
    }
    out.subjects.sort();
    out.subjects.dedup();
    out.scalars.sort();
    out.scalars.dedup();
    out
}

pub fn guard_matches_sign(
    sign: &SignDef,
    guard: &str,
    registry: &OntologyRegistry,
) -> Result<bool, String> {
    match evaluate_sign_guard(sign, guard, registry) {
        (RuleStatus::Matched, _, _, _) => Ok(true),
        (RuleStatus::Unmatched, _, _, _) => Ok(false),
        (RuleStatus::Error, _, _, error) => {
            Err(error.unwrap_or_else(|| format!("GUARD_ERROR: {guard:?}")))
        }
    }
}

pub(crate) fn evaluate_sign_guard(
    sign: &SignDef,
    source: &str,
    registry: &OntologyRegistry,
) -> (RuleStatus, Vec<SlotRead>, Vec<SelfRead>, Option<String>) {
    let guard = match parse_guard(source) {
        Ok(guard) => guard,
        Err(error) => return (RuleStatus::Error, Vec::new(), Vec::new(), Some(error)),
    };
    let slots = slots_of(sign);
    let context = EvalContext {
        fillers: &[],
        slots: &slots,
    };
    let mut slot_reads = Vec::new();
    let mut self_reads = Vec::new();
    let result = guard_matches(
        &guard,
        sign,
        Dim::Phon,
        registry,
        &context,
        &mut slot_reads,
        &mut self_reads,
        &GuardBindings::new(),
        &std::collections::BTreeMap::new(),
    );
    match result {
        GuardResult::Matched => (RuleStatus::Matched, slot_reads, self_reads, None),
        GuardResult::Unmatched => (RuleStatus::Unmatched, slot_reads, self_reads, None),
        GuardResult::Error(error) => (RuleStatus::Error, slot_reads, self_reads, Some(error)),
    }
}

pub fn run_token_dim_rules(
    token: &crate::construction::DerivedToken,
    dim: Dim,
    registry: &OntologyRegistry,
) -> (crate::construction::DerivedToken, Vec<RuleRecord>) {
    assert!(dim != Dim::Phon, "phon token rules execute in Tshiatūn");
    let mut items = token
        .rule_sign
        .items
        .iter()
        .filter(|item| {
            matches!(
                item,
                SignItem::TraitMount {
                    name: _,
                    kind: crate::TraitMountKind::Declaration,
                    ..
                } | SignItem::Slot(_)
                    | SignItem::FeatureDecl(_)
                    | SignItem::RoleDecl(_)
                    | SignItem::RoleBinding(_)
                    | SignItem::Realization(_)
                    | SignItem::Rule(_)
                    | SignItem::FeatureRule(_)
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    items.extend(token.phon.iter().map(|(path, value)| {
        SignItem::Def(Def {
            path: path.clone(),
            value: value.clone(),
        })
    }));
    items.extend(token.syn.iter().map(|(path, value)| {
        SignItem::Def(Def {
            path: path.clone(),
            value: value.clone(),
        })
    }));
    items.extend(token.sem.fields.iter().map(|(field, value)| {
        SignItem::Def(Def {
            path: format!("sem.{field}"),
            value: value.clone(),
        })
    }));
    items.extend(token.sem.features.iter().map(|(field, value)| {
        SignItem::Def(Def {
            path: format!("sem.{field}"),
            value: value.clone(),
        })
    }));
    items.extend(token.prag.iter().map(|(path, value)| {
        SignItem::Def(Def {
            path: path.clone(),
            value: value.clone(),
        })
    }));
    let sign = SignDef {
        id: token.rule_sign.id.clone(),
        name: format!("{}#token", token.construction),
        items,
    };
    let slots = slots_of(&token.rule_sign);
    let (sign, records) = run_dim_rules(&sign, dim, registry, &token.fillers, &slots);
    let mut output = token.clone();
    match dim {
        Dim::Syn => output.syn = sign.project(Dim::Syn, registry).defs,
        Dim::Sem => {
            let declarations = token
                .rule_sign
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::FeatureDecl(feature) if feature.dim == Dim::Sem => {
                        Some(feature.name.as_str())
                    }
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>();
            output.sem.fields.clear();
            output.sem.features.clear();
            for (path, value) in sign.project(Dim::Sem, registry).defs {
                let name = path.strip_prefix("sem.").unwrap_or(&path).to_owned();
                if declarations.contains(name.as_str()) {
                    output.sem.features.insert(name, value);
                } else {
                    output.sem.fields.push((name, value));
                }
            }
        }
        Dim::Prag => output.prag = sign.project(Dim::Prag, registry).defs,
        Dim::Phon => unreachable!(),
    }
    (output, records)
}
