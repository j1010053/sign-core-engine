//! Public M1++ runtime path: Language -> validated compiled system -> stored
//! signs / constructions / derived tokens -> phon surface and complete trace.

use crate::codegen::{self, Artifacts, CodegenError};
use crate::construction::{
    self, BoundFiller, CxgError, DerivedToken, Filler, OccurrenceRecord, SlotFiller, SlotMap,
    SlotMapOp,
};
use crate::diagnostic::{Diagnostic, DiagnosticSource, Severity, SourceLocation, ValidationReport};
use crate::library::{self, LibraryId, LibraryLoadError, LibrarySpec};
use crate::ontology::OntologyRegistry;
use crate::path::parse_path;
use crate::semantic_dto::{SemanticDocumentError, SemanticDocumentV1, SemanticNodeV1};
use crate::synchronic::{self, RuleRecord, RuleStatus, SelfRead, SlotRead};
use crate::{
    CaseCondition, Dim, Expression, Language, LanguageDocument, SignApplication, SignArgumentValue,
    SignDef, SignId, SignItem, SignLifecycle, SignProvenance, SlotConstraint, TypedCase,
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
    #[error("unknown candidate {0}")]
    UnknownCandidate(SignId),
    #[error("all candidate entrenchment weights are zero")]
    ZeroCandidateWeight,
    #[error("invalid Sign expression: {0}")]
    InvalidSignExpression(String),
    #[error("Sign application cycle: {0:?}")]
    SignApplicationCycle(Vec<String>),
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignEvaluation {
    pub sign: SignDef,
    pub records: Vec<RuleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignValue {
    Stored(SignEvaluation),
    Applied(DerivedToken),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialSign {
    token: DerivedToken,
}

impl PartialSign {
    pub fn token(&self) -> &DerivedToken {
        &self.token
    }

    pub fn parameters(&self) -> Vec<crate::SignParameter> {
        self.token
            .residual_slots()
            .iter()
            .map(crate::SignParameter::from)
            .collect()
    }
}

impl SignValue {
    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Applied(token) if !token.is_saturated())
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

    pub fn partial(&self) -> Option<PartialSign> {
        match self {
            Self::Applied(token) if !token.is_saturated() => Some(PartialSign {
                token: token.clone(),
            }),
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

fn validate_defs_and_rules(
    language: &Language,
    registry: &OntologyRegistry,
    report: &mut ValidationReport,
) {
    let mut validate_items = |owner: &str,
                              items: &[SignItem],
                              sign_metadata: bool,
                              slots: &[crate::Slot]| {
        let mut slot_feature_targets = BTreeSet::new();
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
                            "provenance" => SignProvenance::parse(&def.value).is_some(),
                            "lifecycle" => SignLifecycle::parse(&def.value).is_some(),
                            "source_package" => LibraryId::from_str(&def.value).is_ok(),
                            _ => false,
                        };
                    let valid_dim = path_dimension(&def.path).is_some()
                        && (def.path == "phon"
                            || def
                                .path
                                .split_once('.')
                                .is_some_and(|(_, field)| !field.is_empty()))
                        && parse_path(&def.path).is_ok();
                    if !valid_meta && !valid_dim {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "DEF_INVALID_PATH_OR_VALUE",
                                format!(
                                    "{owner:?} has invalid Definition {} = {}",
                                    def.path, def.value
                                ),
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
                    for error in synchronic::validate_rule(rule, registry, slots) {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "RULE_INVALID",
                                format!("{owner:?}: {error}"),
                            )
                            .with_sources(vec![DiagnosticSource {
                                owner: owner.to_owned(),
                                path: Some(format!("rule {}", rule.id)),
                                location: rule.source,
                            }]),
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
        let synthetic = SignDef {
            id: crate::SignId::synthetic(),
            name: format!("{}#rule-validation", trait_def.name),
            items: vec![SignItem::Belongs(trait_def.name.clone())],
        };
        let effective = registry.effective_sign(&synthetic);
        let slots = construction::slots_of(&effective);
        for block in &trait_def.blocks {
            validate_items(&trait_def.name, &block.items, false, &slots);
        }
    }
    for sign in &language.signs {
        let effective = registry.effective_sign(sign);
        let slots = construction::slots_of(&effective);
        validate_items(&sign.name, &sign.items, true, &slots);
    }
}

fn validate_typed_schemas(
    language: &Language,
    registry: &OntologyRegistry,
    report: &mut ValidationReport,
) {
    fn slot_feature_read(value: &str) -> Option<(&str, &str)> {
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

    fn category_feature_domain(
        registry: &OntologyRegistry,
        category: &str,
        feature: &str,
    ) -> Option<Vec<String>> {
        if !registry.has(category) {
            return None;
        }
        let synthetic = SignDef {
            id: crate::SignId::synthetic(),
            name: format!("{category}#slot-feature-schema"),
            items: vec![SignItem::Belongs(category.to_owned())],
        };
        registry
            .effective_sign(&synthetic)
            .items
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
                        if shadowed.values != feature.values {
                            report.push(
                                Diagnostic::new(
                                    Severity::Warning,
                                    "FEATURE_DECLARATION_SHADOWED",
                                    format!(
                                        "{owner:?} resolves {}.{} enum({}) over enum({})",
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
                                        previous.constraint,
                                        if previous.optional { "?" } else { "" },
                                        role.constraint,
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
            items: vec![SignItem::Belongs(trait_def.name.clone())],
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
            items: vec![SignItem::Belongs(trait_def.name.clone())],
        };
        (trait_def.name.clone(), registry.effective_sign(&synthetic))
    }));

    for (owner, effective) in candidates {
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
                category_feature_domain(registry, target_category, &binding.feature);
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
                    category_feature_domain(registry, source_category, source_feature);
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
                Some(declaration) if !declaration.values.contains(&value.value) => report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "FEATURE_VALUE_OUT_OF_DOMAIN",
                        format!(
                            "{owner:?} assigns {:?} outside enum({}) for {}.{}",
                            value.value,
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
                ),
                Some(_) => {}
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
        for role in role_declarations.values() {
            if !registry.has(&role.constraint) {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "ROLE_UNKNOWN_CONSTRAINT",
                        format!(
                            "{owner:?} role {:?} requires unknown trait {:?}",
                            role.name, role.constraint
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

        for realization in effective.items.iter().filter_map(|item| match item {
            SignItem::Realization(realization) => Some(realization),
            _ => None,
        }) {
            if realization.branches.is_empty() && realization.expression.is_none() {
                report.push(Diagnostic::new(
                    Severity::Error,
                    "REALIZATION_EMPTY",
                    format!("{owner:?} has an empty realization block"),
                ));
            }
            if let Some(case) = &realization.expression {
                if case.branches.is_empty() {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "CASE_EMPTY",
                        format!("{owner:?} has an empty phon case"),
                    ));
                }
                if let Some(scrutinee) = &case.scrutinee {
                    if let Some((slot, "phon")) = scrutinee.split_once('.') {
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
                    let template = match &branch.result {
                        Expression::PhonTemplate(template) => template,
                        Expression::PhonInterpolation(_) => continue,
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
                            format!("{owner:?} phon case must return a complete `/.../` template"),
                        ));
                        continue;
                    }
                    match template_references(inner.unwrap()) {
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
            let mut saw_else = false;
            for branch in &realization.branches {
                if branch.guard.is_none() {
                    if saw_else {
                        report.push(Diagnostic::new(
                            Severity::Error,
                            "REALIZATION_MULTIPLE_ELSE",
                            format!("{owner:?} has more than one realization else branch"),
                        ));
                    }
                    saw_else = true;
                } else if saw_else {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "REALIZATION_BRANCH_AFTER_ELSE",
                        format!("{owner:?} has a guarded realization after else"),
                    ));
                }
                let inner = branch
                    .template
                    .strip_prefix('/')
                    .and_then(|value| value.strip_suffix('/'));
                if inner.is_none() {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "REALIZATION_INVALID_TEMPLATE",
                        format!("{owner:?} realization must be a complete `/.../` template"),
                    ));
                    continue;
                }
                match template_references(inner.unwrap()) {
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
                if let Some(guard) = &branch.guard {
                    if let Err(error) =
                        synchronic::validate_realization_guard(guard, registry, &slots)
                    {
                        report.push(
                            Diagnostic::new(
                                Severity::Error,
                                "REALIZATION_INVALID_GUARD",
                                format!("{owner:?}: {error}"),
                            )
                            .with_sources(vec![DiagnosticSource {
                                owner: owner.clone(),
                                path: Some("phon.realization".to_owned()),
                                location: branch.source,
                            }]),
                        );
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
            Expression::Projection { value, .. } => applications(value, output),
            Expression::Case(case) => {
                for branch in &case.branches {
                    applications(&branch.result, output);
                }
            }
            _ => {}
        }
    }

    let mut calls = BTreeMap::<String, Vec<String>>::new();
    for trait_def in &language.traits {
        if trait_def
            .blocks
            .iter()
            .flat_map(|block| &block.items)
            .any(|item| matches!(item, SignItem::SignExpression(_)))
        {
            report.push(Diagnostic::new(
                Severity::Error,
                "CASE_SIGN_CONTEXT_REQUIRED",
                format!(
                    "trait {:?} cannot contain a Sign-returning case",
                    trait_def.name
                ),
            ));
        }
    }
    for local in &language.signs {
        let effective = registry.effective_sign(local);
        let local_parameters = construction::parameters_of(&effective);
        let mut cases = effective
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::SignExpression(expression) => match &expression.expression {
                    Expression::Case(case) => Some(case.as_ref()),
                    _ => None,
                },
                SignItem::Realization(realization) => realization.expression.as_ref(),
                _ => None,
            })
            .collect::<Vec<_>>();
        for case in cases.drain(..) {
            let mut saw_else = false;
            for branch in &case.branches {
                if matches!(branch.condition, CaseCondition::Else) {
                    saw_else = true;
                } else if saw_else {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "CASE_BRANCH_AFTER_ELSE",
                        format!("sign {:?} has a case branch after else", local.name),
                    ));
                }
                if !matches!(case.expected, crate::ExpressionType::Sign)
                    && !branch.belongs.is_empty()
                {
                    report.push(Diagnostic::new(
                        Severity::Error,
                        "CASE_BELONGS_TYPE_MISMATCH",
                        format!("sign {:?} uses belongs in a non-Sign case", local.name),
                    ));
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
        references.push(name.to_owned());
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
                let authorized = match constraint {
                    SlotConstraint::AnySign => true,
                    SlotConstraint::Category(required) => {
                        categories.iter().any(|category| category == required)
                    }
                };
                if !authorized {
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
                for branch in &realization.branches {
                    if let Some(inner) = branch
                        .template
                        .strip_prefix('/')
                        .and_then(|value| value.strip_suffix('/'))
                    {
                        if let Ok(references) = template_references(inner) {
                            used_slots.extend(references);
                        }
                    }
                    if let Some(guard) = &branch.guard {
                        used_slots.extend(synchronic::realization_guard_slot_references(guard));
                    }
                }
                if let Some(case) = &realization.expression {
                    if let Some((slot, "phon")) = case
                        .scrutinee
                        .as_deref()
                        .and_then(|value| value.split_once('.'))
                    {
                        used_slots.insert(slot.to_owned());
                    }
                    for branch in &case.branches {
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
                    let slot = operand
                        .strip_prefix("$slot.")
                        .unwrap_or(operand)
                        .split('.')
                        .next()
                        .unwrap_or(operand);
                    if slot_names.contains(&slot) {
                        used_slots.insert(slot.to_owned());
                    } else {
                        report.push(Diagnostic::new(
                            Severity::Error,
                            "CONSTRAINT_UNKNOWN_SLOT",
                            format!(
                                "construction {:?} constraint refers to unknown slot {slot:?}",
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
                    .map(str::trim)
                else {
                    continue;
                };
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
                if let Some((slot, _)) = binding
                    .value
                    .strip_prefix("$slot.")
                    .and_then(|value| value.split_once(".syn."))
                {
                    used_slots.insert(slot.to_owned());
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

fn validate_source_language(std: &Language, effective_source: &Language) -> ValidationReport {
    let (registry, ontology_diags) = OntologyRegistry::build(&[std, effective_source]);
    let mut report = registry.validation_report(&[std, effective_source], &ontology_diags);
    validate_duplicate_signs(effective_source, &mut report);
    validate_defs_and_rules(std, &registry, &mut report);
    validate_defs_and_rules(effective_source, &registry, &mut report);
    validate_typed_schemas(std, &registry, &mut report);
    validate_typed_schemas(effective_source, &registry, &mut report);
    validate_fp_expressions(effective_source, &registry, &mut report);
    validate_origin_graph(effective_source, &mut report);
    validate_constructions_and_local_phon(effective_source, &registry, None, &mut report);
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
    let selection = match library::embedded_catalog().and_then(|catalog| catalog.select(spec)) {
        Ok(selection) => selection,
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
    let std = selection.standard;
    let mut effective_source = selection.overlay;
    effective_source.append_library(language.clone());
    validate_source_language(&std, &effective_source)
}

/// Validate both sidecar/source identity invariants and synchronic language
/// invariants.  The caller document remains immutable.
pub fn check_document(document: &crate::LanguageDocument, spec: &LibrarySpec) -> ValidationReport {
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
        check_language_with_libraries(document.language(), spec)
            .diagnostics()
            .iter()
            .cloned(),
    );
    report
}

/// Exact owned entry point requested by P38–P44. Use `compile_system_ref` when
/// the caller wants to retain its original `Language` without cloning first.
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
    let selection = library::embedded_catalog()?.select(spec)?;
    let std = selection.standard;
    let mut effective_source = selection.overlay;
    effective_source.append_library(language.clone());
    // Validate source-level ontology/rules before the legacy compile pipeline
    // can collapse duplicate names into an unstructured CompileError.
    let pre_validation = validate_source_language(&std, &effective_source);
    if pre_validation.has_errors() {
        return Err(CompileSystemError::Validation(pre_validation));
    }

    let artifacts = codegen::compile_full(&effective_source)?;
    let ordered = artifacts.pipeline.ordered.clone();
    let (registry, ontology_diags) = OntologyRegistry::build(&[&std, &ordered]);
    let mut validation = registry.validation_report(&[&std, &ordered], &ontology_diags);
    validate_duplicate_signs(&ordered, &mut validation);
    validate_defs_and_rules(&ordered, &registry, &mut validation);
    validate_typed_schemas(&ordered, &registry, &mut validation);
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
        libraries: selection.packages,
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
    let report = crate::check_document(document, spec);
    if report.has_errors() {
        return Err(CompileSystemError::Validation(report));
    }
    compile_with_libraries_ref(document.language(), spec)
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
                if system.ontology.has("Semantic")
                    && !system.ontology.category_is_a(category, "Semantic")
                {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "trait {category:?} is not a Semantic type"
                    )));
                }
            }
            let schema_sign = SignDef {
                id: crate::SignId::synthetic(),
                name: "#semantic-document-schema".to_owned(),
                items: node.types.iter().cloned().map(SignItem::Belongs).collect(),
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
                if !child.types.iter().any(|category| {
                    system
                        .ontology
                        .category_is_a(category, &declaration.constraint)
                }) {
                    return Err(SemanticDocumentError::Invalid(format!(
                        "role {name:?} requires [{}]",
                        declaration.constraint
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
        let local = self
            .effective_language
            .sign_named(name)
            .ok_or_else(|| SystemError::UnknownSign(name.to_owned()))?;
        let mut sign = self.ontology.effective_sign(local);
        let mut records = Vec::new();
        for dim in [Dim::Syn, Dim::Sem, Dim::Prag] {
            let (next, pass) = synchronic::run_sign_dim_rules(&sign, dim, &self.ontology);
            sign = next;
            records.extend(pass);
        }
        Ok(SignEvaluation { sign, records })
    }

    pub fn evaluate_sign_with_context(
        &self,
        name: &str,
        context: &DerivationContext,
    ) -> Result<SignEvaluation, SystemError> {
        let local = self
            .effective_language
            .sign_named(name)
            .ok_or_else(|| SystemError::UnknownSign(name.to_owned()))?;
        let mut sign = self.ontology.effective_sign(local);
        for ((dim, feature), value) in context.features() {
            let declaration = sign.items.iter().find_map(|item| match item {
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
        for dim in [Dim::Syn, Dim::Sem, Dim::Prag] {
            let (next, pass) = synchronic::run_sign_dim_rules(&sign, dim, &self.ontology);
            sign = next;
            records.extend(pass);
        }
        Ok(SignEvaluation { sign, records })
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

    fn scalar_from_value<'a>(value: &'a SignValue, path: &str) -> Option<&'a str> {
        match value {
            SignValue::Stored(evaluation) => {
                evaluation
                    .sign
                    .items
                    .iter()
                    .rev()
                    .find_map(|item| match item {
                        SignItem::Def(def) if def.path == path => Some(def.value.as_str()),
                        SignItem::FeatureValue(feature)
                            if format!("{}.{}", feature.dim.keyword(), feature.name) == path =>
                        {
                            Some(feature.value.as_str())
                        }
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
                        .map(|(_, value)| value.as_str()),
                    Dim::Sem => token.sem.features.get(field).map(String::as_str),
                    Dim::Prag => token
                        .prag
                        .iter()
                        .find(|(candidate, _)| candidate == path)
                        .map(|(_, value)| value.as_str()),
                    Dim::Phon => token
                        .phon
                        .iter()
                        .find(|(candidate, _)| candidate == path)
                        .map(|(_, value)| value.as_str()),
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
        let scrutinee = scrutinee
            .trim()
            .strip_prefix("$self.")
            .unwrap_or(scrutinee.trim());
        if let SignValue::Applied(token) = value {
            if let Some((slot, projection)) = scrutinee.split_once('.') {
                if projection == "phon" {
                    let filler = token.fillers.iter().find(|filler| filler.slot == slot);
                    let Some(filler) = filler else {
                        return Ok(false);
                    };
                    if !self.ontology.has(expected) {
                        return Err(SystemError::InvalidSignExpression(format!(
                            "unknown case category {expected:?}"
                        )));
                    }
                    return Ok(filler.categories.iter().any(|category| {
                        category == expected || self.ontology.category_is_a(category, expected)
                    }));
                }
            }
        }
        Ok(Self::scalar_from_value(value, scrutinee) == Some(expected))
    }

    fn apply_sign_application(
        &self,
        current: &SignValue,
        application: &SignApplication,
        stack: &[String],
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
        let mut bindings = Vec::<(String, Option<SignValue>)>::new();
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
                SignArgumentValue::SelfSign => Some(current.clone()),
                SignArgumentValue::Slot(slot) => match current {
                    SignValue::Stored(_) => None,
                    SignValue::Applied(token) => token
                        .bound_fillers()
                        .iter()
                        .find(|(name, _)| name == slot)
                        .map(|(_, filler)| match filler {
                            BoundFiller::Stored(name) => {
                                self.evaluate_sign(name).map(SignValue::Stored)
                            }
                            BoundFiller::Owned(sign) => Ok(SignValue::Stored(SignEvaluation {
                                sign: sign.clone(),
                                records: Vec::new(),
                            })),
                            BoundFiller::Derived(token) => {
                                Ok(SignValue::Applied((**token).clone()))
                            }
                        })
                        .transpose()?,
                },
                SignArgumentValue::Application(nested) => {
                    Some(self.apply_sign_application(current, nested, &next_stack)?)
                }
            };
            bindings.push((name, value));
        }

        let mut runtime = self.effective_language.clone();
        let mut owned_names = Vec::with_capacity(bindings.len());
        let mut owned_signs = Vec::new();
        for (index, (_, value)) in bindings.iter().enumerate() {
            if let Some(SignValue::Stored(evaluation)) = value {
                let mut sign = evaluation.sign.clone();
                sign.name = format!(
                    "#application-{}-{index}-{}",
                    application.callee,
                    current.sign_id()
                );
                owned_names.push(Some(sign.name.clone()));
                runtime.signs.push(sign.clone());
                owned_signs.push(sign);
            } else {
                owned_names.push(None);
            }
        }
        let fillers = bindings
            .iter()
            .zip(&owned_names)
            .filter_map(|((name, value), owned_name)| {
                let filler = match value.as_ref()? {
                    SignValue::Stored(_) => Filler::Sign(owned_name.as_deref()?),
                    SignValue::Applied(token) => Filler::Token(token),
                };
                Some(SlotFiller {
                    slot: name.as_str(),
                    filler,
                })
            })
            .collect::<Vec<_>>();
        let mut token = construction::apply_with(
            &runtime,
            &self.ontology,
            &application.callee,
            &fillers,
            &SlotMap::identity(),
        )?;
        for sign in &owned_signs {
            token.preserve_owned_filler(&sign.name, sign);
        }
        Ok(SignValue::Applied(token))
    }

    fn evaluate_sign_case(
        &self,
        current: SignValue,
        case: &TypedCase,
        records: &mut Vec<CaseRecord>,
        stack: &[String],
    ) -> Result<SignValue, SystemError> {
        for (index, branch) in case.branches.iter().enumerate() {
            let matches = match &branch.condition {
                CaseCondition::Else => true,
                CaseCondition::Guard(guard) => self.case_guard_matches(&current, guard)?,
                CaseCondition::Equals(expected) => self.case_equals_matches(
                    &current,
                    case.scrutinee.as_deref().ok_or_else(|| {
                        SystemError::InvalidSignExpression(
                            "equality case is missing its scrutinee".to_owned(),
                        )
                    })?,
                    expected,
                )?,
            };
            if !matches {
                records.push(CaseRecord {
                    branch: index,
                    status: CaseBranchStatus::Unmatched,
                    source: branch.source,
                    diagnostic_code: None,
                });
                continue;
            }
            let result = match &branch.result {
                Expression::SignApplication(application) => {
                    self.apply_sign_application(&current, application, stack)
                }
                Expression::SelfSign => Ok(current.clone()),
                other => Err(SystemError::InvalidSignExpression(format!(
                    "Sign case returned non-Sign expression {other:?}"
                ))),
            };
            let mut result = match result {
                Ok(value) => value,
                Err(SystemError::Construction(
                    CxgError::CategoryMismatch { .. }
                    | CxgError::ConstraintEqualityConflict { .. }
                    | CxgError::ConstraintOrderConflict { .. },
                )) => {
                    records.push(CaseRecord {
                        branch: index,
                        status: CaseBranchStatus::MoreSpecificBlocked,
                        source: branch.source,
                        diagnostic_code: Some("CASE_MORE_SPECIFIC_BLOCKED"),
                    });
                    continue;
                }
                Err(error) => return Err(error),
            };
            for category in &branch.belongs {
                if !self.ontology.has(category) {
                    return Err(SystemError::InvalidSignExpression(format!(
                        "unknown branch membership {category:?}"
                    )));
                }
                match &mut result {
                    SignValue::Stored(evaluation) => {
                        if !evaluation.sign.items.iter().any(
                            |item| matches!(item, SignItem::Belongs(value) if value == category),
                        ) {
                            evaluation
                                .sign
                                .items
                                .push(SignItem::Belongs(category.clone()));
                        }
                    }
                    SignValue::Applied(token) => {
                        token.add_membership(category.clone(), &self.ontology)
                    }
                }
            }
            records.push(CaseRecord {
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
        let evaluated = self.evaluate_sign_with_context(name, context)?;
        let expressions = evaluated
            .sign
            .items
            .iter()
            .filter_map(|item| match item {
                SignItem::SignExpression(expression) => Some(expression.expression.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut value = SignValue::Stored(evaluated);
        let mut cases = Vec::new();
        let stack = vec![name.to_owned()];
        for expression in expressions {
            let Expression::Case(case) = expression else {
                return Err(SystemError::InvalidSignExpression(
                    "Sign body expression must return Sign".to_owned(),
                ));
            };
            value = self.evaluate_sign_case(value, &case, &mut cases, &stack)?;
        }
        Ok(SignExpressionEvaluation { value, cases })
    }

    fn prepared_fillers<'a>(
        &self,
        construction: &str,
        fillers: &[SlotFiller<'a>],
        mapping: &SlotMap,
    ) -> Result<(Language, Vec<UnitRuleRecord>), SystemError> {
        let mut runtime = self.effective_language.clone();
        let mut names: Vec<&str> = fillers
            .iter()
            .filter_map(|fill| match fill.filler {
                Filler::Sign(name) => Some(name),
                Filler::Token(_) => None,
            })
            .collect();
        let local = self
            .effective_language
            .sign_named(construction)
            .ok_or_else(|| CxgError::UnknownConstruction(construction.to_owned()))?;
        let effective = self.ontology.effective_sign(local);
        let effective_mapping = construction::slot_map_of(&effective).and(mapping);
        for operation in effective_mapping.ops() {
            if let SlotMapOp::AutoFill { filler, .. } = operation {
                names.push(filler);
            }
        }
        names.sort_unstable();
        names.dedup();
        let mut trace = Vec::new();
        for name in names {
            let evaluated = self.evaluate_sign(name)?;
            let Some(slot) = runtime.signs.iter_mut().find(|sign| sign.name == name) else {
                return Err(SystemError::UnknownSign(name.to_owned()));
            };
            *slot = evaluated.sign;
            trace.extend(evaluated.records.into_iter().map(|record| UnitRuleRecord {
                unit: name.to_owned(),
                record,
            }));
        }
        Ok((runtime, trace))
    }

    pub fn apply_construction<'a>(
        &self,
        construction: &str,
        fillers: &[SlotFiller<'a>],
        mapping: &SlotMap,
    ) -> Result<DerivedToken, SystemError> {
        let _ = self.prepared_fillers(construction, fillers, mapping)?;
        construction::apply_with(
            &self.effective_language,
            &self.ontology,
            construction,
            fillers,
            mapping,
        )
        .map_err(SystemError::from)
    }

    /// Supply additional arguments to the same Sign function.  Previously
    /// bound occurrences are replayed from immutable owned values; the
    /// original PartialSign is never modified.
    pub fn resume_partial<'a>(
        &self,
        partial: &PartialSign,
        additional: &[SlotFiller<'a>],
    ) -> Result<SignValue, SystemError> {
        let mut runtime = self.effective_language.clone();
        for (_, filler) in partial.token.bound_fillers() {
            if let BoundFiller::Owned(sign) = filler {
                runtime.signs.push(sign.clone());
            }
        }
        let mut fillers = partial
            .token
            .bound_fillers()
            .iter()
            .map(|(slot, filler)| SlotFiller {
                slot: slot.as_str(),
                filler: match filler {
                    BoundFiller::Stored(name) => Filler::Sign(name.as_str()),
                    BoundFiller::Owned(sign) => Filler::Sign(sign.name.as_str()),
                    BoundFiller::Derived(token) => Filler::Token(token),
                },
            })
            .collect::<Vec<_>>();
        fillers.extend(additional.iter().copied());
        let token = construction::apply_with(
            &runtime,
            &self.ontology,
            &partial.token.construction,
            &fillers,
            partial.token.invocation_mapping(),
        )?;
        Ok(SignValue::Applied(token))
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
        let (token, records) = self.run_token_rules(token);
        Self::check_context(&token, &merged)?;
        Ok((
            token,
            records.into_iter().map(|record| record.record).collect(),
        ))
    }

    fn run_token_rules(&self, mut token: DerivedToken) -> (DerivedToken, Vec<UnitRuleRecord>) {
        let mut trace = Vec::new();
        for dim in [Dim::Syn, Dim::Sem, Dim::Prag] {
            let (next, records) = synchronic::run_token_dim_rules(&token, dim, &self.ontology);
            token = next;
            trace.extend(records.into_iter().map(|record| UnitRuleRecord {
                unit: token.construction.clone(),
                record,
            }));
        }
        (token, trace)
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
            let declaration = token.rule_sign.items.iter().find_map(|item| match item {
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

    fn check_context(token: &DerivedToken, context: &DerivationContext) -> Result<(), SystemError> {
        for ((dim, name), expected) in context.features() {
            if let Some(actual) = Self::token_feature(token, *dim, name) {
                if actual != expected {
                    return Err(SystemError::DerivationFeatureConflict {
                        dim: *dim,
                        name: name.clone(),
                        expected: expected.clone(),
                        actual: actual.to_owned(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn realize_phon(&self, token: &DerivedToken) -> Result<PhonRealization, SystemError> {
        let realization = token.rule_sign.items.iter().find_map(|item| match item {
            SignItem::Realization(realization) => Some(realization),
            _ => None,
        });
        let mut selected = None;
        let mut selected_expression = None;
        let mut slot_reads = Vec::new();
        let mut self_reads = Vec::new();
        if let Some(realization) = realization {
            if let Some(case) = &realization.expression {
                let value = SignValue::Applied(token.clone());
                for (index, branch) in case.branches.iter().enumerate() {
                    let matches = match &branch.condition {
                        CaseCondition::Else => true,
                        CaseCondition::Guard(guard) => self.case_guard_matches(&value, guard)?,
                        CaseCondition::Equals(expected) => self.case_equals_matches(
                            &value,
                            case.scrutinee.as_deref().ok_or_else(|| {
                                SystemError::InvalidSignExpression(
                                    "phon case equality is missing its scrutinee".to_owned(),
                                )
                            })?,
                            expected,
                        )?,
                    };
                    if matches {
                        selected_expression = Some((index, branch));
                        break;
                    }
                }
            }
            for (index, branch) in
                realization
                    .branches
                    .iter()
                    .enumerate()
                    .take(if selected_expression.is_none() {
                        usize::MAX
                    } else {
                        0
                    })
            {
                if let Some(guard) = &branch.guard {
                    let (status, slots, self_values, error) =
                        synchronic::evaluate_token_guard(token, guard, &self.ontology);
                    slot_reads.extend(slots);
                    self_reads.extend(self_values);
                    match status {
                        RuleStatus::Matched => {
                            selected = Some((index, branch));
                            break;
                        }
                        RuleStatus::Unmatched => continue,
                        RuleStatus::Error => {
                            return Err(SystemError::RealizationGuard(
                                error.unwrap_or_else(|| {
                                    "unknown realization guard error".to_owned()
                                }),
                            ));
                        }
                    }
                } else {
                    selected = Some((index, branch));
                    break;
                }
            }
        }
        let (input, branch, source) = if let Some((index, selected)) = selected_expression {
            let input = match &selected.result {
                Expression::PhonTemplate(template) => token.expand_phon_template(template)?,
                Expression::PhonInterpolation(application) => {
                    // `$self` inside its own phon projection denotes the
                    // already-finalized deep Sign, not a request to enter the
                    // same realization expression recursively.
                    let current = SignValue::Applied(token.as_phon_projection_base());
                    let nested = self.apply_sign_application(
                        &current,
                        application,
                        std::slice::from_ref(&token.construction),
                    )?;
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
                    self.realize_phon(&nested)?.input.into_inner()
                }
                other => {
                    return Err(SystemError::InvalidSignExpression(format!(
                        "phon case returned non-Phon expression {other:?}"
                    )))
                }
            };
            (input, Some(index), selected.source)
        } else if let Some((index, selected)) = selected {
            (
                token.expand_phon_template(&selected.template)?,
                Some(index),
                selected.source,
            )
        } else {
            (token.phon_form()?, None, SourceLocation::unknown())
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
                    .sign_categories(&effective)
                    .iter()
                    .any(|item| item == category || self.ontology.category_is_a(item, category))
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
            let Ok(token) = applied else {
                // Slot/category/constraint incompatibility removes a
                // candidate; it never grants another candidate an implicit
                // priority win.
                continue;
            };
            if self.apply_context(token, context).is_err() {
                continue;
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
                let total: f64 = candidates
                    .candidates
                    .iter()
                    .map(|candidate| candidate.entrenchment)
                    .sum();
                if total <= 0.0 {
                    return Err(SystemError::ZeroCandidateWeight);
                }
                // SplitMix64 is small, stable and entirely specified here;
                // the exact draw is therefore replayable across platforms.
                let mut state = seed.wrapping_add(0x9e3779b97f4a7c15);
                state = (state ^ (state >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                state = (state ^ (state >> 27)).wrapping_mul(0x94d049bb133111eb);
                state ^= state >> 31;
                let unit = (state >> 11) as f64 / ((1u64 << 53) as f64);
                let mut draw = unit * total;
                candidates
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.entrenchment > 0.0)
                    .find(|candidate| {
                        if draw < candidate.entrenchment {
                            true
                        } else {
                            draw -= candidate.entrenchment;
                            false
                        }
                    })
                    .or_else(|| {
                        candidates
                            .candidates
                            .iter()
                            .rev()
                            .find(|candidate| candidate.entrenchment > 0.0)
                    })
                    .expect("positive total guarantees a candidate")
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
        if candidates.candidates.len() != 1 {
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
        let _ = self.prepared_fillers(construction, fillers, mapping)?;
        let mut token = construction::apply_with(
            &self.effective_language,
            &self.ontology,
            construction,
            fillers,
            mapping,
        )?;
        let occurrences = token.take_occurrence_records();
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
        let (mut token, token_records) = self.run_token_rules(token);
        rules.extend(token_records);
        Self::check_context(&token, &context)?;

        let program = self.phon_program(&token)?;
        let realization = self.realize_phon(&token)?;
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
        })
    }
}
