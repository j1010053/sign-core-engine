//! Public M1++ runtime path: Language -> validated compiled system -> stored
//! signs / constructions / derived tokens -> phon surface and complete trace.

use crate::codegen::{self, Artifacts, CodegenError};
use crate::construction::{
    self, BoundFiller, CxgError, DerivedToken, FillerProvenance, OccurrenceRecord, SlotFiller,
    SlotMap, SlotMapOp,
};
use crate::diagnostic::{Diagnostic, DiagnosticSource, Severity, SourceLocation, ValidationReport};
use crate::library::{self, LibraryId, LibraryLoadError, LibrarySpec, ResolvedPackages};
use crate::ontology::OntologyRegistry;
use crate::path::parse_path;
use crate::reference;
use crate::sampling::{sample_weighted_index, WeightedSampleError};
use crate::semantic_dto::{SemanticDocumentError, SemanticDocumentV1, SemanticNodeV1};
use crate::synchronic::{self, RuleRecord, RuleStatus, SelfRead, SlotRead};
use crate::{
    CaseCondition, CaseSelection, Dim, Expression, Language, LanguageDocument, SignApplication,
    SignArgumentValue, SignDef, SignId, SignItem, SignLifecycle, SignProvenance, TypedCase,
};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use tshiatun_dsl::StepRecord;

#[derive(Debug, thiserror::Error)]
pub enum CompileSystemError {
    #[error(transparent)]
    Codegen(#[from] CodegenError),
    #[error(transparent)]
    Library(#[from] LibraryLoadError),
    #[error("M1++ validation failed")]
    Validation(ValidationReport),
}

#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("unknown sign {0:?}")]
    UnknownSign(String),
    #[error(transparent)]
    Construction(#[from] CxgError),
    #[error("phon rule-set rejected: {0}")]
    PhonCompile(String),
    #[error("phon runtime: {0}")]
    PhonRuntime(String),
    #[error("derivation feature {dim:?}.{name} is undeclared")]
    UndeclaredDerivationFeature { dim: Dim, name: String },
    #[error("typed features are not supported in the {dim:?} dimension")]
    UnsupportedFeatureDimension { dim: Dim },
    #[error("derivation feature {dim:?}.{name} value {value:?} is outside enum({domain})")]
    DerivationFeatureOutOfDomain {
        dim: Dim,
        name: String,
        value: String,
        domain: String,
    },
    #[error(
        "derivation feature conflict at {dim:?}.{name}: expected {expected:?}, got {actual:?}"
    )]
    DerivationFeatureConflict {
        dim: Dim,
        name: String,
        expected: String,
        actual: String,
    },
    #[error("realization guard failed: {0}")]
    RealizationGuard(String),
    #[error("realized phon input is not pure: {0}")]
    ImpureRealizedPhon(String),
    #[error("unknown construction category {0:?}")]
    UnknownConstructionCategory(String),
    #[error("AMBIGUOUS_CONSTRUCTION: category {category:?} has candidates {candidates:?}")]
    AmbiguousConstruction {
        category: String,
        candidates: Vec<String>,
    },
    #[error("NO_MATCHING_CONSTRUCTION: category {category:?} has no compatible candidate")]
    NoMatchingConstruction { category: String },
    #[error("unknown candidate {0}")]
    UnknownCandidate(SignId),
    #[error("all candidate entrenchment weights are zero")]
    ZeroCandidateWeight,
    #[error("invalid Sign expression: {0}")]
    InvalidSignExpression(String),
    #[error("CASE_DEFAULT_MISSING: {context} has no matching branch and no base value")]
    CaseDefaultMissing { context: String },
    #[error("Sign application cycle: {0:?}")]
    SignApplicationCycle(Vec<String>),
}

/// Errors in this closed list mean that a matched case branch attempted a
/// more-specific Sign application whose hard typed constraints cannot be
/// satisfied by the current value. They may fall through to the next branch;
/// malformed grammar, unknown paths, evaluator failures, and purity errors
/// must remain fatal and therefore do not belong here.
fn is_case_blocking_constraint(error: &SystemError) -> bool {
    matches!(
        error,
        SystemError::UndeclaredDerivationFeature { .. }
            | SystemError::DerivationFeatureOutOfDomain { .. }
            | SystemError::DerivationFeatureConflict { .. }
            | SystemError::Construction(
                CxgError::CategoryMismatch { .. }
                    | CxgError::ResidualConstraintConflict { .. }
                    | CxgError::MissingRoles { .. }
                    | CxgError::RoleCategoryMismatch { .. }
                    | CxgError::SlotFeatureUndeclared { .. }
                    | CxgError::SlotFeatureSourceMissing { .. }
                    | CxgError::SlotFeatureOutOfDomain { .. }
                    | CxgError::SlotFeatureConflict { .. }
                    | CxgError::ConstraintDomainMismatch { .. }
                    | CxgError::ConstraintEqualityConflict { .. }
                    | CxgError::ConstraintOrderConflict { .. }
            )
    )
}

fn is_candidate_compatibility_mismatch(error: &SystemError) -> bool {
    is_case_blocking_constraint(error)
        || matches!(
            error,
            SystemError::Construction(CxgError::UnknownSlot { .. } | CxgError::Unsaturated(_))
        )
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DerivationContext {
    features: BTreeMap<(Dim, String), String>,
}

impl DerivationContext {
    pub fn new() -> DerivationContext {
        DerivationContext::default()
    }

    pub fn feature(
        mut self,
        dim: Dim,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> DerivationContext {
        self.features.insert((dim, name.into()), value.into());
        self
    }

    pub fn features(&self) -> &BTreeMap<(Dim, String), String> {
        &self.features
    }
}

/// 一個跑完 `evaluate_applied_sign`(token rules + sign-body `case:`)的 token。
///
/// 存在理由與 [`RealizedPhonInput`] 同一路數:用型別記錄「已經跨過哪一道關」。
/// `DerivedToken` 一種型別涵蓋了兩個差很多的狀態——`apply_construction` 之後的
/// 半成品,與完整求值後的成品——而 `realize_phon` 只對後者有意義。
///
/// 只由 [`CompiledSystem::evaluate_token`] 與內部的求值路徑產生。
#[derive(Debug, Clone)]
pub struct EvaluatedToken(DerivedToken);

impl EvaluatedToken {
    /// 只給已經走過 `evaluate_applied_sign` 的內部路徑用。crate 外無法構造,
    /// 這正是這個型別的用處。
    pub(crate) fn already_evaluated(token: DerivedToken) -> EvaluatedToken {
        EvaluatedToken(token)
    }

    pub fn as_token(&self) -> &DerivedToken {
        &self.0
    }

    /// **刻意不公開**:公開它等於開一條 `evaluate_token(x.into_token())` 的回頭路,
    /// 而重跑一次 token rules 不是冪等的。crate 外只需要 [`Self::as_token`] 讀。
    pub(crate) fn into_token(self) -> DerivedToken {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealizedPhonInput(String);

impl RealizedPhonInput {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhonRealization {
    pub input: RealizedPhonInput,
    /// `None` means the deep/default template was used.
    pub branch: Option<usize>,
    pub source: SourceLocation,
    pub slot_reads: Vec<SlotRead>,
    pub self_reads: Vec<SelfRead>,
    /// Typed phon cases, including recursively evaluated branches.
    pub cases: Vec<CaseRecord>,
    /// Rules executed by nested full-Sign applications used by phon projection.
    pub nested_rules: Vec<UnitRuleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignEvaluation {
    pub sign: SignDef,
    pub records: Vec<RuleRecord>,
    /// Unevaluated local source used when a Sign-valued case adds a trait.
    /// Keeping it separate prevents inherited rules from being replayed over
    /// values already committed by an earlier Syn -> Sem -> Prag pass.
    source_sign: SignDef,
    context: DerivationContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum SignValue {
    Stored(SignEvaluation),
    Applied(DerivedToken),
}

impl SignValue {
    /// 取出可實現的 token。`Stored` 回 `None`——它是尚未套用的 Sign,沒有可實現的
    /// occurrence。
    ///
    /// 公開 API 交出的 `SignValue`(`evaluate_sign_expression`、`apply_arguments`)
    /// 一律已走過 `evaluate_applied_sign`,故 `Applied` 就是 [`EvaluatedToken`]。
    pub fn into_evaluated(self) -> Option<EvaluatedToken> {
        match self {
            SignValue::Applied(token) => Some(EvaluatedToken::already_evaluated(token)),
            SignValue::Stored(_) => None,
        }
    }

    /// Saturation is a state of a Sign value, never a separate entity type.
    pub fn is_saturated(&self) -> bool {
        self.residual_parameters()
            .iter()
            .all(|parameter| parameter.optional)
    }

    pub fn has_free_variables(&self) -> bool {
        !self.is_saturated()
    }

    pub fn residual_parameters(&self) -> Vec<crate::SignParameter> {
        match self {
            Self::Stored(evaluation) => construction::parameters_of(&evaluation.sign),
            Self::Applied(token) => token
                .residual_slots()
                .iter()
                .map(crate::SignParameter::from)
                .collect(),
        }
    }

    pub fn sign_id(&self) -> &SignId {
        match self {
            Self::Stored(evaluation) => &evaluation.sign.id,
            Self::Applied(token) => &token.construction_id,
        }
    }

    pub fn token(&self) -> Option<&DerivedToken> {
        match self {
            Self::Applied(token) => Some(token),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseBranchStatus {
    Matched,
    Unmatched,
    MoreSpecificBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRecord {
    pub selection: CaseSelection,
    pub branch: usize,
    pub status: CaseBranchStatus,
    pub source: SourceLocation,
    pub diagnostic_code: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignExpressionEvaluation {
    pub value: SignValue,
    pub cases: Vec<CaseRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstructionCandidate {
    pub id: SignId,
    pub name: String,
    pub entrenchment: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateSet {
    pub category: String,
    pub candidates: Vec<ConstructionCandidate>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateSelectionTrace {
    pub seed: Option<u64>,
    pub ordered: Vec<(SignId, f64)>,
    pub selected: SignId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateSelector {
    Deterministic,
    SampleEntrenchment { seed: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitRuleRecord {
    pub unit: String,
    pub record: RuleRecord,
}

#[derive(Debug, Clone)]
pub struct SystemDerivation {
    pub token: DerivedToken,
    pub surface: String,
    /// Filler records followed by construction/token records.
    pub rules: Vec<UnitRuleRecord>,
    pub phon_steps: Vec<StepRecord>,
    pub diagnostics: Vec<Diagnostic>,
    pub realization: PhonRealization,
    pub occurrences: Vec<OccurrenceRecord>,
    /// Typed feature/role/Sign cases in their committed evaluation order.
    pub cases: Vec<CaseRecord>,
}

#[derive(Debug, Clone)]
pub struct CompiledSystem {
    /// Caller-owned source retained for compatibility and serialization.
    language: Language,
    /// Selected natural/plugin packages followed by caller source. Std traits
    /// remain ontology-only and are not copied into this view.
    effective_language: Language,
    libraries: Vec<LibraryId>,
    pub artifacts: Artifacts,
    pub ontology: OntologyRegistry,
    pub validation: ValidationReport,
}

fn path_dimension(path: &str) -> Option<Dim> {
    let head = path.split_once('.').map(|(head, _)| head).unwrap_or(path);
    Dim::parse(head)
}

/// 引擎自有的 `Def` 路徑(P71 §4.2)——引擎自己讀的那幾條。
const ENGINE_DEF_PATHS: &[&str] = &["phon", "phon.realization"];

/// 套件座標的合法**前綴**(P71 §4.2:Phase 1 硬編;Phase 2 改為套件自行宣告,見 §5 ④)。
///
/// 命中規則:路徑等於該字串,或以 `<字串>.` 起頭。內容取自 `crates/language/lib`
/// 下 std/natural 套件**實際使用**的座標(A4 重新量測,見規格 §7.5),
/// 不是憑印象列的——漏一條就會讓套件自己編不過。
const PACKAGE_DEF_PREFIXES: &[&str] = &[
    "prag.clause-type",
    "prag.evidence",
    "prag.identifiability",
    "prag.illocution",
    "prag.information-structure",
    "prag.perspective",
    "prag.reference",
    "sem.aspect",
    "sem.causation",
    "sem.event",
    "sem.number",
    "sem.person",
    "sem.polarity",
    "sem.possession",
    "sem.predication",
    "sem.quantification",
    "sem.reference",
    // `sem.roles` 同時是引擎自有(內部 context 標籤)與套件座標(`sem.roles.beneficiary`)。
    "sem.roles",
    "sem.time",
    "syn.adposition",
    "syn.alignment",
    "syn.argument",
    "syn.complex-predicate",
    "syn.determination",
    "syn.evidential",
    "syn.interrogative",
    "syn.negation",
    "syn.number",
    "syn.numeral",
    "syn.possession",
    "syn.predication",
    "syn.pronoun",
    "syn.tam",
    "syn.typology",
    "syn.valency",
    "syn.voice",
    "syn.word-order",
];

/// P71 R1 + §7 A1:路徑是否在封閉清單上。**`Def` 與 synchronic rule 目標共用**
/// ——否則關了前門(Def)還留著側門(規則目標),而規則寫的是同一個路徑空間。
pub(crate) fn def_path_allowed(path: &str) -> bool {
    ENGINE_DEF_PATHS.contains(&path)
        || PACKAGE_DEF_PREFIXES.iter().any(|prefix| {
            path == *prefix
                || path
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
}

/// 一個(已 `effective_sign` 解析過的)sign 上可見的 typed feature 名。
/// P71 增修 D 的讀取白名單第二半;主體是 `$self` 時用它。
fn visible_features(effective: &SignDef) -> BTreeSet<(Dim, String)> {
    effective
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::FeatureDecl(feature) => Some((feature.dim, feature.name.clone())),
            _ => None,
        })
        .collect()
}

/// 全語言(含 registry 帶進來的套件節點)宣告過的 typed feature 名。
///
/// `$slot.NAME` 的主體是 filler,靜態未知——`[*]` 槽可填任何 sign,具名約束的
/// filler 也能自帶本地 feature,故不能用槽的約束範疇去收窄。取全域集合是**不會
/// 誤擋**的最強上界:全語言沒有任何一處宣告過的名字,沒有任何 filler 能有它。
fn language_wide_features(
    language: &Language,
    registry: &OntologyRegistry,
) -> BTreeSet<(Dim, String)> {
    let mut features = BTreeSet::new();
    let mut absorb = |items: &[SignItem]| {
        for item in items {
            if let SignItem::FeatureDecl(feature) = item {
                features.insert((feature.dim, feature.name.clone()));
            }
        }
    };
    for sign in &language.signs {
        absorb(&sign.items);
    }
    for trait_def in &language.traits {
        for block in &trait_def.blocks {
            absorb(&block.items);
        }
    }
    for name in registry.names() {
        if let Some(node) = registry.node(name) {
            absorb(&node.items);
        }
    }
    features
}

/// P71 增修 D:guard **讀**到清單外路徑時的指路訊息。與 `closed_list_hint` 分開,
/// 因為讀取端的白名單多一半(可見的 typed feature),訊息若不說出這半,作者會
/// 以為宣告了 feature 也還是不能讀。
pub(crate) fn read_path_hint(path: &str, subject: &str) -> String {
    let dim = path_dimension(path)
        .map(|dim| dim.keyword())
        .unwrap_or("syn");
    format!(
        "guard reads {subject}.{path}, which is neither on the closed list (P71) \
         nor a feature declared on that subject; \
         fields a guard reads must be declared under `{dim}: feature:` with an enum domain"
    )
}

/// 不在清單上時給作者的指路訊息(§4.2:訊息**必須指向 `feature:`**,
/// 否則只看到一句 invalid Definition 而不知正解)。
pub(crate) fn closed_list_hint(path: &str) -> String {
    let dim = path_dimension(path)
        .map(|dim| dim.keyword())
        .unwrap_or("syn");
    format!(
        "path {path:?} is not on the closed list (P71); \
         author-defined fields must be declared under `{dim}: feature:` with an enum domain"
    )
}

/// `validate_items` 需要的 sign/trait 局部脈絡。合成一個結構而非再加一個閉包參數,
/// 是為了不讓 130 行的閉包本體因參數表變長而整段重排。
struct ItemContext<'a> {
    slots: &'a [crate::Slot],
    /// P71 增修 D:guard 讀取白名單的第二半。具體 sign 傳自己的有效宣告(嚴查),
    /// trait 傳語言全域集合(`$self` 靜態未知)——理由見
    /// [`synchronic::rule_guard_violations`]。
    subject_features: &'a BTreeSet<(Dim, String)>,
}

fn validate_defs_and_rules(
    language: &Language,
    externals: &[&Language],
    registry: &OntologyRegistry,
    report: &mut ValidationReport,
) {
    let filler_features = language_wide_features(language, registry);
    let mut validate_items = |owner: &str,
                              items: &[SignItem],
                              sign_metadata: bool,
                              context: &ItemContext<'_>| {
        let ItemContext {
            slots,
            subject_features,
        } = context;
        let mut slot_feature_targets = BTreeSet::new();
        // 同一個 sign/trait 內重複宣告同名義項 = 撰寫錯誤(繼承層的覆寫走 effective)。
        let mut seen_senses: Vec<&str> = Vec::new();
        for sense in items.iter().filter_map(|item| match item {
            SignItem::Sense(sense) => Some(sense),
            _ => None,
        }) {
            if seen_senses.contains(&sense.name.as_str()) {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "SENSE_DUPLICATE",
                    format!("{owner:?} declares sense {:?} more than once", sense.name),
                ));
            }
            seen_senses.push(&sense.name);
        }
        for item in items {
            match item {
                SignItem::Def(def) => {
                    let valid_meta = sign_metadata
                        && match def.path.as_str() {
                            "entrenchment" => def
                                .value
                                .parse::<f64>()
                                .is_ok_and(|value| value.is_finite() && value >= 0.0),
                            "lexicalized" => matches!(def.value.as_str(), "true" | "false"),
                            "origin" => crate::metadata::parse_origin(&def.value).is_some(),
                            // P54:至少兩個 `sign(x)`,逗號分隔。
                            "components" => crate::metadata::parse_components(&def.value).is_some(),
                            "provenance" => SignProvenance::parse(&def.value).is_some(),
                            "lifecycle" => SignLifecycle::parse(&def.value).is_some(),
                            "source_package" => LibraryId::from_str(&def.value).is_ok(),
                            _ => false,
                        };
                    // P71 §4.2:此處**曾是**開放逃生口——只檢查「長得像 `<dim>.<field>`」,
                    // 欄位名不查、值不查。現改為封閉清單,比照上面的 `valid_meta`。
                    let valid_dim = path_dimension(&def.path).is_some()
                        && (def.path == "phon"
                            || def
                                .path
                                .split_once('.')
                                .is_some_and(|(_, field)| !field.is_empty()))
                        && parse_path(&def.path).is_ok()
                        && def_path_allowed(&def.path);
                    if !valid_meta && !valid_dim {
                        let detail = if path_dimension(&def.path).is_some()
                            && !def_path_allowed(&def.path)
                        {
                            closed_list_hint(&def.path)
                        } else {
                            format!("invalid Definition {} = {}", def.path, def.value)
                        };
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "DEF_INVALID_PATH_OR_VALUE",
                                format!("{owner:?} has {detail}"),
                            )
                            .with_sources(vec![DiagnosticSource {
                                owner: owner.to_owned(),
                                path: Some(def.path.clone()),
                                location: SourceLocation::unknown(),
                            }]),
                        );
                    }
                }
                SignItem::Rule(rule) | SignItem::FeatureRule(rule) if rule.dim != Dim::Phon => {
                    let source = || {
                        vec![DiagnosticSource {
                            owner: owner.to_owned(),
                            path: Some(format!("rule {}", rule.id)),
                            location: rule.source,
                        }]
                    };
                    for error in synchronic::validate_rule(rule, registry, slots) {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "RULE_INVALID",
                                format!("{owner:?}: {error}"),
                            )
                            .with_sources(source()),
                        );
                    }
                    // P71 §7 A1:目標路徑亦受封閉清單約束。只查**普通規則**——
                    // `FeatureRule` 的出口是 `feature:`,自有兩道既存檢查。
                    if matches!(item, SignItem::Rule(_)) {
                        for error in synchronic::rule_target_violations(rule) {
                            report.push(
                                Diagnostic::new(
                                    Severity::Error,
                                    "RULE_TARGET_NOT_ALLOWED",
                                    format!("{owner:?}: {error}"),
                                )
                                .with_sources(source()),
                            );
                        }
                    }
                    // P71 增修 D:guard 讀的路徑同受約束。**兩種規則都查**——
                    // 上面豁免 `FeatureRule` 的理由只及於它的目標,不及於 guard。
                    for error in
                        synchronic::rule_guard_violations(rule, subject_features, &filler_features)
                    {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "RULE_GUARD_NOT_ALLOWED",
                                format!("{owner:?}: {error}"),
                            )
                            .with_sources(source()),
                        );
                    }
                    // P71 增修 E:值表達式的讀取同理(理由同上,兩種規則都查)。
                    for error in
                        synchronic::rule_value_violations(rule, subject_features, &filler_features)
                    {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "RULE_VALUE_NOT_ALLOWED",
                                format!("{owner:?}: {error}"),
                            )
                            .with_sources(source()),
                        );
                    }
                }
                SignItem::SlotFeatureBinding(binding)
                    if !slot_feature_targets
                        .insert((binding.slot.as_str(), binding.feature.as_str())) =>
                {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "SLOT_FEATURE_DUPLICATE_TARGET",
                            format!(
                                "{owner:?} assigns slot feature {}.{} more than once",
                                binding.slot, binding.feature
                            ),
                        )
                        .with_sources(vec![DiagnosticSource {
                            owner: owner.to_owned(),
                            path: Some(format!(
                                "syn.slot_features.{}.{}",
                                binding.slot, binding.feature
                            )),
                            location: binding.source,
                        }]),
                    );
                }
                _ => {}
            }
        }
    };
    for trait_def in &language.traits {
        // [A] 3-2:trait 的視圖走**展開**,不走投影的內容那一半。展開會遞迴把
        // 祖先的內容拉進來,而且與真實編譯同一條路——驗證看到的不會與編譯產出分岔。
        // 展開失敗時退回空視圖:那個錯誤由編譯路徑自己報,這裡不重複也不假裝。
        let effective = SignDef {
            id: crate::SignId::synthetic(),
            name: format!("{}#rule-validation", trait_def.name),
            items: crate::compile::trait_view(language, externals, &trait_def.name)
                .unwrap_or_default(),
        };
        // P71 增修 D:**trait 的 `$self` 靜態未知**,故與 filler 同樣用全域上界。
        // trait 是模板,合成後的 sign 帶什麼 feature 不由它決定:菱形繼承下
        // `Right` 的規則合法地 guard 在**兄弟** `Left` 宣告的 feature 上
        // (`m1pp_system::inherited_rules_are_diamond_deduplicated_…` 即此形狀),
        // 用 trait 自己的繼承視野去嚴查會誤擋。嚴查留給具體 sign——它的
        // feature 集合是封閉的。
        let context = ItemContext {
            slots: &construction::slots_of(&effective),
            subject_features: &filler_features,
        };
        for block in &trait_def.blocks {
            validate_items(&trait_def.name, &block.items, false, &context);
        }
    }
    for sign in &language.signs {
        let effective = registry.effective_sign(sign);
        let context = ItemContext {
            slots: &construction::slots_of(&effective),
            subject_features: &visible_features(&effective),
        };
        validate_items(&sign.name, &sign.items, true, &context);
    }
}

fn validate_typed_schemas(
    language: &Language,
    externals: &[&Language],
    registry: &OntologyRegistry,
    report: &mut ValidationReport,
) {
    fn expression_leaves<'a>(expression: &'a Expression, output: &mut Vec<&'a Expression>) {
        match expression {
            Expression::Case(case) => {
                for branch in &case.branches {
                    expression_leaves(&branch.result, output);
                }
            }
            expression => output.push(expression),
        }
    }

    fn collect_sign_fragments(
        expression: &Expression,
        inherited: &[SignItem],
        output: &mut Vec<Vec<SignItem>>,
    ) {
        match expression {
            Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
                let mut context = inherited.to_vec();
                context.extend(items.iter().cloned());
                output.push(context.clone());
                for item in items {
                    collect_item_fragments(item, &context, output);
                }
            }
            Expression::Case(case) => {
                for branch in &case.branches {
                    collect_sign_fragments(&branch.result, inherited, output);
                }
            }
            Expression::Projection { value, .. } => {
                collect_sign_fragments(value, inherited, output)
            }
            _ => {}
        }
    }

    fn collect_item_fragments(
        item: &SignItem,
        inherited: &[SignItem],
        output: &mut Vec<Vec<SignItem>>,
    ) {
        match item {
            SignItem::SignExpression(expression) => {
                collect_sign_fragments(&expression.expression, inherited, output)
            }
            SignItem::FeatureExpression(expression) => {
                collect_sign_fragments(&expression.expression, inherited, output)
            }
            SignItem::RoleExpression(expression) => {
                collect_sign_fragments(&expression.expression, inherited, output)
            }
            SignItem::Realization(realization) => {
                for branch in &realization.expression.branches {
                    collect_sign_fragments(&branch.result, inherited, output);
                }
            }
            _ => {}
        }
    }

    fn slot_feature_read(value: &str) -> Option<(String, String)> {
        let reference = reference::parse(&reference::SLOT_SYN_FEATURE, value).ok()?;
        Some((reference.slot()?.to_owned(), reference.path.clone()?))
    }

    fn category_feature_domain(
        language: &Language,
        externals: &[&Language],
        registry: &OntologyRegistry,
        category: &str,
        feature: &str,
    ) -> Option<Vec<String>> {
        if !registry.has(category) {
            return None;
        }
        crate::compile::trait_view(language, externals, category)
            .unwrap_or_default()
            .into_iter()
            .find_map(|item| match item {
                SignItem::FeatureDecl(declaration)
                    if declaration.dim == Dim::Syn && declaration.name == feature =>
                {
                    Some(declaration.values)
                }
                _ => None,
            })
    }

    fn inherited_items(source: &SignDef, registry: &OntologyRegistry) -> Vec<SignItem> {
        let mut items = Vec::new();
        for provenance in registry.inheritance_order(source) {
            if let Some(node) = registry.node(&provenance.trait_name) {
                items.extend(node.items.iter().cloned());
            }
        }
        items.extend(source.items.iter().cloned());
        items
    }
    fn validate_inherited_contracts(
        owner: &str,
        source: &SignDef,
        registry: &OntologyRegistry,
        report: &mut ValidationReport,
    ) {
        let mut features = BTreeMap::<(Dim, String), crate::FeatureDecl>::new();
        let mut roles = BTreeMap::<String, crate::RoleDecl>::new();
        for item in inherited_items(source, registry) {
            match item {
                SignItem::FeatureDecl(feature) => {
                    let key = (feature.dim, feature.name.clone());
                    if let Some(shadowed) = features.insert(key.clone(), feature.clone()) {
                        // Q1:**型別宣告一次,不得改變**(與 ROLE_SCHEMA_CONFLICT /
                        // SLOT_CONFLICT 同級)。feature 的值域是「這個語言的範式
                        // 長什麼樣」,是語言層級的事實,一處說了算。
                        //
                        // 收窄不會因此失去出口——「這個類只有單數」寫**賦值**
                        // (`number = singular`)即可,而賦值層還能表達未定案,
                        // 表達力比重宣告值域更強。
                        //
                        // 先前是 Warning + 訊息說「resolves A over B」,也就是承認
                        // 會靜默挑一個。而挑法無法從語法讀出意圖:`enum(sg)` 可能
                        // 是收窄(單數專用類),`enum(sg,dual,pl)` 可能是擴充(有
                        // 雙數的語言),交集與聯集各會做錯一整類案例。
                        if shadowed.values != feature.values {
                            report.push(
                                Diagnostic::new(
                                    Severity::Error,
                                    "FEATURE_DECLARATION_SHADOWED",
                                    format!(
                                        "{owner:?} re-declares {}.{} as enum({}) over enum({}); \
                                         a feature domain is declared once — to narrow it for a \
                                         subclass, assign a value instead",
                                        key.0.keyword(),
                                        key.1,
                                        feature.values.join(", "),
                                        shadowed.values.join(", ")
                                    ),
                                )
                                .with_sources(vec![
                                    DiagnosticSource {
                                        owner: owner.to_owned(),
                                        path: Some(format!("{}.{}", key.0.keyword(), key.1)),
                                        location: feature.source,
                                    },
                                    DiagnosticSource {
                                        owner: owner.to_owned(),
                                        path: Some(format!("{}.{}", key.0.keyword(), key.1)),
                                        location: shadowed.source,
                                    },
                                ]),
                            );
                        }
                    }
                }
                SignItem::RoleDecl(role) => {
                    if let Some(previous) = roles.insert(role.name.clone(), role.clone()) {
                        if previous.constraint != role.constraint
                            || previous.optional != role.optional
                        {
                            report.push(
                                Diagnostic::new(
                                    Severity::Error,
                                    "ROLE_SCHEMA_CONFLICT",
                                    format!(
                                        "{owner:?} gives role {:?} incompatible contracts [{}]{} and [{}]{}",
                                        role.name,
                                        previous.constraint.display_name(),
                                        if previous.optional { "?" } else { "" },
                                        role.constraint.display_name(),
                                        if role.optional { "?" } else { "" },
                                    ),
                                )
                                .with_sources(vec![
                                    DiagnosticSource {
                                        owner: owner.to_owned(),
                                        path: Some(format!("sem.roles.{}", role.name)),
                                        location: previous.source,
                                    },
                                    DiagnosticSource {
                                        owner: owner.to_owned(),
                                        path: Some(format!("sem.roles.{}", role.name)),
                                        location: role.source,
                                    },
                                ]),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    for sign in &language.signs {
        validate_inherited_contracts(&sign.name, sign, registry, report);
    }
    for trait_def in &language.traits {
        let source = SignDef {
            id: crate::SignId::synthetic(),
            name: trait_def.name.clone(),
            items: crate::compile::trait_view(language, externals, &trait_def.name)
                .unwrap_or_default(),
        };
        validate_inherited_contracts(&trait_def.name, &source, registry, report);
    }

    let mut candidates = language
        .signs
        .iter()
        .map(|sign| (sign.name.clone(), registry.effective_sign(sign)))
        .collect::<Vec<_>>();
    candidates.extend(language.traits.iter().map(|trait_def| {
        let synthetic = SignDef {
            id: crate::SignId::synthetic(),
            name: format!("{}#schema", trait_def.name),
            items: crate::compile::trait_view(language, externals, &trait_def.name)
                .unwrap_or_default(),
        };
        (trait_def.name.clone(), synthetic)
    }));
    let roots = candidates.clone();
    for (owner, effective) in roots {
        let mut fragments = Vec::new();
        for item in &effective.items {
            collect_item_fragments(item, &[], &mut fragments);
        }
        for (index, fragment) in fragments.into_iter().enumerate() {
            // Before the compile expansion pass a fragment may still contain
            // a trait macro.  Its complete contract is validated by the
            // ordered-language pass after the macro has been expanded.
            if fragment
                .iter()
                .any(|item| matches!(item, SignItem::TraitMount { kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_), .. }))
            {
                continue;
            }
            let mut virtual_sign = effective.clone();
            virtual_sign.name = format!("{owner}#SignContext[{index}]");
            virtual_sign.items.extend(fragment);
            candidates.push((
                virtual_sign.name.clone(),
                registry.effective_sign(&virtual_sign),
            ));
        }
    }

    for (owner, effective) in candidates {
        for item in &effective.items {
            let dim = match item {
                SignItem::FeatureDecl(feature) => Some(feature.dim),
                SignItem::FeatureValue(feature) => Some(feature.dim),
                SignItem::FeatureExpression(feature) => Some(feature.dim),
                _ => None,
            };
            // P71-C:`prag` 已納入 typed feature 支援(R2 需要一個宣告值域的出口)。
            // `phon` 仍不支援:其內容是 UR/模板與 DSL 音變規則,不是 enum 值域欄位。
            if let Some(dim @ Dim::Phon) = dim {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "FEATURE_DIMENSION_UNSUPPORTED",
                        format!(
                            "{owner:?} declares or writes a typed feature in unsupported {} dimension",
                            dim.keyword()
                        ),
                    )
                    .with_sources(vec![DiagnosticSource {
                        owner: owner.clone(),
                        path: Some(format!("{}.feature", dim.keyword())),
                        location: SourceLocation::unknown(),
                    }]),
                );
            }
        }
        let slots = construction::slots_of(&effective);
        let declarations = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::FeatureDecl(feature) => {
                    Some(((feature.dim, feature.name.clone()), feature))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let mut slot_feature_targets = BTreeSet::new();
        for binding in effective.items.iter().filter_map(|item| match item {
            SignItem::SlotFeatureBinding(binding) => Some(binding),
            _ => None,
        }) {
            let source = vec![DiagnosticSource {
                owner: owner.clone(),
                path: Some(format!(
                    "syn.slot_features.{}.{}",
                    binding.slot, binding.feature
                )),
                location: binding.source,
            }];
            if !slot_feature_targets.insert((binding.slot.clone(), binding.feature.clone())) {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "SLOT_FEATURE_DUPLICATE_TARGET",
                        format!(
                            "{owner:?} assigns slot feature {}.{} more than once",
                            binding.slot, binding.feature
                        ),
                    )
                    .with_sources(source),
                );
                continue;
            }
            let Some(target_slot) = slots.iter().find(|slot| slot.name == binding.slot) else {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "SLOT_FEATURE_UNKNOWN_TARGET",
                        format!(
                            "{owner:?} binds feature {:?} on unknown slot {:?}",
                            binding.feature, binding.slot
                        ),
                    )
                    .with_sources(source),
                );
                continue;
            };
            let Some(target_category) = target_slot.constraint.category() else {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "SLOT_FEATURE_ANY_SIGN_TARGET",
                        format!(
                            "{owner:?} cannot bind feature {:?} on unconstrained [*] slot {:?}",
                            binding.feature, binding.slot
                        ),
                    )
                    .with_sources(source),
                );
                continue;
            };
            let target_domain =
                category_feature_domain(language, externals, registry, target_category, &binding.feature);
            if target_domain.is_none() {
                report.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "SLOT_FEATURE_TARGET_RUNTIME_TYPED",
                        format!(
                            "{owner:?} slot {:?} category [{target_category}] is broader than syn feature {:?}; each actual filler is checked at runtime",
                            binding.slot, binding.feature
                        ),
                    )
                    .with_sources(source.clone()),
                );
            }

            if let Some((source_slot_name, source_feature)) = slot_feature_read(&binding.value) {
                let Some(source_slot) = slots.iter().find(|slot| slot.name == source_slot_name)
                else {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "SLOT_FEATURE_UNKNOWN_SOURCE",
                            format!(
                                "{owner:?} slot feature binding reads unknown slot {source_slot_name:?}"
                            ),
                        )
                        .with_sources(source),
                    );
                    continue;
                };
                let Some(source_category) = source_slot.constraint.category() else {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "SLOT_FEATURE_ANY_SIGN_SOURCE",
                            format!(
                                "{owner:?} cannot statically read syn feature {source_feature:?} from unconstrained [*] slot {source_slot_name:?}"
                            ),
                        )
                        .with_sources(source),
                    );
                    continue;
                };
                let source_domain =
                    category_feature_domain(language, externals, registry, source_category, &source_feature);
                if source_domain.is_none() {
                    report.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "SLOT_FEATURE_SOURCE_RUNTIME_TYPED",
                            format!(
                                "{owner:?} source slot {source_slot_name:?} category [{source_category}] is broader than syn feature {source_feature:?}; the actual filler is checked at runtime"
                            ),
                        )
                        .with_sources(source.clone()),
                    );
                }
                if let (Some(source_domain), Some(target_domain)) =
                    (source_domain.as_ref(), target_domain.as_ref())
                {
                    let incompatible = source_domain
                        .iter()
                        .filter(|value| !target_domain.contains(value))
                        .cloned()
                        .collect::<Vec<_>>();
                    if !incompatible.is_empty() {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "SLOT_FEATURE_DOMAIN_MISMATCH",
                                format!(
                                    "{owner:?} source enum({}) can produce values outside target enum({}): {}",
                                    source_domain.join(", "),
                                    target_domain.join(", "),
                                    incompatible.join(", ")
                                ),
                            )
                            .with_sources(source),
                        );
                    }
                }
            } else if binding.value.starts_with('$') {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "SLOT_FEATURE_INVALID_SOURCE",
                        format!(
                            "{owner:?} slot feature value must be a literal or `$slot.NAME.syn.FEATURE`"
                        ),
                    )
                    .with_sources(source),
                );
            } else if target_domain
                .as_ref()
                .is_some_and(|domain| !domain.contains(&binding.value))
            {
                let target_domain = target_domain.as_ref().expect("checked above");
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "SLOT_FEATURE_VALUE_OUT_OF_DOMAIN",
                        format!(
                            "{owner:?} assigns {:?} outside enum({}) to slot {:?}.{}",
                            binding.value,
                            target_domain.join(", "),
                            binding.slot,
                            binding.feature
                        ),
                    )
                    .with_sources(source),
                );
            }
        }
        for value in effective.items.iter().filter_map(|item| match item {
            SignItem::FeatureValue(value) => Some(value),
            _ => None,
        }) {
            let key = (value.dim, value.name.clone());
            match declarations.get(&key) {
                None => report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "FEATURE_UNDECLARED",
                        format!(
                            "{owner:?} assigns undeclared {} feature {:?}",
                            value.dim.keyword(),
                            value.name
                        ),
                    )
                    .with_sources(vec![DiagnosticSource {
                        owner: owner.clone(),
                        path: Some(format!("{}.{}", value.dim.keyword(), value.name)),
                        location: value.source,
                    }]),
                ),
                // 值域必須整個落在宣告域內:未定案(多候選)時每一個候選都要合法,
                // 否則「留給構式決議」會把一個非法值一路帶到構式層才爆。
                Some(declaration)
                    if value
                        .values
                        .iter()
                        .any(|candidate| !declaration.values.contains(candidate)) =>
                {
                    report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "FEATURE_VALUE_OUT_OF_DOMAIN",
                        format!(
                            "{owner:?} assigns {:?} outside enum({}) for {}.{}",
                            value.values.join(" | "),
                            declaration.values.join(", "),
                            value.dim.keyword(),
                            value.name
                        ),
                    )
                    .with_sources(vec![DiagnosticSource {
                        owner: owner.clone(),
                        path: Some(format!("{}.{}", value.dim.keyword(), value.name)),
                        location: value.source,
                    }]),
                    )
                }
                Some(_) => {}
            }
        }
        for expression in effective.items.iter().filter_map(|item| match item {
            SignItem::FeatureExpression(expression) => Some(expression),
            _ => None,
        }) {
            let key = (expression.dim, expression.name.clone());
            let Some(declaration) = declarations.get(&key) else {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "FEATURE_EXPRESSION_UNDECLARED",
                        format!(
                            "{owner:?} assigns undeclared {} feature {:?} with a typed case",
                            expression.dim.keyword(),
                            expression.name
                        ),
                    )
                    .with_sources(vec![DiagnosticSource {
                        owner: owner.clone(),
                        path: Some(format!("{}.{}", expression.dim.keyword(), expression.name)),
                        location: expression.source,
                    }]),
                );
                continue;
            };
            let Expression::Case(_) = &expression.expression else {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "FEATURE_EXPRESSION_NOT_CASE",
                    format!("{owner:?} feature expression must contain a typed case"),
                ));
                continue;
            };
            let mut leaves = Vec::new();
            expression_leaves(&expression.expression, &mut leaves);
            for leaf in leaves {
                if let Expression::EnumValue(value) = leaf {
                    if !declaration.values.contains(value) {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "FEATURE_EXPRESSION_VALUE_OUT_OF_DOMAIN",
                                format!(
                                    "{owner:?} feature case returns {value:?} outside enum({}) for {}.{}",
                                    declaration.values.join(", "),
                                    expression.dim.keyword(),
                                    expression.name
                                ),
                            )
                            .with_sources(vec![DiagnosticSource {
                                owner: owner.clone(),
                                path: Some(format!(
                                    "{}.{}",
                                    expression.dim.keyword(),
                                    expression.name
                                )),
                                location: expression.source,
                            }]),
                        );
                    }
                }
            }
        }
        for rule in effective.items.iter().filter_map(|item| match item {
            SignItem::FeatureRule(rule) => Some(rule),
            _ => None,
        }) {
            for (index, branch) in std::iter::once(rule.body.as_str())
                .chain(rule.else_chain.iter().map(String::as_str))
                .chain(rule.then_chain.iter().map(String::as_str))
                .enumerate()
            {
                let Some((lhs, rhs)) = branch.split_once("=>") else {
                    continue;
                };
                let name = lhs.trim();
                let location = if index == 0 {
                    rule.source
                } else {
                    rule.branch_sources
                        .get(index - 1)
                        .copied()
                        .unwrap_or(rule.source)
                };
                let Some(declaration) = declarations.get(&(rule.dim, name.to_owned())) else {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "FEATURE_RULE_UNDECLARED",
                            format!(
                                "{owner:?} rule branch {index} writes undeclared {} feature {name:?}",
                                rule.dim.keyword()
                            ),
                        )
                        .with_sources(vec![DiagnosticSource {
                            owner: owner.clone(),
                            path: Some(format!("{}.{}", rule.dim.keyword(), name)),
                            location,
                        }]),
                    );
                    continue;
                };
                let literal = rhs
                    .split_once(" / ")
                    .map(|(value, _)| value)
                    .unwrap_or(rhs)
                    .trim();
                if literal.starts_with('$')
                    || literal.starts_with("unify(")
                    || literal.starts_with("require(")
                {
                    continue;
                }
                if !declaration.values.iter().any(|value| value == literal) {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "FEATURE_RULE_VALUE_OUT_OF_DOMAIN",
                            format!(
                                "{owner:?} rule writes {literal:?} outside enum({}) for {}.{name}",
                                declaration.values.join(", "),
                                rule.dim.keyword(),
                            ),
                        )
                        .with_sources(vec![DiagnosticSource {
                            owner: owner.clone(),
                            path: Some(format!("{}.{}", rule.dim.keyword(), name)),
                            location,
                        }]),
                    );
                }
            }
        }

        let role_declarations = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::RoleDecl(role) => Some((role.name.clone(), role)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for expression in effective.items.iter().filter_map(|item| match item {
            SignItem::RoleExpression(expression) => Some(expression),
            _ => None,
        }) {
            if !role_declarations.contains_key(&expression.name) {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "ROLE_EXPRESSION_UNDECLARED",
                        format!(
                            "{owner:?} assigns undeclared semantic role {:?} with a typed case",
                            expression.name
                        ),
                    )
                    .with_sources(vec![DiagnosticSource {
                        owner: owner.clone(),
                        path: Some(format!("sem.roles.{}", expression.name)),
                        location: expression.source,
                    }]),
                );
            }
            let Expression::Case(_) = &expression.expression else {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "ROLE_EXPRESSION_NOT_CASE",
                    format!("{owner:?} role expression must contain a typed case"),
                ));
                continue;
            };
            let mut leaves = Vec::new();
            expression_leaves(&expression.expression, &mut leaves);
            for leaf in leaves {
                if let Expression::Slot(slot) = leaf {
                    if !slots.iter().any(|candidate| &candidate.name == slot) {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "ROLE_EXPRESSION_UNKNOWN_SLOT",
                                format!(
                                    "{owner:?} role {:?} case returns unknown slot {slot:?}",
                                    expression.name
                                ),
                            )
                            .with_sources(vec![DiagnosticSource {
                                owner: owner.clone(),
                                path: Some(format!("sem.roles.{}", expression.name)),
                                location: expression.source,
                            }]),
                        );
                    }
                }
            }
        }
        for role in role_declarations.values() {
            // `[*]` 不指名任何 trait,無存在性可驗。
            if let Some(category) = role.constraint.category() {
                if !registry.has(category) {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "ROLE_UNKNOWN_CONSTRAINT",
                            format!(
                                "{owner:?} role {:?} requires unknown trait {:?}{}",
                                role.name,
                                category,
                                registry.missing_name_hint(category)
                            ),
                        )
                        .with_sources(vec![DiagnosticSource {
                            owner: owner.clone(),
                            path: Some(format!("sem.roles.{}", role.name)),
                            location: role.source,
                        }]),
                    );
                }
            }
        }
        for binding in effective.items.iter().filter_map(|item| match item {
            SignItem::RoleBinding(binding) => Some(binding),
            _ => None,
        }) {
            if !role_declarations.contains_key(&binding.name) {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "ROLE_UNDECLARED",
                    format!("{owner:?} binds undeclared role {:?}", binding.name),
                ));
            }
            if !slots.iter().any(|slot| slot.name == binding.slot) {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "ROLE_UNKNOWN_SLOT",
                    format!(
                        "{owner:?} role {:?} binds unknown slot {:?}",
                        binding.name, binding.slot
                    ),
                ));
            }
        }

        // §10.3 義項網絡:義項名在一個 sign 內唯一;衍生邊兩端都必須是已宣告的義項
        // (不默默略過——否則 lexicalize_sense/derive_sense 會作用在幽靈節點上)。
        // 注意:`effective` 已依名字合併義項(本地覆寫繼承是**功能**),故重複宣告
        // 的偵測放在 `validate_defs_and_rules` 看**原始** items 之處。
        let sense_names: Vec<&str> = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::Sense(sense) => Some(sense.name.as_str()),
                _ => None,
            })
            .collect();
        for edge in effective.items.iter().filter_map(|item| match item {
            SignItem::SenseEdge(edge) => Some(edge),
            _ => None,
        }) {
            for (role, name) in [("to", &edge.to), ("from", &edge.from)] {
                if !sense_names.contains(&name.as_str()) {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "SENSE_EDGE_UNKNOWN",
                        format!("{owner:?} sense edge {role} refers to undeclared sense {name:?}"),
                    ));
                }
            }
            if edge.to == edge.from {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "SENSE_EDGE_SELF",
                    format!("{owner:?} sense edge derives {:?} from itself", edge.to),
                ));
            }
        }

        for realization in effective.items.iter().filter_map(|item| match item {
            SignItem::Realization(realization) => Some(realization),
            _ => None,
        }) {
            // `REALIZATION_EMPTY` 已刪:`Realization.expression` 不再是 `Option`,
            // 空 realization 由 parser 擋下(2026-08-18),型別上不可能到這裡。
            {
                let case = &realization.expression;
                if case.branches.is_empty() {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "CASE_EMPTY",
                        format!("{owner:?} has an empty phon case"),
                    ));
                }
                if let Some(scrutinee) = &case.scrutinee {
                    if let Err(error) = reference::parse(&reference::SCRUTINEE, scrutinee) {
                        report.push(Diagnostic::new(
                            Severity::Error,
                            "CASE_INVALID_SCRUTINEE",
                            format!("{owner:?} phon case scrutinee {scrutinee:?}: {error}"),
                        ));
                    } else if let Some(slot) = scrutinee_slot(scrutinee) {
                        if !slots.iter().any(|candidate| candidate.name == slot) {
                            report.push(Diagnostic::new(
                                Severity::Error,
                                "CASE_UNKNOWN_SLOT",
                                format!("{owner:?} phon case reads unknown slot {slot:?}"),
                            ));
                        }
                    }
                }
                for branch in &case.branches {
                    let mut pending = vec![&branch.result];
                    while let Some(result) = pending.pop() {
                        let template = match result {
                            Expression::PhonTemplate(template) => template,
                            Expression::PhonInterpolation(_) => continue,
                            Expression::Case(nested) => {
                                pending.extend(nested.branches.iter().map(|branch| &branch.result));
                                continue;
                            }
                            _ => {
                                report.push(Diagnostic::new(
                                    Severity::Error,
                                    "CASE_BRANCH_TYPE_MISMATCH",
                                    format!("{owner:?} phon case branch does not return Phon"),
                                ));
                                continue;
                            }
                        };
                        let inner = template
                            .strip_prefix('/')
                            .and_then(|value| value.strip_suffix('/'));
                        if inner.is_none() {
                            report.push(Diagnostic::new(
                                Severity::Error,
                                "REALIZATION_INVALID_TEMPLATE",
                                format!(
                                    "{owner:?} phon case must return a complete `/.../` template"
                                ),
                            ));
                            continue;
                        }
                        let Some(inner) = inner else {
                            continue;
                        };
                        match template_references(inner) {
                            Ok(references) => {
                                for reference in references {
                                    if !slots.iter().any(|slot| slot.name == reference) {
                                        report.push(Diagnostic::new(
                                            Severity::Error,
                                            "REALIZATION_UNKNOWN_SLOT",
                                            format!("{owner:?} realization refers to unknown slot {reference:?}"),
                                        ));
                                    }
                                }
                            }
                            Err(error) => report.push(Diagnostic::new(
                                Severity::Error,
                                "REALIZATION_INVALID_TEMPLATE",
                                format!("{owner:?} realization template: {error}"),
                            )),
                        }
                    }
                }
            }
        }
    }
}

fn validate_origin_graph(language: &Language, report: &mut ValidationReport) {
    let names: BTreeSet<_> = language
        .signs
        .iter()
        .map(|sign| sign.name.as_str())
        .collect();
    for sign in &language.signs {
        let Some(origin) = sign.origin() else {
            continue;
        };
        if !origin.0.contains("::") && !names.contains(origin.0.as_str()) {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "META_ORIGIN_UNKNOWN",
                    format!(
                        "sign {:?} has unknown local origin {:?}",
                        sign.name, origin.0
                    ),
                )
                .with_sources(vec![DiagnosticSource {
                    owner: sign.name.clone(),
                    path: Some("origin".to_owned()),
                    location: SourceLocation::unknown(),
                }]),
            );
        }
    }

    let mut reported = BTreeSet::new();
    for start in &language.signs {
        let mut path = Vec::<String>::new();
        let mut current = start;
        loop {
            if let Some(at) = path.iter().position(|name| name == &current.name) {
                let mut cycle = path[at..].to_vec();
                cycle.sort();
                if reported.insert(cycle.clone()) {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "META_ORIGIN_CYCLE",
                        format!("origin cycle among {}", cycle.join(", ")),
                    ));
                }
                break;
            }
            path.push(current.name.clone());
            let Some(origin) = current.origin() else {
                break;
            };
            if origin.0.contains("::") {
                break;
            }
            let Some(next) = language.sign_named(&origin.0) else {
                break;
            };
            current = next;
        }
    }
}

fn validate_duplicate_signs(language: &Language, report: &mut ValidationReport) {
    let mut seen = BTreeSet::new();
    for sign in &language.signs {
        if !seen.insert(&sign.name) {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "SIGN_DUPLICATE",
                    format!("duplicate sign {:?}", sign.name),
                )
                .with_sources(vec![DiagnosticSource {
                    owner: sign.name.clone(),
                    path: None,
                    location: SourceLocation::unknown(),
                }]),
            );
        }
    }
}

fn validate_fp_expressions(
    language: &Language,
    registry: &OntologyRegistry,
    report: &mut ValidationReport,
) {
    fn application_and_nested<'a>(
        application: &'a SignApplication,
        output: &mut Vec<&'a SignApplication>,
    ) {
        output.push(application);
        for argument in &application.arguments {
            if let SignArgumentValue::Application(nested) = &argument.value {
                application_and_nested(nested, output);
            }
        }
    }

    fn applications<'a>(expression: &'a Expression, output: &mut Vec<&'a SignApplication>) {
        match expression {
            Expression::SignApplication(application)
            | Expression::PhonInterpolation(application) => {
                application_and_nested(application, output);
            }
            Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
                for item in items {
                    match item {
                        SignItem::SignExpression(expression) => {
                            applications(&expression.expression, output)
                        }
                        SignItem::FeatureExpression(expression) => {
                            applications(&expression.expression, output)
                        }
                        SignItem::RoleExpression(expression) => {
                            applications(&expression.expression, output)
                        }
                        SignItem::Realization(realization) => {
                            for branch in &realization.expression.branches {
                                applications(&branch.result, output);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Expression::Projection { value, .. } => applications(value, output),
            // Nested cases are visited by the enclosing validation queue, so
            // only inspect direct branch expressions here.
            Expression::Case(_) => {}
            _ => {}
        }
    }

    fn expression_matches_type(expression: &Expression, expected: &crate::ExpressionType) -> bool {
        match (expression, expected) {
            (Expression::SignApplication(_), crate::ExpressionType::SignContext)
            | (Expression::SignFragment(_), crate::ExpressionType::SignContext)
            | (Expression::SelfSign, crate::ExpressionType::SignContext)
            | (Expression::PhonInterpolation(_), crate::ExpressionType::PhonContext)
            | (Expression::PhonTemplate(_), crate::ExpressionType::PhonContext)
            | (Expression::EnumValue(_), crate::ExpressionType::Feature { .. })
            | (Expression::Slot(_), crate::ExpressionType::Role { .. }) => true,
            (Expression::DimFragment { dim: Dim::Syn, .. }, crate::ExpressionType::SynContext)
            | (Expression::DimFragment { dim: Dim::Sem, .. }, crate::ExpressionType::SemContext)
            | (
                Expression::DimFragment { dim: Dim::Prag, .. },
                crate::ExpressionType::PragContext,
            ) => true,
            (
                Expression::Projection {
                    value,
                    dimension: crate::SignProjection::Phon,
                },
                crate::ExpressionType::PhonContext,
            ) => matches!(value.as_ref(), Expression::SignApplication(_)),
            (Expression::Case(nested), expected) => {
                &nested.expected == expected
                    && nested
                        .branches
                        .iter()
                        .all(|branch| expression_matches_type(&branch.result, expected))
            }
            _ => false,
        }
    }

    fn context_dim(expected: &crate::ExpressionType) -> Option<Dim> {
        match expected {
            crate::ExpressionType::SynContext => Some(Dim::Syn),
            crate::ExpressionType::SemContext => Some(Dim::Sem),
            crate::ExpressionType::PragContext => Some(Dim::Prag),
            _ => None,
        }
    }

    fn item_allowed_in_context(item: &SignItem, dim: Dim) -> bool {
        match item {
            // `pass` 是塊層級的標記,不屬於任何維度區塊
            SignItem::Pass => false,
            SignItem::FeatureDecl(value) => value.dim == dim,
            SignItem::FeatureValue(value) => value.dim == dim,
            SignItem::FeatureExpression(value) => value.dim == dim,
            SignItem::FeatureRule(rule) | SignItem::Rule(rule) => rule.dim == dim,
            // 義項與衍生邊只屬 sem 維(《修補05》§10.3)。
            SignItem::Sense(_) | SignItem::SenseEdge(_) => dim == Dim::Sem,
            SignItem::Def(definition) => definition
                .path
                .strip_prefix(dim.keyword())
                .is_some_and(|suffix| suffix.starts_with('.')),
            SignItem::SignExpression(expression) => match &expression.expression {
                Expression::Case(case) => context_dim(&case.expected) == Some(dim),
                _ => false,
            },
            SignItem::Slot(_)
            | SignItem::SlotMap(_)
            | SignItem::SlotFeatureBinding(_)
            | SignItem::Constraint(_) => dim == Dim::Syn,
            SignItem::RoleDecl(_) | SignItem::RoleBinding(_) | SignItem::RoleExpression(_) => {
                dim == Dim::Sem
            }
            SignItem::TraitMount { kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_), .. } | SignItem::TraitMount { name: _, kind: crate::TraitMountKind::Declaration, .. } | SignItem::Realization(_) => false,
        }
    }

    let mut calls = BTreeMap::<String, Vec<String>>::new();
    let filler_features = language_wide_features(language, registry);
    for local in &language.signs {
        let effective = registry.effective_sign(local);
        let local_parameters = construction::parameters_of(&effective);
        let local_slots = construction::slots_of(&effective);
        let local_features = visible_features(&effective);
        let mut cases = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::SignExpression(expression) => match &expression.expression {
                    Expression::Case(case) => Some((
                        case.as_ref(),
                        case.expected.clone(),
                        match &case.expected {
                            crate::ExpressionType::SignContext => "sign",
                            crate::ExpressionType::SynContext => "syn",
                            crate::ExpressionType::SemContext => "sem",
                            crate::ExpressionType::PragContext => "prag",
                            _ => "sign.expression",
                        },
                    )),
                    _ => None,
                },
                SignItem::Realization(realization) => Some((
                    &realization.expression,
                    crate::ExpressionType::PhonContext,
                    "phon.realization",
                )),
                SignItem::FeatureExpression(expression) => match &expression.expression {
                    Expression::Case(case) => Some((
                        case.as_ref(),
                        crate::ExpressionType::Feature {
                            dim: expression.dim,
                            name: expression.name.clone(),
                        },
                        "feature",
                    )),
                    _ => None,
                },
                SignItem::RoleExpression(expression) => match &expression.expression {
                    Expression::Case(case) => Some((
                        case.as_ref(),
                        crate::ExpressionType::Role {
                            name: expression.name.clone(),
                        },
                        "sem.roles",
                    )),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut case_index = 0;
        while case_index < cases.len() {
            let (case, site_type, site) = cases[case_index].clone();
            case_index += 1;
            if case.expected != site_type {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "CASE_CONTEXT_TYPE_MISMATCH",
                    format!(
                        "sign {:?} has {:?} case in {site} context expecting {:?}",
                        local.name, case.expected, site_type
                    ),
                ));
            }
            let mut saw_else = false;
            for branch in &case.branches {
                if matches!(branch.condition, CaseCondition::Else) {
                    if saw_else {
                        report.push(Diagnostic::new(
                            Severity::Error,
                            "CASE_MULTIPLE_ELSE",
                            format!("sign {:?} has more than one case else branch", local.name),
                        ));
                    }
                    saw_else = true;
                } else if saw_else {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "CASE_BRANCH_AFTER_ELSE",
                        format!("sign {:?} has a case branch after else", local.name),
                    ));
                }
                match &branch.condition {
                    CaseCondition::Guard(guard) => {
                        for conjunct in guard.split("&&").map(str::trim) {
                            if let Err(error) = synchronic::validate_realization_guard(
                                conjunct,
                                registry,
                                &local_slots,
                                &local_features,
                                &filler_features,
                            ) {
                                report.push(
                                    Diagnostic::new(
                                        Severity::Error,
                                        "CASE_INVALID_GUARD",
                                        format!("sign {:?}: {error}", local.name),
                                    )
                                    .with_sources(vec![
                                        DiagnosticSource {
                                            owner: local.name.clone(),
                                            path: Some(site.to_owned()),
                                            location: branch.source,
                                        },
                                    ]),
                                );
                            }
                        }
                    }
                    CaseCondition::Equals(_) | CaseCondition::Else => {}
                }
                if !matches!(case.expected, crate::ExpressionType::SignContext)
                    && !branch.belongs.is_empty()
                {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "CASE_BELONGS_TYPE_MISMATCH",
                        format!("sign {:?} uses belongs in a non-Sign case", local.name),
                    ));
                }
                if !expression_matches_type(&branch.result, &site_type) {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "CASE_BRANCH_TYPE_MISMATCH",
                            format!(
                                "sign {:?} case in {site} returns {:?}, expected {:?}",
                                local.name, branch.result, site_type
                            ),
                        )
                        .with_sources(vec![DiagnosticSource {
                            owner: local.name.clone(),
                            path: Some(site.to_owned()),
                            location: branch.source,
                        }]),
                    );
                }
                if case.selection == CaseSelection::Accumulate {
                    if !matches!(
                        case.expected,
                        crate::ExpressionType::SignContext
                            | crate::ExpressionType::SynContext
                            | crate::ExpressionType::SemContext
                            | crate::ExpressionType::PragContext
                    ) {
                        report.push(Diagnostic::new(
                            Severity::Error,
                            "WHEN_CONTEXT_TYPE_MISMATCH",
                            format!(
                                "sign {:?} uses `when` in a non-fragment {:?} context",
                                local.name, case.expected
                            ),
                        ));
                    }
                    if !matches!(
                        &branch.result,
                        Expression::SignFragment(_) | Expression::DimFragment { .. }
                    ) || !branch.belongs.is_empty()
                    {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "WHEN_NON_FRAGMENT_RESULT",
                                format!(
                                    "sign {:?} `when` branch must return an anonymous context fragment",
                                    local.name
                                ),
                            )
                            .with_sources(vec![DiagnosticSource {
                                owner: local.name.clone(),
                                path: Some(site.to_owned()),
                                location: branch.source,
                            }]),
                        );
                    }
                }
                for category in &branch.belongs {
                    if !registry.has(category) {
                        report.push(Diagnostic::new(
                            Severity::Error,
                            "CASE_UNKNOWN_MEMBERSHIP",
                            format!("sign {:?} case adds unknown trait {category:?}", local.name),
                        ));
                    }
                }
                if let Expression::SignFragment(items) | Expression::DimFragment { items, .. } =
                    &branch.result
                {
                    if let Some(dim) = context_dim(&case.expected) {
                        for item in items {
                            if !item_allowed_in_context(item, dim) {
                                report.push(
                                    Diagnostic::new(
                                        Severity::Error,
                                        "CASE_FRAGMENT_CONTEXT_VIOLATION",
                                        format!(
                                            "sign {:?} places {item:?} outside <{}Context>",
                                            local.name,
                                            dim.keyword()
                                        ),
                                    )
                                    .with_sources(vec![
                                        DiagnosticSource {
                                            owner: local.name.clone(),
                                            path: Some(format!("{}.fragment", dim.keyword())),
                                            location: branch.source,
                                        },
                                    ]),
                                );
                            }
                        }
                    }
                    for item in items {
                        match item {
                            SignItem::TraitMount { name: category, kind: crate::TraitMountKind::Declaration, .. }
                            | SignItem::TraitMount { name: category, .. }
                                if !registry.has(category) =>
                            {
                                report.push(
                                    Diagnostic::new(
                                        Severity::Error,
                                        "CASE_FRAGMENT_UNKNOWN_TRAIT",
                                        format!(
                                            "sign {:?} SignContext fragment references unknown trait {category:?}",
                                            local.name
                                        ),
                                    )
                                    .with_sources(vec![DiagnosticSource {
                                        owner: local.name.clone(),
                                        path: Some("sign.fragment".to_owned()),
                                        location: branch.source,
                                    }]),
                                );
                            }
                            SignItem::SignExpression(expression) => {
                                if let Expression::Case(nested) = &expression.expression {
                                    cases.push((
                                        nested.as_ref(),
                                        nested.expected.clone(),
                                        "context.fragment",
                                    ));
                                }
                            }
                            SignItem::FeatureExpression(expression) => {
                                if let Expression::Case(nested) = &expression.expression {
                                    cases.push((
                                        nested.as_ref(),
                                        crate::ExpressionType::Feature {
                                            dim: expression.dim,
                                            name: expression.name.clone(),
                                        },
                                        "sign.fragment.feature",
                                    ));
                                }
                            }
                            SignItem::RoleExpression(expression) => {
                                if let Expression::Case(nested) = &expression.expression {
                                    cases.push((
                                        nested.as_ref(),
                                        crate::ExpressionType::Role {
                                            name: expression.name.clone(),
                                        },
                                        "sign.fragment.roles",
                                    ));
                                }
                            }
                            SignItem::Realization(realization) => {
                                cases.push((
                                    &realization.expression,
                                    crate::ExpressionType::PhonContext,
                                    "sign.fragment.phon.realization",
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                let mut branch_calls = Vec::new();
                applications(&branch.result, &mut branch_calls);
                for call in branch_calls {
                    calls
                        .entry(local.name.clone())
                        .or_default()
                        .push(call.callee.clone());
                    let Some(callee) = language.sign_named(&call.callee) else {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "APPLICATION_UNKNOWN_SIGN",
                                format!(
                                    "sign {:?} calls unknown Sign function {:?}",
                                    local.name, call.callee
                                ),
                            )
                            .with_sources(vec![DiagnosticSource {
                                owner: local.name.clone(),
                                path: Some("application".to_owned()),
                                location: call.source,
                            }]),
                        );
                        continue;
                    };
                    let parameters = construction::parameters_of(&registry.effective_sign(callee));
                    let mut supplied = BTreeSet::new();
                    for argument in &call.arguments {
                        if let SignArgumentValue::Slot(slot) = &argument.value {
                            if !local_parameters
                                .iter()
                                .any(|parameter| &parameter.name == slot)
                            {
                                report.push(Diagnostic::new(
                                    Severity::Error,
                                    "APPLICATION_UNKNOWN_SLOT_VARIABLE",
                                    format!("sign {:?} has no slot variable {slot:?}", local.name),
                                ));
                            }
                        }
                        let name = match &argument.name {
                            Some(name) => Some(name.as_str()),
                            None if parameters.len() == 1 => Some(parameters[0].name.as_str()),
                            None => {
                                report.push(Diagnostic::new(
                                    Severity::Error,
                                    "APPLICATION_POSITIONAL_ARITY",
                                    format!(
                                        "Sign function {:?} has {} parameters; positional shorthand is invalid",
                                        call.callee,
                                        parameters.len()
                                    ),
                                ));
                                None
                            }
                        };
                        if let Some(name) = name {
                            if !parameters.iter().any(|parameter| parameter.name == name) {
                                report.push(Diagnostic::new(
                                    Severity::Error,
                                    "APPLICATION_UNKNOWN_PARAMETER",
                                    format!(
                                        "Sign function {:?} has no parameter {name:?}",
                                        call.callee
                                    ),
                                ));
                            } else if !supplied.insert(name.to_owned()) {
                                report.push(Diagnostic::new(
                                    Severity::Error,
                                    "APPLICATION_DUPLICATE_PARAMETER",
                                    format!(
                                        "Sign function {:?} receives {name:?} twice",
                                        call.callee
                                    ),
                                ));
                            }
                        }
                    }
                }
                if let Expression::Case(nested) = &branch.result {
                    cases.push((nested.as_ref(), site_type.clone(), site));
                }
            }
        }
    }

    fn visit(
        node: &str,
        calls: &BTreeMap<String, Vec<String>>,
        active: &mut Vec<String>,
        done: &mut BTreeSet<String>,
    ) -> Option<Vec<String>> {
        if let Some(index) = active.iter().position(|candidate| candidate == node) {
            let mut cycle = active[index..].to_vec();
            cycle.push(node.to_owned());
            return Some(cycle);
        }
        if done.contains(node) {
            return None;
        }
        active.push(node.to_owned());
        for callee in calls.get(node).into_iter().flatten() {
            if let Some(cycle) = visit(callee, calls, active, done) {
                return Some(cycle);
            }
        }
        active.pop();
        done.insert(node.to_owned());
        None
    }
    let mut done = BTreeSet::new();
    for node in calls.keys() {
        if let Some(cycle) = visit(node, &calls, &mut Vec::new(), &mut done) {
            report.push(Diagnostic::new(
                Severity::Error,
                "APPLICATION_CYCLE",
                format!("Sign application cycle: {}", cycle.join(" -> ")),
            ));
            break;
        }
    }
}

fn template_references(template: &str) -> Result<Vec<String>, String> {
    let mut references = Vec::new();
    let mut chars = template.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if ch == '}' {
            return Err(format!("unmatched `}}` at byte {at}"));
        }
        if ch != '{' {
            continue;
        }
        let mut name = String::new();
        let mut closed = false;
        for (_, inner) in chars.by_ref() {
            if inner == '}' {
                closed = true;
                break;
            }
            if inner == '{' {
                return Err(format!("nested `{{` at byte {at}"));
            }
            name.push(inner);
        }
        if !closed {
            return Err(format!("unclosed `{{` at byte {at}"));
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("empty slot reference at byte {at}"));
        }
        // 括號的良構性(未閉合/巢狀)與括號**內容**的良構性是兩件事;
        // 前者在上面,後者交給 reference。
        let read = reference::parse(&reference::SUBJECT_ONLY, name)
            .map_err(|error| format!("`{{{name}}}` at byte {at}: {error}"))?;
        // P75 增修 A:構式內部不回指構式本身。phon 模板**就是**這個 sign 的
        // 形式,把 `$self` 求值後嵌進去等於把自己的 surface 嵌進自己的 surface
        // ——無條件遞迴,與 slot 是否存在無關,故不能落到 unknown-slot。
        let Some(slot) = read.slot() else {
            return Err(format!(
                "`{{$self}}` at byte {at} embeds this sign's own surface into itself; \
                 a phon template may only interpolate slots (`{{$slot.NAME}}`)"
            ));
        };
        references.push(slot.to_owned());
    }
    Ok(references)
}

fn validate_constructions_and_local_phon(
    language: &Language,
    registry: &OntologyRegistry,
    global_phon_source: Option<&str>,
    report: &mut ValidationReport,
) {
    for local in &language.signs {
        let effective = registry.effective_sign(local);
        let slots = construction::slots_of(&effective);
        let declared_mapping = construction::slot_map_of(&effective);
        if slots.is_empty() && !declared_mapping.ops().is_empty() {
            report.push(Diagnostic::new(
                Severity::Error,
                "SLOT_MAP_WITHOUT_SLOTS",
                format!(
                    "sign {:?} declares slot mapping but is not a construction",
                    effective.name
                ),
            ));
        }
        if !slots.is_empty() {
            let mut used_slots = BTreeSet::new();
            for rule in effective.items.iter().filter_map(|item| match item {
                SignItem::Rule(rule) | SignItem::FeatureRule(rule) if rule.dim != Dim::Phon => {
                    Some(rule)
                }
                _ => None,
            }) {
                used_slots.extend(synchronic::rule_slot_references(rule));
                for error in synchronic::validate_rule(rule, registry, &slots) {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "RULE_INVALID",
                            format!("{:?}: {error}", effective.name),
                        )
                        .with_sources(vec![DiagnosticSource {
                            owner: effective.name.clone(),
                            path: Some(format!("rule {}", rule.id)),
                            location: rule.source,
                        }]),
                    );
                }
            }
            if let Err(error) =
                construction::validate_slot_mapping(&effective, &SlotMap::identity())
            {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "SLOT_MAP_INVALID",
                    format!("construction {:?}: {error}", effective.name),
                ));
            }
            for operation in declared_mapping.ops() {
                let SlotMapOp::AutoFill { slot, filler } = operation else {
                    continue;
                };
                let Some(filler_sign) = language.sign_named(filler) else {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "SLOT_MAP_UNKNOWN_FILLER",
                        format!(
                            "construction {:?} auto-fills {slot:?} with unknown sign {filler:?}",
                            effective.name
                        ),
                    ));
                    continue;
                };
                let Some(constraint) = slots
                    .iter()
                    .find(|candidate| candidate.name == *slot)
                    .map(|candidate| &candidate.constraint)
                else {
                    continue;
                };
                let effective_filler = registry.effective_sign(filler_sign);
                let categories = registry.sign_categories(&effective_filler);
                if !constraint.is_satisfied_by(&categories, registry) {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "SLOT_MAP_FILLER_CATEGORY",
                        format!(
                            "construction {:?} slot {slot:?} requires [{}], but auto-fill {filler:?} has {categories:?}",
                            effective.name,
                            constraint.display_name()
                        ),
                    ));
                }
            }
            let slot_names: Vec<_> = slots.iter().map(|slot| slot.name.as_str()).collect();
            let phon = effective
                .project(Dim::Phon, registry)
                .get("phon")
                .map(str::to_owned);
            match phon {
                None => report.push(Diagnostic::new(
                    Severity::Error,
                    "CONSTRUCTION_PHON_MISSING",
                    format!("construction {:?} has no phon template", effective.name),
                )),
                Some(value) => match template_references(
                    value
                        .strip_prefix('/')
                        .and_then(|inner| inner.strip_suffix('/'))
                        .unwrap_or(&value),
                ) {
                    Err(error) => report.push(Diagnostic::new(
                        Severity::Error,
                        "CONSTRUCTION_TEMPLATE_INVALID",
                        format!("construction {:?}: {error}", effective.name),
                    )),
                    Ok(references) => {
                        for reference in references {
                            used_slots.insert(reference.clone());
                            if !slot_names.contains(&reference.as_str()) {
                                report.push(Diagnostic::new(
                                    Severity::Error,
                                    "CONSTRUCTION_TEMPLATE_UNKNOWN_SLOT",
                                    format!(
                                        "construction {:?} template refers to unknown slot {reference:?}",
                                        effective.name
                                    ),
                                ));
                            }
                        }
                    }
                },
            }
            for realization in effective.items.iter().filter_map(|item| match item {
                SignItem::Realization(realization) => Some(realization),
                _ => None,
            }) {
                {
                    let case = &realization.expression;
                    if let Some(slot) = case.scrutinee.as_deref().and_then(scrutinee_slot) {
                        used_slots.insert(slot);
                    }
                    for branch in &case.branches {
                        if let crate::CaseCondition::Guard(guard) = &branch.condition {
                            used_slots.extend(synchronic::realization_guard_slot_references(guard));
                        }
                        if let Expression::PhonTemplate(template) = &branch.result {
                            if let Some(inner) = template
                                .strip_prefix('/')
                                .and_then(|value| value.strip_suffix('/'))
                            {
                                if let Ok(references) = template_references(inner) {
                                    used_slots.extend(references);
                                }
                            }
                        }
                    }
                }
            }
            for constraint in effective.items.iter().filter_map(|item| match item {
                SignItem::Constraint(constraint) => Some(constraint),
                _ => None,
            }) {
                for operand in [&constraint.left, &constraint.right] {
                    // 解析不出主體就是解析不出——不再退回把整串當 slot 名,
                    // 否則裸寫法會靜默地繼續通過。
                    let slot = reference::parse(&reference::CONSTRAINT_OPERAND, operand)
                        .ok()
                        .and_then(|reference| reference.slot().map(str::to_owned));
                    if slot
                        .as_deref()
                        .is_some_and(|slot| slot_names.contains(&slot))
                    {
                        used_slots.insert(slot.expect("checked above"));
                    } else {
                        report.push(Diagnostic::new(
                            Severity::Error,
                            "CONSTRAINT_UNKNOWN_SLOT",
                            format!(
                                "construction {:?} constraint operand {operand:?} does not name a known slot",
                                effective.name
                            ),
                        ));
                    }
                }
            }
            for (path, value) in effective.project(Dim::Sem, registry).defs {
                let Some(reference) = value
                    .strip_prefix('{')
                    .and_then(|inner| inner.strip_suffix('}'))
                    .and_then(|inner| reference::parse(&reference::SUBJECT_ONLY, inner).ok())
                    .and_then(|read| read.slot().map(str::to_owned))
                else {
                    continue;
                };
                let reference = reference.as_str();
                if reference.is_empty() || !slot_names.contains(&reference) {
                    report.push(
                        Diagnostic::new(
                            Severity::Error,
                            "CONSTRUCTION_SEM_UNKNOWN_SLOT",
                            format!(
                                "construction {:?} semantic field {path:?} refers to unknown slot {reference:?}",
                                effective.name
                            ),
                        )
                        .with_sources(vec![DiagnosticSource {
                            owner: effective.name.clone(),
                            path: Some(path),
                            location: SourceLocation::unknown(),
                        }]),
                    );
                } else {
                    used_slots.insert(reference.to_owned());
                }
            }
            for binding in effective.items.iter().filter_map(|item| match item {
                SignItem::RoleBinding(binding) => Some(binding),
                _ => None,
            }) {
                used_slots.insert(binding.slot.clone());
            }
            for binding in effective.items.iter().filter_map(|item| match item {
                SignItem::SlotFeatureBinding(binding) => Some(binding),
                _ => None,
            }) {
                used_slots.insert(binding.slot.clone());
                if let Ok(read) = reference::parse(&reference::SLOT_SYN_FEATURE, &binding.value) {
                    if let Some(slot) = read.slot() {
                        used_slots.insert(slot.to_owned());
                    }
                }
            }
            let has_meaning = !effective.project(Dim::Sem, registry).defs.is_empty()
                || effective
                    .items
                    .iter()
                    .any(|item| matches!(item, SignItem::RoleDecl(_) | SignItem::RoleBinding(_)))
                || !effective.project(Dim::Prag, registry).defs.is_empty();
            if !has_meaning {
                report.push(Diagnostic::new(
                    Severity::Warning,
                    "CONSTRUCTION_MEANING_MISSING",
                    format!(
                        "construction {:?} has no sem/prag meaning or function pole",
                        effective.name
                    ),
                ));
            }
            for slot in &slots {
                if !used_slots.contains(&slot.name) {
                    report.push(Diagnostic::new(
                        Severity::Warning,
                        "CONSTRUCTION_SLOT_UNUSED",
                        format!(
                            "construction {:?} declares unused slot {:?}",
                            effective.name, slot.name
                        ),
                    ));
                }
            }
        }

        let Some(global_phon_source) = global_phon_source else {
            continue;
        };
        let phon_rules: Vec<_> = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::Rule(rule) if rule.dim == Dim::Phon => Some(rule),
                _ => None,
            })
            .collect();
        if phon_rules.is_empty() {
            continue;
        }
        let mut source = global_phon_source.to_owned();
        let mut number = 1_000_000u32;
        let mut emit_error = None;
        for rule in &phon_rules {
            if let Err(error) = codegen::emit_rule(&mut source, &mut number, rule) {
                emit_error = Some((rule, error.to_string()));
                break;
            }
        }
        let compile_error = emit_error
            .map(|(rule, message)| (rule.source, message))
            .or_else(|| {
                tshiatun_dsl::compile(&source)
                    .err()
                    .map(|error| (phon_rules[0].source, error.to_string()))
            });
        if let Some((location, message)) = compile_error {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "PHON_LOCAL_RULE_INVALID",
                    format!("sign {:?}: {message}", effective.name),
                )
                .with_sources(vec![DiagnosticSource {
                    owner: effective.name.clone(),
                    path: Some("phon rule".to_owned()),
                    location,
                }]),
            );
        }
    }
}

fn validate_source_language(
    std: &Language,
    effective_source: &Language,
    available_exports: &BTreeMap<String, LibraryId>,
) -> ValidationReport {
    let (registry, ontology_diags) = OntologyRegistry::build(&[std, effective_source]);
    let registry = registry.with_available(available_exports.clone());
    let mut report = registry.validation_report(&[std, effective_source], &ontology_diags);
    // [A] 第 1 步:只吃 ① Source——展開會把 `TraitUse` 消去
    report.extend(crate::ontology::belongs_reference_diagnostics(&[
        std,
        effective_source,
    ]));
    validate_duplicate_signs(effective_source, &mut report);

    // [A] 3-3:**內容級的驗證看展開後的形態。**
    //
    // 兩階段之後 trait 的內容從展開來,而這條路是 `.chg` 每一次原語編輯都會走的
    // (`apply_edit` → `check_document`)。餵 ① Source 的話,`X[n]` 帶進來的內容
    // 在驗證眼裡還不存在——規則引用的 slot、schema 的 feature 全都會誤報找不到。
    //
    // 展開失敗時退回 ① Source:那類錯誤(環、未知 trait)由 registry 與編譯路徑
    // 各自報,這裡不重複;而退回讓其餘檢查仍然跑得完,不會因為一個展開錯誤就
    // 讓整份報告變空。
    //
    // 成本實測:展開佔 `check_document` 的 **1–4%**(20/100/400 signs,release)
    // ——`check_document` 本身有約 3.5 ms 的固定開銷(每次重建 std registry),
    // 展開相對它是雜訊。
    let expanded = crate::compile::expand_traits_with(effective_source, &[std])
        .unwrap_or_else(|_| effective_source.clone());

    validate_defs_and_rules(std, &[], &registry, &mut report);
    validate_defs_and_rules(&expanded, &[std], &registry, &mut report);
    validate_typed_schemas(std, &[], &registry, &mut report);
    validate_typed_schemas(&expanded, &[std], &registry, &mut report);
    validate_fp_expressions(&expanded, &registry, &mut report);
    report.extend(crate::ontology::type_param_bound_diagnostics(
        &[std, &expanded],
        &registry,
    ));
    validate_origin_graph(effective_source, &mut report);
    validate_constructions_and_local_phon(&expanded, &registry, None, &mut report);
    report
}

/// Validate caller source with the default enabled standard libraries without
/// lowering or phonological code generation.
pub fn check_language(language: &Language) -> ValidationReport {
    check_language_with_libraries(language, &LibrarySpec::default())
}

/// Validate caller source with an explicit embedded-library selection.  A
/// catalog/configuration failure is represented as a normal diagnostic so
/// edit callers never need to panic or attempt code generation for checking.
pub fn check_language_with_libraries(language: &Language, spec: &LibrarySpec) -> ValidationReport {
    let packages =
        match library::embedded_catalog().and_then(|catalog| catalog.resolve_legacy(spec)) {
            Ok(packages) => packages,
            Err(error) => {
                let mut report = ValidationReport::new();
                report.push(Diagnostic::new(
                    Severity::Error,
                    "LIBRARY_SELECTION_INVALID",
                    error.to_string(),
                ));
                return report;
            }
        };
    check_language_with_packages(language, &packages)
}

/// Validate with one already-resolved package snapshot.  This is the v2
/// canonical entry point; no catalog or filesystem lookup occurs here.
pub fn check_language_with_packages(
    language: &Language,
    packages: &ResolvedPackages,
) -> ValidationReport {
    let std = packages.selection.standard.clone();
    let mut effective_source = packages.selection.overlay.clone();
    effective_source.append_library(language.clone());
    validate_source_language(&std, &effective_source, packages.available_exports())
}

/// Validate both sidecar/source identity invariants and synchronic language
/// invariants.  The caller document remains immutable.
pub fn check_document(document: &crate::LanguageDocument, spec: &LibrarySpec) -> ValidationReport {
    let packages =
        match library::embedded_catalog().and_then(|catalog| catalog.resolve_legacy(spec)) {
            Ok(packages) => packages,
            Err(error) => {
                let mut report = ValidationReport::new();
                report.push(Diagnostic::new(
                    Severity::Error,
                    "LIBRARY_SELECTION_INVALID",
                    error.to_string(),
                ));
                return report;
            }
        };
    check_document_with_packages(document, &packages)
}

pub fn check_document_with_packages(
    document: &crate::LanguageDocument,
    packages: &ResolvedPackages,
) -> ValidationReport {
    let mut report = ValidationReport::new();
    let ids: std::collections::BTreeMap<_, _> = document
        .identities()
        .nodes
        .iter()
        .map(|entry| (entry.id.clone(), entry.kind))
        .collect();
    for binding in &document.identities().refs {
        if !ids.contains_key(&binding.owner) {
            report.push(Diagnostic::new(
                Severity::Error,
                "IDENTITY_REF_OWNER_MISSING",
                format!("reference owner {} is absent", binding.owner),
            ));
        }
        if let crate::RefTargetV1::Local { target } = &binding.target {
            match ids.get(&target.id) {
                None => report.push(Diagnostic::new(
                    Severity::Error,
                    "IDENTITY_REF_DANGLING",
                    format!("reference {} points to absent {}", binding.field, target.id),
                )),
                Some(kind) if *kind != target.expected => report.push(Diagnostic::new(
                    Severity::Error,
                    "IDENTITY_REF_KIND_MISMATCH",
                    format!(
                        "reference {} expects {:?}, found {:?}",
                        binding.field, target.expected, kind
                    ),
                )),
                Some(_) => {}
            }
        }
    }
    report.extend(
        check_language_with_packages(document.language(), packages)
            .diagnostics()
            .iter()
            .cloned(),
    );
    report
}

/// **編譯語意版本**——同一份輸入在不同版本下可能編出不同的 `CompiledSystem`。
///
/// 用途:呼叫端的記憶體快取必須把它納入鍵,否則引擎升級後會沿用舊的編譯結果
/// (見 `conlang-app::compile::CompileKey`)。
///
/// # 什麼時候要 bump
///
/// **compile 的任何可觀測輸出改變時**——五個 pass 的產物、ontology 閉包語意、
/// projection 內容、診斷碼與嚴重度。純效能改動與註解不算。
///
/// # 誠實說明:沒有東西強制你 bump
///
/// 這是個約定,不是機制。唯一的旁證是 compile 的 **dump golden**(每 pass 一份)
/// ——編譯語意一變它們就會 churn,審查時看得到。若某次改動讓那些 golden 動了
/// 而此處沒動,那就是漏了。
pub const COMPILER_SEMANTICS_VERSION: &str = "conlang-compile/1";

/// Exact owned entry point requested by P38–P44. Use `compile_system_ref` when
/// the caller wants to retain its original `Language` without cloning first.
/// scrutinee 引用的 slot(不論之後讀範疇還是讀欄位)。slot 存在性驗證與
/// used-slot 統計用這個。
fn scrutinee_slot(scrutinee: &str) -> Option<String> {
    let read = reference::parse(&reference::SCRUTINEE, scrutinee).ok()?;
    read.slot().map(str::to_owned)
}

pub fn compile_system(language: Language) -> Result<CompiledSystem, CompileSystemError> {
    compile_with_libraries(language, LibrarySpec::default())
}

pub fn compile_system_ref(language: &Language) -> Result<CompiledSystem, CompileSystemError> {
    compile_with_libraries_ref(language, &LibrarySpec::default())
}

pub fn compile_with_libraries(
    language: Language,
    spec: LibrarySpec,
) -> Result<CompiledSystem, CompileSystemError> {
    compile_with_libraries_ref(&language, &spec)
}

pub fn compile_with_libraries_ref(
    language: &Language,
    spec: &LibrarySpec,
) -> Result<CompiledSystem, CompileSystemError> {
    let catalog = library::embedded_catalog()?;
    let packages = catalog.resolve_legacy(spec)?;
    compile_with_packages_ref(language, &packages)
}

/// Compile against an immutable resolver result.  Every downstream lookup,
/// diagnostic hint, and reported package ID comes from this same snapshot.
pub fn compile_with_packages_ref(
    language: &Language,
    packages: &ResolvedPackages,
) -> Result<CompiledSystem, CompileSystemError> {
    let std = packages.selection.standard.clone();
    let mut effective_source = packages.selection.overlay.clone();
    effective_source.append_library(language.clone());
    // Validate source-level ontology/rules before the legacy compile pipeline
    // can collapse duplicate names into an unstructured CompileError.
    let pre_validation =
        validate_source_language(&std, &effective_source, packages.available_exports());
    if pre_validation.has_errors() {
        return Err(CompileSystemError::Validation(pre_validation));
    }

    // 展開要看得到 std 的 trait,否則顯式 `X[n]` 引用不到套件的東西
    let artifacts = codegen::compile_full_with(&effective_source, &[&std])?;
    let ordered = artifacts.pipeline.ordered.clone();
    let (registry, ontology_diags) = OntologyRegistry::build(&[&std, &ordered]);
    let registry = registry.with_available(packages.available_exports().clone());
    let mut validation = registry.validation_report(&[&std, &ordered], &ontology_diags);
    validation.extend(crate::ontology::belongs_reference_diagnostics(&[
        &std,
        &effective_source,
    ]));
    validate_duplicate_signs(&ordered, &mut validation);
    validate_defs_and_rules(&ordered, &[&std], &registry, &mut validation);
    validate_typed_schemas(&ordered, &[&std], &registry, &mut validation);
    validate_fp_expressions(&ordered, &registry, &mut validation);
    validate_origin_graph(&ordered, &mut validation);
    validate_constructions_and_local_phon(
        &ordered,
        &registry,
        Some(&artifacts.grammar.phon_source),
        &mut validation,
    );
    // Source-level validation can observe conflicts that the compile pipeline
    // legitimately resolves away.  Preserve those warnings in the public
    // report instead of returning a falsely empty post-resolution report.
    for diagnostic in pre_validation.diagnostics().iter().cloned() {
        if !validation.diagnostics().contains(&diagnostic) {
            validation.push(diagnostic);
        }
    }
    if validation.has_errors() {
        return Err(CompileSystemError::Validation(validation));
    }
    Ok(CompiledSystem {
        language: language.clone(),
        effective_language: ordered,
        libraries: packages.selection.packages.clone(),
        artifacts,
        ontology: registry,
        validation,
    })
}

/// Compile a stable caller document after running the same identity-aware
/// validation used by Primitive Edit and ChangeSet replay.
pub fn compile_document(
    document: &LanguageDocument,
    spec: &LibrarySpec,
) -> Result<CompiledSystem, CompileSystemError> {
    let catalog = library::embedded_catalog()?;
    let packages = catalog.resolve_legacy(spec)?;
    compile_document_with_packages(document, &packages)
}

pub fn compile_document_with_packages(
    document: &LanguageDocument,
    packages: &ResolvedPackages,
) -> Result<CompiledSystem, CompileSystemError> {
    let report = check_document_with_packages(document, packages);
    if report.has_errors() {
        return Err(CompileSystemError::Validation(report));
    }
    compile_with_packages_ref(document.language(), packages)
}

impl CompiledSystem {
    pub fn language(&self) -> &Language {
        &self.language
    }

    pub fn effective_language(&self) -> &Language {
        &self.effective_language
    }

    pub fn libraries(&self) -> &[LibraryId] {
        &self.libraries
    }

    pub fn validate_semantic_document(
        &self,
        document: &SemanticDocumentV1,
    ) -> Result<crate::sem::SemNode, SemanticDocumentError> {
        if document.schema != crate::SEMANTIC_SCHEMA_V1 {
            return Err(SemanticDocumentError::UnknownSchema(
                document.schema.clone(),
            ));
        }
        fn validate_node(
            system: &CompiledSystem,
            node: &SemanticNodeV1,
        ) -> Result<(), SemanticDocumentError> {
            if node.source.sign.trim().is_empty() {
                return Err(SemanticDocumentError::Invalid(
                    "semantic source sign cannot be empty".to_owned(),
                ));
            }
            if let Some(package) = &node.source.package {
                let id = LibraryId::from_str(package).map_err(|_| {
                    SemanticDocumentError::Invalid(format!(
                        "invalid semantic source package {package:?}"
                    ))
                })?;
                if !system.libraries.iter().any(|loaded| loaded == &id) {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "semantic source package {package:?} is not loaded"
                    )));
                }
            }
            for category in &node.types {
                if !system.ontology.has(category) {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "unknown semantic trait {category:?}"
                    )));
                }
                // P71-S:此處曾要求每個型別都在 `Semantic` 之下。`Semantic` 只是
                // `std:core` 的一個 trait,引擎不得據以設限;且 R14 之後 `types`
                // 是**完整閉包**,合法 sign 的閉包本就含非 Semantic 範疇
                // (如 `AgreementBearer`),該檢查會讓正常的 round-trip 失敗。
                // 上面的 `has(category)` 已足以擋掉不存在的範疇。
            }
            let schema_sign = SignDef {
                id: crate::SignId::synthetic(),
                name: "#semantic-document-schema".to_owned(),
                items: node
                    .types
                    .iter()
                    .cloned()
                    .map(|name| SignItem::TraitMount {
                        name,
                        kind: crate::TraitMountKind::Declaration,
                        args: vec![],
                    })
                    .collect(),
            };
            let effective = system.ontology.effective_sign(&schema_sign);
            let features = effective
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::FeatureDecl(feature) if feature.dim == Dim::Sem => {
                        Some((feature.name.as_str(), feature))
                    }
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();
            for (name, value) in &node.features {
                let Some(declaration) = features.get(name.as_str()) else {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "undeclared semantic feature {name:?}"
                    )));
                };
                if !declaration.values.contains(value) {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "semantic feature {name:?} value {value:?} is outside enum({})",
                        declaration.values.join(", ")
                    )));
                }
            }
            let roles = effective
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::RoleDecl(role) => Some((role.name.as_str(), role)),
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();
            for name in node.roles.keys() {
                if !roles.contains_key(name.as_str()) {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "unknown semantic role {name:?}"
                    )));
                }
            }
            for role in roles.values().filter(|role| !role.optional) {
                if !node.roles.contains_key(&role.name) {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "missing required semantic role {:?}",
                        role.name
                    )));
                }
            }
            for (name, child) in &node.roles {
                let declaration = roles[name.as_str()];
                if !declaration
                    .constraint
                    .is_satisfied_by(&child.types, &system.ontology)
                {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "role {name:?} requires [{}]",
                        declaration.constraint.display_name()
                    )));
                }
                validate_node(system, child)?;
            }
            Ok(())
        }
        validate_node(self, &document.root)?;
        Ok(document.root.clone().into_sem_node())
    }

    pub fn evaluate_sign(&self, name: &str) -> Result<SignEvaluation, SystemError> {
        self.evaluate_sign_with_context_internal(name, &DerivationContext::new())
            .map(|(evaluation, _)| evaluation)
    }

    fn evaluate_sign_with_context_internal(
        &self,
        name: &str,
        context: &DerivationContext,
    ) -> Result<(SignEvaluation, Vec<CaseRecord>), SystemError> {
        let local = self
            .effective_language
            .sign_named(name)
            .ok_or_else(|| SystemError::UnknownSign(name.to_owned()))?;
        self.evaluate_source_sign_with_context(local, context)
    }

    fn evaluate_source_sign_with_context(
        &self,
        local: &SignDef,
        context: &DerivationContext,
    ) -> Result<(SignEvaluation, Vec<CaseRecord>), SystemError> {
        let mut sign = self.ontology.effective_sign(local);
        for ((dim, feature), value) in context.features() {
            if !matches!(dim, Dim::Syn | Dim::Sem) {
                return Err(SystemError::UnsupportedFeatureDimension { dim: *dim });
            }
            let declaration = sign.items.iter().rev().find_map(|item| match item {
                SignItem::FeatureDecl(declaration)
                    if declaration.dim == *dim && declaration.name == *feature =>
                {
                    Some(declaration)
                }
                _ => None,
            });
            let Some(declaration) = declaration else {
                return Err(SystemError::UndeclaredDerivationFeature {
                    dim: *dim,
                    name: feature.clone(),
                });
            };
            if !declaration.values.contains(value) {
                return Err(SystemError::DerivationFeatureOutOfDomain {
                    dim: *dim,
                    name: feature.clone(),
                    value: value.clone(),
                    domain: declaration.values.join(", "),
                });
            }
            let path = format!("{}.{}", dim.keyword(), feature);
            if let Some(actual) = sign
                .project(*dim, &self.ontology)
                .defs
                .iter()
                .find(|(candidate, _)| candidate == &path)
                .map(|(_, actual)| actual)
            {
                if actual != value {
                    return Err(SystemError::DerivationFeatureConflict {
                        dim: *dim,
                        name: feature.clone(),
                        expected: value.clone(),
                        actual: actual.clone(),
                    });
                }
            } else {
                sign = synchronic::Patch::for_dim(*dim)
                    .set(feature, value)
                    .apply(&sign);
            }
        }
        let mut records = Vec::new();
        let mut cases = Vec::new();
        for dim in [Dim::Syn, Dim::Sem, Dim::Prag] {
            let (next, pass) = synchronic::run_sign_dim_rules(&sign, dim, &self.ontology);
            sign = next;
            records.extend(pass);
            sign = self.apply_sign_feature_expressions(sign, dim, &mut cases)?;
        }
        Self::check_sign_context(&sign, context, &self.ontology)?;
        Ok((
            SignEvaluation {
                sign,
                records,
                source_sign: local.clone(),
                context: context.clone(),
            },
            cases,
        ))
    }

    pub fn evaluate_sign_with_context(
        &self,
        name: &str,
        context: &DerivationContext,
    ) -> Result<SignEvaluation, SystemError> {
        self.evaluate_sign_with_context_internal(name, context)
            .map(|(evaluation, _)| evaluation)
    }

    fn case_condition_matches(
        &self,
        value: &SignValue,
        case: &TypedCase,
        condition: &CaseCondition,
    ) -> Result<bool, SystemError> {
        match condition {
            CaseCondition::Else => Ok(true),
            CaseCondition::Guard(guard) => self.case_guard_matches(value, guard),
            CaseCondition::Equals(expected) => self.case_equals_matches(
                value,
                case.scrutinee.as_deref().ok_or_else(|| {
                    SystemError::InvalidSignExpression(
                        "equality case is missing its scrutinee".to_owned(),
                    )
                })?,
                expected,
            ),
        }
    }

    fn apply_sign_feature_expressions(
        &self,
        mut sign: SignDef,
        dim: Dim,
        records: &mut Vec<CaseRecord>,
    ) -> Result<SignDef, SystemError> {
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
        for expression in expressions {
            let Expression::Case(case) = &expression.expression else {
                return Err(SystemError::InvalidSignExpression(format!(
                    "{}.{} feature expression must be a typed case",
                    dim.keyword(),
                    expression.name
                )));
            };
            let declaration = sign.items.iter().rev().find_map(|item| match item {
                SignItem::FeatureDecl(declaration)
                    if declaration.dim == dim && declaration.name == expression.name =>
                {
                    Some(declaration)
                }
                _ => None,
            });
            let Some(declaration) = declaration else {
                return Err(SystemError::UndeclaredDerivationFeature {
                    dim,
                    name: expression.name.clone(),
                });
            };
            let current = SignValue::Stored(SignEvaluation {
                sign: sign.clone(),
                records: Vec::new(),
                source_sign: sign.clone(),
                context: DerivationContext::new(),
            });
            // 未定案原樣輸出:值域照傳,收斂留給構式。中途 Error 反而會把
            // 「這個 sign 在此維度尚未收斂」這個事實擋在構式看得到它之前。
            let base = Self::value_set_from_value(
                &current,
                &format!("{}.{}", dim.keyword(), expression.name),
            )
            .map(|values| values.join(" | "));
            let selected = self.evaluate_feature_case(
                &current,
                case,
                dim,
                &expression.name,
                &declaration.values,
                base,
                records,
            )?;
            if let Some(value) = selected {
                sign = synchronic::Patch::for_dim(dim)
                    .set(&expression.name, &value)
                    .apply(&sign);
            } else {
                return Err(SystemError::CaseDefaultMissing {
                    context: format!("{}.{}", dim.keyword(), expression.name),
                });
            }
        }
        Ok(sign)
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_feature_case(
        &self,
        current: &SignValue,
        case: &TypedCase,
        dim: Dim,
        name: &str,
        domain: &[String],
        base: Option<String>,
        records: &mut Vec<CaseRecord>,
    ) -> Result<Option<String>, SystemError> {
        for (index, branch) in case.branches.iter().enumerate() {
            if !self.case_condition_matches(current, case, &branch.condition)? {
                records.push(CaseRecord {
                    selection: case.selection,
                    branch: index,
                    status: CaseBranchStatus::Unmatched,
                    source: branch.source,
                    diagnostic_code: None,
                });
                continue;
            }
            let value = match &branch.result {
                Expression::EnumValue(value) => Some(value.clone()),
                Expression::Case(nested) => self.evaluate_feature_case(
                    current,
                    nested,
                    dim,
                    name,
                    domain,
                    base.clone(),
                    records,
                )?,
                _ => {
                    return Err(SystemError::InvalidSignExpression(format!(
                        "feature case {}.{name} returned a non-enum expression",
                        dim.keyword()
                    )))
                }
            };
            if let Some(value) = &value {
                if !domain.contains(value) {
                    return Err(SystemError::DerivationFeatureOutOfDomain {
                        dim,
                        name: name.to_owned(),
                        value: value.clone(),
                        domain: domain.join(", "),
                    });
                }
            }
            records.push(CaseRecord {
                selection: case.selection,
                branch: index,
                status: CaseBranchStatus::Matched,
                source: branch.source,
                diagnostic_code: None,
            });
            return Ok(value);
        }
        Ok(base)
    }

    fn case_guard_matches(&self, value: &SignValue, guard: &str) -> Result<bool, SystemError> {
        for conjunct in guard.split("&&").map(str::trim) {
            let (status, _, _, error) = match value {
                SignValue::Stored(evaluation) => {
                    synchronic::evaluate_sign_guard(&evaluation.sign, conjunct, &self.ontology)
                }
                SignValue::Applied(token) => {
                    synchronic::evaluate_token_guard(token, conjunct, &self.ontology)
                }
            };
            match status {
                RuleStatus::Matched => {}
                RuleStatus::Unmatched => return Ok(false),
                RuleStatus::Error => {
                    return Err(SystemError::InvalidSignExpression(
                        error.unwrap_or_else(|| format!("case guard {conjunct:?} failed")),
                    ))
                }
            }
        }
        Ok(true)
    }

    /// 讀出某路徑的**值域**。回 `Vec` 而不是單值,是因為 feature 的值本來就可能
    /// 未定案(`len >= 2`)——把「是哪幾個」壓成一個 scalar 的動作必須發生在呼叫端,
    /// 由呼叫端按自己的語意決定:產值處原樣傳遞,布林判定處拒答。
    fn value_set_from_value(value: &SignValue, path: &str) -> Option<Vec<String>> {
        match value {
            SignValue::Stored(evaluation) => {
                // 先定位最後一個相符項,再取值——不能在 `find_map` 裡對未定案回
                // `None`,那會讓搜尋繼續往前,靜默拿到更早的某個值。未定案就是
                // 沒有 scalar,必須在這裡停下。
                evaluation
                    .sign
                    .items
                    .iter()
                    .rev()
                    .find(|item| match item {
                        SignItem::Def(def) => def.path == path,
                        SignItem::FeatureValue(feature) => {
                            format!("{}.{}", feature.dim.keyword(), feature.name) == path
                        }
                        _ => false,
                    })
                    .and_then(|item| match item {
                        SignItem::Def(def) => Some(vec![def.value.clone()]),
                        SignItem::FeatureValue(feature) => Some(feature.values.clone()),
                        _ => None,
                    })
            }
            SignValue::Applied(token) => {
                let (dimension, field) = path.split_once('.')?;
                match Dim::parse(dimension)? {
                    Dim::Syn => token
                        .syn
                        .iter()
                        .find(|(candidate, _)| candidate == path)
                        .map(|(_, value)| vec![value.clone()]),
                    Dim::Sem => token.sem.features.get(field).map(|v| vec![v.clone()]),
                    Dim::Prag => token
                        .prag
                        .iter()
                        .find(|(candidate, _)| candidate == path)
                        .map(|(_, value)| vec![value.clone()]),
                    Dim::Phon => token
                        .phon
                        .iter()
                        .find(|(candidate, _)| candidate == path)
                        .map(|(_, value)| vec![value.clone()]),
                }
            }
        }
    }

    fn case_equals_matches(
        &self,
        value: &SignValue,
        scrutinee: &str,
        expected: &str,
    ) -> Result<bool, SystemError> {
        let read = reference::parse(&reference::SCRUTINEE, scrutinee).map_err(|error| {
            SystemError::InvalidSignExpression(format!("case scrutinee {scrutinee:?}: {error}"))
        })?;
        match (&read.subject, read.dim, read.path.as_deref()) {
            (reference::Subject::Slot(slot), Some(dim), Some(path)) => {
                let SignValue::Applied(token) = value else {
                    return Ok(false);
                };
                let Some(filler) = token.fillers.iter().find(|filler| &filler.slot == slot) else {
                    return Ok(false);
                };
                Ok(filler.scalar(dim, path) == Some(expected))
            }
            (reference::Subject::SelfSign, _, _) => {
                let path = read.dim_path().expect("SCRUTINEE requires a dimension");
                match Self::value_set_from_value(value, &path) {
                    Some(values) if values.len() > 1 => Err(SystemError::InvalidSignExpression(format!(
                        "{path} is undecided ({}); decide it on the sign or let a construction \
                         narrow it before testing it with `==`",
                        values.join(" | ")
                    ))),
                    other => Ok(other.as_deref() == Some(std::slice::from_ref(&expected.to_owned()))),
                }
            }
            _ => Err(SystemError::InvalidSignExpression(format!(
                "case scrutinee {scrutinee:?} needs a field: `$slot.NAME.DIM.FIELD`; \
                 to test a filler's category use a guard case (`$slot.NAME == [Trait]`)"
            ))),
        }
    }

    fn contextual_base_sign(&self, evaluation: &SignEvaluation) -> Result<SignDef, SystemError> {
        let mut base = self.ontology.effective_sign(&evaluation.source_sign);
        for ((dim, name), value) in evaluation.context.features() {
            if !matches!(dim, Dim::Syn | Dim::Sem) {
                return Err(SystemError::UnsupportedFeatureDimension { dim: *dim });
            }
            let declaration = base.items.iter().rev().find_map(|item| match item {
                SignItem::FeatureDecl(feature) if feature.dim == *dim && feature.name == *name => {
                    Some(feature)
                }
                _ => None,
            });
            let Some(declaration) = declaration else {
                return Err(SystemError::UndeclaredDerivationFeature {
                    dim: *dim,
                    name: name.clone(),
                });
            };
            if !declaration.values.contains(value) {
                return Err(SystemError::DerivationFeatureOutOfDomain {
                    dim: *dim,
                    name: name.clone(),
                    value: value.clone(),
                    domain: declaration.values.join(", "),
                });
            }
            let path = format!("{}.{}", dim.keyword(), name);
            let actual = base
                .project(*dim, &self.ontology)
                .defs
                .iter()
                .find(|(candidate, _)| candidate == &path)
                .map(|(_, actual)| actual.clone());
            match actual.as_deref() {
                Some(actual) if actual != value => {
                    return Err(SystemError::DerivationFeatureConflict {
                        dim: *dim,
                        name: name.clone(),
                        expected: value.clone(),
                        actual: actual.to_owned(),
                    })
                }
                Some(_) => {}
                None => {
                    base = synchronic::Patch::for_dim(*dim)
                        .set(name, value)
                        .apply(&base);
                }
            }
        }
        Ok(base)
    }

    fn bound_filler_from_value(&self, value: &SignValue) -> Result<BoundFiller, SystemError> {
        match value {
            SignValue::Stored(evaluation) => Ok(BoundFiller::Owned {
                committed: evaluation.sign.clone(),
                base: self.contextual_base_sign(evaluation)?,
                provenance: FillerProvenance::StoredSign(evaluation.source_sign.name.clone()),
            }),
            SignValue::Applied(token) => Ok(BoundFiller::Derived(Box::new(token.clone()))),
        }
    }

    fn apply_sign_application(
        &self,
        current: &SignValue,
        application: &SignApplication,
        stack: &[String],
        rules: &mut Vec<UnitRuleRecord>,
        cases: &mut Vec<CaseRecord>,
    ) -> Result<SignValue, SystemError> {
        if stack.iter().any(|name| name == &application.callee) {
            let mut cycle = stack.to_vec();
            cycle.push(application.callee.clone());
            return Err(SystemError::SignApplicationCycle(cycle));
        }
        let local = self
            .effective_language
            .sign_named(&application.callee)
            .ok_or_else(|| SystemError::UnknownSign(application.callee.clone()))?;
        let effective = self.ontology.effective_sign(local);
        let parameters = construction::parameters_of(&effective);
        let mut bindings = Vec::<(String, Option<BoundFiller>)>::new();
        let mut application_mapping = SlotMap::identity();
        let mut next_stack = stack.to_vec();
        next_stack.push(application.callee.clone());
        for argument in &application.arguments {
            let name = match &argument.name {
                Some(name) => name.clone(),
                None if parameters.len() == 1 => parameters[0].name.clone(),
                None => {
                    return Err(SystemError::InvalidSignExpression(format!(
                        "positional shorthand for {:?} requires exactly one slot",
                        application.callee
                    )))
                }
            };
            if !parameters.iter().any(|parameter| parameter.name == name) {
                return Err(SystemError::InvalidSignExpression(format!(
                    "unknown argument {name:?} for {:?}",
                    application.callee
                )));
            }
            if bindings.iter().any(|(bound, _)| bound == &name) {
                return Err(SystemError::InvalidSignExpression(format!(
                    "argument {name:?} is supplied more than once"
                )));
            }
            let value = match &argument.value {
                SignArgumentValue::SelfSign => Some(self.bound_filler_from_value(current)?),
                SignArgumentValue::Slot(slot) => match current {
                    SignValue::Stored(_) => {
                        if slot != &name {
                            application_mapping = application_mapping.rename(&name, slot);
                        }
                        None
                    }
                    SignValue::Applied(token) => {
                        if let Some((_, filler)) = token
                            .bound_fillers()
                            .iter()
                            .find(|(bound, _)| bound == slot)
                        {
                            Some(filler.clone())
                        } else if token
                            .residual_slots()
                            .iter()
                            .any(|parameter| parameter.name == *slot)
                        {
                            // A free variable from an already-applied Sign is
                            // passed by alias, not silently replaced by the
                            // callee's parameter name.
                            if slot != &name {
                                application_mapping = application_mapping.rename(&name, slot);
                            }
                            None
                        } else {
                            return Err(SystemError::InvalidSignExpression(format!(
                                "unknown slot variable {slot:?} in applied Sign {:?}",
                                token.construction
                            )));
                        }
                    }
                },
                SignArgumentValue::Application(nested) => {
                    let nested =
                        self.apply_sign_application(current, nested, &next_stack, rules, cases)?;
                    Some(self.bound_filler_from_value(&nested)?)
                }
            };
            bindings.push((name, value));
        }

        let committed = bindings
            .into_iter()
            .filter_map(|(name, value)| value.map(|value| (name, value)))
            .collect::<Vec<_>>();
        let mut token = construction::resume_with(
            &self.effective_language,
            &self.ontology,
            &application.callee,
            &committed,
            &[],
            &application_mapping,
        )?;
        if let SignValue::Applied(source) = current {
            let mut inherited_context = DerivationContext::new();
            for ((dim, name), value) in source.context_features() {
                inherited_context = inherited_context.feature(*dim, name.clone(), value.clone());
            }
            token = self.apply_context(token, &inherited_context)?;
        }
        self.evaluate_applied_sign(token, &next_stack, rules, cases)
    }

    fn evaluate_applied_sign(
        &self,
        token: DerivedToken,
        stack: &[String],
        rules: &mut Vec<UnitRuleRecord>,
        cases: &mut Vec<CaseRecord>,
    ) -> Result<SignValue, SystemError> {
        let expressions = token
            .rule_sign
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::SignExpression(expression) => Some(expression.expression.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let (token, token_rules, token_cases) = self.run_token_rules(token)?;
        rules.extend(token_rules);
        cases.extend(token_cases);
        let mut value = SignValue::Applied(token);
        for expression in expressions {
            let Expression::Case(case) = expression else {
                return Err(SystemError::InvalidSignExpression(
                    "Sign body expression must return Sign".to_owned(),
                ));
            };
            value = self.evaluate_sign_case(value, &case, cases, stack, rules)?;
        }
        Ok(value)
    }

    fn rebuild_applied_with_source(
        &self,
        source: &DerivedToken,
        source_sign: SignDef,
        _stack: &[String],
        rules: &mut Vec<UnitRuleRecord>,
        cases: &mut Vec<CaseRecord>,
    ) -> Result<SignValue, SystemError> {
        let mut runtime = self.effective_language.clone();
        if let Some(existing) = runtime
            .signs
            .iter_mut()
            .find(|candidate| candidate.name == source_sign.name)
        {
            *existing = source_sign.clone();
        } else {
            runtime.signs.push(source_sign.clone());
        }
        let mut token = construction::resume_with(
            &runtime,
            &self.ontology,
            &source_sign.name,
            source.bound_fillers(),
            &[],
            source.invocation_mapping(),
        )?;
        let mut context = DerivationContext::new();
        for ((dim, name), value) in source.context_features() {
            context = context.feature(*dim, name.clone(), value.clone());
        }
        token = self.apply_context(token, &context)?;
        // Rebuild from the deep/base token and run its dimension rules, but do
        // not replay Sign-level cases already traversed by the caller.  A
        // fragment may add nested cases; `apply_sign_fragment` evaluates only
        // those newly introduced expressions after this rebuild.
        let (token, token_rules, token_cases) = self.run_token_rules(token)?;
        rules.extend(token_rules);
        cases.extend(token_cases);
        let value = SignValue::Applied(token);
        if let SignValue::Applied(token) = &value {
            Self::check_context(token, &context)?;
        }
        Ok(value)
    }

    fn apply_sign_fragment(
        &self,
        current: SignValue,
        items: &[SignItem],
        records: &mut Vec<CaseRecord>,
        stack: &[String],
        rules: &mut Vec<UnitRuleRecord>,
    ) -> Result<SignValue, SystemError> {
        if items
            .iter()
            .any(|item| matches!(item, SignItem::TraitMount { kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_), .. }))
        {
            return Err(SystemError::InvalidSignExpression(
                "unexpanded trait use reached a SignContext fragment".to_owned(),
            ));
        }

        let nested = items
            .iter()
            .filter_map(|item| match item {
                SignItem::SignExpression(expression) => Some(expression.expression.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        let mut rebuilt = match current {
            SignValue::Stored(evaluation) => {
                let mut source = evaluation.source_sign.clone();
                source.items.extend(items.iter().cloned());
                let (evaluation, fragment_cases) =
                    self.evaluate_source_sign_with_context(&source, &evaluation.context)?;
                records.extend(fragment_cases);
                SignValue::Stored(evaluation)
            }
            SignValue::Applied(token) => {
                let mut source = token.source_sign().clone();
                source.items.extend(items.iter().cloned());
                self.rebuild_applied_with_source(&token, source, stack, rules, records)?
            }
        };

        for expression in nested {
            let Expression::Case(case) = expression else {
                return Err(SystemError::InvalidSignExpression(
                    "SignContext fragment contains a non-case Sign expression".to_owned(),
                ));
            };
            rebuilt = self.evaluate_sign_case(rebuilt, &case, records, stack, rules)?;
        }
        Ok(rebuilt)
    }

    fn apply_case_memberships(
        &self,
        result: SignValue,
        memberships: &[String],
        records: &mut Vec<CaseRecord>,
        stack: &[String],
        rules: &mut Vec<UnitRuleRecord>,
    ) -> Result<SignValue, SystemError> {
        if memberships.is_empty() {
            return Ok(result);
        }
        for category in memberships {
            if !self.ontology.has(category) {
                return Err(SystemError::InvalidSignExpression(format!(
                    "unknown branch membership {category:?}"
                )));
            }
        }
        match result {
            SignValue::Stored(evaluation) => {
                let mut source = evaluation.source_sign.clone();
                for category in memberships {
                    if !source
                        .items
                        .iter()
                        .any(|item| matches!(item, SignItem::TraitMount { name: value, kind: crate::TraitMountKind::Declaration, .. } if value == category))
                    {
                        source.items.push(SignItem::TraitMount { name: category.clone(), kind: crate::TraitMountKind::Declaration, args: vec![] });
                    }
                }
                let (evaluation, membership_cases) =
                    self.evaluate_source_sign_with_context(&source, &evaluation.context)?;
                records.extend(membership_cases);
                Ok(SignValue::Stored(evaluation))
            }
            SignValue::Applied(token) => {
                let mut source = token.source_sign().clone();
                for category in memberships {
                    if !source
                        .items
                        .iter()
                        .any(|item| matches!(item, SignItem::TraitMount { name: value, kind: crate::TraitMountKind::Declaration, .. } if value == category))
                    {
                        source.items.push(SignItem::TraitMount { name: category.clone(), kind: crate::TraitMountKind::Declaration, args: vec![] });
                    }
                }
                self.rebuild_applied_with_source(&token, source, stack, rules, records)
            }
        }
    }

    fn evaluate_sign_case(
        &self,
        current: SignValue,
        case: &TypedCase,
        records: &mut Vec<CaseRecord>,
        stack: &[String],
        rules: &mut Vec<UnitRuleRecord>,
    ) -> Result<SignValue, SystemError> {
        if case.selection == CaseSelection::Accumulate {
            // `when` guards form one matching phase.  Every guard observes
            // this same pre-merge value; fragments are not allowed to feed
            // later guards in the same `when`.
            let probe = current.clone();
            let mut statuses = Vec::with_capacity(case.branches.len());
            let mut any_matched = false;
            let mut merged_items = Vec::new();

            for branch in &case.branches {
                let matches = match &branch.condition {
                    CaseCondition::Else => !any_matched,
                    condition => self.case_condition_matches(&probe, case, condition)?,
                };
                statuses.push(matches);
                if !matches {
                    continue;
                }
                any_matched = true;
                if !branch.belongs.is_empty() {
                    return Err(SystemError::InvalidSignExpression(
                        "`when` memberships must be part of its anonymous SignContext fragment"
                            .to_owned(),
                    ));
                }
                match &branch.result {
                    Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
                        merged_items.extend(items.iter().cloned());
                    }
                    other => {
                        return Err(SystemError::InvalidSignExpression(format!(
                            "`when` returned a non-fragment expression {other:?}"
                        )));
                    }
                }
            }

            if !any_matched {
                records.extend(case.branches.iter().enumerate().map(|(index, branch)| {
                    CaseRecord {
                        selection: case.selection,
                        branch: index,
                        status: CaseBranchStatus::Unmatched,
                        source: branch.source,
                        diagnostic_code: None,
                    }
                }));
                return Ok(current);
            }

            // Keep trace/rule side effects transactional as well: a failed
            // fragment merge exposes neither a partially rebuilt Sign nor a
            // partially committed trace.
            let mut staged_records = Vec::new();
            let mut staged_rules = Vec::new();
            let result = self.apply_sign_fragment(
                current,
                &merged_items,
                &mut staged_records,
                stack,
                &mut staged_rules,
            )?;
            records.extend(
                case.branches
                    .iter()
                    .enumerate()
                    .map(|(index, branch)| CaseRecord {
                        selection: case.selection,
                        branch: index,
                        status: if statuses[index] {
                            CaseBranchStatus::Matched
                        } else {
                            CaseBranchStatus::Unmatched
                        },
                        source: branch.source,
                        diagnostic_code: None,
                    }),
            );
            records.extend(staged_records);
            rules.extend(staged_rules);
            return Ok(result);
        }

        for (index, branch) in case.branches.iter().enumerate() {
            let matches = self.case_condition_matches(&current, case, &branch.condition)?;
            if !matches {
                records.push(CaseRecord {
                    selection: case.selection,
                    branch: index,
                    status: CaseBranchStatus::Unmatched,
                    source: branch.source,
                    diagnostic_code: None,
                });
                continue;
            }
            let result = match &branch.result {
                Expression::SignApplication(application) => {
                    self.apply_sign_application(&current, application, stack, rules, records)
                }
                Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
                    self.apply_sign_fragment(current.clone(), items, records, stack, rules)
                }
                Expression::SelfSign => Ok(current.clone()),
                Expression::Case(nested) => {
                    self.evaluate_sign_case(current.clone(), nested, records, stack, rules)
                }
                other => Err(SystemError::InvalidSignExpression(format!(
                    "Sign case returned non-Sign expression {other:?}"
                ))),
            };
            let result = match result {
                Ok(value) => value,
                Err(error) if is_case_blocking_constraint(&error) => {
                    records.push(CaseRecord {
                        selection: case.selection,
                        branch: index,
                        status: CaseBranchStatus::MoreSpecificBlocked,
                        source: branch.source,
                        diagnostic_code: Some("CASE_MORE_SPECIFIC_BLOCKED"),
                    });
                    continue;
                }
                Err(error) => return Err(error),
            };
            let result =
                self.apply_case_memberships(result, &branch.belongs, records, stack, rules)?;
            records.push(CaseRecord {
                selection: case.selection,
                branch: index,
                status: CaseBranchStatus::Matched,
                source: branch.source,
                diagnostic_code: None,
            });
            return Ok(result);
        }
        // The enclosing Sign is the external/default expression.
        Ok(current)
    }

    /// Evaluate a V2 Sign as a total FP expression.  V1 signs simply return
    /// their ordinary evaluated four-dimensional value.
    pub fn evaluate_sign_expression(
        &self,
        name: &str,
        context: &DerivationContext,
    ) -> Result<SignExpressionEvaluation, SystemError> {
        let (evaluated, mut cases) = self.evaluate_sign_with_context_internal(name, context)?;
        let expressions = evaluated
            .sign
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::SignExpression(expression) => Some(expression.expression.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        // The context constrains the requested deep Sign. A Sign-valued case
        // may return a different construction whose public feature shape is
        // intentionally different (for example, an inflectional wrapper
        // around `$self`), so the result need not re-export every feature.
        Self::check_sign_context(&evaluated.sign, context, &self.ontology)?;
        let mut value = SignValue::Stored(evaluated);
        let mut rules = Vec::new();
        let stack = vec![name.to_owned()];
        for expression in expressions {
            let Expression::Case(case) = expression else {
                return Err(SystemError::InvalidSignExpression(
                    "Sign body expression must return Sign".to_owned(),
                ));
            };
            value = self.evaluate_sign_case(value, &case, &mut cases, &stack, &mut rules)?;
        }
        Ok(SignExpressionEvaluation { value, cases })
    }

    /// 把一個 `DerivedToken` 跑完 token rules 與 sign-body `case:`,得到可實現的
    /// [`EvaluatedToken`]。這是 [`Self::apply_construction`] 與 [`Self::realize_phon`]
    /// 之間缺的那一段——`derive_with_context` 內部走的也是同一段。
    ///
    /// sign-body 的 case 可以回傳**另一個** sign(如 `walk` 的 `en_3sg({$self})`);
    /// 那種情形不是一個可實現的 token,故回 `InvalidSignExpression`,與
    /// `derive_with_context` 的處置一致。
    pub fn evaluate_token(&self, token: DerivedToken) -> Result<EvaluatedToken, SystemError> {
        let stack = vec![token.construction.clone()];
        let mut rules = Vec::new();
        let mut cases = Vec::new();
        let value = self.evaluate_applied_sign(token, &stack, &mut rules, &mut cases)?;
        let SignValue::Applied(token) = value else {
            return Err(SystemError::InvalidSignExpression(
                "a saturated construction must evaluate to an applied Sign".to_owned(),
            ));
        };
        Ok(EvaluatedToken(token))
    }

    /// **只跑到「filler 填進槽」**(§3.1:部分入口)。token rules 與 sign-body `case:`
    /// 都還沒跑,故它的結果**不能**直接餵給 `realize_phon`——要嘛接 [`Self::evaluate_token`],
    /// 要嘛整條用 [`Self::derive`]。保留公開是因為 SlotMap 變價、飽和性與槽授權的
    /// 負例本來就該在這一層驗。
    pub fn apply_construction<'a>(
        &self,
        construction: &str,
        fillers: &[SlotFiller<'a>],
        mapping: &SlotMap,
    ) -> Result<DerivedToken, SystemError> {
        construction::apply_with(
            &self.effective_language,
            &self.ontology,
            construction,
            fillers,
            mapping,
        )
        .map_err(SystemError::from)
    }

    fn apply_arguments_to_token<'a>(
        &self,
        source: &DerivedToken,
        additional: &[SlotFiller<'a>],
    ) -> Result<DerivedToken, SystemError> {
        let mut supplied = BTreeSet::new();
        for filler in additional {
            if !supplied.insert(filler.slot) {
                return Err(SystemError::Construction(CxgError::DuplicateFill(
                    filler.slot.to_owned(),
                )));
            }
        }

        let own_names = source
            .own_residual_slots()
            .iter()
            .map(|slot| slot.name.as_str())
            .collect::<BTreeSet<_>>();
        let own_additional = additional
            .iter()
            .copied()
            .filter(|filler| own_names.contains(filler.slot))
            .collect::<Vec<_>>();
        let mut consumed = own_additional
            .iter()
            .map(|filler| filler.slot)
            .collect::<BTreeSet<_>>();

        let mut rebound = source.bound_fillers().to_vec();
        for (_, filler) in &mut rebound {
            let BoundFiller::Derived(nested) = filler else {
                continue;
            };
            let nested_names = nested
                .residual_slots()
                .iter()
                .map(|slot| slot.name.as_str())
                .collect::<BTreeSet<_>>();
            let nested_additional = additional
                .iter()
                .copied()
                .filter(|argument| nested_names.contains(argument.slot))
                .collect::<Vec<_>>();
            if nested_additional.is_empty() {
                continue;
            }
            consumed.extend(nested_additional.iter().map(|argument| argument.slot));
            **nested = self.apply_arguments_to_token(nested, &nested_additional)?;
        }
        if let Some(unknown) = additional
            .iter()
            .find(|filler| !consumed.contains(filler.slot))
        {
            return Err(SystemError::Construction(CxgError::UnknownSlot {
                construction: source.construction.clone(),
                slot: unknown.slot.to_owned(),
            }));
        }

        let mut runtime = self.effective_language.clone();
        if let Some(existing) = runtime
            .signs
            .iter_mut()
            .find(|candidate| candidate.name == source.source_sign().name)
        {
            *existing = source.source_sign().clone();
        } else {
            runtime.signs.push(source.source_sign().clone());
        }
        let mut token = construction::resume_with(
            &runtime,
            &self.ontology,
            &source.construction,
            &rebound,
            &own_additional,
            source.invocation_mapping(),
        )?;
        let mut context = DerivationContext::new();
        for ((dim, name), value) in source.context_features() {
            context = context.feature(*dim, name.clone(), value.clone());
        }
        token = self.apply_context(token, &context)?;
        let mut rules = Vec::new();
        let mut cases = Vec::new();
        let stack = vec![source.construction.clone()];
        let value = self.evaluate_applied_sign(token, &stack, &mut rules, &mut cases)?;
        let SignValue::Applied(token) = value else {
            return Err(SystemError::InvalidSignExpression(
                "supplying arguments must return an applied Sign".to_owned(),
            ));
        };
        Self::check_context(&token, &context)?;
        Ok(token)
    }

    /// Apply more named arguments to a Sign value. An unsaturated Sign stays
    /// an ordinary Sign value with free variables and residual constraints;
    /// the input value is immutable.
    pub fn apply_arguments<'a>(
        &self,
        sign: &SignValue,
        additional: &[SlotFiller<'a>],
    ) -> Result<SignValue, SystemError> {
        match sign {
            SignValue::Stored(evaluation) => {
                let mut runtime = self.effective_language.clone();
                if let Some(existing) = runtime
                    .signs
                    .iter_mut()
                    .find(|candidate| candidate.name == evaluation.sign.name)
                {
                    *existing = evaluation.source_sign.clone();
                } else {
                    runtime.signs.push(evaluation.source_sign.clone());
                }
                let mut token = construction::apply_with(
                    &runtime,
                    &self.ontology,
                    &evaluation.sign.name,
                    additional,
                    &SlotMap::identity(),
                )?;
                token = self.apply_context(token, &evaluation.context)?;
                let mut rules = Vec::new();
                let mut cases = Vec::new();
                let value = self.evaluate_applied_sign(
                    token,
                    std::slice::from_ref(&evaluation.sign.name),
                    &mut rules,
                    &mut cases,
                )?;
                if let SignValue::Applied(token) = &value {
                    Self::check_context(token, &evaluation.context)?;
                }
                Ok(value)
            }
            SignValue::Applied(token) => self
                .apply_arguments_to_token(token, additional)
                .map(SignValue::Applied),
        }
    }

    pub fn recontextualize_token(
        &self,
        token: &DerivedToken,
        context: &DerivationContext,
    ) -> Result<(DerivedToken, Vec<RuleRecord>), SystemError> {
        let mut merged = DerivationContext::new();
        for ((dim, name), value) in token.context_features() {
            merged = merged.feature(*dim, name.clone(), value.clone());
        }
        for ((dim, name), value) in context.features() {
            if let Some(existing) = token.context_features().get(&(*dim, name.clone())) {
                if existing != value {
                    return Err(SystemError::DerivationFeatureConflict {
                        dim: *dim,
                        name: name.clone(),
                        expected: value.clone(),
                        actual: existing.clone(),
                    });
                }
            }
            merged = merged.feature(*dim, name.clone(), value.clone());
        }
        let token = token.reset_to_deep();
        let token = self.apply_context(token, &merged)?;
        let (token, records, _) = self.run_token_rules(token)?;
        Self::check_context(&token, &merged)?;
        Ok((
            token,
            records.into_iter().map(|record| record.record).collect(),
        ))
    }

    fn run_token_rules(
        &self,
        token: DerivedToken,
    ) -> Result<(DerivedToken, Vec<UnitRuleRecord>, Vec<CaseRecord>), SystemError> {
        let unit = token.construction.clone();
        let (token, records, occurrence_cases) =
            construction::evaluate_token_pipeline(token, &self.ontology).map_err(|error| {
                match error {
                    construction::TokenExpressionError::UndeclaredFeature { dim, name } => {
                        SystemError::UndeclaredDerivationFeature { dim, name }
                    }
                    construction::TokenExpressionError::FeatureOutOfDomain {
                        dim,
                        name,
                        value,
                        domain,
                    } => SystemError::DerivationFeatureOutOfDomain {
                        dim,
                        name,
                        value,
                        domain,
                    },
                    construction::TokenExpressionError::CaseDefaultMissing { context } => {
                        SystemError::CaseDefaultMissing { context }
                    }
                    construction::TokenExpressionError::Invalid(message) => {
                        SystemError::InvalidSignExpression(message)
                    }
                    construction::TokenExpressionError::Construction(error) => {
                        SystemError::Construction(error)
                    }
                }
            })?;
        let trace = records
            .into_iter()
            .map(|record| UnitRuleRecord {
                unit: unit.clone(),
                record,
            })
            .collect();
        let cases = occurrence_cases
            .into_iter()
            .map(|record| CaseRecord {
                selection: CaseSelection::FirstMatch,
                branch: record.branch,
                status: match record.status {
                    construction::OccurrenceCaseStatus::Matched => CaseBranchStatus::Matched,
                    construction::OccurrenceCaseStatus::Unmatched => CaseBranchStatus::Unmatched,
                    construction::OccurrenceCaseStatus::MoreSpecificBlocked => {
                        CaseBranchStatus::MoreSpecificBlocked
                    }
                },
                source: record.source,
                diagnostic_code: record.diagnostic_code,
            })
            .collect();
        Ok((token, trace, cases))
    }

    fn token_feature<'a>(token: &'a DerivedToken, dim: Dim, name: &str) -> Option<&'a str> {
        match dim {
            Dim::Syn => token
                .syn
                .iter()
                .find(|(path, _)| path == &format!("syn.{name}"))
                .map(|(_, value)| value.as_str()),
            Dim::Sem => token.sem.features.get(name).map(String::as_str),
            Dim::Phon | Dim::Prag => None,
        }
    }

    fn apply_context(
        &self,
        token: DerivedToken,
        context: &DerivationContext,
    ) -> Result<DerivedToken, SystemError> {
        for ((dim, name), value) in context.features() {
            if !matches!(dim, Dim::Syn | Dim::Sem) {
                return Err(SystemError::UnsupportedFeatureDimension { dim: *dim });
            }
            let declaration = token
                .rule_sign
                .items
                .iter()
                .rev()
                .find_map(|item| match item {
                    SignItem::FeatureDecl(feature)
                        if feature.dim == *dim && feature.name == *name =>
                    {
                        Some(feature)
                    }
                    _ => None,
                });
            let Some(declaration) = declaration else {
                return Err(SystemError::UndeclaredDerivationFeature {
                    dim: *dim,
                    name: name.clone(),
                });
            };
            if !declaration.values.contains(value) {
                return Err(SystemError::DerivationFeatureOutOfDomain {
                    dim: *dim,
                    name: name.clone(),
                    value: value.clone(),
                    domain: declaration.values.join(", "),
                });
            }
            if let Some(actual) = Self::token_feature(&token, *dim, name) {
                if actual != value {
                    return Err(SystemError::DerivationFeatureConflict {
                        dim: *dim,
                        name: name.clone(),
                        expected: value.clone(),
                        actual: actual.to_owned(),
                    });
                }
            }
        }
        let mut output = token;
        for ((dim, name), value) in context.features() {
            if Self::token_feature(&output, *dim, name).is_some() {
                output.remember_context(*dim, name.clone(), value.clone());
                continue;
            }
            match dim {
                Dim::Syn => output.syn.push((format!("syn.{name}"), value.clone())),
                Dim::Sem => {
                    output.sem.features.insert(name.clone(), value.clone());
                }
                Dim::Phon | Dim::Prag => unreachable!("parser only permits syn/sem features"),
            }
            output.remember_context(*dim, name.clone(), value.clone());
        }
        Ok(output)
    }

    fn check_sign_context(
        sign: &SignDef,
        context: &DerivationContext,
        ontology: &OntologyRegistry,
    ) -> Result<(), SystemError> {
        for ((dim, name), expected) in context.features() {
            if !matches!(dim, Dim::Syn | Dim::Sem) {
                return Err(SystemError::UnsupportedFeatureDimension { dim: *dim });
            }
            let path = format!("{}.{}", dim.keyword(), name);
            let projection = sign.project(*dim, ontology);
            let actual = projection
                .defs
                .iter()
                .find(|(candidate, _)| candidate == &path)
                .map(|(_, value)| value.as_str());
            if actual != Some(expected.as_str()) {
                return Err(SystemError::DerivationFeatureConflict {
                    dim: *dim,
                    name: name.clone(),
                    expected: expected.clone(),
                    actual: actual.unwrap_or("<missing>").to_owned(),
                });
            }
        }
        Ok(())
    }

    fn check_context(token: &DerivedToken, context: &DerivationContext) -> Result<(), SystemError> {
        for ((dim, name), expected) in context.features() {
            if !matches!(dim, Dim::Syn | Dim::Sem) {
                return Err(SystemError::UnsupportedFeatureDimension { dim: *dim });
            }
            let actual = Self::token_feature(token, *dim, name);
            if actual != Some(expected.as_str()) {
                return Err(SystemError::DerivationFeatureConflict {
                    dim: *dim,
                    name: name.clone(),
                    expected: expected.clone(),
                    actual: actual.unwrap_or("<missing>").to_owned(),
                });
            }
        }
        Ok(())
    }

    fn evaluate_phon_case(
        &self,
        token: &DerivedToken,
        case: &TypedCase,
        default: &str,
        cases: &mut Vec<CaseRecord>,
        nested_rules: &mut Vec<UnitRuleRecord>,
    ) -> Result<(String, Option<usize>, SourceLocation), SystemError> {
        let current = SignValue::Applied(token.clone());
        for (index, branch) in case.branches.iter().enumerate() {
            if !self.case_condition_matches(&current, case, &branch.condition)? {
                cases.push(CaseRecord {
                    selection: case.selection,
                    branch: index,
                    status: CaseBranchStatus::Unmatched,
                    source: branch.source,
                    diagnostic_code: None,
                });
                continue;
            }
            let input = match &branch.result {
                Expression::PhonTemplate(template) => token.expand_phon_template(template)?,
                Expression::PhonInterpolation(application) => {
                    // `$self` inside its own phon projection denotes the
                    // finalized deep Sign, not a recursive realization call.
                    let projection_base = SignValue::Applied(token.as_phon_projection_base());
                    let mut application_cases = Vec::new();
                    let nested = self.apply_sign_application(
                        &projection_base,
                        application,
                        std::slice::from_ref(&token.construction),
                        nested_rules,
                        &mut application_cases,
                    )?;
                    cases.extend(application_cases);
                    let SignValue::Applied(nested) = nested else {
                        return Err(SystemError::InvalidSignExpression(
                            "phon projection did not produce an applied Sign".to_owned(),
                        ));
                    };
                    if !nested.is_saturated() {
                        return Err(SystemError::Construction(CxgError::Unsaturated(
                            nested.missing_required(),
                        )));
                    }
                    // `nested` 來自 `apply_sign_application` → `evaluate_applied_sign`,
                    // 已是求值後的 token,故可直接標記。
                    let nested = EvaluatedToken::already_evaluated(nested);
                    let realization = self.realize_phon(&nested)?;
                    cases.extend(realization.cases);
                    nested_rules.extend(realization.nested_rules);
                    realization.input.into_inner()
                }
                Expression::Case(nested) => {
                    self.evaluate_phon_case(token, nested, default, cases, nested_rules)?
                        .0
                }
                other => {
                    return Err(SystemError::InvalidSignExpression(format!(
                        "phon case returned non-Phon expression {other:?}"
                    )))
                }
            };
            cases.push(CaseRecord {
                selection: case.selection,
                branch: index,
                status: CaseBranchStatus::Matched,
                source: branch.source,
                diagnostic_code: None,
            });
            return Ok((input, Some(index), branch.source));
        }
        Ok((default.to_owned(), None, SourceLocation::unknown()))
    }

    /// 把一個**已求值**的 token 實現為 phon 輸入。
    ///
    /// 參數刻意是 [`EvaluatedToken`] 而非 `DerivedToken`:`apply_construction` 只跑到
    /// 「filler 填進槽」,token rules 與 sign-body `case:` 都還沒跑,而真實管線裡
    /// `realize_phon` **永遠**只拿到 `evaluate_applied_sign` 之後的 token。舊簽名收
    /// `&DerivedToken` 時,`apply_construction(...) + realize_phon(...)` 是一個編譯得過、
    /// 執行不報錯、卻在產品路徑上不存在的組合——guard 若讀 `$self.<由 token rule 算出的
    /// 特徵>` 會讀到空值、靜默掉進 `else`,測試照樣綠燈。改型別讓這個組合寫不出來。
    ///
    /// 取得 [`EvaluatedToken`]:[`Self::evaluate_token`](完整求值但不跑音變)或
    /// [`Self::derive`](一路到表層)。
    pub fn realize_phon(&self, token: &EvaluatedToken) -> Result<PhonRealization, SystemError> {
        let token = &token.0;
        let realization = token.rule_sign.items.iter().find_map(|item| match item {
            SignItem::Realization(realization) => Some(realization),
            _ => None,
        });
        let slot_reads = Vec::new();
        let self_reads = Vec::new();
        let mut cases = Vec::new();
        let mut nested_rules = Vec::new();
        let default = token.phon_form()?;
        let mut typed = None;
        if let Some(realization) = realization {
            typed = Some(self.evaluate_phon_case(
                token,
                &realization.expression,
                &default,
                &mut cases,
                &mut nested_rules,
            )?);
        }
        let (input, branch, source) = if let Some(result) = typed {
            result
        } else {
            (default, None, SourceLocation::unknown())
        };
        if input.contains("$self")
            || input.contains("$slot")
            || input.contains('{')
            || input.contains('}')
            || input.contains('/')
        {
            return Err(SystemError::ImpureRealizedPhon(input));
        }
        Ok(PhonRealization {
            input: RealizedPhonInput(input),
            branch,
            source,
            slot_reads,
            self_reads,
            cases,
            nested_rules,
        })
    }

    fn phon_program(&self, token: &DerivedToken) -> Result<tshiatun_dsl::Program, SystemError> {
        let mut source = self.artifacts.grammar.phon_source.clone();
        let mut number = self.artifacts.grammar.program.rules.len() as u32 + 1;
        for item in &token.rule_sign.items {
            if let SignItem::Rule(rule) = item {
                if rule.dim == Dim::Phon {
                    codegen::emit_rule(&mut source, &mut number, rule)
                        .map_err(|error| SystemError::PhonCompile(error.to_string()))?;
                }
            }
        }
        tshiatun_dsl::compile(&source).map_err(|error| SystemError::PhonCompile(error.to_string()))
    }

    pub fn derive<'a>(
        &self,
        construction: &str,
        fillers: &[SlotFiller<'a>],
        mapping: &SlotMap,
    ) -> Result<SystemDerivation, SystemError> {
        self.derive_with_context(construction, fillers, mapping, DerivationContext::new())
    }

    pub fn derive_candidates<'a>(
        &self,
        category: &str,
        fillers: &[SlotFiller<'a>],
        mapping: &SlotMap,
        context: &DerivationContext,
    ) -> Result<CandidateSet, SystemError> {
        if !self.ontology.has(category) {
            return Err(SystemError::UnknownConstructionCategory(
                category.to_owned(),
            ));
        }
        let mut candidates = Vec::new();
        for sign in &self.effective_language.signs {
            let effective = self.ontology.effective_sign(sign);
            if !construction::is_construction(&effective)
                || !self
                    .ontology
                    .categories_satisfy(&self.ontology.sign_categories(&effective), category)
            {
                continue;
            }
            let applied = construction::apply_with(
                &self.effective_language,
                &self.ontology,
                &sign.name,
                fillers,
                mapping,
            );
            let token = match applied {
                Ok(token) => token,
                Err(error) => {
                    let error = SystemError::Construction(error);
                    if is_candidate_compatibility_mismatch(&error) {
                        continue;
                    }
                    return Err(error);
                }
            };
            let token = match self.apply_context(token, context) {
                Ok(token) => token,
                Err(error) if is_candidate_compatibility_mismatch(&error) => continue,
                Err(error) => return Err(error),
            };
            let mut rules = Vec::new();
            let mut cases = Vec::new();
            let stack = vec![sign.name.clone()];
            let value = match self.evaluate_applied_sign(token, &stack, &mut rules, &mut cases) {
                Ok(value) => value,
                Err(error) if is_candidate_compatibility_mismatch(&error) => continue,
                Err(error) => return Err(error),
            };
            let SignValue::Applied(token) = value else {
                return Err(SystemError::InvalidSignExpression(format!(
                    "candidate {:?} did not evaluate to an applied Sign",
                    sign.name
                )));
            };
            if !token.is_saturated() {
                continue;
            }
            match Self::check_context(&token, context) {
                Ok(()) => {}
                Err(error) if is_candidate_compatibility_mismatch(&error) => continue,
                Err(error) => return Err(error),
            }
            let entrenchment = effective
                .items
                .iter()
                .rev()
                .find_map(|item| match item {
                    SignItem::Def(def) if def.path == "entrenchment" => {
                        def.value.parse::<f64>().ok()
                    }
                    _ => None,
                })
                .unwrap_or(1.0);
            if !entrenchment.is_finite() || entrenchment < 0.0 {
                return Err(SystemError::InvalidSignExpression(format!(
                    "candidate {:?} has invalid entrenchment {entrenchment}",
                    sign.name
                )));
            }
            candidates.push(ConstructionCandidate {
                id: effective.id,
                name: sign.name.clone(),
                entrenchment,
            });
        }
        candidates.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(CandidateSet {
            category: category.to_owned(),
            candidates,
        })
    }

    pub fn select_candidate(
        &self,
        candidates: &CandidateSet,
        selector: CandidateSelector,
        deterministic_id: Option<&SignId>,
    ) -> Result<CandidateSelectionTrace, SystemError> {
        let selected = match selector {
            CandidateSelector::Deterministic => {
                let id = deterministic_id.ok_or_else(|| SystemError::AmbiguousConstruction {
                    category: candidates.category.clone(),
                    candidates: candidates
                        .candidates
                        .iter()
                        .map(|candidate| candidate.name.clone())
                        .collect(),
                })?;
                candidates
                    .candidates
                    .iter()
                    .find(|candidate| &candidate.id == id)
                    .ok_or_else(|| SystemError::UnknownCandidate(id.clone()))?
            }
            CandidateSelector::SampleEntrenchment { seed } => {
                let weights = candidates
                    .candidates
                    .iter()
                    .map(|candidate| candidate.entrenchment)
                    .collect::<Vec<_>>();
                let sample =
                    sample_weighted_index(&weights, seed).map_err(|error| match error {
                        WeightedSampleError::Empty | WeightedSampleError::AllZero => {
                            SystemError::ZeroCandidateWeight
                        }
                        WeightedSampleError::InvalidWeight { .. } => {
                            SystemError::InvalidSignExpression(error.to_string())
                        }
                    })?;
                candidates
                    .candidates
                    .get(sample.selected_index)
                    .ok_or(SystemError::ZeroCandidateWeight)?
            }
        };
        Ok(CandidateSelectionTrace {
            seed: match selector {
                CandidateSelector::Deterministic => None,
                CandidateSelector::SampleEntrenchment { seed } => Some(seed),
            },
            ordered: candidates
                .candidates
                .iter()
                .map(|candidate| (candidate.id.clone(), candidate.entrenchment))
                .collect(),
            selected: selected.id.clone(),
        })
    }

    pub fn derive_category<'a>(
        &self,
        category: &str,
        fillers: &[SlotFiller<'a>],
        mapping: &SlotMap,
        context: DerivationContext,
    ) -> Result<SystemDerivation, SystemError> {
        let candidates = self.derive_candidates(category, fillers, mapping, &context)?;
        if candidates.candidates.is_empty() {
            return Err(SystemError::NoMatchingConstruction {
                category: category.to_owned(),
            });
        }
        if candidates.candidates.len() > 1 {
            return Err(SystemError::AmbiguousConstruction {
                category: category.to_owned(),
                candidates: candidates
                    .candidates
                    .iter()
                    .map(|candidate| candidate.name.clone())
                    .collect(),
            });
        }
        self.derive_with_context(&candidates.candidates[0].name, fillers, mapping, context)
    }

    pub fn derive_with_context<'a>(
        &self,
        construction: &str,
        fillers: &[SlotFiller<'a>],
        mapping: &SlotMap,
        context: DerivationContext,
    ) -> Result<SystemDerivation, SystemError> {
        let mut token = construction::apply_with(
            &self.effective_language,
            &self.ontology,
            construction,
            fillers,
            mapping,
        )?;
        let mut occurrences = token.take_occurrence_records();
        let mut rules = occurrences
            .iter()
            .flat_map(|occurrence| {
                occurrence
                    .committed_rules
                    .iter()
                    .cloned()
                    .map(|record| UnitRuleRecord {
                        unit: occurrence.slot_path.clone(),
                        record,
                    })
            })
            .collect::<Vec<_>>();
        let token = self.apply_context(token, &context)?;
        let mut cases = occurrences
            .iter()
            .flat_map(|occurrence| occurrence.cases.iter())
            .map(|record| CaseRecord {
                selection: CaseSelection::FirstMatch,
                branch: record.branch,
                status: match record.status {
                    construction::OccurrenceCaseStatus::Matched => CaseBranchStatus::Matched,
                    construction::OccurrenceCaseStatus::Unmatched => CaseBranchStatus::Unmatched,
                    construction::OccurrenceCaseStatus::MoreSpecificBlocked => {
                        CaseBranchStatus::MoreSpecificBlocked
                    }
                },
                source: record.source,
                diagnostic_code: record.diagnostic_code,
            })
            .collect::<Vec<_>>();
        let stack = vec![construction.to_owned()];
        let value = self.evaluate_applied_sign(token, &stack, &mut rules, &mut cases)?;
        let SignValue::Applied(mut token) = value else {
            return Err(SystemError::InvalidSignExpression(
                "a saturated construction must evaluate to an applied Sign".to_owned(),
            ));
        };
        Self::check_context(&token, &context)?;
        let final_occurrences = token.take_occurrence_records();
        rules.extend(final_occurrences.iter().flat_map(|occurrence| {
            occurrence
                .committed_rules
                .iter()
                .cloned()
                .map(|record| UnitRuleRecord {
                    unit: occurrence.slot_path.clone(),
                    record,
                })
        }));
        cases.extend(
            final_occurrences
                .iter()
                .flat_map(|occurrence| occurrence.cases.iter())
                .map(|record| CaseRecord {
                    selection: CaseSelection::FirstMatch,
                    branch: record.branch,
                    status: match record.status {
                        construction::OccurrenceCaseStatus::Matched => CaseBranchStatus::Matched,
                        construction::OccurrenceCaseStatus::Unmatched => {
                            CaseBranchStatus::Unmatched
                        }
                        construction::OccurrenceCaseStatus::MoreSpecificBlocked => {
                            CaseBranchStatus::MoreSpecificBlocked
                        }
                    },
                    source: record.source,
                    diagnostic_code: record.diagnostic_code,
                }),
        );
        occurrences.extend(final_occurrences);

        let program = self.phon_program(&token)?;
        // `token` 是上方 `evaluate_applied_sign` 解構出來的 `SignValue::Applied`。
        let evaluated = EvaluatedToken::already_evaluated(token);
        let realization = self.realize_phon(&evaluated)?;
        let mut token = evaluated.into_token();
        rules.extend(realization.nested_rules.iter().cloned());
        cases.extend(realization.cases.iter().cloned());
        token.record_realized_phon_input(realization.input.as_str().to_owned());
        let word = tshiatun_dsl::build_phrase(&program, realization.input.as_str())
            .map_err(|error| SystemError::PhonRuntime(error.to_string()))?;
        let fallback = word.clone();
        let phon_steps = tshiatun_dsl::run_program(&program, word)
            .map_err(|error| SystemError::PhonRuntime(error.to_string()))?;
        let last = phon_steps
            .last()
            .map(|step| &step.word)
            .unwrap_or(&fallback);
        let surface = tshiatun_dsl::surface_phrase(&program, last)
            .map_err(|error| SystemError::PhonRuntime(error.to_string()))?;
        let diagnostics: Vec<_> = rules
            .iter()
            .filter(|entry| entry.record.status == RuleStatus::Error)
            .map(|entry| {
                Diagnostic::new(
                    Severity::Error,
                    "RULE_RUNTIME_ERROR",
                    entry
                        .record
                        .diag
                        .clone()
                        .unwrap_or_else(|| "rule evaluation failed".to_owned()),
                )
                .with_sources(vec![DiagnosticSource {
                    owner: entry.unit.clone(),
                    path: Some(format!("rule {}", entry.record.rule_id)),
                    location: entry.record.source,
                }])
            })
            .collect();
        Ok(SystemDerivation {
            token,
            surface,
            rules,
            phon_steps,
            diagnostics,
            realization,
            occurrences,
            cases,
        })
    }
}
