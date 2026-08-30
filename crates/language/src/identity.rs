//! Persistent source identity for Step 13.
//!
//! `.lang` remains the linguistic source.  A versioned sidecar binds every
//! editable source node and reference occurrence to a stable identity.  The
//! binding is only accepted for the exact canonical source digest: this
//! module never guesses identity from a changed name or vector position.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{metadata, Language, Realization, RuleId, SignId, SignItem, SlotConstraint};

pub const IDENTITY_SCHEMA_V2: &str = "conlang.language-identities/v2";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum IdentityNamespace {
    Ephemeral,
    Document(String),
    Library(String),
    Synthetic,
}

impl fmt::Display for IdentityNamespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentityNamespace::Ephemeral => formatter.write_str("local"),
            IdentityNamespace::Document(value) | IdentityNamespace::Library(value) => {
                formatter.write_str(value)
            }
            IdentityNamespace::Synthetic => formatter.write_str("synthetic"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeId {
    pub namespace: IdentityNamespace,
    pub ordinal: u64,
}

impl NodeId {
    pub const fn new(namespace: IdentityNamespace, ordinal: u64) -> NodeId {
        NodeId { namespace, ordinal }
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.namespace, self.ordinal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Language,
    DslDeclaration,
    Distribution,
    Trait,
    Sign,
    Block,
    TraitUse,
    Belongs,
    Slot,
    SlotMap,
    FeatureDeclaration,
    FeatureValue,
    SlotFeatureBinding,
    RoleDeclaration,
    RoleBinding,
    /// 義項節點(sem 維一級節點,《修補05》§10.3「sem(senses + 衍生邊)」)。
    Sense,
    /// 義項間的衍生邊(同上)。
    SenseEdge,
    FeatureRule,
    Definition,
    Rule,
    RuleElseBranch,
    RuleThenBranch,
    /// A single statement line inside a phon `PhonBlock::Leaf` (P46 S3).
    PhonStatement,
    /// One element of a phon `PhonBlock::Then`/`Else` vec — itself a recursive
    /// `PhonBlock` (P46 S3).
    PhonBlockNode,
    Application,
    Case,
    CaseBranch,
    Constraint,
    /// `pass`:故意留白的塊標記。
    Pass,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRef {
    pub id: NodeId,
    pub expected: NodeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EditableField {
    Name,
    Global,
    Text,
    DistributionKey,
    DistributionValue,
    DefinitionPath,
    DefinitionValue,
    RuleBody,
    RuleStage,
    RuleDimension,
    /// P46 S4: rule-level (`name propagate:`) or block-element
    /// (`Then propagate:`) fixpoint iteration.
    Propagate,
    BranchBody,
    SlotName,
    SlotConstraint,
    Optional,
    TraitUseName,
    TraitUseBlock,
    BelongsTarget,
    FeatureDomain,
    FeatureValue,
    SlotFeatureValue,
    SlotMap,
    RoleConstraint,
    RoleSlot,
    /// §10.3 義項/衍生邊的可編輯欄位(Atomic Rewrite drift / lexicalize_sense)。
    SenseGloss,
    SenseEdgeKind,
    SenseEdgeTransparency,
    CaseSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub node: NodeRef,
    pub parent: Option<NodeRef>,
    pub address: NodeAddress,
    pub field: Option<EditableField>,
}

impl NodeRef {
    pub fn new(id: NodeId, expected: NodeKind) -> NodeRef {
        NodeRef { id, expected }
    }
}

/// Generic node wrapper used by detached/edit-facing source models.  The
/// existing synchronic AST keeps its compatibility layout; `LanguageDocument`
/// supplies the corresponding wrappers through its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstNode<T> {
    pub id: NodeId,
    pub payload: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "collection", content = "index", rename_all = "snake_case")]
pub enum AddressSegment {
    DslDeclarations(usize),
    Distribution(usize),
    Traits(usize),
    Signs(usize),
    Blocks(usize),
    Items(usize),
    RuleElse(usize),
    RuleThen(usize),
    /// Index into a phon `PhonBlock::Leaf` statement list (P46 S3).
    PhonLeaf(usize),
    /// Index into a phon `PhonBlock::Then` element vec (P46 S3).
    PhonThen(usize),
    /// Index into a phon `PhonBlock::Else` element vec (P46 S3).
    PhonElse(usize),
    CaseExpression,
    CaseBranches(usize),
    CaseResult,
    ApplicationArguments(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct NodeAddress(pub Vec<AddressSegment>);

impl NodeAddress {
    pub fn root() -> NodeAddress {
        NodeAddress::default()
    }

    pub fn child(&self, segment: AddressSegment) -> NodeAddress {
        let mut path = self.0.clone();
        path.push(segment);
        NodeAddress(path)
    }

    pub fn parent(&self) -> Option<NodeAddress> {
        (!self.0.is_empty()).then(|| {
            let mut path = self.0.clone();
            path.pop();
            NodeAddress(path)
        })
    }

    pub fn starts_with(&self, prefix: &NodeAddress) -> bool {
        self.0.starts_with(&prefix.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeEntryV1 {
    pub id: NodeId,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub address: NodeAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum RefTargetV1 {
    Local {
        target: NodeRef,
    },
    External {
        spelling: String,
        expected: NodeKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefBindingV1 {
    pub owner: NodeId,
    pub field: String,
    pub target: RefTargetV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityAllocatorV2 {
    pub namespace: String,
    pub next_ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityManifestV2 {
    pub schema: String,
    pub root_namespace: String,
    pub active_namespace: String,
    pub allocators: Vec<IdentityAllocatorV2>,
    pub source_sha256: String,
    pub nodes: Vec<NodeEntryV1>,
    pub refs: Vec<RefBindingV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    #[error("IDENTITY_NAMESPACE_INVALID: document namespace must be non-empty and stable")]
    InvalidNamespace,
    #[error("IDENTITY_MANIFEST_INVALID: {0}")]
    InvalidManifest(String),
    #[error("IDENTITY_SCHEMA_UNKNOWN: {0}")]
    UnknownSchema(String),
    #[error("IDENTITY_SOURCE_MISMATCH: sidecar digest does not match canonical .lang source")]
    SourceMismatch,
    #[error("IDENTITY_SHAPE_MISMATCH: {0}")]
    ShapeMismatch(String),
    #[error("IDENTITY_PARSE_ERROR: {0}")]
    Parse(String),
    #[error("IDENTITY_SERIALIZE_ERROR: {0}")]
    Serialize(String),
    #[error("IDENTITY_RESOLVE_ERROR: {0}")]
    Resolve(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageDocument {
    language: Language,
    identities: IdentityManifestV2,
}

impl LanguageDocument {
    /// Import externally edited source as a new historical root.  All caller
    /// identities are allocated in the supplied deterministic namespace.
    pub fn import_new_root(
        source: &str,
        namespace: impl Into<String>,
    ) -> Result<LanguageDocument, IdentityError> {
        let namespace = namespace.into();
        validate_namespace(&namespace)?;
        let parsed =
            Language::parse(source).map_err(|error| IdentityError::Parse(error.to_string()))?;
        let canonical_source = parsed.dump();
        let mut language = Language::parse(&canonical_source)
            .map_err(|error| IdentityError::Parse(error.to_string()))?;
        let mut next = 0;
        let nodes = enumerate_nodes(&language, &namespace, &mut next);
        bind_runtime_ids(&mut language, &nodes)?;
        let refs = collect_refs(&language, &nodes);
        let identities = IdentityManifestV2 {
            schema: IDENTITY_SCHEMA_V2.to_owned(),
            root_namespace: namespace.clone(),
            active_namespace: namespace.clone(),
            allocators: vec![IdentityAllocatorV2 {
                namespace,
                next_ordinal: next,
            }],
            source_sha256: sha256_hex(canonical_source.as_bytes()),
            nodes,
            refs,
        };
        Ok(LanguageDocument {
            language,
            identities,
        })
    }

    /// Reopen an exact canonical source/sidecar pair.  No recovery or fuzzy
    /// identity matching is attempted on mismatch.
    pub fn open(source: &str, manifest_json: &str) -> Result<LanguageDocument, IdentityError> {
        let envelope: serde_json::Value = serde_json::from_str(manifest_json)
            .map_err(|error| IdentityError::InvalidManifest(error.to_string()))?;
        let schema = envelope
            .get("schema")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| IdentityError::InvalidManifest("missing schema".to_owned()))?;
        // v1 identity 已淘汰(硬移除 2026-07-24):只接受 v2 sidecar;v1/未知 → UnknownSchema。
        let identities: IdentityManifestV2 = match schema {
            IDENTITY_SCHEMA_V2 => serde_json::from_value(envelope)
                .map_err(|error| IdentityError::InvalidManifest(error.to_string()))?,
            other => return Err(IdentityError::UnknownSchema(other.to_owned())),
        };
        validate_manifest_namespaces(&identities)?;
        let parsed =
            Language::parse(source).map_err(|error| IdentityError::Parse(error.to_string()))?;
        let canonical_source = parsed.dump();
        if sha256_hex(canonical_source.as_bytes()) != identities.source_sha256 {
            return Err(IdentityError::SourceMismatch);
        }
        let mut language = Language::parse(&canonical_source)
            .map_err(|error| IdentityError::Parse(error.to_string()))?;
        validate_shape(&language, &identities)?;
        bind_runtime_ids(&mut language, &identities.nodes)?;
        let actual_refs = collect_refs(&language, &identities.nodes);
        if actual_refs != identities.refs {
            return Err(IdentityError::ShapeMismatch(
                "reference bindings differ from the sidecar".to_owned(),
            ));
        }
        Ok(LanguageDocument {
            language,
            identities,
        })
    }

    pub fn language(&self) -> &Language {
        &self.language
    }

    pub fn identities(&self) -> &IdentityManifestV2 {
        &self.identities
    }

    pub fn owns(&self, id: &NodeId) -> bool {
        let IdentityNamespace::Document(namespace) = &id.namespace else {
            return false;
        };
        self.identities
            .allocators
            .iter()
            .any(|allocator| &allocator.namespace == namespace)
    }

    pub fn fork(&self, namespace: impl Into<String>) -> Result<LanguageDocument, IdentityError> {
        let namespace = namespace.into();
        validate_namespace(&namespace)?;
        if self
            .identities
            .allocators
            .iter()
            .any(|allocator| allocator.namespace == namespace)
        {
            return Err(IdentityError::InvalidManifest(format!(
                "duplicate allocator namespace {namespace}"
            )));
        }
        let mut fork = self.clone();
        fork.identities.active_namespace = namespace.clone();
        fork.identities.allocators.push(IdentityAllocatorV2 {
            namespace,
            next_ordinal: 0,
        });
        fork.identities
            .allocators
            .sort_by(|left, right| left.namespace.cmp(&right.namespace));
        Ok(fork)
    }

    pub fn source(&self) -> String {
        self.language.dump()
    }

    pub fn manifest_json(&self) -> Result<String, IdentityError> {
        let mut json = serde_json::to_string_pretty(&self.identities)
            .map_err(|error| IdentityError::Serialize(error.to_string()))?;
        json.push('\n');
        Ok(json)
    }

    pub fn dump_pair(&self) -> Result<(String, String), IdentityError> {
        Ok((self.source(), self.manifest_json()?))
    }

    pub fn node(&self, reference: &NodeRef) -> Option<&NodeEntryV1> {
        self.identities.nodes.iter().find(|entry| {
            entry.id == reference.id && Self::kinds_interchangeable(entry.kind, reference.expected)
        })
    }

    /// `Rule` 與 `FeatureRule` 在定址上是**同一種**。
    ///
    /// `.chg` 的 `kind_keyword`/`parse_kind` 兩表刻意讓兩者共用關鍵字 `"rule"`,
    /// `rule[…]` selector 也同時接受兩者。若此處仍用嚴格相等,一個 `FeatureRule`
    /// 節點印成 `rule` 後就**讀不回自己**(印得出、解不開)。P71 §4.3 把自造欄位
    /// 遷入 `feature:` 後,大量既有規則變成 `FeatureRule`,這個不一致才顯形。
    fn kinds_interchangeable(actual: NodeKind, expected: NodeKind) -> bool {
        actual == expected
            || matches!(
                (actual, expected),
                (NodeKind::Rule, NodeKind::FeatureRule) | (NodeKind::FeatureRule, NodeKind::Rule)
            )
    }

    pub fn node_at(&self, address: &NodeAddress) -> Option<&NodeEntryV1> {
        self.identities
            .nodes
            .iter()
            .find(|entry| &entry.address == address)
    }

    pub fn resolve_node(&self, reference: &NodeRef) -> Result<ResolvedTarget, IdentityError> {
        let entry = self.node(reference).ok_or_else(|| {
            IdentityError::Resolve(format!(
                "unknown {:?} node {}",
                reference.expected, reference.id
            ))
        })?;
        let parent = entry.parent.as_ref().and_then(|id| {
            self.identities
                .nodes
                .iter()
                .find(|candidate| &candidate.id == id)
                .map(|candidate| NodeRef::new(candidate.id.clone(), candidate.kind))
        });
        Ok(ResolvedTarget {
            node: reference.clone(),
            parent,
            address: entry.address.clone(),
            field: None,
        })
    }

    /// Resolve a typed field path relative to an already-stable node anchor.
    /// Step 13 deliberately has no textual `.chg` target syntax; the existing
    /// `Path` parser supplies the field segments and this method supplies the
    /// source-aware type check.
    pub fn resolve_path(
        &self,
        anchor: &NodeRef,
        path: &crate::path::Path,
    ) -> Result<ResolvedTarget, IdentityError> {
        let mut resolved = self.resolve_node(anchor)?;
        // P92 之後 Path 只有具名段,故不再需要過濾非 Name 段。
        let names: Vec<_> = path
            .0
            .iter()
            .map(|crate::path::PathSeg::Name(name)| name.as_str())
            .collect();
        if names.len() != 1 {
            return Err(IdentityError::Resolve(
                "Step 13 field paths must identify exactly one atomic field".to_owned(),
            ));
        }
        let field = editable_field(anchor.expected, names[0]).ok_or_else(|| {
            IdentityError::Resolve(format!(
                "field {:?} is not editable on {:?}",
                names[0], anchor.expected
            ))
        })?;
        resolved.field = Some(field);
        Ok(resolved)
    }

    pub fn root_ref(&self) -> NodeRef {
        let root = self
            .identities
            .nodes
            .iter()
            .find(|entry| entry.kind == NodeKind::Language)
            .expect("validated identity manifest has a root");
        NodeRef::new(root.id.clone(), NodeKind::Language)
    }

    pub fn ref_for_sign(&self, name: &str) -> Option<NodeRef> {
        let index = self
            .language
            .signs
            .iter()
            .position(|sign| sign.name == name)?;
        self.node_at(&NodeAddress(vec![AddressSegment::Signs(index)]))
            .map(|entry| NodeRef::new(entry.id.clone(), NodeKind::Sign))
    }

    pub fn ref_for_trait(&self, name: &str) -> Option<NodeRef> {
        let index = self
            .language
            .traits
            .iter()
            .position(|item| item.name == name)?;
        self.node_at(&NodeAddress(vec![AddressSegment::Traits(index)]))
            .map(|entry| NodeRef::new(entry.id.clone(), NodeKind::Trait))
    }

    /// Controlled bridge for the edit crate.  Reconstitution validates every
    /// address/kind binding and refreshes reference bindings and source hash.
    #[doc(hidden)]
    pub fn into_edit_parts(self) -> (Language, IdentityManifestV2) {
        (self.language, self.identities)
    }

    #[doc(hidden)]
    pub fn from_edit_parts(
        mut language: Language,
        mut identities: IdentityManifestV2,
    ) -> Result<LanguageDocument, IdentityError> {
        canonicalize_named_containers(&mut language, &mut identities);
        validate_shape(&language, &identities)?;
        bind_runtime_ids(&mut language, &identities.nodes)?;
        let previous_refs: BTreeMap<_, _> = identities
            .refs
            .iter()
            .map(|binding| {
                (
                    (binding.owner.clone(), binding.field.clone()),
                    binding.target.clone(),
                )
            })
            .collect();
        identities.refs = collect_refs(&language, &identities.nodes)
            .into_iter()
            .map(|mut binding| {
                if matches!(binding.target, RefTargetV1::External { .. }) {
                    if let Some(RefTargetV1::Local { target }) =
                        previous_refs.get(&(binding.owner.clone(), binding.field.clone()))
                    {
                        binding.target = RefTargetV1::Local {
                            target: target.clone(),
                        };
                    }
                }
                binding
            })
            .collect();
        identities.source_sha256 = sha256_hex(language.dump().as_bytes());
        identities
            .nodes
            .sort_by(|left, right| left.id.cmp(&right.id));
        identities
            .refs
            .sort_by(|left, right| (&left.owner, &left.field).cmp(&(&right.owner, &right.field)));
        Ok(LanguageDocument {
            language,
            identities,
        })
    }
}

/// The canonical printer orders named top-level containers.  A rename can
/// therefore move a Trait or Sign in canonical source even though its stable
/// identity did not move semantically.  Keep the in-memory AST and every
/// descendant address in lockstep before validating or persisting the edit.
fn canonicalize_named_containers(language: &mut Language, identities: &mut IdentityManifestV2) {
    // `Language::dump()` sorts distribution entries.  Keep the editable AST
    // and its stable addresses in the same order before hashing or reopening;
    // otherwise an update that changes a key can silently bind two existing
    // NodeIds to each other's entries after the canonical source is reparsed.
    let mut distribution_order = (0..language.distribution.len()).collect::<Vec<_>>();
    distribution_order.sort_by(|left, right| {
        (&language.distribution[*left], *left).cmp(&(&language.distribution[*right], *right))
    });
    let mut distribution_new_index = vec![0; distribution_order.len()];
    for (new_index, old_index) in distribution_order.iter().copied().enumerate() {
        distribution_new_index[old_index] = new_index;
    }
    if distribution_order
        .iter()
        .copied()
        .enumerate()
        .any(|(new, old)| new != old)
    {
        let old = language.distribution.clone();
        language.distribution = distribution_order
            .iter()
            .map(|index| old[*index].clone())
            .collect();
    }

    let mut trait_order = (0..language.traits.len()).collect::<Vec<_>>();
    trait_order.sort_by(|left, right| {
        let left_trait = &language.traits[*left];
        let right_trait = &language.traits[*right];
        (!left_trait.global, &left_trait.name, *left).cmp(&(
            !right_trait.global,
            &right_trait.name,
            *right,
        ))
    });
    let mut trait_new_index = vec![0; trait_order.len()];
    for (new_index, old_index) in trait_order.iter().copied().enumerate() {
        trait_new_index[old_index] = new_index;
    }
    if trait_order
        .iter()
        .copied()
        .enumerate()
        .any(|(new, old)| new != old)
    {
        let old = language.traits.clone();
        language.traits = trait_order
            .iter()
            .map(|index| old[*index].clone())
            .collect();
    }

    let mut sign_order = (0..language.signs.len()).collect::<Vec<_>>();
    sign_order.sort_by(|left, right| {
        let left_sign = &language.signs[*left];
        let right_sign = &language.signs[*right];
        (&left_sign.name, *left).cmp(&(&right_sign.name, *right))
    });
    let mut sign_new_index = vec![0; sign_order.len()];
    for (new_index, old_index) in sign_order.iter().copied().enumerate() {
        sign_new_index[old_index] = new_index;
    }
    if sign_order
        .iter()
        .copied()
        .enumerate()
        .any(|(new, old)| new != old)
    {
        let old = language.signs.clone();
        language.signs = sign_order.iter().map(|index| old[*index].clone()).collect();
    }

    for entry in &mut identities.nodes {
        let Some(first) = entry.address.0.first_mut() else {
            continue;
        };
        match first {
            AddressSegment::Distribution(index) if *index < distribution_new_index.len() => {
                *index = distribution_new_index[*index];
            }
            AddressSegment::Traits(index) if *index < trait_new_index.len() => {
                *index = trait_new_index[*index];
            }
            AddressSegment::Signs(index) if *index < sign_new_index.len() => {
                *index = sign_new_index[*index];
            }
            _ => {}
        }
    }
}

fn editable_field(kind: NodeKind, name: &str) -> Option<EditableField> {
    match (kind, name) {
        (NodeKind::Trait | NodeKind::Sign, "name") => Some(EditableField::Name),
        (NodeKind::Trait, "global") => Some(EditableField::Global),
        (NodeKind::DslDeclaration, "text") => Some(EditableField::Text),
        (NodeKind::Distribution, "key") => Some(EditableField::DistributionKey),
        (NodeKind::Distribution, "value") => Some(EditableField::DistributionValue),
        (NodeKind::Definition, "path") => Some(EditableField::DefinitionPath),
        (NodeKind::Definition, "value") => Some(EditableField::DefinitionValue),
        (NodeKind::Rule | NodeKind::FeatureRule, "body") => Some(EditableField::RuleBody),
        (NodeKind::Rule | NodeKind::FeatureRule, "stage") => Some(EditableField::RuleStage),
        (NodeKind::Rule | NodeKind::FeatureRule, "dimension") => Some(EditableField::RuleDimension),
        (NodeKind::RuleElseBranch | NodeKind::RuleThenBranch, "body") => {
            Some(EditableField::BranchBody)
        }
        (NodeKind::PhonStatement, "body") => Some(EditableField::BranchBody),
        (NodeKind::Sense, "gloss") => Some(EditableField::SenseGloss),
        (NodeKind::SenseEdge, "kind") => Some(EditableField::SenseEdgeKind),
        (NodeKind::SenseEdge, "transparency") => Some(EditableField::SenseEdgeTransparency),
        (NodeKind::Rule | NodeKind::FeatureRule | NodeKind::PhonBlockNode, "propagate") => {
            Some(EditableField::Propagate)
        }
        (NodeKind::Slot, "name") => Some(EditableField::SlotName),
        (NodeKind::Slot, "constraint") => Some(EditableField::SlotConstraint),
        (NodeKind::Slot | NodeKind::RoleDeclaration, "optional") => Some(EditableField::Optional),
        (NodeKind::TraitUse, "name") => Some(EditableField::TraitUseName),
        (NodeKind::TraitUse, "block") => Some(EditableField::TraitUseBlock),
        (NodeKind::Belongs, "target") => Some(EditableField::BelongsTarget),
        (NodeKind::FeatureDeclaration, "domain") => Some(EditableField::FeatureDomain),
        (NodeKind::FeatureValue, "value") => Some(EditableField::FeatureValue),
        (NodeKind::SlotFeatureBinding, "value") => Some(EditableField::SlotFeatureValue),
        (NodeKind::SlotMap, "operation") => Some(EditableField::SlotMap),
        (NodeKind::RoleDeclaration, "constraint") => Some(EditableField::RoleConstraint),
        (NodeKind::RoleBinding, "slot") => Some(EditableField::RoleSlot),
        (NodeKind::Case, "selection") => Some(EditableField::CaseSelection),
        _ => None,
    }
}

fn validate_namespace(namespace: &str) -> Result<(), IdentityError> {
    if namespace.is_empty()
        || !namespace
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, ':' | '-' | '_' | '/' | '.'))
    {
        return Err(IdentityError::InvalidNamespace);
    }
    Ok(())
}

fn validate_manifest_namespaces(manifest: &IdentityManifestV2) -> Result<(), IdentityError> {
    validate_namespace(&manifest.root_namespace)?;
    validate_namespace(&manifest.active_namespace)?;
    let mut seen = std::collections::BTreeSet::new();
    for allocator in &manifest.allocators {
        validate_namespace(&allocator.namespace)?;
        if !seen.insert(allocator.namespace.as_str()) {
            return Err(IdentityError::InvalidManifest(format!(
                "duplicate allocator namespace {}",
                allocator.namespace
            )));
        }
    }
    if !seen.contains(manifest.root_namespace.as_str()) {
        return Err(IdentityError::InvalidManifest(
            "root namespace has no allocator".to_owned(),
        ));
    }
    if !seen.contains(manifest.active_namespace.as_str()) {
        return Err(IdentityError::InvalidManifest(
            "active namespace has no allocator".to_owned(),
        ));
    }
    Ok(())
}

fn item_kind(item: &SignItem) -> NodeKind {
    match item {
        SignItem::Pass => NodeKind::Pass,
        SignItem::TraitMount {
            kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_),
            ..
        } => NodeKind::TraitUse,
        SignItem::TraitMount {
            name: _,
            kind: crate::TraitMountKind::Declaration,
            ..
        } => NodeKind::Belongs,
        SignItem::Slot(_) => NodeKind::Slot,
        SignItem::SlotMap(_) => NodeKind::SlotMap,
        SignItem::FeatureDecl(_) => NodeKind::FeatureDeclaration,
        SignItem::FeatureValue(_) => NodeKind::FeatureValue,
        SignItem::SlotFeatureBinding(_) => NodeKind::SlotFeatureBinding,
        SignItem::RoleDecl(_) => NodeKind::RoleDeclaration,
        SignItem::RoleBinding(_) => NodeKind::RoleBinding,
        SignItem::Sense(_) => NodeKind::Sense,
        SignItem::SenseEdge(_) => NodeKind::SenseEdge,
        // 取徑 A:比照另外三個帶運算式的 SignItem **塌陷成自己的 expression root**。
        // 先前它是唯一保留 wrapper kind 的一個(V1 扁平 `RealizationBranch` 清單的遺留),
        // 於是它的 `Case` 成了 sign 的**孫**節點;而 `resolve_path_child` 只取直屬子節點、
        // 又沒有 `"realization"` 這一支可下降——`case["X"]` 因此永遠解不到,
        // realization 內的 `@name` 成了「印得出來卻到不了」的語法。
        SignItem::Realization(_) => NodeKind::Case,
        SignItem::SignExpression(expression) => expression_root_kind(&expression.expression),
        SignItem::FeatureExpression(expression) => expression_root_kind(&expression.expression),
        SignItem::RoleExpression(expression) => expression_root_kind(&expression.expression),
        SignItem::Constraint(_) => NodeKind::Constraint,
        SignItem::FeatureRule(_) => NodeKind::FeatureRule,
        SignItem::Def(_) => NodeKind::Definition,
        SignItem::Rule(_) => NodeKind::Rule,
    }
}

fn expression_root_kind(expression: &crate::Expression) -> NodeKind {
    match expression {
        crate::Expression::SignApplication(_) | crate::Expression::PhonInterpolation(_) => {
            NodeKind::Application
        }
        crate::Expression::Projection { value, .. } => expression_root_kind(value),
        crate::Expression::Case(_) => NodeKind::Case,
        // The three expression-bearing SignItem variants currently lower
        // only typed cases.  Keep a deterministic node kind for a future
        // scalar expression until a generic Expression NodeKind is added.
        _ => NodeKind::Case,
    }
}

fn push_entry(
    entries: &mut Vec<NodeEntryV1>,
    namespace: &str,
    next: &mut u64,
    kind: NodeKind,
    parent: Option<NodeId>,
    address: NodeAddress,
) -> NodeId {
    let id = NodeId::new(IdentityNamespace::Document(namespace.to_owned()), *next);
    *next += 1;
    entries.push(NodeEntryV1 {
        id: id.clone(),
        kind,
        parent,
        address,
    });
    id
}

/// Recursively enumerate a phon `PhonBlock` into addressable nodes (P46 S3).
/// `address`/`parent` are the enclosing container's address/id (the rule item, or
/// an outer `PhonBlockNode`). Deterministic order = address order (P26).
fn enumerate_phon_block(
    block: &crate::PhonBlock,
    address: &NodeAddress,
    parent: &NodeId,
    namespace: &str,
    next: &mut u64,
    entries: &mut Vec<NodeEntryV1>,
) {
    match block {
        crate::PhonBlock::Leaf(statements) => {
            for (index, _) in statements.iter().enumerate() {
                push_entry(
                    entries,
                    namespace,
                    next,
                    NodeKind::PhonStatement,
                    Some(parent.clone()),
                    address.child(AddressSegment::PhonLeaf(index)),
                );
            }
        }
        crate::PhonBlock::Then(elements) | crate::PhonBlock::Else(elements) => {
            let is_then = matches!(block, crate::PhonBlock::Then(_));
            for (index, element) in elements.iter().enumerate() {
                let segment = if is_then {
                    AddressSegment::PhonThen(index)
                } else {
                    AddressSegment::PhonElse(index)
                };
                let element_address = address.child(segment);
                let element_id = push_entry(
                    entries,
                    namespace,
                    next,
                    NodeKind::PhonBlockNode,
                    Some(parent.clone()),
                    element_address.clone(),
                );
                enumerate_phon_block(
                    element,
                    &element_address,
                    &element_id,
                    namespace,
                    next,
                    entries,
                );
            }
        }
        crate::PhonBlock::Propagate(inner) => {
            // P46 S4: `Propagate` is a *modifier* on the element it wraps, not an
            // addressing level — it contributes no segment. Toggling it therefore
            // never moves a child, so statement identities stay stable across an
            // `update <node>.propagate = …` (P25/P26).
            enumerate_phon_block(inner, address, parent, namespace, next, entries);
        }
    }
}

fn enumerate_item_children(
    item: &SignItem,
    address: &NodeAddress,
    parent: &NodeId,
    namespace: &str,
    next: &mut u64,
    entries: &mut Vec<NodeEntryV1>,
) {
    fn enumerate_application_children(
        application: &crate::SignApplication,
        address: &NodeAddress,
        parent: &NodeId,
        namespace: &str,
        next: &mut u64,
        entries: &mut Vec<NodeEntryV1>,
    ) {
        for (index, argument) in application.arguments.iter().enumerate() {
            let crate::SignArgumentValue::Application(nested) = &argument.value else {
                continue;
            };
            let nested_address = address.child(AddressSegment::ApplicationArguments(index));
            let nested_id = push_entry(
                entries,
                namespace,
                next,
                NodeKind::Application,
                Some(parent.clone()),
                nested_address.clone(),
            );
            enumerate_application_children(
                nested,
                &nested_address,
                &nested_id,
                namespace,
                next,
                entries,
            );
        }
    }

    fn enumerate_expression_node(
        expression: &crate::Expression,
        address: &NodeAddress,
        parent: &NodeId,
        namespace: &str,
        next: &mut u64,
        entries: &mut Vec<NodeEntryV1>,
    ) {
        match expression {
            crate::Expression::SignApplication(application)
            | crate::Expression::PhonInterpolation(application) => {
                let application_id = push_entry(
                    entries,
                    namespace,
                    next,
                    NodeKind::Application,
                    Some(parent.clone()),
                    address.clone(),
                );
                enumerate_application_children(
                    application,
                    address,
                    &application_id,
                    namespace,
                    next,
                    entries,
                );
            }
            crate::Expression::SignFragment(items)
            | crate::Expression::DimFragment { items, .. } => {
                // A SignContext fragment is anonymous.  Its editable Sign
                // items are therefore direct children of the owning case
                // branch rather than children of a synthetic fragment node.
                let branch_address = address.parent().unwrap_or_else(|| address.clone());
                for (index, item) in items.iter().enumerate() {
                    let item_address = branch_address.child(AddressSegment::Items(index));
                    let item_id = push_entry(
                        entries,
                        namespace,
                        next,
                        item_kind(item),
                        Some(parent.clone()),
                        item_address.clone(),
                    );
                    enumerate_item_children(
                        item,
                        &item_address,
                        &item_id,
                        namespace,
                        next,
                        entries,
                    );
                }
            }
            crate::Expression::Projection { value, .. } => {
                // A projection is a typed view of its input rather than an
                // independently editable node.  The underlying expression
                // therefore occupies the projection's address.
                enumerate_expression_node(value, address, parent, namespace, next, entries);
            }
            crate::Expression::Case(case) => {
                let case_id = push_entry(
                    entries,
                    namespace,
                    next,
                    NodeKind::Case,
                    Some(parent.clone()),
                    address.clone(),
                );
                enumerate_case(case, address, &case_id, namespace, next, entries);
            }
            _ => {}
        }
    }

    fn enumerate_case(
        case: &crate::TypedCase,
        address: &NodeAddress,
        parent: &NodeId,
        namespace: &str,
        next: &mut u64,
        entries: &mut Vec<NodeEntryV1>,
    ) {
        for (index, branch) in case.branches.iter().enumerate() {
            let branch_address = address.child(AddressSegment::CaseBranches(index));
            let branch_id = push_entry(
                entries,
                namespace,
                next,
                NodeKind::CaseBranch,
                Some(parent.clone()),
                branch_address.clone(),
            );
            enumerate_expression_node(
                &branch.result,
                &branch_address.child(AddressSegment::CaseResult),
                &branch_id,
                namespace,
                next,
                entries,
            );
        }
    }

    fn enumerate_root_expression_children(
        expression: &crate::Expression,
        address: &NodeAddress,
        parent: &NodeId,
        namespace: &str,
        next: &mut u64,
        entries: &mut Vec<NodeEntryV1>,
    ) {
        match expression {
            crate::Expression::SignApplication(application)
            | crate::Expression::PhonInterpolation(application) => {
                enumerate_application_children(
                    application,
                    address,
                    parent,
                    namespace,
                    next,
                    entries,
                );
            }
            crate::Expression::Projection { value, .. } => {
                enumerate_root_expression_children(
                    value, address, parent, namespace, next, entries,
                );
            }
            crate::Expression::Case(case) => {
                enumerate_case(case, address, parent, namespace, next, entries);
            }
            _ => {}
        }
    }

    match item {
        SignItem::Rule(rule) | SignItem::FeatureRule(rule) => {
            for (index, _) in rule.else_chain.iter().enumerate() {
                push_entry(
                    entries,
                    namespace,
                    next,
                    NodeKind::RuleElseBranch,
                    Some(parent.clone()),
                    address.child(AddressSegment::RuleElse(index)),
                );
            }
            for (index, _) in rule.then_chain.iter().enumerate() {
                push_entry(
                    entries,
                    namespace,
                    next,
                    NodeKind::RuleThenBranch,
                    Some(parent.clone()),
                    address.child(AddressSegment::RuleThen(index)),
                );
            }
            // P46 S3: structured phon block statements/sub-blocks are addressable
            // nodes (recursive). Only walked when the rule carries a phon_block.
            if let Some(block) = &rule.phon_block {
                enumerate_phon_block(block, address, parent, namespace, next, entries);
            }
        }
        // 取徑 A(《修補08》更正欄):`Realization` 不再多一層 wrapper 節點——
        // item 自己的 kind 就是 `Case`(見 `item_kind`),branches 直接掛在 item 位址上。
        // 少掉的那一段 `CaseExpression` 正是先前讓 `case["X"]` 解不到的中間階。
        SignItem::Realization(Realization { expression: case }) => {
            enumerate_case(case, address, parent, namespace, next, entries);
        }
        SignItem::SignExpression(expression) => enumerate_root_expression_children(
            &expression.expression,
            address,
            parent,
            namespace,
            next,
            entries,
        ),
        SignItem::FeatureExpression(expression) => enumerate_root_expression_children(
            &expression.expression,
            address,
            parent,
            namespace,
            next,
            entries,
        ),
        SignItem::RoleExpression(expression) => enumerate_root_expression_children(
            &expression.expression,
            address,
            parent,
            namespace,
            next,
            entries,
        ),
        _ => {}
    }
}

fn enumerate_nodes(language: &Language, namespace: &str, next: &mut u64) -> Vec<NodeEntryV1> {
    let mut entries = Vec::new();
    let root = push_entry(
        &mut entries,
        namespace,
        next,
        NodeKind::Language,
        None,
        NodeAddress::root(),
    );
    for index in 0..language.dsl_decls.len() {
        push_entry(
            &mut entries,
            namespace,
            next,
            NodeKind::DslDeclaration,
            Some(root.clone()),
            NodeAddress(vec![AddressSegment::DslDeclarations(index)]),
        );
    }
    for index in 0..language.distribution.len() {
        push_entry(
            &mut entries,
            namespace,
            next,
            NodeKind::Distribution,
            Some(root.clone()),
            NodeAddress(vec![AddressSegment::Distribution(index)]),
        );
    }
    for (trait_index, trait_def) in language.traits.iter().enumerate() {
        let trait_address = NodeAddress(vec![AddressSegment::Traits(trait_index)]);
        let trait_id = push_entry(
            &mut entries,
            namespace,
            next,
            NodeKind::Trait,
            Some(root.clone()),
            trait_address.clone(),
        );
        for (block_index, block) in trait_def.blocks.iter().enumerate() {
            let block_address = trait_address.child(AddressSegment::Blocks(block_index));
            let block_id = push_entry(
                &mut entries,
                namespace,
                next,
                NodeKind::Block,
                Some(trait_id.clone()),
                block_address.clone(),
            );
            for (item_index, item) in block.items.iter().enumerate() {
                let address = block_address.child(AddressSegment::Items(item_index));
                let item_id = push_entry(
                    &mut entries,
                    namespace,
                    next,
                    item_kind(item),
                    Some(block_id.clone()),
                    address.clone(),
                );
                enumerate_item_children(item, &address, &item_id, namespace, next, &mut entries);
            }
        }
    }
    for (sign_index, sign) in language.signs.iter().enumerate() {
        let sign_address = NodeAddress(vec![AddressSegment::Signs(sign_index)]);
        let sign_id = push_entry(
            &mut entries,
            namespace,
            next,
            NodeKind::Sign,
            Some(root.clone()),
            sign_address.clone(),
        );
        for (item_index, item) in sign.items.iter().enumerate() {
            let address = sign_address.child(AddressSegment::Items(item_index));
            let item_id = push_entry(
                &mut entries,
                namespace,
                next,
                item_kind(item),
                Some(sign_id.clone()),
                address.clone(),
            );
            enumerate_item_children(item, &address, &item_id, namespace, next, &mut entries);
        }
    }
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    entries
}

fn expected_shape(language: &Language) -> Vec<(NodeAddress, NodeKind)> {
    let mut next = 0;
    enumerate_nodes(language, "shape", &mut next)
        .into_iter()
        .map(|entry| (entry.address, entry.kind))
        .collect()
}

fn validate_shape(language: &Language, manifest: &IdentityManifestV2) -> Result<(), IdentityError> {
    validate_manifest_namespaces(manifest)?;
    let mut expected = expected_shape(language);
    let mut actual: Vec<_> = manifest
        .nodes
        .iter()
        .map(|entry| (entry.address.clone(), entry.kind))
        .collect();
    expected.sort();
    actual.sort();
    if expected != actual {
        return Err(IdentityError::ShapeMismatch(
            "node addresses or kinds do not match the canonical source".to_owned(),
        ));
    }
    let root_count = manifest
        .nodes
        .iter()
        .filter(|entry| entry.kind == NodeKind::Language && entry.parent.is_none())
        .count();
    if root_count != 1 {
        return Err(IdentityError::ShapeMismatch(
            "manifest must contain exactly one immutable Language root".to_owned(),
        ));
    }
    let mut ids = manifest
        .nodes
        .iter()
        .map(|entry| &entry.id)
        .collect::<Vec<_>>();
    ids.sort();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(IdentityError::ShapeMismatch(
            "manifest contains duplicate NodeId".to_owned(),
        ));
    }
    let address_to_id: BTreeMap<_, _> = manifest
        .nodes
        .iter()
        .map(|entry| (entry.address.clone(), entry.id.clone()))
        .collect();
    for entry in &manifest.nodes {
        let expected_parent = entry
            .address
            .parent()
            .and_then(|address| address_to_id.get(&address).cloned());
        if entry.parent != expected_parent {
            return Err(IdentityError::ShapeMismatch(format!(
                "wrong parent binding for {}",
                entry.id
            )));
        }
        let IdentityNamespace::Document(namespace) = &entry.id.namespace else {
            return Err(IdentityError::ShapeMismatch(format!(
                "editable node {} does not use a document namespace",
                entry.id
            )));
        };
        if !manifest
            .allocators
            .iter()
            .any(|allocator| &allocator.namespace == namespace)
        {
            return Err(IdentityError::ShapeMismatch(format!(
                "node {} has no matching allocator",
                entry.id
            )));
        }
    }
    let root = manifest
        .nodes
        .iter()
        .find(|entry| entry.kind == NodeKind::Language && entry.parent.is_none())
        .expect("root count checked above");
    if root.id.namespace != IdentityNamespace::Document(manifest.root_namespace.clone()) {
        return Err(IdentityError::ShapeMismatch(
            "Language root is outside root_namespace".to_owned(),
        ));
    }
    for allocator in &manifest.allocators {
        let max = manifest
            .nodes
            .iter()
            .filter(|entry| {
                entry.id.namespace == IdentityNamespace::Document(allocator.namespace.clone())
            })
            .map(|entry| entry.id.ordinal)
            .max();
        if max.is_some_and(|max| allocator.next_ordinal <= max) {
            return Err(IdentityError::ShapeMismatch(format!(
                "allocator {} counter must exceed all allocated IDs",
                allocator.namespace
            )));
        }
    }
    Ok(())
}

fn bind_runtime_ids(language: &mut Language, nodes: &[NodeEntryV1]) -> Result<(), IdentityError> {
    for entry in nodes {
        let IdentityNamespace::Document(namespace) = &entry.id.namespace else {
            return Err(IdentityError::ShapeMismatch(format!(
                "runtime source node {} is not caller-owned",
                entry.id
            )));
        };
        match entry.address.0.as_slice() {
            [AddressSegment::Signs(index)] if entry.kind == NodeKind::Sign => {
                let sign = language.signs.get_mut(*index).ok_or_else(|| {
                    IdentityError::ShapeMismatch("sign address is out of bounds".to_owned())
                })?;
                sign.id = SignId::document(namespace, entry.id.ordinal);
            }
            _ if matches!(entry.kind, NodeKind::Rule | NodeKind::FeatureRule) => bind_rule(
                item_at_mut(language, &entry.address),
                namespace,
                entry.id.ordinal,
            )?,
            _ => {}
        }
    }
    Ok(())
}

fn bind_rule(
    item: Option<&mut SignItem>,
    namespace: &str,
    ordinal: u64,
) -> Result<(), IdentityError> {
    match item {
        Some(SignItem::Rule(rule) | SignItem::FeatureRule(rule)) => {
            rule.id = RuleId::document(namespace, ordinal);
            Ok(())
        }
        _ => Err(IdentityError::ShapeMismatch(
            "rule identity points to a non-rule item".to_owned(),
        )),
    }
}

fn node_ids_by_name(
    language: &Language,
    entries: &[NodeEntryV1],
) -> (BTreeMap<String, NodeId>, BTreeMap<String, NodeId>) {
    let mut traits = BTreeMap::new();
    let mut signs = BTreeMap::new();
    for entry in entries {
        match entry.address.0.as_slice() {
            [AddressSegment::Traits(index)] if entry.kind == NodeKind::Trait => {
                if let Some(trait_def) = language.traits.get(*index) {
                    traits.insert(trait_def.name.clone(), entry.id.clone());
                }
            }
            [AddressSegment::Signs(index)] if entry.kind == NodeKind::Sign => {
                if let Some(sign) = language.signs.get(*index) {
                    signs.insert(sign.name.clone(), entry.id.clone());
                }
            }
            _ => {}
        }
    }
    (traits, signs)
}

fn reference_target(
    spelling: &str,
    expected: NodeKind,
    locals: &BTreeMap<String, NodeId>,
) -> RefTargetV1 {
    match locals.get(spelling) {
        Some(id) => RefTargetV1::Local {
            target: NodeRef::new(id.clone(), expected),
        },
        None => RefTargetV1::External {
            spelling: spelling.to_owned(),
            expected,
        },
    }
}

fn item_at<'a>(language: &'a Language, address: &NodeAddress) -> Option<&'a SignItem> {
    fn expression_item_at<'a>(
        expression: &'a crate::Expression,
        path: &[AddressSegment],
    ) -> Option<&'a SignItem> {
        fn case_item_at<'a>(
            case: &'a crate::TypedCase,
            path: &[AddressSegment],
        ) -> Option<&'a SignItem> {
            let [AddressSegment::CaseBranches(branch), rest @ ..] = path else {
                return None;
            };
            let result = &case.branches.get(*branch)?.result;
            match rest {
                [AddressSegment::Items(item), tail @ ..] => {
                    let items = match result {
                        crate::Expression::SignFragment(items)
                        | crate::Expression::DimFragment { items, .. } => items,
                        _ => return None,
                    };
                    nested_item_at(items.get(*item)?, tail)
                }
                [AddressSegment::CaseResult, tail @ ..] => expression_item_at(result, tail),
                _ => None,
            }
        }

        match expression {
            crate::Expression::Projection { value, .. } => expression_item_at(value, path),
            crate::Expression::Case(case) => case_item_at(case, path),
            crate::Expression::SignFragment(items)
            | crate::Expression::DimFragment { items, .. } => {
                let [AddressSegment::Items(item), tail @ ..] = path else {
                    return None;
                };
                nested_item_at(items.get(*item)?, tail)
            }
            _ => None,
        }
    }

    fn nested_item_at<'a>(item: &'a SignItem, path: &[AddressSegment]) -> Option<&'a SignItem> {
        if path.is_empty() {
            return Some(item);
        }
        match item {
            SignItem::SignExpression(expression) => {
                expression_item_at(&expression.expression, path)
            }
            SignItem::FeatureExpression(expression) => {
                expression_item_at(&expression.expression, path)
            }
            SignItem::RoleExpression(expression) => {
                expression_item_at(&expression.expression, path)
            }
            SignItem::Realization(realization) => {
                let case = &realization.expression;
                let [AddressSegment::CaseBranches(branch), rest @ ..] = path else {
                    return None;
                };
                let result = &case.branches.get(*branch)?.result;
                match rest {
                    [AddressSegment::Items(index), nested @ ..] => {
                        let items = match result {
                            crate::Expression::SignFragment(items)
                            | crate::Expression::DimFragment { items, .. } => items,
                            _ => return None,
                        };
                        nested_item_at(items.get(*index)?, nested)
                    }
                    [AddressSegment::CaseResult, nested @ ..] => expression_item_at(result, nested),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    let (root, tail) = match address.0.as_slice() {
        [AddressSegment::Signs(sign), AddressSegment::Items(item), tail @ ..] => {
            (language.signs.get(*sign)?.items.get(*item)?, tail)
        }
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block), AddressSegment::Items(item), tail @ ..] => {
            (
                language
                    .traits
                    .get(*trait_index)?
                    .blocks
                    .get(*block)?
                    .items
                    .get(*item)?,
                tail,
            )
        }
        _ => return None,
    };
    nested_item_at(root, tail)
}

fn item_at_mut<'a>(language: &'a mut Language, address: &NodeAddress) -> Option<&'a mut SignItem> {
    fn expression_item_at_mut<'a>(
        expression: &'a mut crate::Expression,
        path: &[AddressSegment],
    ) -> Option<&'a mut SignItem> {
        match expression {
            crate::Expression::Projection { value, .. } => {
                expression_item_at_mut(value.as_mut(), path)
            }
            crate::Expression::Case(case) => case_item_at_mut(case, path),
            crate::Expression::SignFragment(items)
            | crate::Expression::DimFragment { items, .. } => {
                let [AddressSegment::Items(item), tail @ ..] = path else {
                    return None;
                };
                nested_item_at_mut(items.get_mut(*item)?, tail)
            }
            _ => None,
        }
    }

    fn case_item_at_mut<'a>(
        case: &'a mut crate::TypedCase,
        path: &[AddressSegment],
    ) -> Option<&'a mut SignItem> {
        let [AddressSegment::CaseBranches(branch), rest @ ..] = path else {
            return None;
        };
        let result = &mut case.branches.get_mut(*branch)?.result;
        match rest {
            [AddressSegment::Items(item), tail @ ..] => {
                let items = match result {
                    crate::Expression::SignFragment(items)
                    | crate::Expression::DimFragment { items, .. } => items,
                    _ => return None,
                };
                nested_item_at_mut(items.get_mut(*item)?, tail)
            }
            [AddressSegment::CaseResult, tail @ ..] => expression_item_at_mut(result, tail),
            _ => None,
        }
    }

    fn nested_item_at_mut<'a>(
        item: &'a mut SignItem,
        path: &[AddressSegment],
    ) -> Option<&'a mut SignItem> {
        if path.is_empty() {
            return Some(item);
        }
        match item {
            SignItem::SignExpression(expression) => {
                expression_item_at_mut(&mut expression.expression, path)
            }
            SignItem::FeatureExpression(expression) => {
                expression_item_at_mut(&mut expression.expression, path)
            }
            SignItem::RoleExpression(expression) => {
                expression_item_at_mut(&mut expression.expression, path)
            }
            SignItem::Realization(realization) => {
                case_item_at_mut(&mut realization.expression, path)
            }
            _ => None,
        }
    }

    let (root, tail) = match address.0.as_slice() {
        [AddressSegment::Signs(sign), AddressSegment::Items(item), tail @ ..] => {
            (language.signs.get_mut(*sign)?.items.get_mut(*item)?, tail)
        }
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block), AddressSegment::Items(item), tail @ ..] => {
            (
                language
                    .traits
                    .get_mut(*trait_index)?
                    .blocks
                    .get_mut(*block)?
                    .items
                    .get_mut(*item)?,
                tail,
            )
        }
        _ => return None,
    };
    nested_item_at_mut(root, tail)
}

fn entry_at<'a>(
    entries: &'a [NodeEntryV1],
    address: &NodeAddress,
    kind: NodeKind,
) -> Option<&'a NodeEntryV1> {
    entries
        .iter()
        .find(|entry| entry.kind == kind && &entry.address == address)
}

fn collect_application_refs(
    application: &crate::SignApplication,
    address: &NodeAddress,
    owner: &NodeId,
    entries: &[NodeEntryV1],
    signs: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
) {
    refs.push(RefBindingV1 {
        owner: owner.clone(),
        field: "application.callee".to_owned(),
        target: reference_target(&application.callee, NodeKind::Sign, signs),
    });
    for (index, argument) in application.arguments.iter().enumerate() {
        let crate::SignArgumentValue::Application(nested) = &argument.value else {
            continue;
        };
        let nested_address = address.child(AddressSegment::ApplicationArguments(index));
        let Some(nested_entry) = entry_at(entries, &nested_address, NodeKind::Application) else {
            continue;
        };
        collect_application_refs(
            nested,
            &nested_address,
            &nested_entry.id,
            entries,
            signs,
            refs,
        );
    }
}

fn collect_expression_refs(
    expression: &crate::Expression,
    address: &NodeAddress,
    root_owner: Option<&NodeId>,
    entries: &[NodeEntryV1],
    traits: &BTreeMap<String, NodeId>,
    signs: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
) {
    match expression {
        crate::Expression::SignApplication(application)
        | crate::Expression::PhonInterpolation(application) => {
            let owner = root_owner.cloned().or_else(|| {
                entry_at(entries, address, NodeKind::Application).map(|entry| entry.id.clone())
            });
            if let Some(owner) = owner {
                collect_application_refs(application, address, &owner, entries, signs, refs);
            }
        }
        crate::Expression::Projection { value, .. } => {
            collect_expression_refs(value, address, root_owner, entries, traits, signs, refs)
        }
        crate::Expression::Case(case) => {
            collect_case_refs(case, address, root_owner, entries, traits, signs, refs)
        }
        _ => {}
    }
}

fn collect_case_refs(
    case: &crate::TypedCase,
    address: &NodeAddress,
    root_owner: Option<&NodeId>,
    entries: &[NodeEntryV1],
    traits: &BTreeMap<String, NodeId>,
    signs: &BTreeMap<String, NodeId>,
    refs: &mut Vec<RefBindingV1>,
) {
    if root_owner.is_none() && entry_at(entries, address, NodeKind::Case).is_none() {
        return;
    }
    for (branch_index, branch) in case.branches.iter().enumerate() {
        let branch_address = address.child(AddressSegment::CaseBranches(branch_index));
        let Some(branch_entry) = entry_at(entries, &branch_address, NodeKind::CaseBranch) else {
            continue;
        };
        for (belongs_index, category) in branch.belongs.iter().enumerate() {
            refs.push(RefBindingV1 {
                owner: branch_entry.id.clone(),
                field: format!("case.belongs[{belongs_index}]"),
                target: reference_target(category, NodeKind::Trait, traits),
            });
        }
        match &branch.condition {
            crate::CaseCondition::Guard(guard) => {
                for (guard_index, category) in guard_category_references(guard) {
                    refs.push(RefBindingV1 {
                        owner: branch_entry.id.clone(),
                        field: format!("case.guard[{guard_index}].category"),
                        target: reference_target(&category, NodeKind::Trait, traits),
                    });
                }
            }
            crate::CaseCondition::Equals(category)
                if case
                    .scrutinee
                    .as_deref()
                    .and_then(|value| value.split_once('.'))
                    .is_some_and(|(_, projection)| projection == "phon") =>
            {
                refs.push(RefBindingV1 {
                    owner: branch_entry.id.clone(),
                    field: "case.equals.category".to_owned(),
                    target: reference_target(category, NodeKind::Trait, traits),
                });
            }
            crate::CaseCondition::Equals(_) | crate::CaseCondition::Else => {}
        }
        collect_expression_refs(
            &branch.result,
            &branch_address.child(AddressSegment::CaseResult),
            None,
            entries,
            traits,
            signs,
            refs,
        );
    }
}

/// Extract category-valued conjuncts from the closed guard grammar.  Scalar
/// equality conjuncts intentionally produce no Trait Ref.
fn guard_category_references(source: &str) -> Vec<(usize, String)> {
    source
        .split("&&")
        .enumerate()
        .filter_map(|(index, conjunct)| {
            let conjunct = conjunct.trim();
            let category = conjunct
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
                .or_else(|| {
                    let (field, value) = conjunct.split_once("==")?;
                    let field = field.trim();
                    let category_valued = field == "$self"
                        || field
                            .strip_prefix("$slot.")
                            .is_some_and(|slot| !slot.contains('.'));
                    category_valued.then(|| value.trim()).and_then(|value| {
                        value
                            .strip_prefix('[')
                            .and_then(|value| value.strip_suffix(']'))
                    })
                })?
                .trim();
            (!category.is_empty()).then(|| (index, category.to_owned()))
        })
        .collect()
}

fn collect_refs(language: &Language, entries: &[NodeEntryV1]) -> Vec<RefBindingV1> {
    let (traits, signs) = node_ids_by_name(language, entries);
    let mut refs = Vec::new();
    for entry in entries {
        let Some(item) = item_at(language, &entry.address) else {
            continue;
        };
        let binding = match item {
            SignItem::TraitMount {
                name,
                kind: crate::TraitMountKind::Whole | crate::TraitMountKind::Block(_),
                ..
            } => Some((
                "trait_use.name".to_owned(),
                reference_target(name, NodeKind::Trait, &traits),
            )),
            SignItem::TraitMount {
                name: name,
                kind: crate::TraitMountKind::Declaration,
                ..
            } => Some((
                "belongs".to_owned(),
                reference_target(name, NodeKind::Trait, &traits),
            )),
            SignItem::Slot(slot) => match &slot.constraint {
                SlotConstraint::Category(name) => Some((
                    "slot.constraint".to_owned(),
                    reference_target(name, NodeKind::Trait, &traits),
                )),
                SlotConstraint::AnySign => None,
            },
            SignItem::SlotMap(crate::SlotMapOp::AutoFill { filler, .. }) => Some((
                "slot_map.autofill".to_owned(),
                reference_target(filler, NodeKind::Sign, &signs),
            )),
            // `[*]` 不指名 trait,無引用可綁。
            SignItem::RoleDecl(role) => role.constraint.category().map(|category| {
                (
                    "role.constraint".to_owned(),
                    reference_target(category, NodeKind::Trait, &traits),
                )
            }),
            SignItem::Def(def) if def.path == "origin" => {
                metadata::parse_origin(&def.value).map(|origin| {
                    (
                        "origin".to_owned(),
                        reference_target(&origin.0, NodeKind::Sign, &signs),
                    )
                })
            }
            _ => None,
        };
        if let Some((field, target)) = binding {
            refs.push(RefBindingV1 {
                owner: entry.id.clone(),
                field,
                target,
            });
        }
        match item {
            SignItem::SignExpression(expression) => collect_expression_refs(
                &expression.expression,
                &entry.address,
                Some(&entry.id),
                entries,
                &traits,
                &signs,
                &mut refs,
            ),
            SignItem::FeatureExpression(expression) => collect_expression_refs(
                &expression.expression,
                &entry.address,
                Some(&entry.id),
                entries,
                &traits,
                &signs,
                &mut refs,
            ),
            SignItem::RoleExpression(expression) => collect_expression_refs(
                &expression.expression,
                &entry.address,
                Some(&entry.id),
                entries,
                &traits,
                &signs,
                &mut refs,
            ),
            SignItem::Realization(Realization { expression: case }) => collect_case_refs(
                case,
                &entry.address,
                None,
                entries,
                &traits,
                &signs,
                &mut refs,
            ),
            _ => {}
        }
    }
    refs.sort_by(|left, right| (&left.owner, &left.field).cmp(&(&right.owner, &right.field)));
    refs
}

// Small, self-contained SHA-256 implementation.  Keeping this in the source
// identity boundary avoids making core persistence depend on platform tools.
pub fn sha256_hex(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut bytes = input.to_vec();
    let bit_len = (bytes.len() as u64) * 8;
    bytes.push(0x80);
    while bytes.len() % 64 != 56 {
        bytes.push(0);
    }
    bytes.extend_from_slice(&bit_len.to_be_bytes());
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    for chunk in bytes.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let mut state = h;
        for index in 0..64 {
            let s1 =
                state[4].rotate_right(6) ^ state[4].rotate_right(11) ^ state[4].rotate_right(25);
            let choose = (state[4] & state[5]) ^ ((!state[4]) & state[6]);
            let temp1 = state[7]
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 =
                state[0].rotate_right(2) ^ state[0].rotate_right(13) ^ state[0].rotate_right(22);
            let majority = (state[0] & state[1]) ^ (state[0] & state[2]) ^ (state[1] & state[2]);
            let temp2 = s0.wrapping_add(majority);
            state = [
                temp1.wrapping_add(temp2),
                state[0],
                state[1],
                state[2],
                state[3].wrapping_add(temp1),
                state[4],
                state[5],
                state[6],
            ];
        }
        for index in 0..8 {
            h[index] = h[index].wrapping_add(state[index]);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_standard_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn nested_case_projection_and_application_arguments_have_distinct_addresses() {
        let mut language = Language::parse(
            r#"sign root:
    phon:
        /r/
    case:
        else:
            $self
"#,
        )
        .unwrap();
        let expression = language.signs[0]
            .items
            .iter_mut()
            .find_map(|item| match item {
                SignItem::SignExpression(expression) => Some(expression),
                _ => None,
            })
            .unwrap();
        let inner = crate::SignApplication {
            callee: "Inner".to_owned(),
            arguments: vec![crate::SignArgument {
                name: None,
                value: crate::SignArgumentValue::SelfSign,
            }],
            source: crate::SourceLocation::line(7),
        };
        let outer = crate::SignApplication {
            callee: "Outer".to_owned(),
            arguments: vec![crate::SignArgument {
                name: Some("value".to_owned()),
                value: crate::SignArgumentValue::Application(Box::new(inner)),
            }],
            source: crate::SourceLocation::line(7),
        };
        let nested = crate::TypedCase {
            selection: crate::CaseSelection::FirstMatch,
            expected: crate::ExpressionType::SignContext,
            scrutinee: None,
            name: None,
            branches: vec![crate::CaseBranch {
                condition: crate::CaseCondition::Else,
                result: crate::Expression::Projection {
                    value: Box::new(crate::Expression::SignApplication(outer)),
                    dimension: crate::SignProjection::Syn,
                },
                belongs: Vec::new(),
                name: None,
                source: crate::SourceLocation::line(7),
            }],
            source: crate::SourceLocation::line(7),
        };
        let crate::Expression::Case(case) = &mut expression.expression else {
            panic!("fixture root is a case")
        };
        case.branches[0].result = crate::Expression::Case(Box::new(nested));

        let mut next = 0;
        let nodes = enumerate_nodes(&language, "evo:nested", &mut next);
        let addresses = nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    NodeKind::Case | NodeKind::CaseBranch | NodeKind::Application
                )
            })
            .map(|node| (node.kind, node.address.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            addresses
                .iter()
                .filter(|(kind, _)| *kind == NodeKind::Case)
                .count(),
            2
        );
        assert_eq!(
            addresses
                .iter()
                .filter(|(kind, _)| *kind == NodeKind::CaseBranch)
                .count(),
            2
        );
        assert_eq!(
            addresses
                .iter()
                .filter(|(kind, _)| *kind == NodeKind::Application)
                .count(),
            2
        );
        assert!(addresses.iter().any(|(_, address)| {
            address
                .0
                .ends_with(&[AddressSegment::ApplicationArguments(0)])
        }));
        let mut unique = addresses
            .iter()
            .map(|(_, address)| address.clone())
            .collect::<Vec<_>>();
        unique.sort();
        assert!(unique.windows(2).all(|pair| pair[0] != pair[1]));
    }
}
