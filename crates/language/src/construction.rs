//! Construction 與 slots(步驟 12b;修補07 P41/P42,I21)。
//!
//! **Construction 就是帶 slots 的 Sign**(P42):無 slot = 詞彙 sign,≥1 slot =
//! construction。**Valence = slots 結構**(P41):三價 = 三個必填 slot,不設數字
//! 欄位。填 slot → **新的 derived token**(暫態,不進庫,不就地改原 sign)。
//! filler 授權 = filler 的 **syn `belongs` 閉包**須含 slot 約束範疇(P40,複用 12a)。
//! optional slot 以 **`?`** 標記(I21)。
//!
//! phon 投影(12b 子集):construction 的 `phon` Def 是**模板**(`{slot}` 引用 +
//! 字面素材,如 `ge{stem}t` 環綴);application 代入 filler 的 UR、字面素材直通,
//! 產出 derived UR；`DerivedToken` 同時攜 phon/syn/sem/prag、分類、殘餘 slots 與
//! provenance，並可遞迴充當另一 construction 的 filler。

use crate::ontology::OntologyRegistry;
use crate::sem::{self, SemNode};
use crate::synchronic::{self, Patch, RuleRecord, RuleStatus};
pub use crate::SlotMapOp;
use crate::{
    CaseCondition, Dim, Expression, Language, SignDef, SignItem, Slot, SlotConstraint,
    SourceLocation, TypedCase,
};
use std::collections::BTreeMap;
use tshiatun_dsl::{build_phrase, run_program, surface_phrase, Program};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CxgError {
    #[error("no sign named {0:?}")]
    UnknownConstruction(String),
    #[error("sign {0:?} has no slots (not a construction)")]
    NotAConstruction(String),
    #[error("construction {construction:?} has no slot named {slot:?}")]
    UnknownSlot { construction: String, slot: String },
    #[error("slot {0:?} filled more than once")]
    DuplicateFill(String),
    #[error("no filler sign named {0:?}")]
    UnknownFiller(String),
    #[error("filler {0:?} has no phon UR (`phon = /…/`)")]
    FillerNoUr(String),
    /// filler 的 syn 範疇閉包不含 slot 要求的範疇(P40 授權失敗)。
    #[error("slot {slot:?} requires [{required}], but filler {filler:?} is not (has: {has:?})")]
    CategoryMismatch {
        slot: String,
        filler: String,
        required: String,
        has: Vec<String>,
    },
    /// 模板引用了不存在的 slot 名。
    #[error("construction {construction:?} phon template references unknown slot {slot:?}")]
    TemplateSlotUnknown { construction: String, slot: String },
    /// 對未飽和 token(仍有必填 slot 未填)求表層。
    #[error("token still needs required slot(s): {0:?}")]
    Unsaturated(Vec<String>),
    #[error("free variable {name:?} has incompatible constraints [{left}] and [{right}]")]
    ResidualConstraintConflict {
        name: String,
        left: String,
        right: String,
    },
    /// sem role `{ref}` 引用的名稱不是 construction 的 slot(語意引用無法解析)。
    #[error("construction {construction:?} sem role {role:?} references unknown slot {slot:?}")]
    SemRefUnknown {
        construction: String,
        role: String,
        slot: String,
    },
    #[error("construction {construction:?} binds undeclared semantic role {role:?}")]
    UnknownRole { construction: String, role: String },
    #[error(
        "saturated construction {construction:?} is missing required semantic role(s): {roles:?}"
    )]
    MissingRoles {
        construction: String,
        roles: Vec<String>,
    },
    #[error("semantic role {role:?} requires [{required}], but filler has {has:?}")]
    RoleCategoryMismatch {
        role: String,
        required: String,
        has: Vec<String>,
    },
    #[error("realization must be selected through CompiledSystem::realize_phon")]
    RealizationRequiresSystem,
    #[error("slot mapping refers to unknown slot {0:?}")]
    SlotMapUnknown(String),
    #[error("slot mapping applies {operation} more than once to {slot:?}")]
    SlotMapDuplicate {
        slot: String,
        operation: &'static str,
    },
    #[error("slot mapping exposes duplicate external slot name {0:?}")]
    SlotMapNameCollision(String),
    #[error("slot mapping rename target must be a single identifier, got {0:?}")]
    SlotMapInvalidName(String),
    #[error("internal required slot {0:?} is not auto-filled")]
    InternalRequiredUnfilled(String),
    #[error("construction {construction:?} slot feature binding targets unknown slot {slot:?}")]
    SlotFeatureUnknownTarget { construction: String, slot: String },
    #[error("construction {construction:?} cannot bind feature {feature:?} on unconstrained [*] slot {slot:?}")]
    SlotFeatureAnySign {
        construction: String,
        slot: String,
        feature: String,
    },
    #[error("slot {slot:?} filler does not declare syn feature {feature:?}")]
    SlotFeatureUndeclared { slot: String, feature: String },
    #[error("slot feature binding reads unknown slot {0:?}")]
    SlotFeatureUnknownSource(String),
    #[error(
        "slot feature binding cannot read syn feature {feature:?} from unfilled slot {slot:?}"
    )]
    SlotFeatureSourceMissing { slot: String, feature: String },
    #[error("slot {slot:?} feature {feature:?} value {value:?} is outside enum({domain})")]
    SlotFeatureOutOfDomain {
        slot: String,
        feature: String,
        value: String,
        domain: String,
    },
    #[error("slot {slot:?} feature {feature:?} is fixed to {actual:?}, conflicting with assigned {expected:?}")]
    SlotFeatureConflict {
        slot: String,
        feature: String,
        expected: String,
        actual: String,
    },
    #[error("slot {slot:?} feature {feature:?} is assigned more than once")]
    SlotFeatureDuplicateTarget { slot: String, feature: String },
    #[error("constraint refers to unknown slot {0:?}")]
    ConstraintUnknownSlot(String),
    #[error("equal operands must be syn enum features, got {0:?}")]
    ConstraintInvalidFeature(String),
    #[error("equal operands use different enum domains: {left:?} versus {right:?}")]
    ConstraintDomainMismatch { left: String, right: String },
    #[error("equal constraint conflicts: {left:?} != {right:?}")]
    ConstraintEqualityConflict { left: String, right: String },
    #[error("form order constraints contain a cycle")]
    ConstraintOrderCycle,
    #[error("{predicate}({left}, {right}) is not satisfied by phon template {template:?}")]
    ConstraintOrderConflict {
        predicate: &'static str,
        left: String,
        right: String,
        template: String,
    },
    #[error("stored filler {filler:?} realization guard failed: {message}")]
    FillerRealizationGuard { filler: String, message: String },
    #[error("typed feature expression failed: {0}")]
    FeatureExpression(String),
    #[error("stored filler {filler:?} realization did not produce pure phon input: {input:?}")]
    ImpureFillerRealization { filler: String, input: String },
    #[error("engine: {0}")]
    Engine(String),
}

/// `.lang` 與 Rust 共用的 typed slot mapping。完整操作集在消耗 filler 前驗證。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotMap {
    ops: Vec<SlotMapOp>,
}

impl SlotMap {
    pub fn identity() -> SlotMap {
        SlotMap::default()
    }

    pub fn preserve(mut self, slot: impl Into<String>) -> SlotMap {
        self.ops.push(SlotMapOp::Preserve { slot: slot.into() });
        self
    }

    pub fn rename(mut self, slot: impl Into<String>, to: impl Into<String>) -> SlotMap {
        self.ops.push(SlotMapOp::Rename {
            slot: slot.into(),
            to: to.into(),
        });
        self
    }

    pub fn autofill(mut self, slot: impl Into<String>, filler: impl Into<String>) -> SlotMap {
        self.ops.push(SlotMapOp::AutoFill {
            slot: slot.into(),
            filler: filler.into(),
        });
        self
    }

    pub fn internalize(mut self, slot: impl Into<String>) -> SlotMap {
        self.ops.push(SlotMapOp::Internalize { slot: slot.into() });
        self
    }

    pub fn optional(mut self, slot: impl Into<String>, optional: bool) -> SlotMap {
        self.ops.push(SlotMapOp::Optional {
            slot: slot.into(),
            optional,
        });
        self
    }

    pub fn ops(&self) -> &[SlotMapOp] {
        &self.ops
    }

    pub fn from_ops(ops: impl IntoIterator<Item = SlotMapOp>) -> SlotMap {
        SlotMap {
            ops: ops.into_iter().collect(),
        }
    }

    pub fn and(mut self, other: &SlotMap) -> SlotMap {
        self.ops.extend(other.ops.iter().cloned());
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Filler<'a> {
    Sign(&'a str),
    Token(&'a DerivedToken),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoundFiller {
    Owned {
        committed: SignDef,
        base: SignDef,
        provenance: FillerProvenance,
    },
    Derived(Box<DerivedToken>),
}

#[derive(Debug, Clone, Copy)]
pub struct SlotFiller<'a> {
    pub slot: &'a str,
    pub filler: Filler<'a>,
}

impl<'a> SlotFiller<'a> {
    pub fn sign(slot: &'a str, sign: &'a str) -> SlotFiller<'a> {
        SlotFiller {
            slot,
            filler: Filler::Sign(sign),
        }
    }

    pub fn token(slot: &'a str, token: &'a DerivedToken) -> SlotFiller<'a> {
        SlotFiller {
            slot,
            filler: Filler::Token(token),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FillerProvenance {
    StoredSign(String),
    Derived(Box<TokenProvenance>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotProvenance {
    pub slot: String,
    pub source: FillerProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenProvenance {
    pub construction: String,
    pub fillers: Vec<SlotProvenance>,
}

/// Immutable, fully evaluated filler state captured before construction rules
/// run. Slot-aware rules read this snapshot; later token patches cannot mutate
/// it or the source sign/token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillerSnapshot {
    pub slot: String,
    pub name: String,
    pub categories: Vec<String>,
    pub phon: Vec<(String, String)>,
    pub syn: Vec<(String, String)>,
    pub sem: SemNode,
    pub prag: Vec<(String, String)>,
    pub provenance: FillerProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrenceRecord {
    pub slot_path: String,
    pub source: FillerProvenance,
    pub constraints: Vec<(Dim, String, String)>,
    pub reevaluated: bool,
    pub probe_rules: Vec<RuleRecord>,
    pub committed_rules: Vec<RuleRecord>,
    /// Feature/role case branches evaluated while rebuilding this occurrence
    /// from its immutable deep state.  These records belong to the filler
    /// occurrence, not to the enclosing construction's case trace.
    pub cases: Vec<OccurrenceCaseRecord>,
    pub realization: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceCaseStatus {
    Matched,
    Unmatched,
    MoreSpecificBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrenceCaseRecord {
    /// Dimension-local target, for example `sem.feature.interpreted_case` or
    /// `sem.role.agent`.
    pub target: String,
    pub branch: usize,
    pub status: OccurrenceCaseStatus,
    pub source: SourceLocation,
    pub diagnostic_code: Option<&'static str>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TokenExpressionError {
    #[error("feature {dim:?}.{name} is undeclared")]
    UndeclaredFeature { dim: Dim, name: String },
    #[error("feature {dim:?}.{name} value {value:?} is outside enum({domain})")]
    FeatureOutOfDomain {
        dim: Dim,
        name: String,
        value: String,
        domain: String,
    },
    #[error("CASE_DEFAULT_MISSING: {context} has no matching branch and no base value")]
    CaseDefaultMissing { context: String },
    #[error("invalid token expression: {0}")]
    Invalid(String),
    #[error(transparent)]
    Construction(#[from] CxgError),
}

impl TokenExpressionError {
    fn into_construction_error(self) -> CxgError {
        match self {
            Self::Construction(error) => error,
            other => CxgError::FeatureExpression(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeepTokenState {
    phon: Vec<(String, String)>,
    syn: Vec<(String, String)>,
    sem: SemNode,
    prag: Vec<(String, String)>,
}

impl FillerSnapshot {
    pub fn scalar(&self, dim: Dim, field: &str) -> Option<&str> {
        let path = format!("{}.{}", dim.keyword(), field);
        match dim {
            Dim::Phon => self
                .phon
                .iter()
                .find(|(key, _)| key == &path)
                .map(|(_, value)| value.as_str()),
            Dim::Syn => self
                .syn
                .iter()
                .find(|(key, _)| key == &path)
                .map(|(_, value)| value.as_str()),
            Dim::Sem => self
                .sem
                .features
                .get(field)
                .map(String::as_str)
                .or_else(|| {
                    self.sem
                        .fields
                        .iter()
                        .find(|(key, _)| key == field)
                        .map(|(_, value)| value.as_str())
                }),
            Dim::Prag => self
                .prag
                .iter()
                .find(|(key, _)| key == &path)
                .map(|(_, value)| value.as_str()),
        }
    }
}

/// derived token(P42:暫態,不進庫;殘餘 slots = 剩餘 valence)。
/// **form-meaning pair**(12c):form 極 = `syn_categories` + phon(`phon_form`);
/// meaning 極 = `sem`([`SemNode`],role 綁 filler 語意節點,可容納未來複雜模型)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedToken {
    pub construction: String,
    /// Stable identity of the one deep construction sign used for this token.
    pub construction_id: crate::SignId,
    /// 依 construction 的 belongs 閉包(derived 是該範疇;如填滿的 PresentVerb 是 Verb)。
    pub syn_categories: Vec<String>,
    /// Complete scalar phon projection. The root `phon` value is the current
    /// derived UR in `/…/` notation.
    pub phon: Vec<(String, String)>,
    /// Complete scalar syn projection (category membership is kept separately
    /// in `syn_categories`).
    pub syn: Vec<(String, String)>,
    /// **derived 語意**(meaning 極):construction 的 sem 純量欄位 + role→filler 語意。
    pub sem: SemNode,
    /// Complete scalar pragmatic projection.
    pub prag: Vec<(String, String)>,
    pub provenance: TokenProvenance,
    /// Frozen filler projections in declaration order.
    pub fillers: Vec<FillerSnapshot>,
    /// 已填:slot 名 → filler UR 內文(保 slot 宣告序)。
    filled: Vec<(String, String)>,
    /// phon 模板(construction 的 `phon` Def 值,已剝 `/…/`)。
    template: String,
    /// 未填 slots(= 剩餘 valence;必填未填 → 未飽和)。
    residual: Vec<Slot>,
    /// Parameters declared by this Sign itself. `residual` additionally
    /// contains free variables propagated from nested Sign arguments.
    own_residual: Vec<Slot>,
    /// Effective construction content, including inherited token-level rules.
    pub(crate) rule_sign: SignDef,
    /// Unevaluated local construction source. Dynamic Sign-level `belongs`
    /// refinements are recorded here so the complete effective contract can
    /// be rebuilt without replaying patches over an already evaluated token.
    pub(crate) source_sign: SignDef,
    /// Pure expanded phon input selected by the system realization layer. It
    /// is not a phonological surface and is kept only on transient derived
    /// tokens so a realized token can recursively fill another construction.
    realized_phon_input: Option<String>,
    deep_state: DeepTokenState,
    context_features: BTreeMap<(Dim, String), String>,
    occurrence_records: Vec<OccurrenceRecord>,
    bound_fillers: Vec<(String, BoundFiller)>,
    invocation_mapping: SlotMap,
}

impl DerivedToken {
    pub fn residual_slots(&self) -> &[Slot] {
        &self.residual
    }
    /// 飽和 = 無必填 slot 未填(optional 未填仍算飽和)。
    pub fn is_saturated(&self) -> bool {
        self.residual.iter().all(|s| s.optional)
    }
    /// 未飽和時的欠缺必填 slot 名。
    pub fn missing_required(&self) -> Vec<String> {
        self.residual
            .iter()
            .filter(|s| !s.optional)
            .map(|s| s.name.clone())
            .collect()
    }
    /// derived UR 形(模板代入;未填 optional → 空;未飽和 → Err)。
    pub fn phon_form(&self) -> Result<String, CxgError> {
        let missing = self.missing_required();
        if !missing.is_empty() {
            return Err(CxgError::Unsaturated(missing));
        }
        Ok(substitute(&self.template, &self.filled))
    }

    pub(crate) fn expand_phon_template(&self, template: &str) -> Result<String, CxgError> {
        let missing = self.missing_required();
        if !missing.is_empty() {
            return Err(CxgError::Unsaturated(missing));
        }
        let inner = template
            .strip_prefix('/')
            .and_then(|value| value.strip_suffix('/'))
            .unwrap_or(template);
        Ok(substitute(inner, &self.filled))
    }

    /// Complete scalar projection. Semantic enum features and ordinary fields
    /// are returned through the same dimension-local view.
    pub fn projection(&self, dim: Dim) -> Vec<(String, String)> {
        match dim {
            Dim::Phon => self.phon.clone(),
            Dim::Syn => self.syn.clone(),
            Dim::Sem => self
                .sem
                .fields
                .iter()
                .cloned()
                .chain(
                    self.sem
                        .features
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone())),
                )
                .map(|(name, value)| (format!("sem.{name}"), value))
                .collect(),
            Dim::Prag => self.prag.clone(),
        }
    }

    pub fn realized_phon_input(&self) -> Option<&str> {
        self.realized_phon_input.as_deref()
    }

    pub(crate) fn record_realized_phon_input(&mut self, input: String) {
        self.realized_phon_input = Some(input);
    }

    pub(crate) fn reset_to_deep(&self) -> DerivedToken {
        let mut token = self.clone();
        token.phon = self.deep_state.phon.clone();
        token.syn = self.deep_state.syn.clone();
        token.sem = self.deep_state.sem.clone();
        token.prag = self.deep_state.prag.clone();
        token.realized_phon_input = None;
        token.occurrence_records.clear();
        token
    }

    pub(crate) fn remember_context(&mut self, dim: Dim, name: String, value: String) {
        self.context_features.insert((dim, name), value);
    }

    pub(crate) fn context_features(&self) -> &BTreeMap<(Dim, String), String> {
        &self.context_features
    }

    pub(crate) fn take_occurrence_records(&mut self) -> Vec<OccurrenceRecord> {
        std::mem::take(&mut self.occurrence_records)
    }

    pub(crate) fn set_feature(
        &mut self,
        dim: Dim,
        name: &str,
        value: String,
    ) -> Result<(), TokenExpressionError> {
        match dim {
            Dim::Syn => {
                let path = format!("syn.{name}");
                self.syn.retain(|(candidate, _)| candidate != &path);
                self.syn.push((path, value));
            }
            Dim::Sem => {
                self.sem.features.insert(name.to_owned(), value);
            }
            Dim::Phon | Dim::Prag => {
                return Err(TokenExpressionError::Invalid(format!(
                    "typed features are not supported in the {dim:?} dimension"
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn set_sem_role(&mut self, name: &str, value: SemNode) {
        self.sem.roles.retain(|(candidate, _)| candidate != name);
        self.sem.roles.push((name.to_owned(), value));
    }

    pub(crate) fn bound_fillers(&self) -> &[(String, BoundFiller)] {
        &self.bound_fillers
    }

    pub(crate) fn own_residual_slots(&self) -> &[Slot] {
        &self.own_residual
    }

    pub(crate) fn invocation_mapping(&self) -> &SlotMap {
        &self.invocation_mapping
    }

    pub(crate) fn source_sign(&self) -> &SignDef {
        &self.source_sign
    }

    pub(crate) fn symbolic_phon_form(&self) -> String {
        substitute_symbolic(&self.template, &self.filled)
    }

    pub(crate) fn as_phon_projection_base(&self) -> DerivedToken {
        let mut token = self.clone();
        token
            .rule_sign
            .items
            .retain(|item| !matches!(item, SignItem::Realization(_)));
        token.realized_phon_input = None;
        token
    }
}

/// 抽出一個 sign 的 slots(保宣告序)。
pub fn slots_of(sign: &SignDef) -> Vec<Slot> {
    sign.items
        .iter()
        .filter_map(|it| match it {
            SignItem::Slot(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

pub fn parameters_of(sign: &SignDef) -> Vec<crate::SignParameter> {
    slots_of(sign)
        .iter()
        .map(crate::SignParameter::from)
        .collect()
}

/// Sign／trait 中宣告的 slot mapping，依有效內容順序組成。
pub fn slot_map_of(sign: &SignDef) -> SlotMap {
    SlotMap::from_ops(sign.items.iter().filter_map(|item| match item {
        SignItem::SlotMap(operation) => Some(operation.clone()),
        _ => None,
    }))
}

pub fn is_construction(sign: &SignDef) -> bool {
    sign.items.iter().any(|it| matches!(it, SignItem::Slot(_)))
}

/// sign 本地 `phon` Def 值(construction 為模板;無 → 空)。同 path 取最後(P6)。
fn phon_value(sign: &SignDef) -> String {
    sign.items
        .iter()
        .rev()
        .find_map(|it| match it {
            SignItem::Def(d) if d.path == "phon" => Some(d.value.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// `{slot}` 代入 + 字面素材直通。未填的鍵代入空(飽和性由呼叫端先驗)。
fn substitute(template: &str, filled: &[(String, String)]) -> String {
    let lookup = |name: &str| {
        filled
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    };
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
                name.push(c2);
            }
            out.push_str(lookup(name.trim()));
        } else {
            out.push(c);
        }
    }
    out
}

fn substitute_symbolic(template: &str, filled: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
                name.push(c2);
            }
            if let Some((_, value)) = filled.iter().find(|(key, _)| key == name.trim()) {
                out.push_str(value);
            } else {
                out.push('{');
                out.push_str(name.trim());
                out.push('}');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 模板引用的 slot 名集合(驗證用)。
fn template_refs(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
                name.push(c2);
            }
            out.push(name.trim().to_owned());
        }
    }
    out
}

fn ident_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
}

#[derive(Clone)]
struct FillerMaterial {
    name: String,
    categories: Vec<String>,
    phon: String,
    phon_projection: Vec<(String, String)>,
    syn: Vec<(String, String)>,
    sem: SemNode,
    prag: Vec<(String, String)>,
    provenance: FillerProvenance,
    feature_domains: BTreeMap<String, Vec<String>>,
    occurrence_sign: Option<SignDef>,
    base_sign: Option<SignDef>,
    occurrence_token: Option<DerivedToken>,
    probe_rules: Vec<RuleRecord>,
    committed_rules: Vec<RuleRecord>,
    committed_cases: Vec<OccurrenceCaseRecord>,
    realization: Option<String>,
}

#[derive(Clone)]
struct MappedSlot {
    internal: Slot,
    external: Option<String>,
    autofill: Option<String>,
}

fn validate_slot_map(slots: &[Slot], mapping: &SlotMap) -> Result<Vec<MappedSlot>, CxgError> {
    use std::collections::BTreeSet;

    let mut mapped: Vec<MappedSlot> = slots
        .iter()
        .cloned()
        .map(|internal| MappedSlot {
            external: Some(internal.name.clone()),
            internal,
            autofill: None,
        })
        .collect();
    let mut seen = BTreeSet::new();
    for operation in mapping.ops() {
        let (slot_name, kind) = match operation {
            SlotMapOp::Preserve { slot } => (slot, "exposure"),
            SlotMapOp::Rename { slot, .. } => (slot, "exposure"),
            SlotMapOp::AutoFill { slot, .. } => (slot, "autofill"),
            SlotMapOp::Internalize { slot } => (slot, "exposure"),
            SlotMapOp::Optional { slot, .. } => (slot, "optional"),
        };
        let Some(target) = mapped
            .iter_mut()
            .find(|candidate| candidate.internal.name == *slot_name)
        else {
            return Err(CxgError::SlotMapUnknown(slot_name.clone()));
        };
        if !seen.insert((slot_name.clone(), kind)) {
            return Err(CxgError::SlotMapDuplicate {
                slot: slot_name.clone(),
                operation: kind,
            });
        }
        match operation {
            SlotMapOp::Preserve { .. } => {}
            SlotMapOp::Rename { to, .. } => {
                if !ident_ok(to) {
                    return Err(CxgError::SlotMapInvalidName(to.clone()));
                }
                target.external = Some(to.clone());
            }
            SlotMapOp::AutoFill { filler, .. } => target.autofill = Some(filler.clone()),
            SlotMapOp::Internalize { .. } => target.external = None,
            SlotMapOp::Optional { optional, .. } => target.internal.optional = *optional,
        }
    }
    let mut names = BTreeSet::new();
    for slot in &mapped {
        if let Some(name) = &slot.external {
            if !names.insert(name.clone()) {
                return Err(CxgError::SlotMapNameCollision(name.clone()));
            }
        }
        if slot.external.is_none() && slot.autofill.is_none() && !slot.internal.optional {
            return Err(CxgError::InternalRequiredUnfilled(
                slot.internal.name.clone(),
            ));
        }
    }
    Ok(mapped)
}

/// 驗證 construction 的 source mapping 與額外呼叫端 mapping；不消耗 filler。
pub fn validate_slot_mapping(sign: &SignDef, extra: &SlotMap) -> Result<(), CxgError> {
    let mapping = slot_map_of(sign).and(extra);
    validate_slot_map(&slots_of(sign), &mapping).map(|_| ())
}

fn sign_material(sign: &SignDef, reg: &OntologyRegistry) -> Result<FillerMaterial, CxgError> {
    let base_sign = reg.effective_sign(sign);
    let (sign, probe_rules) = evaluate_occurrence_sign(&base_sign, reg)?;
    let phon_projection = sign.project(Dim::Phon, reg).defs;
    let phon = phon_projection
        .iter()
        .find(|(path, _)| path == "phon")
        .map(|(_, value)| value.as_str())
        .and_then(|value| value.strip_prefix('/').and_then(|v| v.strip_suffix('/')))
        .map(str::to_owned)
        .ok_or_else(|| CxgError::FillerNoUr(sign.name.clone()))?;
    Ok(FillerMaterial {
        name: sign.name.clone(),
        categories: reg.sign_categories(&sign),
        phon,
        phon_projection,
        syn: sign.project(Dim::Syn, reg).defs,
        sem: SemNode::of_sign(&sign, reg),
        prag: sign.project(Dim::Prag, reg).defs,
        provenance: FillerProvenance::StoredSign(sign.name.clone()),
        feature_domains: sign
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::FeatureDecl(feature) if feature.dim == Dim::Syn => {
                    Some((feature.name.clone(), feature.values.clone()))
                }
                _ => None,
            })
            .collect(),
        occurrence_sign: Some(sign),
        base_sign: Some(base_sign),
        occurrence_token: None,
        probe_rules: probe_rules.clone(),
        committed_rules: probe_rules,
        committed_cases: Vec::new(),
        realization: None,
    })
}

/// Rehydrate an occurrence that has already completed its Syn -> Sem -> Prag
/// pass.  Unlike `sign_material`, this deliberately does not expand the
/// ontology or replay executable items: the stored Sign is the committed
/// occurrence value, not its original lexical source.
fn committed_sign_material(
    sign: &SignDef,
    base_sign: &SignDef,
    provenance: &FillerProvenance,
    reg: &OntologyRegistry,
) -> Result<FillerMaterial, CxgError> {
    let phon_projection = sign.project(Dim::Phon, reg).defs;
    let phon = phon_projection
        .iter()
        .find(|(path, _)| path == "phon")
        .map(|(_, value)| value.as_str())
        .and_then(|value| value.strip_prefix('/').and_then(|v| v.strip_suffix('/')))
        .map(str::to_owned)
        .ok_or_else(|| CxgError::FillerNoUr(sign.name.clone()))?;
    Ok(FillerMaterial {
        name: sign.name.clone(),
        categories: reg.sign_categories(sign),
        phon,
        phon_projection,
        syn: sign.project(Dim::Syn, reg).defs,
        sem: SemNode::of_sign(sign, reg),
        prag: sign.project(Dim::Prag, reg).defs,
        provenance: provenance.clone(),
        feature_domains: sign
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::FeatureDecl(feature) if feature.dim == Dim::Syn => {
                    Some((feature.name.clone(), feature.values.clone()))
                }
                _ => None,
            })
            .collect(),
        occurrence_sign: Some(sign.clone()),
        base_sign: Some(base_sign.clone()),
        occurrence_token: None,
        probe_rules: Vec::new(),
        committed_rules: Vec::new(),
        committed_cases: Vec::new(),
        realization: None,
    })
}

fn token_material(token: &DerivedToken) -> Result<FillerMaterial, CxgError> {
    Ok(FillerMaterial {
        name: format!("{} token", token.construction),
        categories: token.syn_categories.clone(),
        phon: token
            .realized_phon_input()
            .map(str::to_owned)
            .unwrap_or_else(|| token.symbolic_phon_form()),
        phon_projection: token.phon.clone(),
        syn: token.syn.clone(),
        sem: token.sem.clone(),
        prag: token.prag.clone(),
        provenance: FillerProvenance::Derived(Box::new(token.provenance.clone())),
        feature_domains: token
            .rule_sign
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::FeatureDecl(feature) if feature.dim == Dim::Syn => {
                    Some((feature.name.clone(), feature.values.clone()))
                }
                _ => None,
            })
            .collect(),
        occurrence_sign: None,
        base_sign: None,
        occurrence_token: Some(token.clone()),
        probe_rules: Vec::new(),
        committed_rules: Vec::new(),
        committed_cases: Vec::new(),
        realization: token.realized_phon_input().map(str::to_owned),
    })
}

fn bound_material(bound: &BoundFiller, reg: &OntologyRegistry) -> Result<FillerMaterial, CxgError> {
    match bound {
        BoundFiller::Owned {
            committed,
            base,
            provenance,
        } => committed_sign_material(committed, base, provenance, reg),
        BoundFiller::Derived(token) => token_material(token),
    }
}

fn committed_bound_filler(material: &FillerMaterial) -> Result<BoundFiller, CxgError> {
    if let Some(token) = &material.occurrence_token {
        return Ok(BoundFiller::Derived(Box::new(token.clone())));
    }
    let Some(sign) = &material.occurrence_sign else {
        return Err(CxgError::Engine(format!(
            "filler occurrence {:?} has no committed Sign material",
            material.name
        )));
    };
    let Some(base) = &material.base_sign else {
        return Err(CxgError::Engine(format!(
            "filler occurrence {:?} has no deep Sign material",
            material.name
        )));
    };
    let mut sign = sign.clone();
    // The values produced by these items are already committed in `sign`.
    // Keeping them would replay the occurrence pipeline when an unsaturated
    // enclosing Sign later receives another argument.
    sign.items.retain(|item| {
        !matches!(
            item,
            SignItem::Rule(_)
                | SignItem::FeatureRule(_)
                | SignItem::FeatureExpression(_)
                | SignItem::RoleExpression(_)
                | SignItem::SignExpression(_)
        )
    });
    Ok(BoundFiller::Owned {
        committed: sign,
        base: base.clone(),
        provenance: material.provenance.clone(),
    })
}

fn feature_operand(source: &str) -> Option<(&str, &str)> {
    let source = source.strip_prefix("$slot.").unwrap_or(source);
    let mut parts = source.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(slot), Some("syn"), Some(feature), None) => Some((slot, feature)),
        _ => None,
    }
}

fn validate_binary_constraints(
    sign: &SignDef,
    slots: &[MappedSlot],
    provided: &[(String, FillerMaterial)],
    template: &str,
) -> Result<(), CxgError> {
    use crate::ConstraintPredicate;
    use std::collections::{BTreeMap, BTreeSet};

    let constraints = sign.items.iter().filter_map(|item| match item {
        SignItem::Constraint(constraint) => Some(constraint),
        _ => None,
    });
    let slot_exists = |name: &str| slots.iter().any(|slot| slot.internal.name == name);
    let material = |name: &str| {
        provided
            .iter()
            .find(|(slot, _)| slot == name)
            .map(|(_, value)| value)
    };
    let order = template_refs(template);
    let mut before_edges = Vec::new();
    for constraint in constraints {
        match constraint.predicate {
            ConstraintPredicate::Equal => {
                let Some((left_slot, left_feature)) = feature_operand(&constraint.left) else {
                    return Err(CxgError::ConstraintInvalidFeature(constraint.left.clone()));
                };
                let Some((right_slot, right_feature)) = feature_operand(&constraint.right) else {
                    return Err(CxgError::ConstraintInvalidFeature(constraint.right.clone()));
                };
                for name in [left_slot, right_slot] {
                    if !slot_exists(name) {
                        return Err(CxgError::ConstraintUnknownSlot(name.to_owned()));
                    }
                }
                let (Some(left), Some(right)) = (material(left_slot), material(right_slot)) else {
                    // The unsaturated Sign keeps this constraint until both
                    // operands become concrete.
                    continue;
                };
                let left_domain = left
                    .feature_domains
                    .get(left_feature)
                    .ok_or_else(|| CxgError::ConstraintInvalidFeature(constraint.left.clone()))?;
                let right_domain = right
                    .feature_domains
                    .get(right_feature)
                    .ok_or_else(|| CxgError::ConstraintInvalidFeature(constraint.right.clone()))?;
                if left_domain != right_domain {
                    return Err(CxgError::ConstraintDomainMismatch {
                        left: left_domain.join(", "),
                        right: right_domain.join(", "),
                    });
                }
                let left_path = format!("syn.{left_feature}");
                let right_path = format!("syn.{right_feature}");
                let left_value = left
                    .syn
                    .iter()
                    .find(|(path, _)| path == &left_path)
                    .map(|(_, value)| value);
                let right_value = right
                    .syn
                    .iter()
                    .find(|(path, _)| path == &right_path)
                    .map(|(_, value)| value);
                if let (Some(left), Some(right)) = (left_value, right_value) {
                    if left != right {
                        return Err(CxgError::ConstraintEqualityConflict {
                            left: left.clone(),
                            right: right.clone(),
                        });
                    }
                }
            }
            ConstraintPredicate::Before | ConstraintPredicate::Adjacent => {
                for name in [&constraint.left, &constraint.right] {
                    if !slot_exists(name) {
                        return Err(CxgError::ConstraintUnknownSlot(name.clone()));
                    }
                }
                if constraint.predicate == ConstraintPredicate::Before {
                    before_edges.push((constraint.left.clone(), constraint.right.clone()));
                }
                let left = order.iter().position(|name| name == &constraint.left);
                let right = order.iter().position(|name| name == &constraint.right);
                let (Some(left), Some(right)) = (left, right) else {
                    // Missing optional/required variables remain residual on
                    // the Sign and are checked at its concrete boundary.
                    continue;
                };
                let satisfied = match constraint.predicate {
                    ConstraintPredicate::Before => left < right,
                    ConstraintPredicate::Adjacent => left.abs_diff(right) == 1,
                    ConstraintPredicate::Equal => unreachable!(),
                };
                if !satisfied {
                    return Err(CxgError::ConstraintOrderConflict {
                        predicate: constraint.predicate.keyword(),
                        left: constraint.left.clone(),
                        right: constraint.right.clone(),
                        template: template.to_owned(),
                    });
                }
            }
        }
    }
    let mut graph = BTreeMap::<String, Vec<String>>::new();
    for (left, right) in before_edges {
        graph.entry(left).or_default().push(right);
    }
    fn visit(
        node: &str,
        graph: &BTreeMap<String, Vec<String>>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> bool {
        if active.contains(node) {
            return true;
        }
        if done.contains(node) {
            return false;
        }
        active.insert(node.to_owned());
        let cycle = graph
            .get(node)
            .is_some_and(|next| next.iter().any(|child| visit(child, graph, active, done)));
        active.remove(node);
        done.insert(node.to_owned());
        cycle
    }
    let mut active = BTreeSet::new();
    let mut done = BTreeSet::new();
    if graph
        .keys()
        .any(|node| visit(node, &graph, &mut active, &mut done))
    {
        return Err(CxgError::ConstraintOrderCycle);
    }
    Ok(())
}

fn evaluate_occurrence_sign(
    base: &SignDef,
    reg: &OntologyRegistry,
) -> Result<(SignDef, Vec<RuleRecord>), CxgError> {
    let mut sign = base.clone();
    let mut records = Vec::new();
    for dim in [Dim::Syn, Dim::Sem, Dim::Prag] {
        let (next, pass) = synchronic::run_sign_dim_rules(&sign, dim, reg);
        sign = next;
        records.extend(pass);
        sign = synchronic::run_sign_feature_expressions(&sign, dim, reg)
            .map_err(CxgError::FeatureExpression)?;
    }
    Ok((sign, records))
}

fn slot_feature_source(value: &str) -> Option<(&str, &str)> {
    let mut parts = value.split('.');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("$slot"), Some(slot), Some("syn"), Some(feature), None) => Some((slot, feature)),
        _ => None,
    }
}

fn validate_occurrence_feature(
    slot: &str,
    feature: &str,
    value: &str,
    material: &FillerMaterial,
) -> Result<(), CxgError> {
    let Some(domain) = material.feature_domains.get(feature) else {
        return Err(CxgError::SlotFeatureUndeclared {
            slot: slot.to_owned(),
            feature: feature.to_owned(),
        });
    };
    if !domain.iter().any(|candidate| candidate == value) {
        return Err(CxgError::SlotFeatureOutOfDomain {
            slot: slot.to_owned(),
            feature: feature.to_owned(),
            value: value.to_owned(),
            domain: domain.join(", "),
        });
    }
    Ok(())
}

fn token_scalar<'a>(token: &'a DerivedToken, path: &str) -> Option<&'a str> {
    let path = path.strip_prefix("$self.").unwrap_or(path);
    let (dimension, field) = path.split_once('.')?;
    match Dim::parse(dimension)? {
        Dim::Phon => token
            .phon
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, value)| value.as_str()),
        Dim::Syn => token
            .syn
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, value)| value.as_str()),
        Dim::Sem => token
            .sem
            .features
            .get(field)
            .map(String::as_str)
            .or_else(|| {
                token
                    .sem
                    .fields
                    .iter()
                    .find(|(candidate, _)| candidate == field)
                    .map(|(_, value)| value.as_str())
            }),
        Dim::Prag => token
            .prag
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, value)| value.as_str()),
    }
}

fn token_case_condition_matches(
    token: &DerivedToken,
    case: &TypedCase,
    condition: &CaseCondition,
    reg: &OntologyRegistry,
) -> Result<bool, TokenExpressionError> {
    match condition {
        CaseCondition::Else => Ok(true),
        CaseCondition::Guard(guard) => {
            for conjunct in guard.split("&&").map(str::trim) {
                let (status, _, _, error) = synchronic::evaluate_token_guard(token, conjunct, reg);
                match status {
                    RuleStatus::Matched => {}
                    RuleStatus::Unmatched => return Ok(false),
                    RuleStatus::Error => {
                        return Err(TokenExpressionError::Invalid(
                            error.unwrap_or_else(|| format!("case guard {conjunct:?} failed")),
                        ))
                    }
                }
            }
            Ok(true)
        }
        CaseCondition::Equals(expected) => {
            let scrutinee = case.scrutinee.as_deref().ok_or_else(|| {
                TokenExpressionError::Invalid("equality case is missing its scrutinee".to_owned())
            })?;
            let scrutinee = scrutinee
                .trim()
                .strip_prefix("$self.")
                .unwrap_or(scrutinee.trim());
            if let Some((slot, projection)) = scrutinee.split_once('.') {
                if projection == "phon" {
                    let Some(filler) = token.fillers.iter().find(|filler| filler.slot == slot)
                    else {
                        return Ok(false);
                    };
                    if !reg.has(expected) {
                        return Err(TokenExpressionError::Invalid(format!(
                            "unknown case category {expected:?}"
                        )));
                    }
                    return Ok(filler.categories.iter().any(|category| {
                        category == expected || reg.category_is_a(category, expected)
                    }));
                }
            }
            Ok(token_scalar(token, scrutinee) == Some(expected))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_token_feature_case(
    token: &DerivedToken,
    case: &TypedCase,
    dim: Dim,
    name: &str,
    domain: &[String],
    base: Option<String>,
    target: &str,
    reg: &OntologyRegistry,
    records: &mut Vec<OccurrenceCaseRecord>,
) -> Result<Option<String>, TokenExpressionError> {
    for (index, branch) in case.branches.iter().enumerate() {
        if !token_case_condition_matches(token, case, &branch.condition, reg)? {
            records.push(OccurrenceCaseRecord {
                target: target.to_owned(),
                branch: index,
                status: OccurrenceCaseStatus::Unmatched,
                source: branch.source,
                diagnostic_code: None,
            });
            continue;
        }
        let value = match &branch.result {
            Expression::EnumValue(value) => Some(value.clone()),
            Expression::Case(nested) => evaluate_token_feature_case(
                token,
                nested,
                dim,
                name,
                domain,
                base.clone(),
                target,
                reg,
                records,
            )?,
            _ => {
                return Err(TokenExpressionError::Invalid(format!(
                    "feature case {}.{name} returned a non-enum expression",
                    dim.keyword()
                )))
            }
        };
        if let Some(value) = &value {
            if !domain.contains(value) {
                return Err(TokenExpressionError::FeatureOutOfDomain {
                    dim,
                    name: name.to_owned(),
                    value: value.clone(),
                    domain: domain.join(", "),
                });
            }
        }
        records.push(OccurrenceCaseRecord {
            target: target.to_owned(),
            branch: index,
            status: OccurrenceCaseStatus::Matched,
            source: branch.source,
            diagnostic_code: None,
        });
        return Ok(value);
    }
    Ok(base)
}

fn apply_token_feature_expressions(
    mut token: DerivedToken,
    dim: Dim,
    reg: &OntologyRegistry,
    records: &mut Vec<OccurrenceCaseRecord>,
) -> Result<DerivedToken, TokenExpressionError> {
    let expressions = token
        .rule_sign
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::FeatureExpression(expression) if expression.dim == dim => {
                Some(expression.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    for expression in expressions {
        let Expression::Case(case) = &expression.expression else {
            return Err(TokenExpressionError::Invalid(format!(
                "{}.{} feature expression must be a typed case",
                dim.keyword(),
                expression.name
            )));
        };
        let domain = token
            .rule_sign
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                SignItem::FeatureDecl(declaration)
                    if declaration.dim == dim && declaration.name == expression.name =>
                {
                    Some(declaration.values.clone())
                }
                _ => None,
            })
            .ok_or_else(|| TokenExpressionError::UndeclaredFeature {
                dim,
                name: expression.name.clone(),
            })?;
        let target = format!("{}.feature.{}", dim.keyword(), expression.name);
        let base = token_scalar(&token, &format!("{}.{}", dim.keyword(), expression.name))
            .map(str::to_owned);
        let selected = evaluate_token_feature_case(
            &token,
            case,
            dim,
            &expression.name,
            &domain,
            base,
            &target,
            reg,
            records,
        )?;
        if let Some(value) = selected {
            token.set_feature(dim, &expression.name, value)?;
        } else {
            return Err(TokenExpressionError::CaseDefaultMissing {
                context: format!("{}.{}", dim.keyword(), expression.name),
            });
        }
    }
    Ok(token)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_token_role_case(
    token: &DerivedToken,
    case: &TypedCase,
    role: &str,
    constraint: Option<&str>,
    base: Option<SemNode>,
    target: &str,
    reg: &OntologyRegistry,
    records: &mut Vec<OccurrenceCaseRecord>,
) -> Result<Option<SemNode>, TokenExpressionError> {
    for (index, branch) in case.branches.iter().enumerate() {
        if !token_case_condition_matches(token, case, &branch.condition, reg)? {
            records.push(OccurrenceCaseRecord {
                target: target.to_owned(),
                branch: index,
                status: OccurrenceCaseStatus::Unmatched,
                source: branch.source,
                diagnostic_code: None,
            });
            continue;
        }
        let selected = match &branch.result {
            Expression::Slot(slot) => {
                let Some(filler) = token
                    .fillers
                    .iter()
                    .find(|filler| &filler.slot == slot)
                    .cloned()
                else {
                    records.push(OccurrenceCaseRecord {
                        target: target.to_owned(),
                        branch: index,
                        status: OccurrenceCaseStatus::MoreSpecificBlocked,
                        source: branch.source,
                        diagnostic_code: Some("CASE_MORE_SPECIFIC_BLOCKED"),
                    });
                    continue;
                };
                if let Some(required) = constraint {
                    if !filler
                        .sem
                        .types
                        .iter()
                        .any(|category| reg.category_is_a(category, required))
                    {
                        return Err(CxgError::RoleCategoryMismatch {
                            role: role.to_owned(),
                            required: required.to_owned(),
                            has: filler.sem.types,
                        }
                        .into());
                    }
                }
                Some(filler.sem)
            }
            Expression::Case(nested) => evaluate_token_role_case(
                token,
                nested,
                role,
                constraint,
                base.clone(),
                target,
                reg,
                records,
            )?,
            _ => {
                return Err(TokenExpressionError::Invalid(format!(
                    "semantic role {role:?} returned a non-slot expression"
                )))
            }
        };
        records.push(OccurrenceCaseRecord {
            target: target.to_owned(),
            branch: index,
            status: OccurrenceCaseStatus::Matched,
            source: branch.source,
            diagnostic_code: None,
        });
        return Ok(selected);
    }
    Ok(base)
}

fn apply_token_role_expressions(
    mut token: DerivedToken,
    reg: &OntologyRegistry,
    records: &mut Vec<OccurrenceCaseRecord>,
) -> Result<DerivedToken, TokenExpressionError> {
    let expressions = token
        .rule_sign
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::RoleExpression(expression) => Some(expression.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for expression in expressions {
        let Expression::Case(case) = &expression.expression else {
            return Err(TokenExpressionError::Invalid(format!(
                "semantic role {:?} expression must be a typed case",
                expression.name
            )));
        };
        let constraint = token
            .rule_sign
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                SignItem::RoleDecl(declaration) if declaration.name == expression.name => {
                    Some(declaration.constraint.clone())
                }
                _ => None,
            })
            .ok_or_else(|| {
                TokenExpressionError::Invalid(format!(
                    "semantic role {:?} is undeclared",
                    expression.name
                ))
            })?;
        let target = format!("sem.role.{}", expression.name);
        let base = token.sem.role(&expression.name).cloned();
        if let Some(selected) = evaluate_token_role_case(
            &token,
            case,
            &expression.name,
            constraint.category(),
            base,
            &target,
            reg,
            records,
        )? {
            token.set_sem_role(&expression.name, selected);
        }
    }
    let missing = token
        .rule_sign
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::RoleDecl(role)
                if !role.optional && token.sem.roles.iter().all(|(name, _)| name != &role.name) =>
            {
                Some(role.name.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() && token.is_saturated() {
        return Err(CxgError::MissingRoles {
            construction: token.construction.clone(),
            roles: missing,
        }
        .into());
    }
    Ok(token)
}

/// Authoritative Syn -> Sem -> Prag evaluation for a constructed Sign.  Both
/// the public system path and derived-token occurrence rebuilding call this
/// function so ordinary rules and typed feature/role expressions cannot
/// silently diverge.
pub(crate) fn evaluate_token_pipeline(
    mut token: DerivedToken,
    reg: &OntologyRegistry,
) -> Result<(DerivedToken, Vec<RuleRecord>, Vec<OccurrenceCaseRecord>), TokenExpressionError> {
    let mut rules = Vec::new();
    let mut cases = Vec::new();
    for dim in [Dim::Syn, Dim::Sem, Dim::Prag] {
        let (next, pass) = synchronic::run_token_dim_rules(&token, dim, reg);
        token = next;
        rules.extend(pass);
        token = apply_token_feature_expressions(token, dim, reg, &mut cases)?;
        if dim == Dim::Sem {
            token = apply_token_role_expressions(token, reg, &mut cases)?;
        }
    }
    Ok((token, rules, cases))
}

fn apply_occurrence_constraints(
    slot: &str,
    constraints: &[(String, String)],
    material: &mut FillerMaterial,
    reg: &OntologyRegistry,
) -> Result<(), CxgError> {
    for (feature, value) in constraints {
        validate_occurrence_feature(slot, feature, value, material)?;
    }
    if let Some(base) = material.base_sign.clone() {
        let mut constrained = base;
        for (feature, value) in constraints {
            let path = format!("syn.{feature}");
            if let Some(actual) = constrained
                .project(Dim::Syn, reg)
                .defs
                .iter()
                .find(|(candidate, _)| candidate == &path)
                .map(|(_, value)| value)
            {
                if actual != value {
                    return Err(CxgError::SlotFeatureConflict {
                        slot: slot.to_owned(),
                        feature: feature.clone(),
                        expected: value.clone(),
                        actual: actual.clone(),
                    });
                }
            } else {
                constrained = Patch::syn().set(feature, value).apply(&constrained);
            }
        }
        let (evaluated, records) = evaluate_occurrence_sign(&constrained, reg)?;
        for (feature, value) in constraints {
            let path = format!("syn.{feature}");
            let projection = evaluated.project(Dim::Syn, reg);
            let actual = projection
                .defs
                .iter()
                .find(|(candidate, _)| candidate == &path)
                .map(|(_, value)| value.as_str());
            if actual != Some(value.as_str()) {
                return Err(CxgError::SlotFeatureConflict {
                    slot: slot.to_owned(),
                    feature: feature.clone(),
                    expected: value.clone(),
                    actual: actual.unwrap_or("<missing>").to_owned(),
                });
            }
        }
        material.phon_projection = evaluated.project(Dim::Phon, reg).defs;
        material.phon = material
            .phon_projection
            .iter()
            .find(|(path, _)| path == "phon")
            .and_then(|(_, value)| value.strip_prefix('/').and_then(|v| v.strip_suffix('/')))
            .unwrap_or_default()
            .to_owned();
        material.syn = evaluated.project(Dim::Syn, reg).defs;
        material.sem = SemNode::of_sign(&evaluated, reg);
        material.prag = evaluated.project(Dim::Prag, reg).defs;
        material.occurrence_sign = Some(evaluated);
        material.committed_rules = records;
        return Ok(());
    }

    let Some(existing) = material.occurrence_token.as_ref() else {
        return Ok(());
    };
    let mut token = existing.reset_to_deep();
    for ((dim, feature), current) in existing.context_features() {
        if *dim == Dim::Syn {
            if let Some((_, requested)) = constraints.iter().find(|(name, _)| name == feature) {
                if requested != current {
                    return Err(CxgError::SlotFeatureConflict {
                        slot: slot.to_owned(),
                        feature: feature.clone(),
                        expected: requested.clone(),
                        actual: current.clone(),
                    });
                }
            }
        }
        match dim {
            Dim::Syn => {
                let path = format!("syn.{feature}");
                if let Some((_, actual)) =
                    token.syn.iter().find(|(candidate, _)| candidate == &path)
                {
                    if actual != current {
                        return Err(CxgError::SlotFeatureConflict {
                            slot: slot.to_owned(),
                            feature: feature.clone(),
                            expected: current.clone(),
                            actual: actual.clone(),
                        });
                    }
                } else {
                    token.syn.push((path, current.clone()));
                }
            }
            Dim::Sem => {
                if let Some(actual) = token.sem.features.get(feature) {
                    if actual != current {
                        return Err(CxgError::SlotFeatureConflict {
                            slot: slot.to_owned(),
                            feature: feature.clone(),
                            expected: current.clone(),
                            actual: actual.clone(),
                        });
                    }
                } else {
                    token.sem.features.insert(feature.clone(), current.clone());
                }
            }
            Dim::Phon | Dim::Prag => {}
        }
        token.remember_context(*dim, feature.clone(), current.clone());
    }
    for (feature, value) in constraints {
        let path = format!("syn.{feature}");
        if let Some((_, actual)) = token.syn.iter().find(|(candidate, _)| candidate == &path) {
            if actual != value {
                return Err(CxgError::SlotFeatureConflict {
                    slot: slot.to_owned(),
                    feature: feature.clone(),
                    expected: value.clone(),
                    actual: actual.clone(),
                });
            }
        } else {
            token.syn.push((path, value.clone()));
        }
        token.remember_context(Dim::Syn, feature.clone(), value.clone());
    }
    let (next, records, cases) = evaluate_token_pipeline(token, reg)
        .map_err(TokenExpressionError::into_construction_error)?;
    token = next;
    for (feature, value) in constraints {
        let path = format!("syn.{feature}");
        let actual = token
            .syn
            .iter()
            .find(|(candidate, _)| candidate == &path)
            .map(|(_, value)| value.as_str());
        if actual != Some(value.as_str()) {
            return Err(CxgError::SlotFeatureConflict {
                slot: slot.to_owned(),
                feature: feature.clone(),
                expected: value.clone(),
                actual: actual.unwrap_or("<missing>").to_owned(),
            });
        }
    }
    material.phon_projection = token.phon.clone();
    material.phon = if let Some(realized) = token.realized_phon_input() {
        realized.to_owned()
    } else if token.is_saturated() {
        token.phon_form()?
    } else {
        token.symbolic_phon_form()
    };
    material.syn = token.syn.clone();
    material.sem = token.sem.clone();
    material.prag = token.prag.clone();
    material.occurrence_token = Some(token);
    material.committed_rules = records;
    material.committed_cases = cases;
    Ok(())
}

fn realize_stored_filler(
    material: &mut FillerMaterial,
    reg: &OntologyRegistry,
) -> Result<(), CxgError> {
    if let Some(token) = material.occurrence_token.as_mut() {
        if !token.is_saturated() {
            // An unsaturated Sign is still a legitimate symbolic filler.
            // Realization is a concrete boundary, so preserve its variables
            // until a later application supplies them.
            material.phon = token.symbolic_phon_form();
            return Ok(());
        }
        let realization = token.rule_sign.items.iter().find_map(|item| match item {
            SignItem::Realization(realization) => Some(realization),
            _ => None,
        });
        let mut selected: Option<String> = None;
        if let Some(realization) = realization {
            if let Some(case) = &realization.expression {
                for branch in &case.branches {
                    let matched = match &branch.condition {
                        crate::CaseCondition::Else => true,
                        crate::CaseCondition::Guard(guard) => {
                            let mut matched = true;
                            for conjunct in guard.split("&&").map(str::trim) {
                                let (status, _, _, error) =
                                    synchronic::evaluate_token_guard(token, conjunct, reg);
                                match status {
                                    RuleStatus::Matched => {}
                                    RuleStatus::Unmatched => {
                                        matched = false;
                                        break;
                                    }
                                    RuleStatus::Error => {
                                        return Err(CxgError::FillerRealizationGuard {
                                            filler: material.name.clone(),
                                            message: error.unwrap_or_else(|| {
                                                "unknown realization guard error".to_owned()
                                            }),
                                        });
                                    }
                                }
                            }
                            matched
                        }
                        crate::CaseCondition::Equals(_) => false,
                    };
                    if matched {
                        match &branch.result {
                            crate::Expression::PhonTemplate(template) => {
                                selected = Some(template.clone())
                            }
                            _ => {
                                return Err(CxgError::FillerRealizationGuard {
                                    filler: material.name.clone(),
                                    message: "occurrence realization requires a direct Phon result"
                                        .to_owned(),
                                })
                            }
                        }
                        break;
                    }
                }
            }
        }
        let input = if let Some(template) = selected.as_deref() {
            token.expand_phon_template(template)?
        } else {
            token.phon_form()?
        };
        if input.contains("$self")
            || input.contains("$slot")
            || input.contains('{')
            || input.contains('}')
            || input.contains('/')
        {
            return Err(CxgError::ImpureFillerRealization {
                filler: material.name.clone(),
                input,
            });
        }
        token.record_realized_phon_input(input.clone());
        material.realization = Some(input.clone());
        material.phon = input.clone();
        let root = format!("/{input}/");
        if let Some((_, value)) = material
            .phon_projection
            .iter_mut()
            .find(|(path, _)| path == "phon")
        {
            *value = root;
        } else {
            material.phon_projection.push(("phon".to_owned(), root));
        }
        return Ok(());
    }
    let Some(sign) = material.occurrence_sign.as_ref() else {
        return Ok(());
    };
    let realization = sign.items.iter().find_map(|item| match item {
        SignItem::Realization(realization) => Some(realization),
        _ => None,
    });
    let Some(realization) = realization else {
        return Ok(());
    };
    let mut selected = None;
    if let Some(case) = &realization.expression {
        for branch in &case.branches {
            let matched = match &branch.condition {
                crate::CaseCondition::Else => true,
                crate::CaseCondition::Guard(guard) => {
                    let mut matched = true;
                    for conjunct in guard.split("&&").map(str::trim) {
                        let (status, _, _, error) =
                            synchronic::evaluate_sign_guard(sign, conjunct, reg);
                        match status {
                            RuleStatus::Matched => {}
                            RuleStatus::Unmatched => {
                                matched = false;
                                break;
                            }
                            RuleStatus::Error => {
                                return Err(CxgError::FillerRealizationGuard {
                                    filler: material.name.clone(),
                                    message: error.unwrap_or_else(|| {
                                        "unknown realization guard error".to_owned()
                                    }),
                                });
                            }
                        }
                    }
                    matched
                }
                crate::CaseCondition::Equals(_) => false,
            };
            if matched {
                match &branch.result {
                    crate::Expression::PhonTemplate(template) => selected = Some(template.as_str()),
                    _ => {
                        return Err(CxgError::FillerRealizationGuard {
                            filler: material.name.clone(),
                            message: "stored realization requires a direct Phon result".to_owned(),
                        })
                    }
                }
                break;
            }
        }
    }
    let Some(template) = selected else {
        return Ok(());
    };
    let input = template
        .strip_prefix('/')
        .and_then(|value| value.strip_suffix('/'))
        .unwrap_or(template)
        .to_owned();
    if input.contains("$self")
        || input.contains("$slot")
        || input.contains('{')
        || input.contains('}')
        || input.contains('/')
    {
        return Err(CxgError::ImpureFillerRealization {
            filler: material.name.clone(),
            input,
        });
    }
    material.phon = input.clone();
    material.realization = Some(input.clone());
    let root = format!("/{input}/");
    if let Some((_, value)) = material
        .phon_projection
        .iter_mut()
        .find(|(path, _)| path == "phon")
    {
        *value = root;
    } else {
        material.phon_projection.push(("phon".to_owned(), root));
    }
    Ok(())
}

/// 解析 construction 的 derived 語意(12c form-meaning pair 的 meaning 極)。
/// construction 的 sem projection 逐欄:值為 `{slot}` → **role 綁 filler 的語意節點**
/// (非字串替換,修補07 §12c);否則 → 純量欄位。`{ref}` 非 slot → SemRefUnknown;
/// 引用未填 slot → 該 role 暫略(部分套用)。合法 synonymy/polysemy:不去重、不排他。
fn resolve_sem(
    cx: &SignDef,
    slots: &[Slot],
    filler_nodes: &[(String, SemNode)],
    reg: &OntologyRegistry,
) -> Result<SemNode, CxgError> {
    let mut node = SemNode::of_sign(cx, reg);
    let role_declarations = cx
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::RoleDecl(role) => Some(role),
            _ => None,
        })
        .collect::<Vec<_>>();
    for binding in cx.items.iter().filter_map(|item| match item {
        SignItem::RoleBinding(binding) => Some(binding),
        _ => None,
    }) {
        let Some(declaration) = role_declarations
            .iter()
            .find(|role| role.name == binding.name)
        else {
            return Err(CxgError::UnknownRole {
                construction: cx.name.clone(),
                role: binding.name.clone(),
            });
        };
        if !slots.iter().any(|slot| slot.name == binding.slot) {
            return Err(CxgError::SemRefUnknown {
                construction: cx.name.clone(),
                role: binding.name.clone(),
                slot: binding.slot.clone(),
            });
        }
        if let Some((_, filler)) = filler_nodes.iter().find(|(slot, _)| slot == &binding.slot) {
            let has = filler.types.clone();
            if let Some(required) = declaration.constraint.category() {
                if !has
                    .iter()
                    .any(|category| reg.category_is_a(category, required))
                {
                    return Err(CxgError::RoleCategoryMismatch {
                        role: binding.name.clone(),
                        required: required.to_owned(),
                        has,
                    });
                }
            }
            node.roles.retain(|(name, _)| name != &binding.name);
            node.roles.push((binding.name.clone(), filler.clone()));
        }
    }
    for (path, value) in cx.project(Dim::Sem, reg).defs {
        let field = path.strip_prefix("sem.").unwrap_or(&path).to_owned();
        if node.features.contains_key(&field) {
            continue;
        }
        match sem::slot_ref(&value) {
            Some(slot_name) => {
                if !slots.iter().any(|s| s.name == slot_name) {
                    return Err(CxgError::SemRefUnknown {
                        construction: cx.name.clone(),
                        role: field,
                        slot: slot_name.to_owned(),
                    });
                }
                if let Some((_, filler)) = filler_nodes.iter().find(|(key, _)| key == slot_name) {
                    node.roles.push((field, filler.clone()));
                }
                // 引用未填(optional/部分)→ role 暫略
            }
            None => {
                if !node
                    .fields
                    .iter()
                    .any(|(name, existing)| name == &field && existing == &value)
                {
                    node.fields.push((field, value));
                }
            }
        }
    }
    Ok(node)
}

/// Construction application(P42):construction + fillers → derived token。
/// 部分套用合法(殘餘 slots = 剩餘 valence);**不就地改任何來源 sign**。
pub fn apply(
    lang: &Language,
    reg: &OntologyRegistry,
    construction: &str,
    fillers: &[(&str, &str)],
) -> Result<DerivedToken, CxgError> {
    let typed: Vec<_> = fillers
        .iter()
        .map(|(slot, filler)| SlotFiller::sign(slot, filler))
        .collect();
    apply_with(lang, reg, construction, &typed, &SlotMap::identity())
}

/// Construction application with typed stored-sign/derived-token fillers and
/// an atomic slot mapping. Source-declared operations run first; caller
/// operations may add constraints but duplicate operations are rejected.
pub fn apply_with<'a>(
    lang: &Language,
    reg: &OntologyRegistry,
    construction: &str,
    fillers: &[SlotFiller<'a>],
    mapping: &SlotMap,
) -> Result<DerivedToken, CxgError> {
    apply_with_committed(lang, reg, construction, &[], fillers, mapping)
}

/// Resume an unsaturated Sign from its committed, internal-slot occurrence
/// material. `additional` still uses the public/external interface; existing
/// bindings do not, so SlotMap rename/internalize/autofill cannot make them
/// stale between applications.
pub(crate) fn resume_with<'a>(
    lang: &Language,
    reg: &OntologyRegistry,
    construction: &str,
    committed: &[(String, BoundFiller)],
    additional: &[SlotFiller<'a>],
    mapping: &SlotMap,
) -> Result<DerivedToken, CxgError> {
    apply_with_committed(lang, reg, construction, committed, additional, mapping)
}

fn apply_with_committed<'a>(
    lang: &Language,
    reg: &OntologyRegistry,
    construction: &str,
    committed: &[(String, BoundFiller)],
    fillers: &[SlotFiller<'a>],
    mapping: &SlotMap,
) -> Result<DerivedToken, CxgError> {
    let local_cx = lang
        .sign_named(construction)
        .ok_or_else(|| CxgError::UnknownConstruction(construction.to_owned()))?;
    let cx = reg.effective_sign(local_cx);
    let slots = slots_of(&cx);
    if slots.is_empty() {
        return Err(CxgError::NotAConstruction(construction.to_owned()));
    }
    // Preserve only the caller-supplied mapping for a later application.
    // Source-declared SlotMap operations are recomputed from the source Sign;
    // storing the combined map would apply those operations twice on resume.
    let invocation_mapping = mapping.clone();
    let mapping = slot_map_of(&cx).and(mapping);
    let mapped = validate_slot_map(&slots, &mapping)?;
    // phon 模板包於 `/…/`(I22 phon 表徵);剝外層再代入 `{slot}`。
    let raw = phon_value(&cx);
    let template = raw
        .strip_prefix('/')
        .and_then(|s| s.strip_suffix('/'))
        .unwrap_or(&raw)
        .to_owned();
    // 模板引用須 ⊆ slot 名
    for r in template_refs(&template) {
        if !slots.iter().any(|s| s.name == r) {
            return Err(CxgError::TemplateSlotUnknown {
                construction: construction.to_owned(),
                slot: r,
            });
        }
    }

    let material = |filler: Filler<'a>| -> Result<FillerMaterial, CxgError> {
        match filler {
            Filler::Sign(name) => {
                let sign = lang
                    .sign_named(name)
                    .ok_or_else(|| CxgError::UnknownFiller(name.to_owned()))?;
                sign_material(sign, reg)
            }
            Filler::Token(token) => token_material(token),
        }
    };

    let mut provided: Vec<(String, FillerMaterial)> = Vec::new();
    for (internal, filler) in committed {
        if !mapped
            .iter()
            .any(|candidate| candidate.internal.name == *internal)
        {
            return Err(CxgError::UnknownSlot {
                construction: construction.to_owned(),
                slot: internal.clone(),
            });
        }
        if provided.iter().any(|(slot, _)| slot == internal) {
            return Err(CxgError::DuplicateFill(internal.clone()));
        }
        provided.push((internal.clone(), bound_material(filler, reg)?));
    }
    for fill in fillers {
        let slot = mapped
            .iter()
            .find(|slot| slot.external.as_deref() == Some(fill.slot))
            .ok_or_else(|| CxgError::UnknownSlot {
                construction: construction.to_owned(),
                slot: fill.slot.to_owned(),
            })?;
        if provided
            .iter()
            .any(|(internal, _)| internal == &slot.internal.name)
        {
            return Err(CxgError::DuplicateFill(fill.slot.to_owned()));
        }
        provided.push((slot.internal.name.clone(), material(fill.filler)?));
    }
    for slot in &mapped {
        let Some(filler_name) = slot.autofill.as_deref() else {
            continue;
        };
        if provided
            .iter()
            .any(|(internal, _)| internal == &slot.internal.name)
        {
            if committed
                .iter()
                .any(|(internal, _)| internal == &slot.internal.name)
            {
                continue;
            }
            return Err(CxgError::DuplicateFill(slot.internal.name.clone()));
        }
        let sign = lang
            .sign_named(filler_name)
            .ok_or_else(|| CxgError::UnknownFiller(filler_name.to_owned()))?;
        provided.push((slot.internal.name.clone(), sign_material(sign, reg)?));
    }

    // Validate every material before creating a token, then reorder by the
    // construction's declaration order for deterministic provenance.
    for (slot_name, filler) in &provided {
        let slot = mapped
            .iter()
            .find(|candidate| candidate.internal.name == *slot_name)
            .expect("mapped slot checked");
        let authorized = match &slot.internal.constraint {
            SlotConstraint::AnySign => true,
            SlotConstraint::Category(required) => filler
                .categories
                .iter()
                .any(|category| category == required),
        };
        if !authorized {
            return Err(CxgError::CategoryMismatch {
                slot: slot
                    .external
                    .clone()
                    .unwrap_or_else(|| slot.internal.name.clone()),
                filler: filler.name.clone(),
                required: slot.internal.constraint.display_name().to_owned(),
                has: filler.categories.clone(),
            });
        }
    }

    // A slot feature binding assigns an ephemeral value to one filler
    // occurrence.  It is resolved by internal slot name after SlotMap, so a
    // renamed public interface cannot redirect the grammatical dependency.
    // The source sign/token remains immutable.
    let probe = provided.clone();
    let mut assignments: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for binding in cx.items.iter().filter_map(|item| match item {
        SignItem::SlotFeatureBinding(binding) => Some(binding),
        _ => None,
    }) {
        let Some(target_slot) = mapped
            .iter()
            .find(|candidate| candidate.internal.name == binding.slot)
        else {
            return Err(CxgError::SlotFeatureUnknownTarget {
                construction: construction.to_owned(),
                slot: binding.slot.clone(),
            });
        };
        if target_slot.internal.constraint == SlotConstraint::AnySign {
            return Err(CxgError::SlotFeatureAnySign {
                construction: construction.to_owned(),
                slot: binding.slot.clone(),
                feature: binding.feature.clone(),
            });
        }
        // An absent optional target has no occurrence to constrain.
        if !provided.iter().any(|(slot, _)| slot == &binding.slot) {
            continue;
        }
        let value = if let Some((source_slot, source_feature)) = slot_feature_source(&binding.value)
        {
            if !mapped
                .iter()
                .any(|candidate| candidate.internal.name == source_slot)
            {
                return Err(CxgError::SlotFeatureUnknownSource(source_slot.to_owned()));
            }
            let Some((_, source)) = probe.iter().find(|(slot, _)| slot == source_slot) else {
                return Err(CxgError::SlotFeatureSourceMissing {
                    slot: source_slot.to_owned(),
                    feature: source_feature.to_owned(),
                });
            };
            let path = format!("syn.{source_feature}");
            source
                .syn
                .iter()
                .find(|(candidate, _)| candidate == &path)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| CxgError::SlotFeatureSourceMissing {
                    slot: source_slot.to_owned(),
                    feature: source_feature.to_owned(),
                })?
        } else {
            binding.value.clone()
        };
        let targets = assignments.entry(binding.slot.clone()).or_default();
        if targets
            .iter()
            .any(|(feature, _)| feature == &binding.feature)
        {
            return Err(CxgError::SlotFeatureDuplicateTarget {
                slot: binding.slot.clone(),
                feature: binding.feature.clone(),
            });
        }
        let target = probe
            .iter()
            .find(|(slot, _)| slot == &binding.slot)
            .map(|(_, material)| material)
            .expect("filled target checked above");
        validate_occurrence_feature(&binding.slot, &binding.feature, &value, target)?;
        targets.push((binding.feature.clone(), value));
    }
    for (slot, constraints) in &assignments {
        let target = provided
            .iter_mut()
            .find(|(candidate, _)| candidate == slot)
            .map(|(_, material)| material)
            .expect("filled target checked above");
        apply_occurrence_constraints(slot, constraints, target, reg)?;
    }
    for (_, filler) in &mut provided {
        realize_stored_filler(filler, reg)?;
    }
    provided.sort_by_key(|(name, _)| {
        mapped
            .iter()
            .position(|slot| slot.internal.name == *name)
            .unwrap_or(usize::MAX)
    });
    validate_binary_constraints(&cx, &mapped, &provided, &template)?;

    let filled: Vec<_> = provided
        .iter()
        .map(|(slot, filler)| (slot.clone(), filler.phon.clone()))
        .collect();
    let filler_nodes: Vec<_> = provided
        .iter()
        .map(|(slot, filler)| (slot.clone(), filler.sem.clone()))
        .collect();
    let provenance = TokenProvenance {
        construction: construction.to_owned(),
        fillers: provided
            .iter()
            .map(|(slot, filler)| SlotProvenance {
                slot: slot.clone(),
                source: filler.provenance.clone(),
            })
            .collect(),
    };

    let own_residual: Vec<Slot> = mapped
        .iter()
        .filter(|slot| !filled.iter().any(|(name, _)| name == &slot.internal.name))
        .filter_map(|slot| {
            slot.external.as_ref().map(|external| Slot {
                name: external.clone(),
                constraint: slot.internal.constraint.clone(),
                optional: slot.internal.optional,
            })
        })
        .collect();
    // An unsaturated application is still a Sign. Propagate the free
    // variables of nested Sign arguments through the enclosing Sign value.
    let mut residual = own_residual.clone();
    for (_, filler) in &provided {
        let Some(nested) = filler.occurrence_token.as_ref() else {
            continue;
        };
        for nested_slot in nested.residual_slots() {
            if let Some(existing) = residual
                .iter_mut()
                .find(|slot| slot.name == nested_slot.name)
            {
                if existing.constraint != nested_slot.constraint {
                    return Err(CxgError::ResidualConstraintConflict {
                        name: nested_slot.name.clone(),
                        left: existing.constraint.display_name().to_owned(),
                        right: nested_slot.constraint.display_name().to_owned(),
                    });
                }
                existing.optional &= nested_slot.optional;
            } else {
                residual.push(nested_slot.clone());
            }
        }
    }

    let sem = resolve_sem(&cx, &slots, &filler_nodes, reg)?;
    if residual.iter().all(|slot| slot.optional) {
        let missing_roles = cx
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::RoleDecl(role)
                    if !role.optional
                        && !sem.roles.iter().any(|(name, _)| name == &role.name)
                        && !cx.items.iter().any(|item| {
                            matches!(item, SignItem::RoleExpression(expression) if expression.name == role.name)
                        }) =>
                {
                    Some(role.name.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !missing_roles.is_empty() {
            return Err(CxgError::MissingRoles {
                construction: construction.to_owned(),
                roles: missing_roles,
            });
        }
    }
    let syn = cx.project(Dim::Syn, reg).defs;
    let prag = cx.project(Dim::Prag, reg).defs;
    let current_form = substitute(&template, &filled);
    let mut phon = cx.project(Dim::Phon, reg).defs;
    let current_root = format!("/{current_form}/");
    if let Some((_, value)) = phon.iter_mut().find(|(path, _)| path == "phon") {
        *value = current_root;
    } else {
        phon.push(("phon".to_owned(), current_root));
    }
    let filler_snapshots = provided
        .iter()
        .map(|(slot, filler)| FillerSnapshot {
            slot: slot.clone(),
            name: filler.name.clone(),
            categories: filler.categories.clone(),
            phon: filler.phon_projection.clone(),
            syn: filler.syn.clone(),
            sem: filler.sem.clone(),
            prag: filler.prag.clone(),
            provenance: filler.provenance.clone(),
        })
        .collect();
    let occurrence_records = provided
        .iter()
        .map(|(slot, filler)| OccurrenceRecord {
            slot_path: slot.clone(),
            source: filler.provenance.clone(),
            constraints: assignments
                .get(slot)
                .into_iter()
                .flatten()
                .map(|(name, value)| (Dim::Syn, name.clone(), value.clone()))
                .collect(),
            reevaluated: assignments.contains_key(slot),
            probe_rules: filler.probe_rules.clone(),
            committed_rules: filler.committed_rules.clone(),
            cases: filler.committed_cases.clone(),
            realization: filler.realization.clone(),
        })
        .collect();
    let bound_fillers = provided
        .iter()
        .map(|(slot, material)| {
            committed_bound_filler(material).map(|filler| (slot.clone(), filler))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut rule_sign = cx.clone();
    for item in &mut rule_sign.items {
        let SignItem::Slot(slot) = item else {
            continue;
        };
        if let Some(runtime_slot) = mapped
            .iter()
            .find(|candidate| candidate.internal.name == slot.name)
        {
            slot.optional = runtime_slot.internal.optional;
        }
    }

    let deep_state = DeepTokenState {
        phon: phon.clone(),
        syn: syn.clone(),
        sem: sem.clone(),
        prag: prag.clone(),
    };
    Ok(DerivedToken {
        construction: construction.to_owned(),
        construction_id: cx.id.clone(),
        syn_categories: reg.sign_categories(&cx),
        phon,
        syn,
        sem,
        prag,
        provenance,
        fillers: filler_snapshots,
        filled,
        template,
        residual,
        own_residual,
        rule_sign,
        source_sign: local_cx.clone(),
        realized_phon_input: None,
        deep_state,
        context_features: BTreeMap::new(),
        occurrence_records,
        bound_fillers,
        invocation_mapping,
    })
}

/// derived token → 表層(經引擎:build_phrase → run → spell-out)。飽和才可求。
pub fn surface(program: &Program, tok: &DerivedToken) -> Result<String, CxgError> {
    if tok
        .rule_sign
        .items
        .iter()
        .any(|item| matches!(item, SignItem::Realization(_)))
    {
        return Err(CxgError::RealizationRequiresSystem);
    }
    let form = tok.phon_form()?;
    let w = build_phrase(program, &form).map_err(|e| CxgError::Engine(e.to_string()))?;
    let fallback = w.clone();
    let steps = run_program(program, w).map_err(|e| CxgError::Engine(e.to_string()))?;
    let last = steps.last().map(|s| &s.word).unwrap_or(&fallback);
    surface_phrase(program, last).map_err(|e| CxgError::Engine(e.to_string()))
}
