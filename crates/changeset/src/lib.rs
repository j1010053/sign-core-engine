//! Step 13 primitive source edits.
//!
//! This crate depends on the synchronic language model, never the reverse.
//! It edits only a caller-owned [`LanguageDocument`]; compiled/effective
//! language state and runtime derived tokens are deliberately absent here.
//!
//! ```
//! use conlang_changeset::Anchor;
//! assert_ne!(Anchor::Start, Anchor::End);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

use std::collections::{BTreeMap, BTreeSet};

mod call;
pub mod evolution;
pub mod function;
pub mod rewrite;

use conlang_language::{
    check_document, compile_document, sha256_hex, AddressSegment, BinaryConstraint, Block,
    CaseBranch, CompileSystemError, CompiledSystem, Def, DerivationKind, Dim, Expression,
    FeatureDecl, FeatureValue, IdentityError, IdentityManifestV2, Language, LanguageDocument,
    LibraryId, LibrarySpec, NodeAddress, NodeEntryV1, NodeId, NodeKind, NodeRef, PhonBlock,
    Realization, RoleBinding, RoleDecl, Rule, SenseTransparency, Severity, SignApplication,
    SignArgumentValue, SignDef, SignItem, Slot, SlotConstraint, SlotFeatureBinding, SlotMapOp,
    Stage, TraitDef, TypedCase, ValidationReport,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    Start,
    End,
    Before(NodeRef),
    After(NodeRef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum DetachedNode {
    DslDeclaration(String),
    Prosody(Vec<String>),
    Distribution {
        key: String,
        value: String,
    },
    Trait(TraitDef),
    Sign(SignDef),
    Block(Block),
    Item(SignItem),
    RuleElseBranch(String),
    RuleThenBranch(String),
    /// A single phon `Leaf` statement line (P46 S3).
    PhonStatement(String),
    /// A whole phon sub-block — one element of a `Then`/`Else` vec (P46 S3).
    PhonBlockNode(PhonBlock),
    CaseBranch(CaseBranch),
}

impl DetachedNode {
    pub fn kind(&self) -> NodeKind {
        match self {
            DetachedNode::DslDeclaration(_) => NodeKind::DslDeclaration,
            DetachedNode::Prosody(_) => NodeKind::Prosody,
            DetachedNode::Distribution { .. } => NodeKind::Distribution,
            DetachedNode::Trait(_) => NodeKind::Trait,
            DetachedNode::Sign(_) => NodeKind::Sign,
            DetachedNode::Block(_) => NodeKind::Block,
            DetachedNode::Item(item) => item_kind(item),
            DetachedNode::RuleElseBranch(_) => NodeKind::RuleElseBranch,
            DetachedNode::RuleThenBranch(_) => NodeKind::RuleThenBranch,
            DetachedNode::PhonStatement(_) => NodeKind::PhonStatement,
            DetachedNode::PhonBlockNode(_) => NodeKind::PhonBlockNode,
            DetachedNode::CaseBranch(_) => NodeKind::CaseBranch,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum NodeUpdate {
    Rename(String),
    TraitGlobal(bool),
    DslDeclaration(String),
    Prosody(Vec<String>),
    Distribution {
        key: String,
        value: String,
    },
    DefinitionPath(String),
    DefinitionValue(String),
    RuleBody(String),
    RuleStage(Stage),
    RuleDimension(Dim),
    RuleBranchBody(String),
    /// P46 S4: rule-level or block-element fixpoint iteration (`propagate`).
    Propagate(bool),
    /// §10.3 義項/衍生邊(Atomic Rewrite drift / lexicalize_sense 的落點)。
    SenseGloss(String),
    SenseEdgeKind(DerivationKind),
    SenseEdgeTransparency(SenseTransparency),
    SlotName(String),
    SlotConstraint(SlotConstraint),
    SlotOptional(bool),
    TraitUse {
        name: String,
        block: Option<u32>,
    },
    Belongs(String),
    FeatureDeclaration(FeatureDecl),
    FeatureValue(FeatureValue),
    SlotFeatureBinding(SlotFeatureBinding),
    SlotMap(SlotMapOp),
    RoleDeclaration(RoleDecl),
    RoleBinding(RoleBinding),
    CaseSelection(conlang_language::CaseSelection),
    CaseBranch(CaseBranch),
    SignApplication(SignApplication),
    Constraint(BinaryConstraint),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum PrimitiveEdit {
    Insert {
        parent: NodeRef,
        anchor: Anchor,
        subtree: DetachedNode,
    },
    Delete {
        node: NodeRef,
    },
    Update {
        node: NodeRef,
        change: NodeUpdate,
    },
    Move {
        node: NodeRef,
        new_parent: NodeRef,
        anchor: Anchor,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    Insert,
    Delete,
    Update,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSnapshot {
    pub id: NodeId,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub address: NodeAddress,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguageDiffEntry {
    Inserted(NodeSnapshot),
    Deleted(NodeSnapshot),
    Updated {
        before: NodeSnapshot,
        after: NodeSnapshot,
    },
    Moved {
        before: NodeSnapshot,
        after: NodeSnapshot,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LanguageDiff {
    pub entries: Vec<LanguageDiffEntry>,
}

impl LanguageDiff {
    pub fn between(before: &LanguageDocument, after: &LanguageDocument) -> LanguageDiff {
        let old = snapshots(before);
        let new = snapshots(after);
        let common: BTreeSet<_> = old
            .keys()
            .filter(|id| new.contains_key(*id))
            .cloned()
            .collect();
        let old_ranks = sibling_ranks(&old, &common);
        let new_ranks = sibling_ranks(&new, &common);
        let mut entries = Vec::new();
        let ids: BTreeSet<_> = old.keys().chain(new.keys()).cloned().collect();
        for id in ids {
            match (old.get(&id), new.get(&id)) {
                (None, Some(after)) => entries.push(LanguageDiffEntry::Inserted(after.clone())),
                (Some(before), None) => entries.push(LanguageDiffEntry::Deleted(before.clone())),
                (Some(before), Some(after)) => {
                    if before.parent != after.parent || old_ranks.get(&id) != new_ranks.get(&id) {
                        entries.push(LanguageDiffEntry::Moved {
                            before: before.clone(),
                            after: after.clone(),
                        });
                    }
                    if before.value != after.value || before.kind != after.kind {
                        entries.push(LanguageDiffEntry::Updated {
                            before: before.clone(),
                            after: after.clone(),
                        });
                    }
                }
                (None, None) => unreachable!(),
            }
        }
        LanguageDiff { entries }
    }
}

fn sibling_ranks(
    snapshots: &BTreeMap<NodeId, NodeSnapshot>,
    common: &BTreeSet<NodeId>,
) -> BTreeMap<NodeId, usize> {
    let mut sequences: BTreeMap<(Option<NodeId>, u8), Vec<&NodeSnapshot>> = BTreeMap::new();
    for (id, snapshot) in snapshots {
        if common.contains(id) {
            let tag = sequence_tag(&snapshot.address);
            // Distribution, Trait, and Sign order is a canonical-printing
            // concern, not a semantic Move. A rename/key update may change
            // their rendered order while every node retains the same parent.
            if matches!(tag, 3..=5) {
                continue;
            }
            sequences
                .entry((snapshot.parent.clone(), tag))
                .or_default()
                .push(snapshot);
        }
    }
    let mut ranks = BTreeMap::new();
    for values in sequences.values_mut() {
        values.sort_by(|left, right| left.address.cmp(&right.address));
        for (rank, value) in values.iter().enumerate() {
            ranks.insert(value.id.clone(), rank);
        }
    }
    ranks
}

fn sequence_tag(address: &NodeAddress) -> u8 {
    match address.0.last() {
        None => 0,
        Some(AddressSegment::DslDeclarations(_)) => 1,
        Some(AddressSegment::Prosody) => 2,
        Some(AddressSegment::Distribution(_)) => 3,
        Some(AddressSegment::Traits(_)) => 4,
        Some(AddressSegment::Signs(_)) => 5,
        Some(AddressSegment::Blocks(_)) => 6,
        Some(AddressSegment::Items(_)) => 7,
        Some(AddressSegment::RuleElse(_)) => 8,
        Some(AddressSegment::RuleThen(_)) => 9,
        Some(AddressSegment::RealizationBranches(_)) => 10,
        Some(AddressSegment::CaseExpression) => 11,
        Some(AddressSegment::CaseBranches(_)) => 12,
        Some(AddressSegment::CaseResult) => 13,
        Some(AddressSegment::ApplicationArguments(_)) => 14,
        Some(AddressSegment::PhonLeaf(_)) => 15,
        Some(AddressSegment::PhonThen(_)) => 16,
        Some(AddressSegment::PhonElse(_)) => 17,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDiagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimitiveRecord {
    pub operation: PrimitiveKind,
    pub target: Option<NodeRef>,
    pub parent: Option<NodeRef>,
    pub anchor: Option<Anchor>,
    pub before: Option<NodeSnapshot>,
    pub after: Option<NodeSnapshot>,
    pub allocated_ids: Vec<NodeId>,
    pub deleted_ids: Vec<NodeId>,
    pub moved_ids: Vec<NodeId>,
    pub diagnostics: Vec<RecordDiagnostic>,
    pub diff: LanguageDiff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOutcome {
    pub document: LanguageDocument,
    pub record: PrimitiveRecord,
}

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("EDIT_TARGET_NOT_FOUND: {0:?}")]
    TargetNotFound(NodeRef),
    #[error("EDIT_EXTERNAL_TARGET: {0}")]
    ExternalTarget(NodeId),
    #[error("EDIT_ROOT_IMMUTABLE")]
    RootImmutable,
    #[error("EDIT_PARENT_KIND: cannot place {child:?} under {parent:?}")]
    ParentKind { child: NodeKind, parent: NodeKind },
    #[error("EDIT_ANCHOR_INVALID: {0}")]
    AnchorInvalid(String),
    #[error("EDIT_FIELD_MISMATCH: {0}")]
    FieldMismatch(String),
    #[error("EDIT_CYCLE: a node cannot be moved into its own subtree")]
    Cycle,
    #[error("EDIT_ID_EXHAUSTED")]
    IdExhausted,
    #[error("EDIT_IDENTITY: {0}")]
    Identity(#[from] IdentityError),
    #[error("EDIT_VALIDATION_FAILED")]
    Validation(Box<ValidationReport>),
}

pub fn apply_edit(
    source: &LanguageDocument,
    edit: PrimitiveEdit,
    libraries: &LibrarySpec,
) -> Result<EditOutcome, EditError> {
    let before = source.clone();
    let operation = primitive_kind(&edit);
    let target = edit_target(&edit);
    let parent = edit_parent(&edit);
    let anchor = edit_anchor(&edit);
    let before_snapshot = target
        .as_ref()
        .and_then(|reference| snapshot_for(source, reference));

    let candidate = apply_structural(source.clone(), &edit)?;
    let validation = check_document(&candidate, libraries);
    if validation.has_errors() {
        return Err(EditError::Validation(Box::new(validation)));
    }
    let diff = LanguageDiff::between(&before, &candidate);
    let mut allocated_ids = Vec::new();
    let mut deleted_ids = Vec::new();
    let mut moved_ids = Vec::new();
    for entry in &diff.entries {
        match entry {
            LanguageDiffEntry::Inserted(node) => allocated_ids.push(node.id.clone()),
            LanguageDiffEntry::Deleted(node) => deleted_ids.push(node.id.clone()),
            LanguageDiffEntry::Moved { after, .. } => moved_ids.push(after.id.clone()),
            LanguageDiffEntry::Updated { .. } => {}
        }
    }
    let after_snapshot = match operation {
        PrimitiveKind::Insert => allocated_ids
            .first()
            .and_then(|id| snapshot_by_id(&candidate, id)),
        PrimitiveKind::Delete => None,
        PrimitiveKind::Update | PrimitiveKind::Move => target
            .as_ref()
            .and_then(|reference| snapshot_for(&candidate, reference)),
    };
    let diagnostics = validation
        .diagnostics()
        .iter()
        .map(|diagnostic| RecordDiagnostic {
            severity: diagnostic.severity,
            code: diagnostic.code.to_owned(),
            message: diagnostic.message.clone(),
        })
        .collect();
    Ok(EditOutcome {
        document: candidate,
        record: PrimitiveRecord {
            operation,
            target,
            parent,
            anchor,
            before: before_snapshot,
            after: after_snapshot,
            allocated_ids,
            deleted_ids,
            moved_ids,
            diagnostics,
            diff,
        },
    })
}

fn primitive_kind(edit: &PrimitiveEdit) -> PrimitiveKind {
    match edit {
        PrimitiveEdit::Insert { .. } => PrimitiveKind::Insert,
        PrimitiveEdit::Delete { .. } => PrimitiveKind::Delete,
        PrimitiveEdit::Update { .. } => PrimitiveKind::Update,
        PrimitiveEdit::Move { .. } => PrimitiveKind::Move,
    }
}

fn edit_target(edit: &PrimitiveEdit) -> Option<NodeRef> {
    match edit {
        PrimitiveEdit::Insert { .. } => None,
        PrimitiveEdit::Delete { node }
        | PrimitiveEdit::Update { node, .. }
        | PrimitiveEdit::Move { node, .. } => Some(node.clone()),
    }
}

fn edit_parent(edit: &PrimitiveEdit) -> Option<NodeRef> {
    match edit {
        PrimitiveEdit::Insert { parent, .. } => Some(parent.clone()),
        PrimitiveEdit::Move { new_parent, .. } => Some(new_parent.clone()),
        _ => None,
    }
}

fn edit_anchor(edit: &PrimitiveEdit) -> Option<Anchor> {
    match edit {
        PrimitiveEdit::Insert { anchor, .. } | PrimitiveEdit::Move { anchor, .. } => {
            Some(anchor.clone())
        }
        _ => None,
    }
}

fn apply_structural(
    source: LanguageDocument,
    edit: &PrimitiveEdit,
) -> Result<LanguageDocument, EditError> {
    match edit {
        PrimitiveEdit::Insert {
            parent,
            anchor,
            subtree,
        } => insert(source, parent, anchor, subtree.clone()),
        PrimitiveEdit::Delete { node } => delete(source, node),
        PrimitiveEdit::Update { node, change } => update(source, node, change.clone()),
        PrimitiveEdit::Move {
            node,
            new_parent,
            anchor,
        } => move_node(source, node, new_parent, anchor),
    }
}

fn ensure_target<'a>(
    source: &'a LanguageDocument,
    reference: &NodeRef,
) -> Result<&'a NodeEntryV1, EditError> {
    if !source.owns(&reference.id) {
        return Err(EditError::ExternalTarget(reference.id.clone()));
    }
    let entry = source
        .node(reference)
        .ok_or_else(|| EditError::TargetNotFound(reference.clone()))?;
    Ok(entry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKey {
    Dsl,
    Distribution,
    Traits,
    Signs,
    Blocks,
    Items,
    RuleElse,
    RuleThen,
    PhonLeaf,
    PhonThen,
    PhonElse,
    Realization,
    CaseBranches,
}

fn segment(key: ListKey, index: usize) -> AddressSegment {
    match key {
        ListKey::Dsl => AddressSegment::DslDeclarations(index),
        ListKey::Distribution => AddressSegment::Distribution(index),
        ListKey::Traits => AddressSegment::Traits(index),
        ListKey::Signs => AddressSegment::Signs(index),
        ListKey::Blocks => AddressSegment::Blocks(index),
        ListKey::Items => AddressSegment::Items(index),
        ListKey::RuleElse => AddressSegment::RuleElse(index),
        ListKey::RuleThen => AddressSegment::RuleThen(index),
        ListKey::PhonLeaf => AddressSegment::PhonLeaf(index),
        ListKey::PhonThen => AddressSegment::PhonThen(index),
        ListKey::PhonElse => AddressSegment::PhonElse(index),
        ListKey::Realization => AddressSegment::RealizationBranches(index),
        ListKey::CaseBranches => AddressSegment::CaseBranches(index),
    }
}

fn segment_index(value: &AddressSegment, key: ListKey) -> Option<usize> {
    match (key, value) {
        (ListKey::Dsl, AddressSegment::DslDeclarations(index))
        | (ListKey::Distribution, AddressSegment::Distribution(index))
        | (ListKey::Traits, AddressSegment::Traits(index))
        | (ListKey::Signs, AddressSegment::Signs(index))
        | (ListKey::Blocks, AddressSegment::Blocks(index))
        | (ListKey::Items, AddressSegment::Items(index))
        | (ListKey::RuleElse, AddressSegment::RuleElse(index))
        | (ListKey::RuleThen, AddressSegment::RuleThen(index))
        | (ListKey::PhonLeaf, AddressSegment::PhonLeaf(index))
        | (ListKey::PhonThen, AddressSegment::PhonThen(index))
        | (ListKey::PhonElse, AddressSegment::PhonElse(index))
        | (ListKey::Realization, AddressSegment::RealizationBranches(index)) => Some(*index),
        (ListKey::CaseBranches, AddressSegment::CaseBranches(index)) => Some(*index),
        _ => None,
    }
}

fn shift_list(
    manifest: &mut IdentityManifestV2,
    parent: &NodeAddress,
    key: ListKey,
    from: usize,
    delta: isize,
) {
    for entry in &mut manifest.nodes {
        if !entry.address.starts_with(parent) || entry.address.0.len() <= parent.0.len() {
            continue;
        }
        let position = parent.0.len();
        let Some(index) = segment_index(&entry.address.0[position], key) else {
            continue;
        };
        if index >= from {
            entry.address.0[position] = segment(key, index.saturating_add_signed(delta));
        }
    }
}

fn extract_identity_subtree(
    manifest: &mut IdentityManifestV2,
    prefix: &NodeAddress,
) -> Vec<NodeEntryV1> {
    let mut extracted = Vec::new();
    manifest.nodes.retain(|entry| {
        if entry.address.starts_with(prefix) {
            extracted.push(entry.clone());
            false
        } else {
            true
        }
    });
    extracted
}

fn attach_identity_subtree(
    manifest: &mut IdentityManifestV2,
    mut subtree: Vec<NodeEntryV1>,
    old_prefix: &NodeAddress,
    new_prefix: &NodeAddress,
    new_parent: &NodeId,
) {
    let root_id = subtree
        .iter()
        .find(|entry| &entry.address == old_prefix)
        .map(|entry| entry.id.clone());
    for entry in &mut subtree {
        if entry.address.starts_with(old_prefix) {
            let suffix = entry.address.0[old_prefix.0.len()..].to_vec();
            entry.address.0 = new_prefix.0.clone();
            entry.address.0.extend(suffix);
        }
        if root_id.as_ref() == Some(&entry.id) {
            entry.parent = Some(new_parent.clone());
        }
    }
    manifest.nodes.extend(subtree);
}

fn allocate_id(manifest: &mut IdentityManifestV2) -> Result<NodeId, EditError> {
    let allocator = manifest
        .allocators
        .iter_mut()
        .find(|allocator| allocator.namespace == manifest.active_namespace)
        .ok_or_else(|| {
            EditError::Identity(IdentityError::InvalidManifest(
                "active namespace has no allocator".to_owned(),
            ))
        })?;
    let ordinal = allocator.next_ordinal;
    allocator.next_ordinal = allocator
        .next_ordinal
        .checked_add(1)
        .ok_or(EditError::IdExhausted)?;
    Ok(NodeId::new(
        conlang_language::IdentityNamespace::Document(allocator.namespace.clone()),
        ordinal,
    ))
}

fn finish_edit(
    language: Language,
    manifest: IdentityManifestV2,
) -> Result<LanguageDocument, EditError> {
    // The source tree is authoritative after an edit. Reparse its canonical
    // spelling so source locations move with their nodes; otherwise a moved
    // branch would dump/reopen with different line provenance despite having
    // the same stable identity manifest.
    let canonical = language.dump();
    let language = Language::parse(&canonical)
        .map_err(|error| EditError::Identity(IdentityError::Parse(error.to_string())))?;
    LanguageDocument::from_edit_parts(language, manifest).map_err(EditError::from)
}

fn insert(
    source: LanguageDocument,
    parent_ref: &NodeRef,
    anchor: &Anchor,
    subtree: DetachedNode,
) -> Result<LanguageDocument, EditError> {
    let parent = ensure_target(&source, parent_ref)?.clone();
    let (mut language, mut manifest) = source.into_edit_parts();
    let (key, index, address) = insertion_site(&language, &manifest, &parent, anchor, &subtree)?;
    if !matches!(address.0.last(), Some(AddressSegment::Prosody)) {
        shift_list(&mut manifest, &parent.address, key, index, 1);
    }
    insert_payload(&mut language, &parent, index, subtree)?;
    allocate_inserted_subtree(&language, &mut manifest, &address, Some(parent.id.clone()))?;
    finish_edit(language, manifest)
}

fn insertion_site(
    language: &Language,
    manifest: &IdentityManifestV2,
    parent: &NodeEntryV1,
    anchor: &Anchor,
    subtree: &DetachedNode,
) -> Result<(ListKey, usize, NodeAddress), EditError> {
    let child = subtree.kind();
    let (key, length, group) = match (parent.kind, child) {
        (NodeKind::Language, NodeKind::DslDeclaration) => {
            (ListKey::Dsl, language.dsl_decls.len(), None)
        }
        (NodeKind::Language, NodeKind::Distribution) => {
            (ListKey::Distribution, language.distribution.len(), None)
        }
        (NodeKind::Language, NodeKind::Trait) => (ListKey::Traits, language.traits.len(), None),
        (NodeKind::Language, NodeKind::Sign) => (ListKey::Signs, language.signs.len(), None),
        (NodeKind::Trait, NodeKind::Block) => {
            let trait_def = trait_at(language, &parent.address)?;
            (ListKey::Blocks, trait_def.blocks.len(), None)
        }
        (NodeKind::Sign | NodeKind::Block | NodeKind::CaseBranch, kind) if is_item_kind(kind) => {
            let items = items_at(language, parent)?;
            (
                ListKey::Items,
                items.len(),
                match subtree {
                    DetachedNode::Item(item) => Some(item_group(item)),
                    _ => None,
                },
            )
        }
        (NodeKind::Rule | NodeKind::FeatureRule, NodeKind::RuleElseBranch) => {
            let rule = rule_at(language, &parent.address)?;
            (ListKey::RuleElse, rule.else_chain.len(), None)
        }
        (NodeKind::Rule | NodeKind::FeatureRule, NodeKind::RuleThenBranch) => {
            let rule = rule_at(language, &parent.address)?;
            (ListKey::RuleThen, rule.then_chain.len(), None)
        }
        (NodeKind::Case, NodeKind::CaseBranch) => {
            let case = case_at(language, &parent.address).ok_or_else(|| {
                EditError::FieldMismatch("case parent address is stale".to_owned())
            })?;
            (ListKey::CaseBranches, case.branches.len(), None)
        }
        // P46 S3: insert a statement into a phon `Leaf`, or a sub-block into a
        // phon `Then`/`Else`. Parent is a rule (its phon_block root) or a nested
        // `PhonBlockNode`.
        (
            NodeKind::Rule | NodeKind::FeatureRule | NodeKind::PhonBlockNode,
            NodeKind::PhonStatement,
        ) => match phon_container_block(language, &parent.address)? {
            PhonBlock::Leaf(statements) => (ListKey::PhonLeaf, statements.len(), None),
            _ => {
                return Err(EditError::ParentKind {
                    child,
                    parent: parent.kind,
                })
            }
        },
        (
            NodeKind::Rule | NodeKind::FeatureRule | NodeKind::PhonBlockNode,
            NodeKind::PhonBlockNode,
        ) => match phon_container_block(language, &parent.address)? {
            PhonBlock::Then(elements) => (ListKey::PhonThen, elements.len(), None),
            PhonBlock::Else(elements) => (ListKey::PhonElse, elements.len(), None),
            _ => {
                return Err(EditError::ParentKind {
                    child,
                    parent: parent.kind,
                })
            }
        },
        (NodeKind::Language, NodeKind::Prosody) if language.prosody.is_empty() => {
            if !matches!(anchor, Anchor::End) {
                return Err(EditError::AnchorInvalid(
                    "the singleton prosody declaration only accepts End".to_owned(),
                ));
            }
            return Ok((ListKey::Dsl, 0, NodeAddress(vec![AddressSegment::Prosody])));
        }
        _ => {
            return Err(EditError::ParentKind {
                child,
                parent: parent.kind,
            })
        }
    };
    if matches!(
        key,
        ListKey::Traits | ListKey::Signs | ListKey::Distribution
    ) {
        if !matches!(anchor, Anchor::End) {
            return Err(EditError::AnchorInvalid(
                "canonical unordered collections accept only End".to_owned(),
            ));
        }
        let index = canonical_unordered_index(language, key, subtree);
        return Ok((key, index, parent.address.child(segment(key, index))));
    }
    let index = anchor_index(language, manifest, parent, key, length, group, anchor)?;
    Ok((key, index, parent.address.child(segment(key, index))))
}

fn canonical_unordered_index(language: &Language, key: ListKey, subtree: &DetachedNode) -> usize {
    match (key, subtree) {
        (ListKey::Traits, DetachedNode::Trait(value)) => language
            .traits
            .iter()
            .position(|item| (!item.global, &item.name) > (!value.global, &value.name))
            .unwrap_or(language.traits.len()),
        (ListKey::Signs, DetachedNode::Sign(value)) => language
            .signs
            .iter()
            .position(|item| item.name > value.name)
            .unwrap_or(language.signs.len()),
        (ListKey::Distribution, DetachedNode::Distribution { key, value }) => language
            .distribution
            .iter()
            .position(|item| item > &(key.clone(), value.clone()))
            .unwrap_or(language.distribution.len()),
        _ => 0,
    }
}

fn anchor_index(
    language: &Language,
    manifest: &IdentityManifestV2,
    parent: &NodeEntryV1,
    key: ListKey,
    length: usize,
    group: Option<u16>,
    anchor: &Anchor,
) -> Result<usize, EditError> {
    let group_bounds = || -> Result<(usize, usize), EditError> {
        if key != ListKey::Items {
            return Ok((0, length));
        }
        let items = items_at(language, parent)?;
        let group =
            group.ok_or_else(|| EditError::AnchorInvalid("missing item group".to_owned()))?;
        let start = items
            .iter()
            .position(|item| item_group(item) >= group)
            .unwrap_or(items.len());
        let end = items
            .iter()
            .position(|item| item_group(item) > group)
            .unwrap_or(items.len());
        Ok((start, end))
    };
    match anchor {
        Anchor::Start => Ok(group_bounds()?.0),
        Anchor::End => Ok(group_bounds()?.1),
        Anchor::Before(reference) | Anchor::After(reference) => {
            let anchor_entry = manifest
                .nodes
                .iter()
                .find(|entry| entry.id == reference.id && entry.kind == reference.expected)
                .ok_or_else(|| EditError::TargetNotFound(reference.clone()))?;
            if anchor_entry.parent.as_ref() != Some(&parent.id) {
                return Err(EditError::AnchorInvalid(
                    "Before/After anchor belongs to another parent".to_owned(),
                ));
            }
            let position = parent.address.0.len();
            let index = anchor_entry
                .address
                .0
                .get(position)
                .and_then(|segment| segment_index(segment, key))
                .ok_or_else(|| {
                    EditError::AnchorInvalid("anchor is in another logical sequence".to_owned())
                })?;
            if let Some(group) = group {
                let anchor_item = items_at(language, parent)?
                    .get(index)
                    .ok_or_else(|| EditError::AnchorInvalid("anchor index is stale".to_owned()))?;
                if item_group(anchor_item) != group {
                    return Err(EditError::AnchorInvalid(
                        "anchor is in another canonical item group".to_owned(),
                    ));
                }
            }
            Ok(index + usize::from(matches!(anchor, Anchor::After(_))))
        }
    }
}

fn insert_payload(
    language: &mut Language,
    parent: &NodeEntryV1,
    index: usize,
    subtree: DetachedNode,
) -> Result<(), EditError> {
    match subtree {
        DetachedNode::DslDeclaration(value) if parent.kind == NodeKind::Language => {
            language.dsl_decls.insert(index, value)
        }
        DetachedNode::Prosody(value) if parent.kind == NodeKind::Language => {
            language.prosody = value
        }
        DetachedNode::Distribution { key, value } if parent.kind == NodeKind::Language => {
            language.distribution.insert(index, (key, value))
        }
        DetachedNode::Trait(value) if parent.kind == NodeKind::Language => {
            language.traits.insert(index, value)
        }
        DetachedNode::Sign(mut value) if parent.kind == NodeKind::Language => {
            value.id = language.fresh_sign_id();
            reassign_rule_ids(language, &mut value.items);
            language.signs.insert(index, value)
        }
        DetachedNode::Block(value) if parent.kind == NodeKind::Trait => {
            trait_at_mut(language, &parent.address)?
                .blocks
                .insert(index, value)
        }
        DetachedNode::Item(mut value)
            if matches!(
                parent.kind,
                NodeKind::Sign | NodeKind::Block | NodeKind::CaseBranch
            ) =>
        {
            reassign_item_rule_id(language, &mut value);
            items_at_mut(language, parent)?.insert(index, value)
        }
        DetachedNode::RuleElseBranch(value)
            if matches!(parent.kind, NodeKind::Rule | NodeKind::FeatureRule) =>
        {
            rule_at_mut(language, &parent.address)?
                .else_chain
                .insert(index, value)
        }
        DetachedNode::RuleThenBranch(value)
            if matches!(parent.kind, NodeKind::Rule | NodeKind::FeatureRule) =>
        {
            rule_at_mut(language, &parent.address)?
                .then_chain
                .insert(index, value)
        }
        DetachedNode::PhonStatement(value)
            if matches!(
                parent.kind,
                NodeKind::Rule | NodeKind::FeatureRule | NodeKind::PhonBlockNode
            ) =>
        {
            match phon_container_block_mut(language, &parent.address)? {
                PhonBlock::Leaf(statements) => statements.insert(index, value),
                _ => {
                    return Err(EditError::ParentKind {
                        child: NodeKind::PhonStatement,
                        parent: parent.kind,
                    })
                }
            }
        }
        DetachedNode::PhonBlockNode(value)
            if matches!(
                parent.kind,
                NodeKind::Rule | NodeKind::FeatureRule | NodeKind::PhonBlockNode
            ) =>
        {
            match phon_container_block_mut(language, &parent.address)? {
                PhonBlock::Then(elements) | PhonBlock::Else(elements) => {
                    elements.insert(index, value)
                }
                _ => {
                    return Err(EditError::ParentKind {
                        child: NodeKind::PhonBlockNode,
                        parent: parent.kind,
                    })
                }
            }
        }
        DetachedNode::CaseBranch(value) if parent.kind == NodeKind::Case => {
            case_at_mut(language, &parent.address)?
                .branches
                .insert(index, value)
        }
        other => {
            return Err(EditError::ParentKind {
                child: other.kind(),
                parent: parent.kind,
            })
        }
    }
    Ok(())
}

fn reassign_rule_ids(language: &mut Language, items: &mut [SignItem]) {
    for item in items {
        reassign_item_rule_id(language, item);
    }
}

fn reassign_item_rule_id(language: &mut Language, item: &mut SignItem) {
    if let SignItem::Rule(rule) | SignItem::FeatureRule(rule) = item {
        rule.id = language.fresh_rule_id();
    }
}

fn allocate_inserted_subtree(
    language: &Language,
    manifest: &mut IdentityManifestV2,
    address: &NodeAddress,
    parent: Option<NodeId>,
) -> Result<NodeId, EditError> {
    let kind = kind_at(language, address).ok_or_else(|| {
        EditError::FieldMismatch(format!("inserted node at {address:?} is not addressable"))
    })?;
    let id = allocate_id(manifest)?;
    manifest.nodes.push(NodeEntryV1 {
        id: id.clone(),
        kind,
        parent,
        address: address.clone(),
    });
    for child in child_addresses(language, address, kind)? {
        allocate_inserted_subtree(language, manifest, &child, Some(id.clone()))?;
    }
    Ok(id)
}

fn child_addresses(
    language: &Language,
    address: &NodeAddress,
    kind: NodeKind,
) -> Result<Vec<NodeAddress>, EditError> {
    let mut children = Vec::new();
    match kind {
        NodeKind::Trait => {
            for index in 0..trait_at(language, address)?.blocks.len() {
                children.push(address.child(AddressSegment::Blocks(index)));
            }
        }
        NodeKind::Sign | NodeKind::Block => {
            let entry = NodeEntryV1 {
                id: NodeId::new(conlang_language::IdentityNamespace::Synthetic, 0),
                kind,
                parent: None,
                address: address.clone(),
            };
            for index in 0..items_at(language, &entry)?.len() {
                children.push(address.child(AddressSegment::Items(index)));
            }
        }
        NodeKind::Rule | NodeKind::FeatureRule => {
            let rule = rule_at(language, address)?;
            for index in 0..rule.else_chain.len() {
                children.push(address.child(AddressSegment::RuleElse(index)));
            }
            for index in 0..rule.then_chain.len() {
                children.push(address.child(AddressSegment::RuleThen(index)));
            }
            if let Some(block) = &rule.phon_block {
                push_phon_children(block, address, &mut children);
            }
        }
        NodeKind::PhonBlockNode => {
            let block = phon_container_block(language, address)?;
            push_phon_children(block, address, &mut children);
        }
        NodeKind::Realization => {
            if realization_at(language, address)?.expression.is_some() {
                children.push(address.child(AddressSegment::CaseExpression));
            }
        }
        NodeKind::Case => {
            let case = case_at(language, address)
                .ok_or_else(|| EditError::FieldMismatch("case address is stale".to_owned()))?;
            for index in 0..case.branches.len() {
                children.push(address.child(AddressSegment::CaseBranches(index)));
            }
        }
        NodeKind::CaseBranch => {
            let branch = case_branch_at(language, address)?;
            match &branch.result {
                Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
                    for index in 0..items.len() {
                        children.push(address.child(AddressSegment::Items(index)));
                    }
                }
                result if expression_node_kind(result).is_some() => {
                    children.push(address.child(AddressSegment::CaseResult));
                }
                _ => {}
            }
        }
        NodeKind::Application => {
            let application = application_at(language, address)?;
            for (index, argument) in application.arguments.iter().enumerate() {
                if matches!(argument.value, SignArgumentValue::Application(_)) {
                    children.push(address.child(AddressSegment::ApplicationArguments(index)));
                }
            }
        }
        _ => {}
    }
    Ok(children)
}

fn delete(source: LanguageDocument, node_ref: &NodeRef) -> Result<LanguageDocument, EditError> {
    let node = ensure_target(&source, node_ref)?.clone();
    if node.kind == NodeKind::Language {
        return Err(EditError::RootImmutable);
    }
    let parent_address = node.address.parent().ok_or(EditError::RootImmutable)?;
    let list_position = if node.kind == NodeKind::Prosody {
        None
    } else {
        Some(address_list_position(&node.address)?)
    };
    let (mut language, mut manifest) = source.into_edit_parts();
    delete_payload(&mut language, &node)?;
    let removed = extract_identity_subtree(&mut manifest, &node.address);
    let removed_ids: BTreeSet<_> = removed.iter().map(|entry| entry.id.clone()).collect();
    manifest
        .refs
        .retain(|binding| !removed_ids.contains(&binding.owner));
    if let Some((key, index)) = list_position {
        shift_list(&mut manifest, &parent_address, key, index + 1, -1);
    }
    finish_edit(language, manifest)
}

fn delete_payload(language: &mut Language, node: &NodeEntryV1) -> Result<(), EditError> {
    match node.address.0.as_slice() {
        [AddressSegment::DslDeclarations(index)] => {
            language.dsl_decls.remove(*index);
        }
        [AddressSegment::Prosody] => language.prosody.clear(),
        [AddressSegment::Distribution(index)] => {
            language.distribution.remove(*index);
        }
        [AddressSegment::Traits(index)] => {
            language.traits.remove(*index);
        }
        [AddressSegment::Signs(index)] => {
            language.signs.remove(*index);
        }
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block)] => {
            language.traits[*trait_index].blocks.remove(*block);
        }
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block), AddressSegment::Items(item)] =>
        {
            language.traits[*trait_index].blocks[*block]
                .items
                .remove(*item);
        }
        [AddressSegment::Signs(sign), AddressSegment::Items(item)] => {
            language.signs[*sign].items.remove(*item);
        }
        path => delete_nested_payload(language, path)?,
    }
    Ok(())
}

fn delete_nested_payload(
    language: &mut Language,
    path: &[AddressSegment],
) -> Result<(), EditError> {
    let address = NodeAddress(path[..path.len() - 1].to_vec());
    match path.last() {
        Some(AddressSegment::RuleElse(index)) => {
            rule_at_mut(language, &address)?.else_chain.remove(*index);
        }
        Some(AddressSegment::RuleThen(index)) => {
            rule_at_mut(language, &address)?.then_chain.remove(*index);
        }
        Some(AddressSegment::PhonLeaf(index)) => {
            match phon_container_block_mut(language, &address)? {
                PhonBlock::Leaf(statements) => {
                    statements.remove(*index);
                }
                _ => {
                    return Err(EditError::FieldMismatch(
                        "phon statement parent is not a Leaf".to_owned(),
                    ))
                }
            }
        }
        Some(AddressSegment::PhonThen(index) | AddressSegment::PhonElse(index)) => {
            match phon_container_block_mut(language, &address)? {
                PhonBlock::Then(elements) | PhonBlock::Else(elements) => {
                    elements.remove(*index);
                }
                _ => {
                    return Err(EditError::FieldMismatch(
                        "phon sub-block parent is not a Then/Else".to_owned(),
                    ))
                }
            }
        }
        Some(AddressSegment::CaseBranches(index)) => {
            case_at_mut(language, &address)?.branches.remove(*index);
        }
        Some(AddressSegment::Items(index)) => {
            let parent = NodeEntryV1 {
                id: NodeId::new(conlang_language::IdentityNamespace::Synthetic, 0),
                kind: NodeKind::CaseBranch,
                parent: None,
                address,
            };
            items_at_mut(language, &parent)?.remove(*index);
        }
        _ => {
            return Err(EditError::FieldMismatch(
                "unsupported delete address".to_owned(),
            ))
        }
    }
    Ok(())
}

fn update(
    source: LanguageDocument,
    node_ref: &NodeRef,
    change: NodeUpdate,
) -> Result<LanguageDocument, EditError> {
    let node = ensure_target(&source, node_ref)?.clone();
    let (mut language, mut manifest) = source.into_edit_parts();
    let old_group = item_at_address(&language, &node.address).map(item_group);
    let explicit_ref_field = update_payload(&mut language, &node, change)?;
    if let Some(field) = explicit_ref_field {
        manifest.refs.retain(|binding| {
            if binding.owner != node.id {
                return true;
            }
            if field == "case.references" {
                !binding.field.starts_with("case.")
            } else {
                binding.field != field
            }
        });
    }
    sync_identity_descendants(&language, &mut manifest, &node)?;
    if matches!(
        node.kind,
        NodeKind::Sign | NodeKind::Trait | NodeKind::Distribution
    ) {
        reorder_named_container(&mut language, &mut manifest, &node)?;
    } else if let Some(old_group) = old_group {
        let new_group = item_at_address(&language, &node.address)
            .map(item_group)
            .unwrap_or(old_group);
        if old_group != new_group {
            reorder_item(&mut language, &mut manifest, &node, new_group)?;
        }
    }
    finish_edit(language, manifest)
}

/// Reconcile expression descendants after a typed update while retaining the
/// identity of every unchanged `(address, kind)` node. Newly introduced
/// nested applications/cases receive IDs from the active allocator, and
/// removed descendants lose both their node entries and owned references.
fn sync_identity_descendants(
    language: &Language,
    manifest: &mut IdentityManifestV2,
    root: &NodeEntryV1,
) -> Result<(), EditError> {
    let mut reusable = BTreeMap::new();
    let mut removed_ids = BTreeSet::new();
    manifest.nodes.retain(|entry| {
        let descendant = entry.address.starts_with(&root.address) && entry.address != root.address;
        if descendant {
            reusable.insert((entry.address.clone(), entry.kind), entry.id.clone());
            removed_ids.insert(entry.id.clone());
            false
        } else {
            true
        }
    });
    let mut reused = BTreeSet::new();
    for address in child_addresses(language, &root.address, root.kind)? {
        restore_identity_subtree(
            language,
            manifest,
            &reusable,
            &mut reused,
            &address,
            root.id.clone(),
        )?;
    }
    removed_ids.retain(|id| !reused.contains(id));
    manifest
        .refs
        .retain(|binding| !removed_ids.contains(&binding.owner));
    Ok(())
}

fn restore_identity_subtree(
    language: &Language,
    manifest: &mut IdentityManifestV2,
    reusable: &BTreeMap<(NodeAddress, NodeKind), NodeId>,
    reused: &mut BTreeSet<NodeId>,
    address: &NodeAddress,
    parent: NodeId,
) -> Result<(), EditError> {
    let kind = kind_at(language, address).ok_or_else(|| {
        EditError::FieldMismatch(format!("updated child at {address:?} is not addressable"))
    })?;
    let id = if let Some(id) = reusable.get(&(address.clone(), kind)) {
        reused.insert(id.clone());
        id.clone()
    } else {
        allocate_id(manifest)?
    };
    manifest.nodes.push(NodeEntryV1 {
        id: id.clone(),
        kind,
        parent: Some(parent),
        address: address.clone(),
    });
    for child in child_addresses(language, address, kind)? {
        restore_identity_subtree(language, manifest, reusable, reused, &child, id.clone())?;
    }
    Ok(())
}

fn update_payload(
    language: &mut Language,
    node: &NodeEntryV1,
    change: NodeUpdate,
) -> Result<Option<String>, EditError> {
    match (&node.kind, change) {
        (NodeKind::Sign, NodeUpdate::Rename(name)) => {
            let sign = sign_at_mut(language, &node.address)?;
            let old = std::mem::replace(&mut sign.name, name.clone());
            rewrite_sign_refs(language, &old, &name);
            Ok(None)
        }
        (NodeKind::Trait, NodeUpdate::Rename(name)) => {
            let trait_def = trait_at_mut(language, &node.address)?;
            let old = std::mem::replace(&mut trait_def.name, name.clone());
            rewrite_trait_refs(language, &old, &name);
            Ok(None)
        }
        (NodeKind::Trait, NodeUpdate::TraitGlobal(value)) => {
            trait_at_mut(language, &node.address)?.global = value;
            Ok(None)
        }
        (NodeKind::DslDeclaration, NodeUpdate::DslDeclaration(value)) => {
            let [AddressSegment::DslDeclarations(index)] = node.address.0.as_slice() else {
                return Err(field_mismatch(node, "dsl declaration"));
            };
            language.dsl_decls[*index] = value;
            Ok(None)
        }
        (NodeKind::Prosody, NodeUpdate::Prosody(value)) => {
            language.prosody = value;
            Ok(None)
        }
        (NodeKind::Distribution, NodeUpdate::Distribution { key, value }) => {
            let [AddressSegment::Distribution(index)] = node.address.0.as_slice() else {
                return Err(field_mismatch(node, "distribution"));
            };
            language.distribution[*index] = (key, value);
            Ok(None)
        }
        (NodeKind::Definition, NodeUpdate::DefinitionPath(value)) => {
            definition_at_mut(language, &node.address)?.path = value;
            Ok(None)
        }
        (NodeKind::Definition, NodeUpdate::DefinitionValue(value)) => {
            let def = definition_at_mut(language, &node.address)?;
            let field = (def.path == "origin").then_some("origin".to_owned());
            def.value = value;
            Ok(field)
        }
        (NodeKind::Rule | NodeKind::FeatureRule, NodeUpdate::RuleBody(value)) => {
            rule_at_mut(language, &node.address)?.body = value;
            Ok(None)
        }
        (NodeKind::Rule | NodeKind::FeatureRule, NodeUpdate::RuleStage(value)) => {
            rule_at_mut(language, &node.address)?.stage = value;
            Ok(None)
        }
        (NodeKind::Rule | NodeKind::FeatureRule, NodeUpdate::RuleDimension(value)) => {
            rule_at_mut(language, &node.address)?.dim = value;
            Ok(None)
        }
        (
            NodeKind::RuleElseBranch | NodeKind::RuleThenBranch,
            NodeUpdate::RuleBranchBody(value),
        ) => {
            set_rule_branch(language, &node.address, value)?;
            Ok(None)
        }
        (NodeKind::Sense, NodeUpdate::SenseGloss(value)) => {
            match item_at_address_mut(language, &node.address)? {
                SignItem::Sense(sense) => sense.gloss = value,
                _ => return Err(field_mismatch(node, "sense")),
            }
            Ok(None)
        }
        (NodeKind::SenseEdge, NodeUpdate::SenseEdgeKind(value)) => {
            match item_at_address_mut(language, &node.address)? {
                SignItem::SenseEdge(edge) => edge.kind = value,
                _ => return Err(field_mismatch(node, "sense edge")),
            }
            Ok(None)
        }
        (NodeKind::SenseEdge, NodeUpdate::SenseEdgeTransparency(value)) => {
            match item_at_address_mut(language, &node.address)? {
                SignItem::SenseEdge(edge) => edge.transparency = value,
                _ => return Err(field_mismatch(node, "sense edge")),
            }
            Ok(None)
        }
        (NodeKind::PhonStatement, NodeUpdate::RuleBranchBody(value)) => {
            *phon_statement_at_mut(language, &node.address)? = value;
            Ok(None)
        }
        (NodeKind::Rule | NodeKind::FeatureRule, NodeUpdate::Propagate(value)) => {
            let rule = rule_at_mut(language, &node.address)?;
            if rule.phon_block.is_none() {
                return Err(EditError::FieldMismatch(
                    "`propagate` applies to a phon block rule".to_owned(),
                ));
            }
            rule.propagate = value;
            Ok(None)
        }
        (NodeKind::PhonBlockNode, NodeUpdate::Propagate(value)) => {
            // Wrap/unwrap the element in place. `Propagate` contributes no address
            // segment, so child identities are untouched by this toggle.
            let slot = phon_element_raw_mut(language, &node.address)?;
            let current = std::mem::replace(slot, PhonBlock::Leaf(Vec::new()));
            let bare = match current {
                PhonBlock::Propagate(inner) => *inner,
                other => other,
            };
            *slot = if value {
                PhonBlock::Propagate(Box::new(bare))
            } else {
                bare
            };
            Ok(None)
        }
        (NodeKind::Slot, NodeUpdate::SlotName(value)) => {
            let old = slot_at_mut(language, &node.address)?.name.clone();
            let scope = slot_rename_scope(language, node, &old)?;
            slot_at_mut(language, &node.address)?.name = value.clone();
            rewrite_slot_consumers(language, &scope, &old, &value);
            Ok(None)
        }
        (NodeKind::Slot, NodeUpdate::SlotConstraint(value)) => {
            slot_at_mut(language, &node.address)?.constraint = value;
            Ok(Some("slot.constraint".to_owned()))
        }
        (NodeKind::Slot, NodeUpdate::SlotOptional(value)) => {
            slot_at_mut(language, &node.address)?.optional = value;
            Ok(None)
        }
        (NodeKind::TraitUse, NodeUpdate::TraitUse { name, block }) => {
            *item_at_address_mut(language, &node.address)? = SignItem::TraitUse { name, block };
            Ok(Some("trait_use.name".to_owned()))
        }
        (NodeKind::Belongs, NodeUpdate::Belongs(value)) => {
            *item_at_address_mut(language, &node.address)? = SignItem::Belongs(value);
            Ok(Some("belongs".to_owned()))
        }
        (NodeKind::FeatureDeclaration, NodeUpdate::FeatureDeclaration(value)) => {
            *item_at_address_mut(language, &node.address)? = SignItem::FeatureDecl(value);
            Ok(None)
        }
        (NodeKind::FeatureValue, NodeUpdate::FeatureValue(value)) => {
            *item_at_address_mut(language, &node.address)? = SignItem::FeatureValue(value);
            Ok(None)
        }
        (NodeKind::SlotFeatureBinding, NodeUpdate::SlotFeatureBinding(value)) => {
            *item_at_address_mut(language, &node.address)? = SignItem::SlotFeatureBinding(value);
            Ok(None)
        }
        (NodeKind::SlotMap, NodeUpdate::SlotMap(value)) => {
            let field = matches!(value, SlotMapOp::AutoFill { .. })
                .then_some("slot_map.autofill".to_owned());
            *item_at_address_mut(language, &node.address)? = SignItem::SlotMap(value);
            Ok(field)
        }
        (NodeKind::RoleDeclaration, NodeUpdate::RoleDeclaration(value)) => {
            *item_at_address_mut(language, &node.address)? = SignItem::RoleDecl(value);
            Ok(Some("role.constraint".to_owned()))
        }
        (NodeKind::RoleBinding, NodeUpdate::RoleBinding(value)) => {
            *item_at_address_mut(language, &node.address)? = SignItem::RoleBinding(value);
            Ok(None)
        }
        (NodeKind::Case, NodeUpdate::CaseSelection(value)) => {
            case_at_mut(language, &node.address)?.selection = value;
            Ok(None)
        }
        (NodeKind::CaseBranch, NodeUpdate::CaseBranch(value)) => {
            *case_branch_at_mut(language, &node.address)? = value;
            Ok(Some("case.references".to_owned()))
        }
        (NodeKind::Application, NodeUpdate::SignApplication(value)) => {
            *application_at_mut(language, &node.address)? = value;
            Ok(Some("application.callee".to_owned()))
        }
        (NodeKind::Constraint, NodeUpdate::Constraint(value)) => {
            *item_at_address_mut(language, &node.address)? = SignItem::Constraint(value);
            Ok(None)
        }
        _ => Err(field_mismatch(node, "update variant")),
    }
}

fn field_mismatch(node: &NodeEntryV1, field: &str) -> EditError {
    EditError::FieldMismatch(format!(
        "{field} is not editable on {:?} {}",
        node.kind, node.id
    ))
}

fn reorder_named_container(
    language: &mut Language,
    manifest: &mut IdentityManifestV2,
    node: &NodeEntryV1,
) -> Result<(), EditError> {
    let (key, old_index, new_index) = match node.address.0.as_slice() {
        [AddressSegment::Distribution(index)] => {
            let value = language.distribution.remove(*index);
            let new_index = language
                .distribution
                .iter()
                .position(|item| item > &value)
                .unwrap_or(language.distribution.len());
            language.distribution.insert(new_index, value);
            (ListKey::Distribution, *index, new_index)
        }
        [AddressSegment::Signs(index)] => {
            let value = language.signs.remove(*index);
            let new_index = language
                .signs
                .iter()
                .position(|item| item.name > value.name)
                .unwrap_or(language.signs.len());
            language.signs.insert(new_index, value);
            (ListKey::Signs, *index, new_index)
        }
        [AddressSegment::Traits(index)] => {
            let value = language.traits.remove(*index);
            let value_key = (!value.global, value.name.clone());
            let new_index = language
                .traits
                .iter()
                .position(|item| (!item.global, item.name.clone()) > value_key)
                .unwrap_or(language.traits.len());
            language.traits.insert(new_index, value);
            (ListKey::Traits, *index, new_index)
        }
        _ => return Ok(()),
    };
    if old_index == new_index {
        return Ok(());
    }
    move_manifest_prefix(manifest, &NodeAddress::root(), key, old_index, new_index);
    Ok(())
}

fn move_manifest_prefix(
    manifest: &mut IdentityManifestV2,
    parent: &NodeAddress,
    key: ListKey,
    old_index: usize,
    new_index: usize,
) {
    let old_prefix = parent.child(segment(key, old_index));
    let mut subtree = extract_identity_subtree(manifest, &old_prefix);
    shift_list(manifest, parent, key, old_index + 1, -1);
    shift_list(manifest, parent, key, new_index, 1);
    let new_prefix = parent.child(segment(key, new_index));
    for entry in &mut subtree {
        let suffix = entry.address.0[old_prefix.0.len()..].to_vec();
        entry.address.0 = new_prefix.0.clone();
        entry.address.0.extend(suffix);
    }
    manifest.nodes.extend(subtree);
}

fn reorder_item(
    language: &mut Language,
    manifest: &mut IdentityManifestV2,
    node: &NodeEntryV1,
    new_group: u16,
) -> Result<(), EditError> {
    let parent_address = node
        .address
        .parent()
        .ok_or_else(|| field_mismatch(node, "parent"))?;
    let parent = manifest
        .nodes
        .iter()
        .find(|entry| entry.address == parent_address)
        .cloned()
        .ok_or_else(|| field_mismatch(node, "parent"))?;
    let (_, old_index) = address_list_position(&node.address)?;
    let value = items_at_mut(language, &parent)?.remove(old_index);
    let old_prefix = node.address.clone();
    let mut subtree = extract_identity_subtree(manifest, &old_prefix);
    shift_list(manifest, &parent.address, ListKey::Items, old_index + 1, -1);
    let items = items_at(language, &parent)?;
    let new_index = items
        .iter()
        .position(|item| item_group(item) > new_group)
        .unwrap_or(items.len());
    shift_list(manifest, &parent.address, ListKey::Items, new_index, 1);
    items_at_mut(language, &parent)?.insert(new_index, value);
    let new_prefix = parent.address.child(AddressSegment::Items(new_index));
    for entry in &mut subtree {
        let suffix = entry.address.0[old_prefix.0.len()..].to_vec();
        entry.address.0 = new_prefix.0.clone();
        entry.address.0.extend(suffix);
    }
    manifest.nodes.extend(subtree);
    Ok(())
}

fn move_node(
    source: LanguageDocument,
    node_ref: &NodeRef,
    new_parent_ref: &NodeRef,
    anchor: &Anchor,
) -> Result<LanguageDocument, EditError> {
    let node = ensure_target(&source, node_ref)?.clone();
    let new_parent = ensure_target(&source, new_parent_ref)?.clone();
    if node.kind == NodeKind::Language {
        return Err(EditError::RootImmutable);
    }
    if new_parent.address.starts_with(&node.address) {
        return Err(EditError::Cycle);
    }
    let detached = detached_at(source.language(), &node)?;
    let old_parent_address = node.address.parent().ok_or(EditError::RootImmutable)?;
    let (old_key, old_index) = address_list_position(&node.address)?;
    let (mut language, mut manifest) = source.into_edit_parts();
    let identity_subtree = extract_identity_subtree(&mut manifest, &node.address);
    delete_payload(&mut language, &node)?;
    shift_list(
        &mut manifest,
        &old_parent_address,
        old_key,
        old_index + 1,
        -1,
    );

    let current_parent = manifest
        .nodes
        .iter()
        .find(|entry| entry.id == new_parent.id)
        .cloned()
        .ok_or_else(|| EditError::TargetNotFound(new_parent_ref.clone()))?;
    let (key, index, new_address) =
        insertion_site(&language, &manifest, &current_parent, anchor, &detached)?;
    shift_list(&mut manifest, &current_parent.address, key, index, 1);
    insert_payload_preserving_runtime(&mut language, &current_parent, index, detached)?;
    attach_identity_subtree(
        &mut manifest,
        identity_subtree,
        &node.address,
        &new_address,
        &current_parent.id,
    );
    finish_edit(language, manifest)
}

fn detached_at(language: &Language, node: &NodeEntryV1) -> Result<DetachedNode, EditError> {
    match node.address.0.as_slice() {
        [AddressSegment::DslDeclarations(index)] => Ok(DetachedNode::DslDeclaration(
            language.dsl_decls[*index].clone(),
        )),
        [AddressSegment::Traits(index)] => Ok(DetachedNode::Trait(language.traits[*index].clone())),
        [AddressSegment::Signs(index)] => Ok(DetachedNode::Sign(language.signs[*index].clone())),
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block)] => Ok(
            DetachedNode::Block(language.traits[*trait_index].blocks[*block].clone()),
        ),
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block), AddressSegment::Items(item)] => {
            Ok(DetachedNode::Item(
                language.traits[*trait_index].blocks[*block].items[*item].clone(),
            ))
        }
        [AddressSegment::Signs(sign), AddressSegment::Items(item)] => Ok(DetachedNode::Item(
            language.signs[*sign].items[*item].clone(),
        )),
        path => {
            let parent = NodeAddress(path[..path.len() - 1].to_vec());
            match path.last() {
                Some(AddressSegment::RuleElse(index)) => Ok(DetachedNode::RuleElseBranch(
                    rule_at(language, &parent)?.else_chain[*index].clone(),
                )),
                Some(AddressSegment::RuleThen(index)) => Ok(DetachedNode::RuleThenBranch(
                    rule_at(language, &parent)?.then_chain[*index].clone(),
                )),
                Some(AddressSegment::CaseBranches(index)) => Ok(DetachedNode::CaseBranch(
                    case_at(language, &parent)
                        .ok_or_else(|| field_mismatch(node, "move"))?
                        .branches
                        .get(*index)
                        .cloned()
                        .ok_or_else(|| field_mismatch(node, "move"))?,
                )),
                Some(AddressSegment::Items(index)) => {
                    let parent_entry = NodeEntryV1 {
                        id: NodeId::new(conlang_language::IdentityNamespace::Synthetic, 0),
                        kind: NodeKind::CaseBranch,
                        parent: None,
                        address: parent,
                    };
                    Ok(DetachedNode::Item(
                        items_at(language, &parent_entry)?
                            .get(*index)
                            .cloned()
                            .ok_or_else(|| field_mismatch(node, "move"))?,
                    ))
                }
                Some(AddressSegment::PhonLeaf(index)) => {
                    match phon_container_block(language, &parent)? {
                        PhonBlock::Leaf(statements) => Ok(DetachedNode::PhonStatement(
                            statements
                                .get(*index)
                                .cloned()
                                .ok_or_else(|| field_mismatch(node, "move"))?,
                        )),
                        _ => Err(field_mismatch(node, "move")),
                    }
                }
                Some(AddressSegment::PhonThen(index) | AddressSegment::PhonElse(index)) => {
                    let elements = match phon_container_block(language, &parent)? {
                        PhonBlock::Then(elements) | PhonBlock::Else(elements) => elements,
                        _ => return Err(field_mismatch(node, "move")),
                    };
                    Ok(DetachedNode::PhonBlockNode(
                        elements
                            .get(*index)
                            .cloned()
                            .ok_or_else(|| field_mismatch(node, "move"))?,
                    ))
                }
                _ => Err(field_mismatch(node, "move")),
            }
        }
    }
}

fn insert_payload_preserving_runtime(
    language: &mut Language,
    parent: &NodeEntryV1,
    index: usize,
    subtree: DetachedNode,
) -> Result<(), EditError> {
    match subtree {
        DetachedNode::Sign(value) if parent.kind == NodeKind::Language => {
            language.signs.insert(index, value);
            Ok(())
        }
        DetachedNode::Item(value)
            if matches!(
                parent.kind,
                NodeKind::Sign | NodeKind::Block | NodeKind::CaseBranch
            ) =>
        {
            items_at_mut(language, parent)?.insert(index, value);
            Ok(())
        }
        other => insert_payload(language, parent, index, other),
    }
}

fn address_list_position(address: &NodeAddress) -> Result<(ListKey, usize), EditError> {
    let segment = address.0.last().ok_or(EditError::RootImmutable)?;
    let result = match segment {
        AddressSegment::DslDeclarations(index) => (ListKey::Dsl, *index),
        AddressSegment::Distribution(index) => (ListKey::Distribution, *index),
        AddressSegment::Traits(index) => (ListKey::Traits, *index),
        AddressSegment::Signs(index) => (ListKey::Signs, *index),
        AddressSegment::Blocks(index) => (ListKey::Blocks, *index),
        AddressSegment::Items(index) => (ListKey::Items, *index),
        AddressSegment::RuleElse(index) => (ListKey::RuleElse, *index),
        AddressSegment::RuleThen(index) => (ListKey::RuleThen, *index),
        AddressSegment::PhonLeaf(index) => (ListKey::PhonLeaf, *index),
        AddressSegment::PhonThen(index) => (ListKey::PhonThen, *index),
        AddressSegment::PhonElse(index) => (ListKey::PhonElse, *index),
        AddressSegment::RealizationBranches(index) => (ListKey::Realization, *index),
        AddressSegment::CaseBranches(index) => (ListKey::CaseBranches, *index),
        AddressSegment::Prosody
        | AddressSegment::CaseExpression
        | AddressSegment::CaseResult
        | AddressSegment::ApplicationArguments(_) => {
            return Err(EditError::FieldMismatch(
                "node is not an editable semantic sequence element".to_owned(),
            ))
        }
    };
    Ok(result)
}

fn item_kind(item: &SignItem) -> NodeKind {
    match item {
        SignItem::TraitUse { .. } => NodeKind::TraitUse,
        SignItem::Belongs(_) => NodeKind::Belongs,
        SignItem::Slot(_) => NodeKind::Slot,
        SignItem::SlotMap(_) => NodeKind::SlotMap,
        SignItem::FeatureDecl(_) => NodeKind::FeatureDeclaration,
        SignItem::FeatureValue(_) => NodeKind::FeatureValue,
        SignItem::SlotFeatureBinding(_) => NodeKind::SlotFeatureBinding,
        SignItem::RoleDecl(_) => NodeKind::RoleDeclaration,
        SignItem::RoleBinding(_) => NodeKind::RoleBinding,
        SignItem::Sense(_) => NodeKind::Sense,
        SignItem::SenseEdge(_) => NodeKind::SenseEdge,
        SignItem::Realization(_) => NodeKind::Realization,
        SignItem::SignExpression(expression) => {
            expression_node_kind(&expression.expression).unwrap_or(NodeKind::Case)
        }
        SignItem::FeatureExpression(expression) => {
            expression_node_kind(&expression.expression).unwrap_or(NodeKind::Case)
        }
        SignItem::RoleExpression(expression) => {
            expression_node_kind(&expression.expression).unwrap_or(NodeKind::Case)
        }
        SignItem::Constraint(_) => NodeKind::Constraint,
        SignItem::FeatureRule(_) => NodeKind::FeatureRule,
        SignItem::Def(_) => NodeKind::Definition,
        SignItem::Rule(_) => NodeKind::Rule,
    }
}

// -------------------------------------------------------------------------
// Step 14: deterministic Primitive-only ChangeSet interpreter.

pub const CHANGESET_SCHEMA_V1: &str = "conlang.changeset/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryLock {
    pub package: LibraryId,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Selector {
    Stable(NodeRef),
    Language,
    Sign(String),
    Trait(String),
    Path(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UnresolvedOperation {
    Update {
        selector: Selector,
        field: String,
        value: String,
    },
    Delete(Selector),
    Move {
        node: Selector,
        parent: Selector,
        anchor: UnresolvedAnchor,
    },
    InsertSign {
        parent: Selector,
        anchor: UnresolvedAnchor,
        source: String,
    },
    /// General single-payload insert: the block is a verbatim `.lang` fragment
    /// yielding exactly one node (a `trait NAME:` / `sign NAME:`, or one sign
    /// body item). Lowers to a single `Insert`; the block reuses the `.lang`
    /// grammar and its dimension validation.
    InsertBlock {
        target: Selector,
        anchor: UnresolvedAnchor,
        block: String,
    },
    /// Authoring sugar: copy an existing sign under a fresh name. Lowers to a
    /// single `Insert` of a deep copy, so the resolved/dumped ChangeSet holds
    /// only the four primitives — `clone` never persists as its own operation.
    Clone {
        source: Selector,
        name: String,
    },
    /// 層②③④ **統一呼叫語法**:`name(位置參數, key: value, …)`,可尾接 `:` 帶
    /// 一段 `.lang` block 當最後一個參數。層級**由名字解析決定**,不靠關鍵字——
    /// 現階段只有 P16 的 12 個內建 Atomic Rewrite 解析得出來(封閉內建集);
    /// Recipe/Goal(步驟 16–17)落地後沿用同一文法,不必改 parser。
    ///
    /// 與 `clone` 同構:呼叫是**未解析層的授權糖**,`resolve` 時降成四原語,
    /// `ResolvedChangeSet` 維持 primitive-only(步驟 14 已封板的契約)。
    Call {
        name: String,
        positional: Option<String>,
        named: Vec<(String, String)>,
        block: Option<String>,
    },
}

/// 切一個具名參數 `key: value`。`key` 必須是識別字,否則視為位置參數
/// (例如 selector `sign("x")` 內部雖然沒有冒號,仍走這條保護)。
fn split_named_argument(argument: &str) -> Option<(String, String)> {
    let (key, value) = argument.split_once(':')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((key.to_owned(), unquote_change_value(value.trim())))
}

/// 解析 `name(arg, key: value, …)`。回傳 `None` 表示這行不是呼叫語法
/// (交回給既有的原語語句處理)。
fn parse_call_head(head: &str) -> Option<(String, Vec<String>)> {
    let head = head.strip_suffix(':').unwrap_or(head).trim_end();
    let open = head.find('(')?;
    let name = head[..open].trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    let args = head[open + 1..].strip_suffix(')')?;
    let args = args.trim();
    if args.is_empty() {
        return Some((name.to_owned(), Vec::new()));
    }
    // 以頂層逗號切分(括號內的逗號屬於 selector,如 `sign("a")` 沒有,但保守處理)。
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in args.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    parts.push(current);
    Some((
        name.to_owned(),
        parts
            .into_iter()
            .map(|part| part.trim().to_owned())
            .collect(),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UnresolvedAnchor {
    Start,
    End,
    Before(Selector),
    After(Selector),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedStatement {
    pub ordinal: u64,
    operations: Vec<UnresolvedOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedChangeSet {
    pub schema: String,
    pub namespace: String,
    pub base_source: String,
    pub base_identities: String,
    pub libraries: Vec<LibraryLock>,
    pub statements: Vec<UnresolvedStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStatement {
    pub ordinal: u64,
    pub edits: Vec<PrimitiveEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedChangeSet {
    pub schema: String,
    pub namespace: String,
    pub base_source: String,
    pub base_identities: String,
    pub libraries: Vec<LibraryLock>,
    pub statements: Vec<ResolvedStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementRecord {
    pub ordinal: u64,
    pub records: Vec<PrimitiveRecord>,
    pub diff: LanguageDiff,
    pub source_sha256: String,
    pub identities_sha256: String,
}

#[derive(Debug, Clone)]
pub struct ReplayOutcome {
    pub document: LanguageDocument,
    pub records: Vec<StatementRecord>,
    pub diff: LanguageDiff,
}

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("CHANGESET_PARSE: {0}")]
    Parse(String),
    #[error("CHANGESET_SCHEMA: unsupported schema {0}")]
    Schema(String),
    #[error("CHANGESET_BASE_SOURCE_MISMATCH")]
    BaseSourceMismatch,
    #[error("CHANGESET_BASE_IDENTITIES_MISMATCH")]
    BaseIdentitiesMismatch,
    #[error("CHANGESET_LIBRARY_LOCK_MISMATCH: {0}")]
    LibraryLockMismatch(String),
    #[error("CHANGESET_NAMESPACE_MISMATCH: {0}")]
    NamespaceMismatch(String),
    #[error("CHANGESET_SELECTOR: {0}")]
    Selector(String),
    #[error("CHANGESET_STATEMENT_{ordinal}: {source}")]
    Statement {
        ordinal: u64,
        #[source]
        source: EditError,
    },
    #[error("CHANGESET_IDENTITY: {0}")]
    Identity(#[from] IdentityError),
    #[error("CHANGESET_LIBRARY: {0}")]
    Library(String),
    #[error("CHANGESET_COMPILE: {0}")]
    Compile(#[from] CompileSystemError),
}

fn strip_digest(value: &str) -> String {
    value
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or(value.trim())
        .to_owned()
}

fn split_last_colon(value: &str) -> Result<(&str, u64), ReplayError> {
    let (namespace, ordinal) = value
        .rsplit_once(':')
        .ok_or_else(|| ReplayError::Parse(format!("invalid NodeId {value:?}")))?;
    let ordinal = ordinal
        .parse()
        .map_err(|_| ReplayError::Parse(format!("invalid NodeId ordinal {value:?}")))?;
    Ok((namespace, ordinal))
}

fn parse_kind(value: &str) -> Result<NodeKind, ReplayError> {
    match value.trim() {
        "language" => Ok(NodeKind::Language),
        "trait" => Ok(NodeKind::Trait),
        "sign" => Ok(NodeKind::Sign),
        "block" => Ok(NodeKind::Block),
        "definition" | "def" => Ok(NodeKind::Definition),
        "slot" => Ok(NodeKind::Slot),
        "role" => Ok(NodeKind::RoleDeclaration),
        "rule" => Ok(NodeKind::Rule),
        "then" => Ok(NodeKind::RuleThenBranch),
        "else" => Ok(NodeKind::RuleElseBranch),
        "phon_statement" => Ok(NodeKind::PhonStatement),
        "sense" => Ok(NodeKind::Sense),
        "sense_edge" => Ok(NodeKind::SenseEdge),
        "phon_block" => Ok(NodeKind::PhonBlockNode),
        "realization_branch" => Ok(NodeKind::RealizationBranch),
        "application" => Ok(NodeKind::Application),
        "case" => Ok(NodeKind::Case),
        "case_branch" => Ok(NodeKind::CaseBranch),
        "constraint" => Ok(NodeKind::Constraint),
        other => Err(ReplayError::Parse(format!("unknown node kind {other:?}"))),
    }
}

fn kind_keyword(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Language => "language",
        NodeKind::Trait => "trait",
        NodeKind::Sign => "sign",
        NodeKind::Block => "block",
        NodeKind::Definition => "definition",
        NodeKind::Slot => "slot",
        NodeKind::RoleDeclaration => "role",
        NodeKind::Rule | NodeKind::FeatureRule => "rule",
        NodeKind::RuleThenBranch => "then",
        NodeKind::RuleElseBranch => "else",
        NodeKind::PhonStatement => "phon_statement",
        NodeKind::Sense => "sense",
        NodeKind::SenseEdge => "sense_edge",
        NodeKind::PhonBlockNode => "phon_block",
        NodeKind::RealizationBranch => "realization_branch",
        NodeKind::Application => "application",
        NodeKind::Case => "case",
        NodeKind::CaseBranch => "case_branch",
        NodeKind::Constraint => "constraint",
        _ => "node",
    }
}

fn parse_selector(value: &str) -> Result<Selector, ReplayError> {
    let value = value.trim();
    if value.starts_with("language.")
        || ((value.starts_with("sign(\"")
            || value.starts_with("trait(\"")
            || value.starts_with("node("))
            && value.contains(")."))
    {
        return Ok(Selector::Path(value.to_owned()));
    }
    if value == "language" {
        return Ok(Selector::Language);
    }
    for (prefix, make) in [
        ("sign(\"", Selector::Sign as fn(String) -> Selector),
        ("trait(\"", Selector::Trait as fn(String) -> Selector),
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            let name = rest
                .strip_suffix("\")")
                .ok_or_else(|| ReplayError::Parse(format!("invalid selector {value:?}")))?;
            return Ok(make(name.to_owned()));
        }
    }
    if let Some(inner) = value
        .strip_prefix("node(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let (kind, id) = inner
            .split_once(',')
            .ok_or_else(|| ReplayError::Parse(format!("invalid node selector {value:?}")))?;
        let id = id.trim().strip_prefix('@').ok_or_else(|| {
            ReplayError::Parse(format!("stable node selector requires @NodeId: {value:?}"))
        })?;
        let (namespace, ordinal) = split_last_colon(id)?;
        return Ok(Selector::Stable(NodeRef::new(
            NodeId::new(
                conlang_language::IdentityNamespace::Document(namespace.to_owned()),
                ordinal,
            ),
            parse_kind(kind)?,
        )));
    }
    Err(ReplayError::Parse(format!("unknown selector {value:?}")))
}

fn dump_selector(selector: &Selector) -> String {
    match selector {
        Selector::Stable(reference) => format!(
            "node({}, @{})",
            kind_keyword(reference.expected),
            reference.id
        ),
        Selector::Language => "language".to_owned(),
        Selector::Sign(name) => format!("sign(\"{name}\")"),
        Selector::Trait(name) => format!("trait(\"{name}\")"),
        Selector::Path(value) => value.clone(),
    }
}

fn split_update_field(value: &str) -> Option<(&str, &str)> {
    let mut square = 0usize;
    let mut round = 0usize;
    let mut split = None;
    for (index, character) in value.char_indices() {
        match character {
            '[' => square += 1,
            ']' => square = square.saturating_sub(1),
            '(' => round += 1,
            ')' => round = round.saturating_sub(1),
            '.' if square == 0 && round == 0 => split = Some(index),
            _ => {}
        }
    }
    split.map(|index| (&value[..index], &value[index + 1..]))
}

fn parse_anchor(value: &str) -> Result<UnresolvedAnchor, ReplayError> {
    let value = value.trim();
    if value == "start" {
        Ok(UnresolvedAnchor::Start)
    } else if value == "end" {
        Ok(UnresolvedAnchor::End)
    } else if let Some(selector) = value.strip_prefix("before ") {
        Ok(UnresolvedAnchor::Before(parse_selector(selector)?))
    } else if let Some(selector) = value.strip_prefix("after ") {
        Ok(UnresolvedAnchor::After(parse_selector(selector)?))
    } else {
        Err(ReplayError::Parse(format!("invalid anchor {value:?}")))
    }
}

fn unquote_change_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_owned()
    }
}

fn parse_statement_body(ordinal: u64, body: &[String]) -> Result<UnresolvedStatement, ReplayError> {
    // Split the body into operation chunks: a line at indent 0 (after the outer
    // 8-space strip) starts a new operation; deeper block lines belong to the
    // current one. This lets a statement hold several operations (e.g. the
    // per-item inserts a fan-out lowers to) and validate only its final state.
    let mut chunks: Vec<Vec<String>> = Vec::new();
    for line in body {
        if line.chars().next().is_some_and(|c| !c.is_whitespace()) {
            chunks.push(vec![line.clone()]);
        } else {
            chunks
                .last_mut()
                .ok_or_else(|| {
                    ReplayError::Parse(format!(
                        "statement {ordinal} has an indented line before any operation"
                    ))
                })?
                .push(line.clone());
        }
    }
    if chunks.is_empty() {
        return Err(ReplayError::Parse(format!("statement {ordinal} is empty")));
    }
    let operations = chunks
        .iter()
        .map(|chunk| parse_operation(ordinal, chunk))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(UnresolvedStatement {
        ordinal,
        operations,
    })
}

fn parse_operation(ordinal: u64, body: &[String]) -> Result<UnresolvedOperation, ReplayError> {
    let first = body
        .first()
        .ok_or_else(|| ReplayError::Parse(format!("statement {ordinal} is empty")))?
        .trim();
    let operation = if let Some(rest) = first.strip_prefix("update ") {
        let (left, value) = rest
            .split_once(" = ")
            .ok_or_else(|| ReplayError::Parse(format!("invalid update in statement {ordinal}")))?;
        let (selector, field) = split_update_field(left).ok_or_else(|| {
            ReplayError::Parse(format!("update has no field in statement {ordinal}"))
        })?;
        UnresolvedOperation::Update {
            selector: parse_selector(selector)?,
            field: field.to_owned(),
            value: unquote_change_value(value),
        }
    } else if let Some(rest) = first.strip_prefix("delete ") {
        UnresolvedOperation::Delete(parse_selector(rest)?)
    } else if let Some(rest) = first.strip_prefix("move ") {
        let joined = body.join(" ");
        let joined = joined.trim();
        let rest = joined.strip_prefix("move ").unwrap_or(rest);
        let (node, tail) = rest.split_once(" to ").ok_or_else(|| {
            ReplayError::Parse(format!("move has no `to` in statement {ordinal}"))
        })?;
        let (parent, anchor) = tail.split_once(" at ").ok_or_else(|| {
            ReplayError::Parse(format!("move has no `at` in statement {ordinal}"))
        })?;
        UnresolvedOperation::Move {
            node: parse_selector(node)?,
            parent: parse_selector(parent)?,
            anchor: parse_anchor(anchor)?,
        }
    } else if let Some(rest) = first.strip_prefix("clone ") {
        let (source, name) = rest.rsplit_once(" as ").ok_or_else(|| {
            ReplayError::Parse(format!("clone has no `as <name>` in statement {ordinal}"))
        })?;
        let name = name.trim();
        if name.is_empty() {
            return Err(ReplayError::Parse(format!(
                "clone has an empty name in statement {ordinal}"
            )));
        }
        UnresolvedOperation::Clone {
            source: parse_selector(source)?,
            name: name.to_owned(),
        }
    } else if let Some(rest) = first.strip_prefix("insert into ") {
        let (target, anchor) = rest
            .strip_suffix(':')
            .unwrap_or(rest)
            .rsplit_once(" at ")
            .ok_or_else(|| {
                ReplayError::Parse(format!("insert has no `at` in statement {ordinal}"))
            })?;
        let block = body.iter().skip(1).cloned().collect::<Vec<_>>().join("\n");
        if block.trim().is_empty() {
            return Err(ReplayError::Parse(format!(
                "insert statement {ordinal} has no .lang block"
            )));
        }
        UnresolvedOperation::InsertBlock {
            target: parse_selector(target)?,
            anchor: parse_anchor(anchor)?,
            block,
        }
    } else if let Some(rest) = first.strip_prefix("insert sign under ") {
        let (parent, anchor) = rest
            .strip_suffix(':')
            .unwrap_or(rest)
            .split_once(" at ")
            .ok_or_else(|| {
                ReplayError::Parse(format!("insert has no `at` in statement {ordinal}"))
            })?;
        let fragment = body.iter().skip(1).cloned().collect::<Vec<_>>().join("\n");
        if fragment.trim().is_empty() {
            return Err(ReplayError::Parse(format!(
                "insert statement {ordinal} has no .lang fragment"
            )));
        }
        UnresolvedOperation::InsertSign {
            parent: parse_selector(parent)?,
            anchor: parse_anchor(anchor)?,
            source: fragment,
        }
    } else if let Some((name, args)) = parse_call_head(first) {
        // 層②③④ 統一呼叫。首個不含 `key:` 的參數是位置參數(通常是 selector)。
        let mut positional = None;
        let mut named = Vec::new();
        for arg in args {
            match split_named_argument(&arg) {
                Some((key, value)) => named.push((key, value)),
                None if positional.is_none() && !arg.is_empty() => positional = Some(arg),
                None if arg.is_empty() => {}
                None => {
                    return Err(ReplayError::Parse(format!(
                        "statement {ordinal}: {name}(…) takes at most one positional argument"
                    )))
                }
            }
        }
        let block = if first.trim_end().ends_with(':') {
            let text = body.iter().skip(1).cloned().collect::<Vec<_>>().join("\n");
            if text.trim().is_empty() {
                return Err(ReplayError::Parse(format!(
                    "statement {ordinal}: {name}(…): has no .lang block"
                )));
            }
            Some(text)
        } else {
            None
        };
        UnresolvedOperation::Call {
            name,
            positional,
            named,
            block,
        }
    } else {
        return Err(ReplayError::Parse(format!(
            "unsupported primitive statement {ordinal}: {first}"
        )));
    };
    Ok(operation)
}

impl UnresolvedChangeSet {
    pub fn parse(source: &str) -> Result<UnresolvedChangeSet, ReplayError> {
        // 三種格式共用 `/* … */` 區塊註解(擁有者 2026-07-12 定案;`#` 在 `.qy`
        // 已被詞界 D19 佔用)。剝除保留換行,行號不漂移。
        let source = conlang_language::parser::strip_comments(source);
        let source = source.as_str();
        let lines = source.lines().collect::<Vec<_>>();
        let header = lines
            .iter()
            .find(|line| !line.trim().is_empty())
            .ok_or_else(|| ReplayError::Parse("empty ChangeSet".to_owned()))?
            .trim();
        let namespace = header
            .strip_prefix("changeset ")
            .and_then(|rest| rest.strip_suffix(':'))
            .ok_or_else(|| ReplayError::Parse("expected `changeset <namespace>:`".to_owned()))?
            .to_owned();
        let mut schema = None;
        let mut base_source = None;
        let mut base_identities = None;
        let mut libraries = Vec::new();
        let mut statements = Vec::new();
        let mut index = 1;
        while index < lines.len() {
            let line = lines[index].trim();
            // 注意:`#` 現在是**語句標記**不是註解(註解已統一為 `/* … */`)。
            if line.is_empty() {
                index += 1;
                continue;
            }
            if let Some(value) = line.strip_prefix("schema = ") {
                schema = Some(value.trim().to_owned());
            } else if let Some(value) = line.strip_prefix("base_source = ") {
                base_source = Some(strip_digest(value));
            } else if let Some(value) = line.strip_prefix("base_identities = ") {
                base_identities = Some(strip_digest(value));
            } else if let Some(value) = line.strip_prefix("library ") {
                let (package_version, digest) = value
                    .split_once(' ')
                    .ok_or_else(|| ReplayError::Parse(format!("invalid library lock {line:?}")))?;
                let (package, version) = package_version.rsplit_once('@').ok_or_else(|| {
                    ReplayError::Parse(format!("library lock has no version {line:?}"))
                })?;
                libraries.push(LibraryLock {
                    package: package
                        .parse()
                        .map_err(|error| ReplayError::Parse(format!("{error}")))?,
                    version: version.to_owned(),
                    digest: strip_digest(digest),
                });
            } else if let Some(value) = line
                // `#N:` 為 canonical;`statement N:` 為舊形,仍接受(dump 排新形,
                // 非 canonical 正規化為不動點——與 `.lang` 的 `=`→`:` 同一作法)。
                .strip_prefix('#')
                .or_else(|| line.strip_prefix("statement "))
                .and_then(|value| value.strip_suffix(':'))
            {
                let ordinal = value.parse().map_err(|_| {
                    ReplayError::Parse(format!("invalid statement ordinal {value:?}"))
                })?;
                let mut body = Vec::new();
                index += 1;
                while index < lines.len() {
                    let candidate = lines[index];
                    // 下一句的標記結束本句 body——兩種寫法都要認,否則 `#1:` 會被
                    // 吞進前一句的 insert block。
                    let trimmed = candidate.trim();
                    if trimmed.starts_with("statement ")
                        || (trimmed.starts_with('#') && trimmed.ends_with(':'))
                    {
                        index -= 1;
                        break;
                    }
                    if !candidate.trim().is_empty() {
                        body.push(
                            candidate
                                .strip_prefix("        ")
                                .unwrap_or(candidate)
                                .to_owned(),
                        );
                    }
                    index += 1;
                }
                statements.push(parse_statement_body(ordinal, &body)?);
            } else {
                return Err(ReplayError::Parse(format!(
                    "unknown ChangeSet line {line:?}"
                )));
            }
            index += 1;
        }
        let schema = schema.ok_or_else(|| ReplayError::Parse("missing schema".to_owned()))?;
        if schema != CHANGESET_SCHEMA_V1 {
            return Err(ReplayError::Schema(schema));
        }
        statements.sort_by_key(|statement| statement.ordinal);
        if statements
            .windows(2)
            .any(|pair| pair[0].ordinal == pair[1].ordinal)
        {
            return Err(ReplayError::Parse(
                "statement ordinals must be unique".to_owned(),
            ));
        }
        libraries.sort_by(|left, right| left.package.cmp(&right.package));
        Ok(UnresolvedChangeSet {
            schema,
            namespace,
            base_source: base_source
                .ok_or_else(|| ReplayError::Parse("missing base_source".to_owned()))?,
            base_identities: base_identities
                .ok_or_else(|| ReplayError::Parse("missing base_identities".to_owned()))?,
            libraries,
            statements,
        })
    }

    pub fn resolve(
        &self,
        base: &LanguageDocument,
        libraries: &LibrarySpec,
    ) -> Result<ResolvedChangeSet, ReplayError> {
        verify_base_and_locks(
            base,
            libraries,
            &self.base_source,
            &self.base_identities,
            &self.libraries,
        )?;
        let mut working = base.fork(self.namespace.clone())?;
        let mut resolved = Vec::new();
        for statement in &self.statements {
            let mut edits = Vec::new();
            for operation in &statement.operations {
                edits.extend(resolve_operation(operation, &working)?);
            }
            let (candidate, _) =
                apply_statement_structural(&working, statement.ordinal, &edits, libraries)?;
            working = candidate;
            resolved.push(ResolvedStatement {
                ordinal: statement.ordinal,
                edits,
            });
        }
        Ok(ResolvedChangeSet {
            schema: self.schema.clone(),
            namespace: self.namespace.clone(),
            base_source: self.base_source.clone(),
            base_identities: self.base_identities.clone(),
            libraries: self.libraries.clone(),
            statements: resolved,
        })
    }
}

fn resolve_selector(
    selector: &Selector,
    document: &LanguageDocument,
) -> Result<NodeRef, ReplayError> {
    let reference = match selector {
        Selector::Stable(reference) => reference.clone(),
        Selector::Language => document.root_ref(),
        Selector::Sign(name) => document
            .ref_for_sign(name)
            .ok_or_else(|| ReplayError::Selector(format!("unknown sign {name:?}")))?,
        Selector::Trait(name) => document
            .ref_for_trait(name)
            .ok_or_else(|| ReplayError::Selector(format!("unknown trait {name:?}")))?,
        Selector::Path(value) => return resolve_authoring_path(value, document),
    };
    document
        .resolve_node(&reference)
        .map_err(|error| ReplayError::Selector(error.to_string()))?;
    Ok(reference)
}

fn resolve_authoring_path(
    value: &str,
    document: &LanguageDocument,
) -> Result<NodeRef, ReplayError> {
    let split = if let Some(suffix) = value.strip_prefix("language.") {
        ("language", suffix)
    } else {
        let marker = ").";
        let index = value
            .find(marker)
            .ok_or_else(|| ReplayError::Selector(format!("invalid authoring path {value:?}")))?;
        (&value[..index + 1], &value[index + marker.len()..])
    };
    let mut current = resolve_selector(&parse_selector(split.0)?, document)?;
    for segment in split_path_segments(split.1)? {
        current = resolve_path_child(document, &current, &segment)?;
    }
    Ok(current)
}

fn split_path_segments(value: &str) -> Result<Vec<String>, ReplayError> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (index, character) in value.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '.' if depth == 0 => {
                segments.push(value[start..index].to_owned());
                start = index + 1;
            }
            _ => {}
        }
    }
    segments.push(value[start..].to_owned());
    if depth != 0 || segments.iter().any(|segment| segment.is_empty()) {
        return Err(ReplayError::Selector(format!(
            "malformed authoring path {value:?}"
        )));
    }
    Ok(segments)
}

fn selector_argument(segment: &str) -> Result<(&str, &str), ReplayError> {
    segment
        .split_once('[')
        .and_then(|(kind, rest)| rest.strip_suffix(']').map(|argument| (kind, argument)))
        .ok_or_else(|| ReplayError::Selector(format!("malformed path segment {segment:?}")))
}

/// `[n]` = ordinal（回傳 None）；`["name"]` 或非數字 `[name]` = keyed（回傳 Some(name)）。
fn keyed_name(argument: &str) -> Option<&str> {
    match argument
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        Some(name) => Some(name),
        None if argument.parse::<usize>().is_err() => Some(argument),
        None => None,
    }
}

/// 取 case-branch 位址對應的 `@name` 標籤。
fn branch_name_at<'a>(language: &'a Language, address: &NodeAddress) -> Option<&'a str> {
    let (last, rest) = address.0.split_last()?;
    let AddressSegment::CaseBranches(index) = last else {
        return None;
    };
    case_at(language, &NodeAddress(rest.to_vec()))?
        .branches
        .get(*index)?
        .name
        .as_deref()
}

fn resolve_path_child(
    document: &LanguageDocument,
    parent: &NodeRef,
    segment: &str,
) -> Result<NodeRef, ReplayError> {
    let (kind, argument) = selector_argument(segment)?;
    let mut children = document
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.parent.as_ref() == Some(&parent.id))
        .collect::<Vec<_>>();
    children.sort_by(|left, right| left.address.cmp(&right.address));
    let numeric = || {
        argument.parse::<usize>().map_err(|_| {
            ReplayError::Selector(format!("selector index must be numeric: {segment:?}"))
        })
    };
    let entry = match kind {
        "block" => children
            .iter()
            .filter(|entry| entry.kind == NodeKind::Block)
            .nth(numeric()?)
            .copied(),
        "rule" => {
            // `rule[n]` = ordinal; `rule["name"]`/`rule[name]` = keyed `@name` label.
            let mut rules = children
                .iter()
                .filter(|entry| matches!(entry.kind, NodeKind::Rule | NodeKind::FeatureRule));
            match keyed_name(argument) {
                Some(name) => rules
                    .find(|entry| {
                        matches!(
                            item_at_address(document.language(), &entry.address),
                            Some(SignItem::Rule(rule) | SignItem::FeatureRule(rule))
                                if rule.name.as_deref() == Some(name)
                        )
                    })
                    .copied(),
                None => rules.nth(numeric()?).copied(),
            }
        }
        "case" => {
            let mut cases = children.iter().filter(|entry| entry.kind == NodeKind::Case);
            match keyed_name(argument) {
                Some(name) => cases
                    .find(|entry| {
                        case_at(document.language(), &entry.address)
                            .and_then(|case| case.name.as_deref())
                            == Some(name)
                    })
                    .copied(),
                None => cases.nth(numeric()?).copied(),
            }
        }
        "branch" => {
            let mut branches = children
                .iter()
                .filter(|entry| entry.kind == NodeKind::CaseBranch);
            match keyed_name(argument) {
                Some(name) => branches
                    .find(|entry| branch_name_at(document.language(), &entry.address) == Some(name))
                    .copied(),
                None => branches.nth(numeric()?).copied(),
            }
        }
        // P46 S3: a phon `Leaf` statement.
        "leaf" => children
            .iter()
            .filter(|entry| matches!(entry.address.0.last(), Some(AddressSegment::PhonLeaf(_))))
            .nth(numeric()?)
            .copied(),
        // `then`/`else` descend into a phon block when the parent has one (a phon
        // rule's flat then_chain/else_chain is empty); otherwise they address the
        // flat RuleThenBranch/RuleElseBranch chain.
        "then" => {
            let index = numeric()?;
            children
                .iter()
                .filter(|entry| matches!(entry.address.0.last(), Some(AddressSegment::PhonThen(_))))
                .nth(index)
                .or_else(|| {
                    children
                        .iter()
                        .filter(|entry| entry.kind == NodeKind::RuleThenBranch)
                        .nth(index)
                })
                .copied()
        }
        "else" => {
            let index = numeric()?;
            children
                .iter()
                .filter(|entry| matches!(entry.address.0.last(), Some(AddressSegment::PhonElse(_))))
                .nth(index)
                .or_else(|| {
                    children
                        .iter()
                        .filter(|entry| entry.kind == NodeKind::RuleElseBranch)
                        .nth(index)
                })
                .copied()
        }
        "def" => children.iter().copied().find(|entry| {
            entry.kind == NodeKind::Definition
                && matches!(
                    item_at_address(document.language(), &entry.address),
                    Some(SignItem::Def(def)) if def.path == argument
                )
        }),
        "slot" => children.iter().copied().find(|entry| {
            entry.kind == NodeKind::Slot
                && matches!(
                    item_at_address(document.language(), &entry.address),
                    Some(SignItem::Slot(slot)) if slot.name == argument
                )
        }),
        "role" => children.iter().copied().find(|entry| {
            entry.kind == NodeKind::RoleDeclaration
                && matches!(
                    item_at_address(document.language(), &entry.address),
                    Some(SignItem::RoleDecl(role)) if role.name == argument
                )
        }),
        // §10.3:`sense["log"]` 依義項名;`edge[n]` 依序數(邊無名字)。
        "sense" => {
            let name = keyed_name(argument).unwrap_or(argument);
            children.iter().copied().find(|entry| {
                entry.kind == NodeKind::Sense
                    && matches!(
                        item_at_address(document.language(), &entry.address),
                        Some(SignItem::Sense(sense)) if sense.name == name
                    )
            })
        }
        "edge" => children
            .iter()
            .filter(|entry| entry.kind == NodeKind::SenseEdge)
            .nth(numeric()?)
            .copied(),
        other => {
            return Err(ReplayError::Selector(format!(
                "unknown path selector {other:?}"
            )))
        }
    };
    entry
        .map(|entry| NodeRef::new(entry.id.clone(), entry.kind))
        .ok_or_else(|| {
            ReplayError::Selector(format!("cannot resolve {segment:?} below {}", parent.id))
        })
}

fn resolve_anchor(
    anchor: &UnresolvedAnchor,
    document: &LanguageDocument,
) -> Result<Anchor, ReplayError> {
    Ok(match anchor {
        UnresolvedAnchor::Start => Anchor::Start,
        UnresolvedAnchor::End => Anchor::End,
        UnresolvedAnchor::Before(selector) => Anchor::Before(resolve_selector(selector, document)?),
        UnresolvedAnchor::After(selector) => Anchor::After(resolve_selector(selector, document)?),
    })
}

fn update_for(reference: &NodeRef, field: &str, value: &str) -> Result<NodeUpdate, ReplayError> {
    match (reference.expected, field) {
        (NodeKind::Sign | NodeKind::Trait, "name") => Ok(NodeUpdate::Rename(value.to_owned())),
        (NodeKind::Definition, "path") => Ok(NodeUpdate::DefinitionPath(value.to_owned())),
        (NodeKind::Definition, "value") => Ok(NodeUpdate::DefinitionValue(value.to_owned())),
        (NodeKind::Rule | NodeKind::FeatureRule, "body") => {
            Ok(NodeUpdate::RuleBody(value.to_owned()))
        }
        (NodeKind::RuleElseBranch | NodeKind::RuleThenBranch, "body") => {
            Ok(NodeUpdate::RuleBranchBody(value.to_owned()))
        }
        (NodeKind::PhonStatement, "body") => Ok(NodeUpdate::RuleBranchBody(value.to_owned())),
        (NodeKind::Slot, "name") => Ok(NodeUpdate::SlotName(value.to_owned())),
        (NodeKind::Case, "selection") => match value {
            "case" | "first_match" => Ok(NodeUpdate::CaseSelection(
                conlang_language::CaseSelection::FirstMatch,
            )),
            "when" | "accumulate" => Ok(NodeUpdate::CaseSelection(
                conlang_language::CaseSelection::Accumulate,
            )),
            _ => Err(ReplayError::Selector(format!(
                "case selection must be `case` or `when`, got {value:?}"
            ))),
        },
        (NodeKind::Trait, "global") => Ok(NodeUpdate::TraitGlobal(parse_bool(value)?)),
        (NodeKind::Rule | NodeKind::FeatureRule | NodeKind::PhonBlockNode, "propagate") => {
            Ok(NodeUpdate::Propagate(parse_bool(value)?))
        }
        (NodeKind::Sense, "gloss") => Ok(NodeUpdate::SenseGloss(value.to_owned())),
        (NodeKind::SenseEdge, "kind") => DerivationKind::parse(value)
            .map(NodeUpdate::SenseEdgeKind)
            .ok_or_else(|| {
                ReplayError::Selector(format!(
                    "sense edge kind must be metaphor|metonymy|narrow|broaden, got {value:?}"
                ))
            }),
        (NodeKind::SenseEdge, "transparency") => SenseTransparency::parse(value)
            .map(NodeUpdate::SenseEdgeTransparency)
            .ok_or_else(|| {
                ReplayError::Selector(format!(
                    "sense edge transparency must be transparent|opaque, got {value:?}"
                ))
            }),
        (NodeKind::Slot, "optional") => Ok(NodeUpdate::SlotOptional(parse_bool(value)?)),
        (NodeKind::Belongs, "target") => Ok(NodeUpdate::Belongs(value.to_owned())),
        (NodeKind::Rule | NodeKind::FeatureRule, "dim") => Ok(NodeUpdate::RuleDimension(
            Dim::parse(value)
                .ok_or_else(|| ReplayError::Selector(format!("unknown dim {value:?}")))?,
        )),
        (NodeKind::Rule | NodeKind::FeatureRule, "stage") => {
            Ok(NodeUpdate::RuleStage(parse_stage(value)?))
        }
        _ => Err(ReplayError::Selector(format!(
            "field {field:?} is not editable on {:?}",
            reference.expected
        ))),
    }
}

fn parse_bool(value: &str) -> Result<bool, ReplayError> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(ReplayError::Selector(format!(
            "expected `true` or `false`, got {other:?}"
        ))),
    }
}

fn parse_stage(value: &str) -> Result<Stage, ReplayError> {
    match value.trim() {
        "stem" => Ok(Stage::Stem),
        "word" => Ok(Stage::Word),
        "phrase" => Ok(Stage::Phrase),
        other => Err(ReplayError::Selector(format!(
            "stage must be stem/word/phrase, got {other:?}"
        ))),
    }
}

fn stage_keyword(stage: Stage) -> &'static str {
    match stage {
        Stage::Stem => "stem",
        Stage::Word => "word",
        Stage::Phrase => "phrase",
    }
}

/// Wrapper name for synthesising a one-item `.lang` fragment so the whole
/// `.lang` parser/printer (and its dimension validation) is reused verbatim.
const FRAGMENT_SIGN: &str = "chg_fragment";

/// Parse an `insert into … :` block (a verbatim `.lang` fragment) into one or
/// more detached payloads: a whole `trait`/`sign`, or the sign-body items of a
/// dimension fragment (each item becomes its own `Insert`, so a multi-item
/// block fans out — §④).
fn parse_insert_block(block: &str) -> Result<Vec<DetachedNode>, ReplayError> {
    let head = block
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("");
    let parse_lang = |source: &str| {
        Language::parse(source)
            .map_err(|error| ReplayError::Parse(format!("insert .lang: {error}")))
    };
    if head.starts_with("trait ") || head.starts_with("global trait ") {
        let language = parse_lang(block)?;
        if language.traits.len() != 1 || !language.signs.is_empty() {
            return Err(ReplayError::Parse(
                "insert block must contain exactly one trait".to_owned(),
            ));
        }
        return Ok(vec![DetachedNode::Trait(language.traits[0].clone())]);
    }
    if head.starts_with("sign ") {
        let language = parse_lang(block)?;
        if language.signs.len() != 1 || !language.traits.is_empty() {
            return Err(ReplayError::Parse(
                "insert block must contain exactly one sign".to_owned(),
            ));
        }
        return Ok(vec![DetachedNode::Sign(language.signs[0].clone())]);
    }
    // Rule-chain branches: the body after `else `/`then ` is an opaque rule
    // line (a `DetachedNode::RuleElseBranch`/`RuleThenBranch` string). The
    // parent Rule is addressed with a `…rule[n]` authoring path.
    if let Some(rest) = head.strip_prefix("else ") {
        return Ok(vec![DetachedNode::RuleElseBranch(rest.trim().to_owned())]);
    }
    if let Some(rest) = head.strip_prefix("then ") {
        return Ok(vec![DetachedNode::RuleThenBranch(rest.trim().to_owned())]);
    }
    // P46 S3: `leaf <stmt>` inserts a single statement line into a phon `Leaf`.
    if let Some(rest) = head.strip_prefix("leaf ") {
        return Ok(vec![DetachedNode::PhonStatement(rest.trim().to_owned())]);
    }
    // Otherwise the block is a sign-body item fragment: wrap it in a synthetic
    // sign so the dimension keywords (`syn:`/`phon:`/`slots:`/…) parse in
    // context, then take each produced item.
    let language = parse_lang(&format!("sign {FRAGMENT_SIGN}:\n{block}"))?;
    let items = language
        .signs
        .first()
        .map(|sign| sign.items.as_slice())
        .unwrap_or_default();
    if items.is_empty() {
        return Err(ReplayError::Parse(
            "insert block produced no item".to_owned(),
        ));
    }
    Ok(items.iter().cloned().map(DetachedNode::Item).collect())
}

/// Render a single sign-body item back to its `.lang` block form (inverse of
/// the item branch of [`parse_insert_block`]): print a synthetic one-item sign,
/// drop the `sign …:` header and dedent one level.
fn render_item_block(item: &SignItem) -> String {
    let mut fragment = Language::new();
    fragment.signs.push(SignDef {
        id: conlang_language::SignId::synthetic(),
        name: FRAGMENT_SIGN.to_owned(),
        items: vec![item.clone()],
    });
    let dumped = fragment.dump();
    dumped
        .lines()
        .skip(1)
        .map(|line| line.strip_prefix("    ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse an `insert into <case> … :` block into `CaseBranch` payloads. The block
/// is re-indented under a synthetic case whose keyword/scrutinee mirror the
/// target case, so the branch grammar parses in the right context. Multi-branch
/// blocks fan out. Scoped to `SignContext` cases (`case:`/`when:` at Sign level).
fn parse_case_branch_block(
    block: &str,
    case: &conlang_language::TypedCase,
) -> Result<Vec<DetachedNode>, ReplayError> {
    if case.expected != conlang_language::ExpressionType::SignContext {
        return Err(ReplayError::Parse(
            "case-branch insert currently supports SignContext `case:`/`when:` cases".to_owned(),
        ));
    }
    let keyword = match case.selection {
        conlang_language::CaseSelection::FirstMatch => "case",
        conlang_language::CaseSelection::Accumulate => "when",
    };
    let header = match &case.scrutinee {
        Some(scrutinee) => format!("    {keyword} {scrutinee}:"),
        None => format!("    {keyword}:"),
    };
    let indented = block
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let wrapped = format!("sign {FRAGMENT_SIGN}:\n{header}\n{indented}");
    let language = Language::parse(&wrapped)
        .map_err(|error| ReplayError::Parse(format!("insert case branch: {error}")))?;
    let branches = language
        .signs
        .first()
        .and_then(|sign| sign.items.first())
        .and_then(|item| match item {
            SignItem::SignExpression(expr) => match &expr.expression {
                Expression::Case(case) => Some(&case.branches),
                _ => None,
            },
            _ => None,
        })
        .filter(|branches| !branches.is_empty())
        .ok_or_else(|| ReplayError::Parse("insert block produced no case branch".to_owned()))?;
    Ok(branches
        .iter()
        .cloned()
        .map(DetachedNode::CaseBranch)
        .collect())
}

/// Render one `CaseBranch` back to its block form (inverse of the branch arm of
/// [`parse_case_branch_block`]): print it inside a synthetic `SignContext` case
/// and strip the `sign …:` + `case:` headers.
fn render_case_branch_block(branch: &CaseBranch) -> String {
    let case = conlang_language::TypedCase {
        selection: conlang_language::CaseSelection::FirstMatch,
        expected: conlang_language::ExpressionType::SignContext,
        scrutinee: None,
        name: None,
        branches: vec![branch.clone()],
        source: conlang_language::SourceLocation::unknown(),
    };
    let mut fragment = Language::new();
    fragment.signs.push(SignDef {
        id: conlang_language::SignId::synthetic(),
        name: FRAGMENT_SIGN.to_owned(),
        items: vec![SignItem::SignExpression(conlang_language::SignExpression {
            expression: Expression::Case(Box::new(case)),
            source: conlang_language::SourceLocation::unknown(),
        })],
    });
    fragment
        .dump()
        .lines()
        .skip(2)
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn resolve_operation(
    operation: &UnresolvedOperation,
    document: &LanguageDocument,
) -> Result<Vec<PrimitiveEdit>, ReplayError> {
    match operation {
        UnresolvedOperation::Update {
            selector,
            field,
            value,
        } => {
            let node = resolve_selector(selector, document)?;
            Ok(vec![PrimitiveEdit::Update {
                change: update_for(&node, field, value)?,
                node,
            }])
        }
        UnresolvedOperation::Delete(selector) => Ok(vec![PrimitiveEdit::Delete {
            node: resolve_selector(selector, document)?,
        }]),
        UnresolvedOperation::Move {
            node,
            parent,
            anchor,
        } => Ok(vec![PrimitiveEdit::Move {
            node: resolve_selector(node, document)?,
            new_parent: resolve_selector(parent, document)?,
            anchor: resolve_anchor(anchor, document)?,
        }]),
        UnresolvedOperation::InsertSign {
            parent,
            anchor,
            source,
        } => {
            let language = Language::parse(source)
                .map_err(|error| ReplayError::Parse(format!("insert .lang: {error}")))?;
            if !language.traits.is_empty() || language.signs.len() != 1 {
                return Err(ReplayError::Parse(
                    "insert sign fragment must contain exactly one sign".to_owned(),
                ));
            }
            Ok(vec![PrimitiveEdit::Insert {
                parent: resolve_selector(parent, document)?,
                anchor: resolve_anchor(anchor, document)?,
                subtree: DetachedNode::Sign(language.signs[0].clone()),
            }])
        }
        UnresolvedOperation::InsertBlock {
            target,
            anchor,
            block,
        } => {
            let parent = resolve_selector(target, document)?;
            let anchor = resolve_anchor(anchor, document)?;
            // A Case target takes case-branch blocks (parsed in the target
            // case's context); any other target takes node/item fragments. Both
            // fan out to one `Insert` per payload, sharing parent/anchor, so the
            // statement validates only its final state.
            let payloads = if parent.expected == NodeKind::Case {
                let entry = ensure_target(document, &parent)
                    .map_err(|error| ReplayError::Selector(error.to_string()))?;
                let case = case_at(document.language(), &entry.address).ok_or_else(|| {
                    ReplayError::Selector("target is not a resolvable case".to_owned())
                })?;
                parse_case_branch_block(block, case)?
            } else {
                parse_insert_block(block)?
            };
            Ok(payloads
                .into_iter()
                .map(|subtree| PrimitiveEdit::Insert {
                    parent: parent.clone(),
                    anchor: anchor.clone(),
                    subtree,
                })
                .collect())
        }
        UnresolvedOperation::Call {
            name,
            positional,
            named,
            block,
        } => call::lower(
            &call::Call {
                name,
                positional: positional.as_deref(),
                named,
                block: block.as_deref(),
            },
            document,
        ),
        UnresolvedOperation::Clone { source, name } => {
            let reference = resolve_selector(source, document)?;
            let entry = ensure_target(document, &reference)
                .map_err(|error| ReplayError::Selector(error.to_string()))?;
            let detached = detached_at(document.language(), entry)
                .map_err(|error| ReplayError::Selector(error.to_string()))?;
            let mut sign = match detached {
                DetachedNode::Sign(sign) => sign,
                other => {
                    return Err(ReplayError::Selector(format!(
                        "clone supports only signs, not {:?}",
                        other.kind()
                    )))
                }
            };
            // The Insert primitive reassigns a fresh SignId, RuleIds and stable
            // NodeIds, so the clone is a new entity; only the name is authored.
            sign.name = name.clone();
            Ok(vec![PrimitiveEdit::Insert {
                parent: document.root_ref(),
                anchor: Anchor::End,
                subtree: DetachedNode::Sign(sign),
            }])
        }
    }
}

fn dump_node(reference: &NodeRef) -> String {
    dump_selector(&Selector::Stable(reference.clone()))
}

fn dump_anchor(anchor: &Anchor) -> String {
    match anchor {
        Anchor::Start => "start".to_owned(),
        Anchor::End => "end".to_owned(),
        Anchor::Before(reference) => format!("before {}", dump_node(reference)),
        Anchor::After(reference) => format!("after {}", dump_node(reference)),
    }
}

fn dump_value(value: &str) -> String {
    if value.chars().all(|character| {
        character.is_alphanumeric() || matches!(character, '_' | '-' | ':' | '/' | '.')
    }) {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

fn dump_update(change: &NodeUpdate) -> Option<(&'static str, String)> {
    match change {
        NodeUpdate::Rename(value) => Some(("name", value.clone())),
        NodeUpdate::DefinitionPath(value) => Some(("path", value.clone())),
        NodeUpdate::DefinitionValue(value) => Some(("value", value.clone())),
        NodeUpdate::RuleBody(value) | NodeUpdate::RuleBranchBody(value) => {
            Some(("body", value.clone()))
        }
        NodeUpdate::SlotName(value) => Some(("name", value.clone())),
        NodeUpdate::CaseSelection(value) => Some((
            "selection",
            match value {
                conlang_language::CaseSelection::FirstMatch => "case",
                conlang_language::CaseSelection::Accumulate => "when",
            }
            .to_owned(),
        )),
        NodeUpdate::TraitGlobal(value) => Some(("global", value.to_string())),
        NodeUpdate::Propagate(value) => Some(("propagate", value.to_string())),
        NodeUpdate::SenseGloss(value) => Some(("gloss", value.clone())),
        NodeUpdate::SenseEdgeKind(value) => Some(("kind", value.keyword().to_owned())),
        NodeUpdate::SenseEdgeTransparency(value) => {
            Some(("transparency", value.keyword().to_owned()))
        }
        NodeUpdate::SlotOptional(value) => Some(("optional", value.to_string())),
        NodeUpdate::Belongs(value) => Some(("target", value.clone())),
        NodeUpdate::RuleDimension(dim) => Some(("dim", dim.keyword().to_owned())),
        NodeUpdate::RuleStage(stage) => Some(("stage", stage_keyword(*stage).to_owned())),
        _ => None,
    }
}

impl ResolvedChangeSet {
    pub fn dump(&self) -> String {
        let mut output = format!(
            "changeset {}:\n    schema = {}\n    base_source = sha256:{}\n    base_identities = sha256:{}\n",
            self.namespace, self.schema, self.base_source, self.base_identities
        );
        for lock in &self.libraries {
            output.push_str(&format!(
                "    library {}@{} sha256:{}\n",
                lock.package, lock.version, lock.digest
            ));
        }
        for statement in &self.statements {
            output.push_str(&format!("\n    #{}:\n", statement.ordinal));
            for edit in &statement.edits {
                match edit {
                    PrimitiveEdit::Update { node, change } => {
                        if let Some((field, value)) = dump_update(change) {
                            output.push_str(&format!(
                                "        update {}.{} = {}\n",
                                dump_node(node),
                                field,
                                dump_value(&value)
                            ));
                        }
                    }
                    PrimitiveEdit::Delete { node } => {
                        output.push_str(&format!("        delete {}\n", dump_node(node)));
                    }
                    PrimitiveEdit::Move {
                        node,
                        new_parent,
                        anchor,
                    } => output.push_str(&format!(
                        "        move {} to {} at {}\n",
                        dump_node(node),
                        dump_node(new_parent),
                        dump_anchor(anchor)
                    )),
                    PrimitiveEdit::Insert {
                        parent,
                        anchor,
                        subtree: DetachedNode::Sign(sign),
                    } => {
                        output.push_str(&format!(
                            "        insert sign under {} at {}:\n",
                            dump_node(parent),
                            dump_anchor(anchor)
                        ));
                        let mut fragment = Language::new();
                        fragment.signs.push(sign.clone());
                        for line in fragment.dump().lines() {
                            output.push_str("            ");
                            output.push_str(line);
                            output.push('\n');
                        }
                    }
                    PrimitiveEdit::Insert {
                        parent,
                        anchor,
                        subtree: DetachedNode::Trait(trait_def),
                    } => {
                        output.push_str(&format!(
                            "        insert into {} at {}:\n",
                            dump_node(parent),
                            dump_anchor(anchor)
                        ));
                        let mut fragment = Language::new();
                        fragment.traits.push(trait_def.clone());
                        for line in fragment.dump().lines() {
                            output.push_str("            ");
                            output.push_str(line);
                            output.push('\n');
                        }
                    }
                    PrimitiveEdit::Insert {
                        parent,
                        anchor,
                        subtree: DetachedNode::Item(item),
                    } => {
                        output.push_str(&format!(
                            "        insert into {} at {}:\n",
                            dump_node(parent),
                            dump_anchor(anchor)
                        ));
                        for line in render_item_block(item).lines() {
                            output.push_str("            ");
                            output.push_str(line);
                            output.push('\n');
                        }
                    }
                    PrimitiveEdit::Insert {
                        parent,
                        anchor,
                        subtree: DetachedNode::CaseBranch(branch),
                    } => {
                        output.push_str(&format!(
                            "        insert into {} at {}:\n",
                            dump_node(parent),
                            dump_anchor(anchor)
                        ));
                        for line in render_case_branch_block(branch).lines() {
                            output.push_str("            ");
                            output.push_str(line);
                            output.push('\n');
                        }
                    }
                    PrimitiveEdit::Insert {
                        parent,
                        anchor,
                        subtree:
                            subtree @ (DetachedNode::RuleElseBranch(_)
                            | DetachedNode::RuleThenBranch(_)
                            | DetachedNode::PhonStatement(_)),
                    } => {
                        let (keyword, body) = match subtree {
                            DetachedNode::RuleElseBranch(body) => ("else", body),
                            DetachedNode::RuleThenBranch(body) => ("then", body),
                            DetachedNode::PhonStatement(body) => ("leaf", body),
                            _ => unreachable!(),
                        };
                        output.push_str(&format!(
                            "        insert into {} at {}:\n            {} {}\n",
                            dump_node(parent),
                            dump_anchor(anchor),
                            keyword,
                            body
                        ));
                    }
                    PrimitiveEdit::Insert { .. } => {}
                }
            }
        }
        output
    }
}

/// 一個套件進 lock digest 的**全部內容**。抽成獨立函式是為了可直接斷言
/// 「哪些東西被涵蓋」——digest 漏掉任何一項都會讓對應檔案改了卻不使 lock 失效
/// (破 P26 可重現性)。
fn package_lock_content(package: &conlang_language::LibraryPackage) -> String {
    let mut content = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n",
        package.id,
        package.version,
        package.rule_namespace,
        package.priority,
        package.code_path,
        package.data_path
    );
    for dependency in &package.requires {
        content.push_str(&format!("requires {dependency}\n"));
    }
    for export in &package.exports {
        content.push_str(&format!(
            "export {} {:?} {}\n",
            export.stable_id, export.kind, export.alias
        ));
    }
    content.push_str(package.code);
    content.push('\n');
    // P50 ③:function 原始碼必須進 digest。
    content.push_str(package.functions);
    content.push('\n');
    content.push_str(package.data);
    content
}

/// 測試用:讓外部斷言 digest 涵蓋範圍(見 `function_loading.rs`)。
#[doc(hidden)]
pub fn __lock_content_for_tests(package: &conlang_language::LibraryPackage) -> String {
    package_lock_content(package)
}

fn package_locks(spec: &LibrarySpec) -> Result<Vec<LibraryLock>, ReplayError> {
    let catalog = conlang_language::library::embedded_catalog()
        .map_err(|error| ReplayError::Library(error.to_string()))?;
    let selection = catalog
        .select(spec)
        .map_err(|error| ReplayError::Library(error.to_string()))?;
    let mut locks = Vec::new();
    for id in selection.packages {
        let package = catalog
            .packages()
            .iter()
            .find(|package| package.id == id)
            .expect("catalog selection returns catalog IDs");
        let content = package_lock_content(package);
        locks.push(LibraryLock {
            package: package.id.clone(),
            version: package.version.clone(),
            digest: sha256_hex(content.as_bytes()),
        });
    }
    locks.sort_by(|left, right| left.package.cmp(&right.package));
    Ok(locks)
}

pub fn identity_manifest_digest(document: &LanguageDocument) -> Result<String, ReplayError> {
    Ok(sha256_hex(document.manifest_json()?.as_bytes()))
}

pub fn change_set_prelude(
    document: &LanguageDocument,
    spec: &LibrarySpec,
    namespace: &str,
) -> Result<String, ReplayError> {
    let locks = package_locks(spec)?;
    let mut source = format!(
        "changeset {namespace}:\n    schema = {CHANGESET_SCHEMA_V1}\n    base_source = sha256:{}\n    base_identities = sha256:{}\n",
        document.identities().source_sha256,
        identity_manifest_digest(document)?
    );
    for lock in locks {
        source.push_str(&format!(
            "    library {}@{} sha256:{}\n",
            lock.package, lock.version, lock.digest
        ));
    }
    Ok(source)
}

fn verify_base_and_locks(
    base: &LanguageDocument,
    spec: &LibrarySpec,
    source_digest: &str,
    identity_digest: &str,
    locks: &[LibraryLock],
) -> Result<(), ReplayError> {
    if base.identities().source_sha256 != source_digest {
        return Err(ReplayError::BaseSourceMismatch);
    }
    if identity_manifest_digest(base)? != identity_digest {
        return Err(ReplayError::BaseIdentitiesMismatch);
    }
    let actual = package_locks(spec)?;
    if actual != locks {
        return Err(ReplayError::LibraryLockMismatch(
            "resolved package/version/content set differs".to_owned(),
        ));
    }
    Ok(())
}

fn primitive_record(
    before: &LanguageDocument,
    after: &LanguageDocument,
    edit: &PrimitiveEdit,
    validation: &ValidationReport,
) -> PrimitiveRecord {
    let operation = primitive_kind(edit);
    let target = edit_target(edit);
    let parent = edit_parent(edit);
    let anchor = edit_anchor(edit);
    let before_snapshot = target
        .as_ref()
        .and_then(|reference| snapshot_for(before, reference));
    let diff = LanguageDiff::between(before, after);
    let allocated_ids = diff
        .entries
        .iter()
        .filter_map(|entry| match entry {
            LanguageDiffEntry::Inserted(node) => Some(node.id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let deleted_ids = diff
        .entries
        .iter()
        .filter_map(|entry| match entry {
            LanguageDiffEntry::Deleted(node) => Some(node.id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let moved_ids = diff
        .entries
        .iter()
        .filter_map(|entry| match entry {
            LanguageDiffEntry::Moved { after, .. } => Some(after.id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let after_snapshot = target
        .as_ref()
        .and_then(|reference| snapshot_for(after, reference));
    PrimitiveRecord {
        operation,
        target,
        parent,
        anchor,
        before: before_snapshot,
        after: after_snapshot,
        allocated_ids,
        deleted_ids,
        moved_ids,
        diagnostics: validation
            .diagnostics()
            .iter()
            .map(|diagnostic| RecordDiagnostic {
                severity: diagnostic.severity,
                code: diagnostic.code.to_owned(),
                message: diagnostic.message.clone(),
            })
            .collect(),
        diff,
    }
}

fn apply_statement_structural(
    source: &LanguageDocument,
    ordinal: u64,
    edits: &[PrimitiveEdit],
    libraries: &LibrarySpec,
) -> Result<(LanguageDocument, StatementRecord), ReplayError> {
    let before = source.clone();
    let mut candidate = source.clone();
    let mut transitions = Vec::new();
    for edit in edits {
        let previous = candidate.clone();
        candidate = apply_structural(candidate, edit)
            .map_err(|source| ReplayError::Statement { ordinal, source })?;
        transitions.push((previous, edit));
    }
    let validation = check_document(&candidate, libraries);
    if validation.has_errors() {
        return Err(ReplayError::Statement {
            ordinal,
            source: EditError::Validation(Box::new(validation)),
        });
    }
    let records = transitions
        .into_iter()
        .map(|(previous, edit)| primitive_record(&previous, &candidate, edit, &validation))
        .collect();
    let identities_sha256 = identity_manifest_digest(&candidate)?;
    Ok((
        candidate.clone(),
        StatementRecord {
            ordinal,
            records,
            diff: LanguageDiff::between(&before, &candidate),
            source_sha256: candidate.identities().source_sha256.clone(),
            identities_sha256,
        },
    ))
}

#[derive(Debug, Clone)]
pub struct ChangeInterpreter {
    base: LanguageDocument,
    libraries: LibrarySpec,
    namespace: String,
}

impl ChangeInterpreter {
    pub fn new(
        base: LanguageDocument,
        libraries: LibrarySpec,
        namespace: impl Into<String>,
    ) -> Result<ChangeInterpreter, ReplayError> {
        let namespace = namespace.into();
        let _ = base.fork(namespace.clone())?;
        Ok(ChangeInterpreter {
            base,
            libraries,
            namespace,
        })
    }

    pub fn apply_statement(
        &self,
        document: &LanguageDocument,
        statement: &ResolvedStatement,
    ) -> Result<(LanguageDocument, StatementRecord), ReplayError> {
        if document.identities().active_namespace != self.namespace {
            return Err(ReplayError::NamespaceMismatch(
                document.identities().active_namespace.clone(),
            ));
        }
        apply_statement_structural(
            document,
            statement.ordinal,
            &statement.edits,
            &self.libraries,
        )
    }

    pub fn run(&self, changeset: &ResolvedChangeSet) -> Result<ReplayOutcome, ReplayError> {
        if changeset.namespace != self.namespace {
            return Err(ReplayError::NamespaceMismatch(changeset.namespace.clone()));
        }
        verify_base_and_locks(
            &self.base,
            &self.libraries,
            &changeset.base_source,
            &changeset.base_identities,
            &changeset.libraries,
        )?;
        let mut document = self.base.fork(self.namespace.clone())?;
        let before = document.clone();
        let mut records = Vec::new();
        for statement in &changeset.statements {
            let (next, record) = self.apply_statement(&document, statement)?;
            document = next;
            records.push(record);
        }
        Ok(ReplayOutcome {
            diff: LanguageDiff::between(&before, &document),
            document,
            records,
        })
    }
}

#[derive(Debug)]
pub struct ChangeSession {
    document: LanguageDocument,
    libraries: LibrarySpec,
    dirty: bool,
    compiled: Option<CompiledSystem>,
    compile_count: u64,
}

impl ChangeSession {
    pub fn new(document: LanguageDocument, libraries: LibrarySpec) -> ChangeSession {
        ChangeSession {
            document,
            libraries,
            dirty: true,
            compiled: None,
            compile_count: 0,
        }
    }

    pub fn document(&self) -> &LanguageDocument {
        &self.document
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn compile_count(&self) -> u64 {
        self.compile_count
    }

    pub fn commit(&mut self, document: LanguageDocument) {
        self.document = document;
        self.dirty = true;
        self.compiled = None;
    }

    pub fn apply_statement(
        &mut self,
        interpreter: &ChangeInterpreter,
        statement: &ResolvedStatement,
    ) -> Result<StatementRecord, ReplayError> {
        let (document, record) = interpreter.apply_statement(&self.document, statement)?;
        self.commit(document);
        Ok(record)
    }

    pub fn compiled_system(&mut self) -> Result<&CompiledSystem, ReplayError> {
        if self.dirty || self.compiled.is_none() {
            let compiled = compile_document(&self.document, &self.libraries)?;
            self.compiled = Some(compiled);
            self.compile_count += 1;
            self.dirty = false;
        }
        Ok(self.compiled.as_ref().expect("compiled cache populated"))
    }
}

fn is_item_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::TraitUse
            | NodeKind::Belongs
            | NodeKind::Slot
            | NodeKind::SlotMap
            | NodeKind::FeatureDeclaration
            | NodeKind::FeatureValue
            | NodeKind::SlotFeatureBinding
            | NodeKind::RoleDeclaration
            | NodeKind::RoleBinding
            | NodeKind::Sense
            | NodeKind::SenseEdge
            | NodeKind::Realization
            | NodeKind::Case
            | NodeKind::Constraint
            | NodeKind::FeatureRule
            | NodeKind::Definition
            | NodeKind::Rule
    )
}

fn def_dimension(path: &str) -> Option<Dim> {
    let head = path.split_once('.').map(|(head, _)| head).unwrap_or(path);
    Dim::parse(head)
}

fn dim_base(dim: Dim) -> u16 {
    match dim {
        Dim::Syn => 10,
        Dim::Phon => 20,
        Dim::Sem => 30,
        Dim::Prag => 40,
    }
}

fn item_group(item: &SignItem) -> u16 {
    match item {
        SignItem::Belongs(_) => 0,
        SignItem::TraitUse { .. } => 1,
        SignItem::Def(def) if def_dimension(&def.path).is_none() => 2,
        SignItem::Slot(_) => dim_base(Dim::Syn),
        SignItem::SlotFeatureBinding(_) => dim_base(Dim::Syn) + 1,
        SignItem::FeatureDecl(value) => dim_base(value.dim) + 2,
        SignItem::FeatureValue(value) => dim_base(value.dim) + 2,
        SignItem::FeatureExpression(value) => dim_base(value.dim) + 2,
        SignItem::FeatureRule(rule) => dim_base(rule.dim) + 2,
        SignItem::RoleDecl(_) | SignItem::RoleBinding(_) | SignItem::RoleExpression(_) => {
            dim_base(Dim::Sem) + 3
        }
        // 義項先於衍生邊(邊引用義項),兩者都在 sem 區段。
        SignItem::Sense(_) => dim_base(Dim::Sem) + 1,
        SignItem::SenseEdge(_) => dim_base(Dim::Sem) + 2,
        SignItem::Realization(_) => dim_base(Dim::Phon) + 3,
        SignItem::Def(def) => dim_base(def_dimension(&def.path).unwrap_or(Dim::Syn)) + 4,
        SignItem::Rule(rule) => dim_base(rule.dim) + 4,
        SignItem::SlotMap(_) => dim_base(Dim::Syn) + 4,
        SignItem::Constraint(_) => 50,
        SignItem::SignExpression(_) => 51,
    }
}

fn kind_at(language: &Language, address: &NodeAddress) -> Option<NodeKind> {
    if let Some(item) = item_at_address(language, address) {
        return Some(item_kind(item));
    }
    match address.0.as_slice() {
        [] => Some(NodeKind::Language),
        [AddressSegment::DslDeclarations(index)] if *index < language.dsl_decls.len() => {
            Some(NodeKind::DslDeclaration)
        }
        [AddressSegment::Prosody] if !language.prosody.is_empty() => Some(NodeKind::Prosody),
        [AddressSegment::Distribution(index)] if *index < language.distribution.len() => {
            Some(NodeKind::Distribution)
        }
        [AddressSegment::Traits(index)] if *index < language.traits.len() => Some(NodeKind::Trait),
        [AddressSegment::Signs(index)] if *index < language.signs.len() => Some(NodeKind::Sign),
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block)]
            if language
                .traits
                .get(*trait_index)?
                .blocks
                .get(*block)
                .is_some() =>
        {
            Some(NodeKind::Block)
        }
        path => {
            let parent = NodeAddress(path[..path.len().checked_sub(1)?].to_vec());
            match path.last()? {
                AddressSegment::RuleElse(index)
                    if *index < rule_at(language, &parent).ok()?.else_chain.len() =>
                {
                    Some(NodeKind::RuleElseBranch)
                }
                AddressSegment::RuleThen(index)
                    if *index < rule_at(language, &parent).ok()?.then_chain.len() =>
                {
                    Some(NodeKind::RuleThenBranch)
                }
                AddressSegment::PhonLeaf(index) => {
                    match phon_container_block(language, &parent).ok()? {
                        PhonBlock::Leaf(statements) if *index < statements.len() => {
                            Some(NodeKind::PhonStatement)
                        }
                        _ => None,
                    }
                }
                AddressSegment::PhonThen(index) => {
                    match phon_container_block(language, &parent).ok()? {
                        PhonBlock::Then(elements) if *index < elements.len() => {
                            Some(NodeKind::PhonBlockNode)
                        }
                        _ => None,
                    }
                }
                AddressSegment::PhonElse(index) => {
                    match phon_container_block(language, &parent).ok()? {
                        PhonBlock::Else(elements) if *index < elements.len() => {
                            Some(NodeKind::PhonBlockNode)
                        }
                        _ => None,
                    }
                }
                AddressSegment::CaseExpression => realization_at(language, &parent)
                    .ok()?
                    .expression
                    .as_ref()
                    .map(|_| NodeKind::Case),
                AddressSegment::CaseBranches(index) => {
                    let case_parent = NodeAddress(path[..path.len() - 1].to_vec());
                    let case = case_at(language, &case_parent)?;
                    (*index < case.branches.len()).then_some(NodeKind::CaseBranch)
                }
                AddressSegment::CaseResult => {
                    let branch_parent = NodeAddress(path[..path.len() - 1].to_vec());
                    case_branch_at(language, &branch_parent)
                        .ok()
                        .and_then(|branch| expression_node_kind(&branch.result))
                }
                AddressSegment::ApplicationArguments(index) => {
                    let application_parent = NodeAddress(path[..path.len() - 1].to_vec());
                    application_at(language, &application_parent)
                        .ok()
                        .and_then(|application| application.arguments.get(*index))
                        .and_then(|argument| match argument.value {
                            SignArgumentValue::Application(_) => Some(NodeKind::Application),
                            _ => None,
                        })
                }
                _ => None,
            }
        }
    }
}

fn case_at<'a>(
    language: &'a Language,
    address: &NodeAddress,
) -> Option<&'a conlang_language::TypedCase> {
    let (item_address, tail) = split_item_address(address)?;
    let item = item_at_address(language, &item_address)?;
    let (case, tail) = root_case(item, tail)?;
    case_at_tail(case, tail)
}

fn case_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut TypedCase, EditError> {
    let (item_address, tail) = split_item_address(address)
        .ok_or_else(|| EditError::FieldMismatch("expected case address".to_owned()))?;
    let item = item_at_address_mut(language, &item_address)?;
    let (case, tail) = root_case_mut(item, tail)
        .ok_or_else(|| EditError::FieldMismatch("expected case address".to_owned()))?;
    case_at_tail_mut(case, tail)
        .ok_or_else(|| EditError::FieldMismatch("case address is stale".to_owned()))
}

fn case_branch_at<'a>(
    language: &'a Language,
    address: &NodeAddress,
) -> Result<&'a CaseBranch, EditError> {
    let parent = address.parent().ok_or(EditError::RootImmutable)?;
    let Some(AddressSegment::CaseBranches(index)) = address.0.last() else {
        return Err(EditError::FieldMismatch(
            "expected case branch address".to_owned(),
        ));
    };
    case_at(language, &parent)
        .and_then(|case| case.branches.get(*index))
        .ok_or_else(|| EditError::FieldMismatch("case branch address is stale".to_owned()))
}

fn case_branch_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut CaseBranch, EditError> {
    let parent = address.parent().ok_or(EditError::RootImmutable)?;
    let Some(AddressSegment::CaseBranches(index)) = address.0.last() else {
        return Err(EditError::FieldMismatch(
            "expected case branch address".to_owned(),
        ));
    };
    case_at_mut(language, &parent)?
        .branches
        .get_mut(*index)
        .ok_or_else(|| EditError::FieldMismatch("case branch address is stale".to_owned()))
}

fn application_at<'a>(
    language: &'a Language,
    address: &NodeAddress,
) -> Result<&'a SignApplication, EditError> {
    let (item_address, tail) = split_item_address(address)
        .ok_or_else(|| EditError::FieldMismatch("expected application address".to_owned()))?;
    let item = item_at_address(language, &item_address)
        .ok_or_else(|| EditError::FieldMismatch("application item is stale".to_owned()))?;
    if let Some(application) = root_application(item, tail) {
        return Ok(application);
    }
    let (case, tail) = root_case(item, tail)
        .ok_or_else(|| EditError::FieldMismatch("expected application address".to_owned()))?;
    application_at_case_tail(case, tail)
        .ok_or_else(|| EditError::FieldMismatch("application address is stale".to_owned()))
}

fn application_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut SignApplication, EditError> {
    let (item_address, tail) = split_item_address(address)
        .ok_or_else(|| EditError::FieldMismatch("expected application address".to_owned()))?;
    let item = item_at_address_mut(language, &item_address)?;
    match item {
        SignItem::SignExpression(expression) => {
            return application_at_expression_tail_mut(&mut expression.expression, tail)
                .ok_or_else(|| {
                    EditError::FieldMismatch("application address is stale".to_owned())
                });
        }
        SignItem::FeatureExpression(expression) => {
            return application_at_expression_tail_mut(&mut expression.expression, tail)
                .ok_or_else(|| {
                    EditError::FieldMismatch("application address is stale".to_owned())
                });
        }
        SignItem::RoleExpression(expression) => {
            return application_at_expression_tail_mut(&mut expression.expression, tail)
                .ok_or_else(|| {
                    EditError::FieldMismatch("application address is stale".to_owned())
                });
        }
        _ => {}
    }
    let (case, tail) = root_case_mut(item, tail)
        .ok_or_else(|| EditError::FieldMismatch("expected application address".to_owned()))?;
    application_at_case_tail_mut(case, tail)
        .ok_or_else(|| EditError::FieldMismatch("application address is stale".to_owned()))
}

fn root_application<'a>(
    item: &'a SignItem,
    tail: &[AddressSegment],
) -> Option<&'a SignApplication> {
    match item {
        SignItem::SignExpression(expression) => {
            application_at_expression_tail(&expression.expression, tail)
        }
        SignItem::FeatureExpression(expression) => {
            application_at_expression_tail(&expression.expression, tail)
        }
        SignItem::RoleExpression(expression) => {
            application_at_expression_tail(&expression.expression, tail)
        }
        _ => None,
    }
}

fn split_item_address(address: &NodeAddress) -> Option<(NodeAddress, &[AddressSegment])> {
    let item_index = address
        .0
        .iter()
        .position(|segment| matches!(segment, AddressSegment::Items(_)))?;
    let split = item_index + 1;
    Some((
        NodeAddress(address.0[..split].to_vec()),
        &address.0[split..],
    ))
}

fn root_case<'a, 'b>(
    item: &'a SignItem,
    tail: &'b [AddressSegment],
) -> Option<(&'a TypedCase, &'b [AddressSegment])> {
    match item {
        SignItem::SignExpression(expression) => {
            expression_case(&expression.expression).map(|case| (case, tail))
        }
        SignItem::FeatureExpression(expression) => {
            expression_case(&expression.expression).map(|case| (case, tail))
        }
        SignItem::RoleExpression(expression) => {
            expression_case(&expression.expression).map(|case| (case, tail))
        }
        SignItem::Realization(realization) => match tail.split_first() {
            Some((AddressSegment::CaseExpression, rest)) => {
                realization.expression.as_ref().map(|case| (case, rest))
            }
            _ => None,
        },
        _ => None,
    }
}

fn root_case_mut<'a, 'b>(
    item: &'a mut SignItem,
    tail: &'b [AddressSegment],
) -> Option<(&'a mut TypedCase, &'b [AddressSegment])> {
    match item {
        SignItem::SignExpression(expression) => {
            expression_case_mut(&mut expression.expression).map(|case| (case, tail))
        }
        SignItem::FeatureExpression(expression) => {
            expression_case_mut(&mut expression.expression).map(|case| (case, tail))
        }
        SignItem::RoleExpression(expression) => {
            expression_case_mut(&mut expression.expression).map(|case| (case, tail))
        }
        SignItem::Realization(realization) => match tail.split_first() {
            Some((AddressSegment::CaseExpression, rest)) => {
                realization.expression.as_mut().map(|case| (case, rest))
            }
            _ => None,
        },
        _ => None,
    }
}

fn expression_case(expression: &Expression) -> Option<&TypedCase> {
    match expression {
        Expression::Case(case) => Some(case),
        Expression::Projection { value, .. } => expression_case(value),
        _ => None,
    }
}

fn expression_case_mut(expression: &mut Expression) -> Option<&mut TypedCase> {
    match expression {
        Expression::Case(case) => Some(case),
        Expression::Projection { value, .. } => expression_case_mut(value),
        _ => None,
    }
}

fn case_at_tail<'a>(case: &'a TypedCase, tail: &[AddressSegment]) -> Option<&'a TypedCase> {
    if tail.is_empty() {
        return Some(case);
    }
    let [AddressSegment::CaseBranches(index), AddressSegment::CaseResult, rest @ ..] = tail else {
        return None;
    };
    expression_case_at_tail(&case.branches.get(*index)?.result, rest)
}

fn expression_case_at_tail<'a>(
    expression: &'a Expression,
    tail: &[AddressSegment],
) -> Option<&'a TypedCase> {
    match expression {
        Expression::Case(case) => case_at_tail(case, tail),
        Expression::Projection { value, .. } => expression_case_at_tail(value, tail),
        _ => None,
    }
}

fn case_at_tail_mut<'a>(
    case: &'a mut TypedCase,
    tail: &[AddressSegment],
) -> Option<&'a mut TypedCase> {
    if tail.is_empty() {
        return Some(case);
    }
    let [AddressSegment::CaseBranches(index), AddressSegment::CaseResult, rest @ ..] = tail else {
        return None;
    };
    expression_case_at_tail_mut(&mut case.branches.get_mut(*index)?.result, rest)
}

fn expression_case_at_tail_mut<'a>(
    expression: &'a mut Expression,
    tail: &[AddressSegment],
) -> Option<&'a mut TypedCase> {
    match expression {
        Expression::Case(case) => case_at_tail_mut(case, tail),
        Expression::Projection { value, .. } => expression_case_at_tail_mut(value, tail),
        _ => None,
    }
}

fn application_at_case_tail<'a>(
    case: &'a TypedCase,
    tail: &[AddressSegment],
) -> Option<&'a SignApplication> {
    let [AddressSegment::CaseBranches(index), AddressSegment::CaseResult, rest @ ..] = tail else {
        return None;
    };
    application_at_expression_tail(&case.branches.get(*index)?.result, rest)
}

fn application_at_expression_tail<'a>(
    expression: &'a Expression,
    tail: &[AddressSegment],
) -> Option<&'a SignApplication> {
    match expression {
        Expression::SignApplication(application) | Expression::PhonInterpolation(application) => {
            application_at_tail(application, tail)
        }
        Expression::Projection { value, .. } => application_at_expression_tail(value, tail),
        Expression::Case(case) => application_at_case_tail(case, tail),
        _ => None,
    }
}

fn application_at_tail<'a>(
    application: &'a SignApplication,
    tail: &[AddressSegment],
) -> Option<&'a SignApplication> {
    if tail.is_empty() {
        return Some(application);
    }
    let [AddressSegment::ApplicationArguments(index), rest @ ..] = tail else {
        return None;
    };
    match &application.arguments.get(*index)?.value {
        SignArgumentValue::Application(nested) => application_at_tail(nested, rest),
        _ => None,
    }
}

fn application_at_case_tail_mut<'a>(
    case: &'a mut TypedCase,
    tail: &[AddressSegment],
) -> Option<&'a mut SignApplication> {
    let [AddressSegment::CaseBranches(index), AddressSegment::CaseResult, rest @ ..] = tail else {
        return None;
    };
    application_at_expression_tail_mut(&mut case.branches.get_mut(*index)?.result, rest)
}

fn application_at_expression_tail_mut<'a>(
    expression: &'a mut Expression,
    tail: &[AddressSegment],
) -> Option<&'a mut SignApplication> {
    match expression {
        Expression::SignApplication(application) | Expression::PhonInterpolation(application) => {
            application_at_tail_mut(application, tail)
        }
        Expression::Projection { value, .. } => application_at_expression_tail_mut(value, tail),
        Expression::Case(case) => application_at_case_tail_mut(case, tail),
        _ => None,
    }
}

fn application_at_tail_mut<'a>(
    application: &'a mut SignApplication,
    tail: &[AddressSegment],
) -> Option<&'a mut SignApplication> {
    if tail.is_empty() {
        return Some(application);
    }
    let [AddressSegment::ApplicationArguments(index), rest @ ..] = tail else {
        return None;
    };
    match &mut application.arguments.get_mut(*index)?.value {
        SignArgumentValue::Application(nested) => application_at_tail_mut(nested, rest),
        _ => None,
    }
}

fn expression_node_kind(expression: &Expression) -> Option<NodeKind> {
    match expression {
        Expression::SignApplication(_) | Expression::PhonInterpolation(_) => {
            Some(NodeKind::Application)
        }
        Expression::Projection { value, .. } => expression_node_kind(value),
        Expression::Case(_) => Some(NodeKind::Case),
        _ => None,
    }
}

fn trait_at<'a>(language: &'a Language, address: &NodeAddress) -> Result<&'a TraitDef, EditError> {
    let [AddressSegment::Traits(index)] = address.0.as_slice() else {
        return Err(EditError::FieldMismatch(
            "expected trait address".to_owned(),
        ));
    };
    language
        .traits
        .get(*index)
        .ok_or_else(|| EditError::FieldMismatch("trait address is stale".to_owned()))
}

fn trait_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut TraitDef, EditError> {
    let [AddressSegment::Traits(index)] = address.0.as_slice() else {
        return Err(EditError::FieldMismatch(
            "expected trait address".to_owned(),
        ));
    };
    language
        .traits
        .get_mut(*index)
        .ok_or_else(|| EditError::FieldMismatch("trait address is stale".to_owned()))
}

fn sign_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut SignDef, EditError> {
    let [AddressSegment::Signs(index)] = address.0.as_slice() else {
        return Err(EditError::FieldMismatch("expected sign address".to_owned()));
    };
    language
        .signs
        .get_mut(*index)
        .ok_or_else(|| EditError::FieldMismatch("sign address is stale".to_owned()))
}

fn items_at<'a>(
    language: &'a Language,
    parent: &NodeEntryV1,
) -> Result<&'a Vec<SignItem>, EditError> {
    match parent.address.0.as_slice() {
        [AddressSegment::Signs(index)] => Ok(&language.signs[*index].items),
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block)] => {
            Ok(&language.traits[*trait_index].blocks[*block].items)
        }
        _ if parent.kind == NodeKind::CaseBranch => {
            let branch = case_branch_at(language, &parent.address)?;
            let items = match &branch.result {
                Expression::SignFragment(items) | Expression::DimFragment { items, .. } => items,
                _ => {
                    return Err(EditError::FieldMismatch(
                        "case branch does not contain a context fragment".to_owned(),
                    ))
                }
            };
            Ok(items)
        }
        _ => Err(EditError::FieldMismatch(
            "parent has no item sequence".to_owned(),
        )),
    }
}

fn items_at_mut<'a>(
    language: &'a mut Language,
    parent: &NodeEntryV1,
) -> Result<&'a mut Vec<SignItem>, EditError> {
    match parent.address.0.as_slice() {
        [AddressSegment::Signs(index)] => Ok(&mut language.signs[*index].items),
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(block)] => {
            Ok(&mut language.traits[*trait_index].blocks[*block].items)
        }
        _ if parent.kind == NodeKind::CaseBranch => {
            let branch = case_branch_at_mut(language, &parent.address)?;
            let items = match &mut branch.result {
                Expression::SignFragment(items) | Expression::DimFragment { items, .. } => items,
                _ => {
                    return Err(EditError::FieldMismatch(
                        "case branch does not contain a context fragment".to_owned(),
                    ))
                }
            };
            Ok(items)
        }
        _ => Err(EditError::FieldMismatch(
            "parent has no item sequence".to_owned(),
        )),
    }
}

fn item_at_address<'a>(language: &'a Language, address: &NodeAddress) -> Option<&'a SignItem> {
    fn expression_item_at<'a>(
        expression: &'a Expression,
        path: &[AddressSegment],
    ) -> Option<&'a SignItem> {
        match expression {
            Expression::Projection { value, .. } => expression_item_at(value, path),
            Expression::Case(case) => {
                let [AddressSegment::CaseBranches(branch), rest @ ..] = path else {
                    return None;
                };
                let result = &case.branches.get(*branch)?.result;
                match rest {
                    [AddressSegment::Items(item), tail @ ..] => {
                        let items = match result {
                            Expression::SignFragment(items)
                            | Expression::DimFragment { items, .. } => items,
                            _ => return None,
                        };
                        nested_item_at(items.get(*item)?, tail)
                    }
                    [AddressSegment::CaseResult, tail @ ..] => expression_item_at(result, tail),
                    _ => None,
                }
            }
            Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
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
                let [AddressSegment::CaseExpression, tail @ ..] = path else {
                    return None;
                };
                let case = realization.expression.as_ref()?;
                let [AddressSegment::CaseBranches(branch), rest @ ..] = tail else {
                    return None;
                };
                let result = &case.branches.get(*branch)?.result;
                match rest {
                    [AddressSegment::Items(index), nested @ ..] => {
                        let items = match result {
                            Expression::SignFragment(items)
                            | Expression::DimFragment { items, .. } => items,
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

fn item_at_address_mut_option<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Option<&'a mut SignItem> {
    fn expression_item_at_mut<'a>(
        expression: &'a mut Expression,
        path: &[AddressSegment],
    ) -> Option<&'a mut SignItem> {
        match expression {
            Expression::Projection { value, .. } => expression_item_at_mut(value, path),
            Expression::Case(case) => {
                let [AddressSegment::CaseBranches(branch), rest @ ..] = path else {
                    return None;
                };
                let result = &mut case.branches.get_mut(*branch)?.result;
                match rest {
                    [AddressSegment::Items(item), tail @ ..] => {
                        let items = match result {
                            Expression::SignFragment(items)
                            | Expression::DimFragment { items, .. } => items,
                            _ => return None,
                        };
                        nested_item_at_mut(items.get_mut(*item)?, tail)
                    }
                    [AddressSegment::CaseResult, tail @ ..] => expression_item_at_mut(result, tail),
                    _ => None,
                }
            }
            Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
                let [AddressSegment::Items(item), tail @ ..] = path else {
                    return None;
                };
                nested_item_at_mut(items.get_mut(*item)?, tail)
            }
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
                let [AddressSegment::CaseExpression, tail @ ..] = path else {
                    return None;
                };
                let case = realization.expression.as_mut()?;
                let [AddressSegment::CaseBranches(branch), rest @ ..] = tail else {
                    return None;
                };
                let result = &mut case.branches.get_mut(*branch)?.result;
                match rest {
                    [AddressSegment::Items(index), nested @ ..] => {
                        let items = match result {
                            Expression::SignFragment(items)
                            | Expression::DimFragment { items, .. } => items,
                            _ => return None,
                        };
                        nested_item_at_mut(items.get_mut(*index)?, nested)
                    }
                    [AddressSegment::CaseResult, nested @ ..] => {
                        expression_item_at_mut(result, nested)
                    }
                    _ => None,
                }
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

fn item_at_address_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut SignItem, EditError> {
    item_at_address_mut_option(language, address)
        .ok_or_else(|| EditError::FieldMismatch("item address is stale".to_owned()))
}

fn rule_at<'a>(language: &'a Language, address: &NodeAddress) -> Result<&'a Rule, EditError> {
    match item_at_address(language, address) {
        Some(SignItem::Rule(rule) | SignItem::FeatureRule(rule)) => Ok(rule),
        _ => Err(EditError::FieldMismatch("expected rule address".to_owned())),
    }
}

fn rule_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut Rule, EditError> {
    match item_at_address_mut(language, address)? {
        SignItem::Rule(rule) | SignItem::FeatureRule(rule) => Ok(rule),
        _ => Err(EditError::FieldMismatch("expected rule address".to_owned())),
    }
}

// ── P46 S3: phon `PhonBlock` navigation ───────────────────────────────────
// A phon node address is the rule item address followed by a trailing run of
// `Phon*` segments. `PhonThen`/`PhonElse`/`PhonPropagate` descend into a
// sub-block; `PhonLeaf` indexes a statement string (terminal, not a descent).

fn is_phon_segment(segment: &AddressSegment) -> bool {
    matches!(
        segment,
        AddressSegment::PhonLeaf(_) | AddressSegment::PhonThen(_) | AddressSegment::PhonElse(_)
    )
}

/// Split an address into (rule-item address, phon path). When the address has no
/// phon segment the whole address is the rule and the phon path is empty (used
/// when a rule itself is the phon container — its `phon_block` root).
fn split_phon_address(address: &NodeAddress) -> (NodeAddress, Vec<AddressSegment>) {
    match address.0.iter().position(is_phon_segment) {
        Some(pos) => (
            NodeAddress(address.0[..pos].to_vec()),
            address.0[pos..].to_vec(),
        ),
        None => (address.clone(), Vec::new()),
    }
}

/// P46 S4: `Propagate` is a modifier, not an addressing level — unwrap it
/// transparently at every step so a propagate toggle never shifts an address.
fn walk_phon_block<'a>(block: &'a PhonBlock, path: &[AddressSegment]) -> Option<&'a PhonBlock> {
    let block = match block {
        PhonBlock::Propagate(inner) => inner.as_ref(),
        other => other,
    };
    match path.split_first() {
        None => Some(block),
        Some((AddressSegment::PhonThen(index), rest)) => match block {
            PhonBlock::Then(elements) => walk_phon_block(elements.get(*index)?, rest),
            _ => None,
        },
        Some((AddressSegment::PhonElse(index), rest)) => match block {
            PhonBlock::Else(elements) => walk_phon_block(elements.get(*index)?, rest),
            _ => None,
        },
        _ => None,
    }
}

fn walk_phon_block_mut<'a>(
    block: &'a mut PhonBlock,
    path: &[AddressSegment],
) -> Option<&'a mut PhonBlock> {
    let block = match block {
        PhonBlock::Propagate(inner) => inner.as_mut(),
        other => other,
    };
    match path.split_first() {
        None => Some(block),
        Some((AddressSegment::PhonThen(index), rest)) => match block {
            PhonBlock::Then(elements) => walk_phon_block_mut(elements.get_mut(*index)?, rest),
            _ => None,
        },
        Some((AddressSegment::PhonElse(index), rest)) => match block {
            PhonBlock::Else(elements) => walk_phon_block_mut(elements.get_mut(*index)?, rest),
            _ => None,
        },
        _ => None,
    }
}

/// The **raw** element slot at a `PhonThen`/`PhonElse` address — i.e. still
/// wrapped in `Propagate` if it carries the modifier. Used by the propagate
/// toggle, which must see and replace the wrapper itself.
fn phon_element_raw_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut PhonBlock, EditError> {
    let (last, head) = address
        .0
        .split_last()
        .ok_or_else(|| EditError::FieldMismatch("expected phon element address".to_owned()))?;
    let index = match last {
        AddressSegment::PhonThen(index) | AddressSegment::PhonElse(index) => *index,
        _ => {
            return Err(EditError::FieldMismatch(
                "expected phon element address".to_owned(),
            ))
        }
    };
    let container = NodeAddress(head.to_vec());
    match phon_container_block_mut(language, &container)? {
        PhonBlock::Then(elements) | PhonBlock::Else(elements) => elements
            .get_mut(index)
            .ok_or_else(|| EditError::FieldMismatch("stale phon element index".to_owned())),
        _ => Err(EditError::FieldMismatch(
            "phon element parent is not a Then/Else".to_owned(),
        )),
    }
}

/// The `PhonBlock` addressed by a container node (a rule with a phon block, or a
/// `PhonBlockNode`).
fn phon_container_block<'a>(
    language: &'a Language,
    address: &NodeAddress,
) -> Result<&'a PhonBlock, EditError> {
    let (rule_address, path) = split_phon_address(address);
    let root = rule_at(language, &rule_address)?
        .phon_block
        .as_ref()
        .ok_or_else(|| EditError::FieldMismatch("rule has no phon block".to_owned()))?;
    walk_phon_block(root, &path)
        .ok_or_else(|| EditError::FieldMismatch("stale phon block address".to_owned()))
}

fn phon_container_block_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut PhonBlock, EditError> {
    let (rule_address, path) = split_phon_address(address);
    let root = rule_at_mut(language, &rule_address)?
        .phon_block
        .as_mut()
        .ok_or_else(|| EditError::FieldMismatch("rule has no phon block".to_owned()))?;
    walk_phon_block_mut(root, &path)
        .ok_or_else(|| EditError::FieldMismatch("stale phon block address".to_owned()))
}

/// Direct addressable children of a phon block: `Leaf` → its statements
/// (`PhonLeaf`), `Then`/`Else` → their elements (`PhonThen`/`PhonElse`),
/// `Propagate` → transparently its inner block's children under `PhonPropagate`.
/// Mirrors `enumerate_phon_block` in the identity walker.
fn push_phon_children(block: &PhonBlock, base: &NodeAddress, out: &mut Vec<NodeAddress>) {
    match block {
        PhonBlock::Leaf(statements) => {
            for index in 0..statements.len() {
                out.push(base.child(AddressSegment::PhonLeaf(index)));
            }
        }
        PhonBlock::Then(elements) => {
            for index in 0..elements.len() {
                out.push(base.child(AddressSegment::PhonThen(index)));
            }
        }
        PhonBlock::Else(elements) => {
            for index in 0..elements.len() {
                out.push(base.child(AddressSegment::PhonElse(index)));
            }
        }
        // Transparent modifier: children keep the enclosing element's address.
        PhonBlock::Propagate(inner) => push_phon_children(inner, base, out),
    }
}

/// Mutable reference to a phon statement (a `Leaf` line). `address` ends in a
/// `PhonLeaf(index)` segment; its container block must be a `Leaf`.
fn phon_statement_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut String, EditError> {
    let Some(&AddressSegment::PhonLeaf(index)) = address.0.last() else {
        return Err(EditError::FieldMismatch(
            "expected phon statement address".to_owned(),
        ));
    };
    let container = NodeAddress(address.0[..address.0.len() - 1].to_vec());
    match phon_container_block_mut(language, &container)? {
        PhonBlock::Leaf(statements) => statements
            .get_mut(index)
            .ok_or_else(|| EditError::FieldMismatch("stale phon statement index".to_owned())),
        _ => Err(EditError::FieldMismatch(
            "phon statement parent is not a Leaf".to_owned(),
        )),
    }
}

fn realization_at<'a>(
    language: &'a Language,
    address: &NodeAddress,
) -> Result<&'a Realization, EditError> {
    match item_at_address(language, address) {
        Some(SignItem::Realization(value)) => Ok(value),
        _ => Err(EditError::FieldMismatch(
            "expected realization address".to_owned(),
        )),
    }
}

fn definition_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut Def, EditError> {
    match item_at_address_mut(language, address)? {
        SignItem::Def(value) => Ok(value),
        _ => Err(EditError::FieldMismatch(
            "expected definition address".to_owned(),
        )),
    }
}

fn slot_at_mut<'a>(
    language: &'a mut Language,
    address: &NodeAddress,
) -> Result<&'a mut Slot, EditError> {
    match item_at_address_mut(language, address)? {
        SignItem::Slot(value) => Ok(value),
        _ => Err(EditError::FieldMismatch("expected slot address".to_owned())),
    }
}

fn set_rule_branch(
    language: &mut Language,
    address: &NodeAddress,
    value: String,
) -> Result<(), EditError> {
    let parent = address.parent().ok_or(EditError::RootImmutable)?;
    match address.0.last() {
        Some(AddressSegment::RuleElse(index)) => {
            rule_at_mut(language, &parent)?.else_chain[*index] = value
        }
        Some(AddressSegment::RuleThen(index)) => {
            rule_at_mut(language, &parent)?.then_chain[*index] = value
        }
        _ => return Err(EditError::FieldMismatch("expected rule branch".to_owned())),
    }
    Ok(())
}

#[derive(Debug)]
struct SlotRenameScope {
    trait_indices: BTreeSet<usize>,
    sign_indices: BTreeSet<usize>,
    callee_names: BTreeSet<String>,
}

fn slot_rename_scope(
    language: &Language,
    node: &NodeEntryV1,
    old: &str,
) -> Result<SlotRenameScope, EditError> {
    let mut scope = SlotRenameScope {
        trait_indices: BTreeSet::new(),
        sign_indices: BTreeSet::new(),
        callee_names: BTreeSet::new(),
    };
    match node.address.0.as_slice() {
        [AddressSegment::Signs(sign), AddressSegment::Items(_)] => {
            scope.sign_indices.insert(*sign);
            scope
                .callee_names
                .insert(language.signs[*sign].name.clone());
        }
        [AddressSegment::Traits(trait_index), AddressSegment::Blocks(_), AddressSegment::Items(_)] =>
        {
            let target = language.traits[*trait_index].name.clone();
            let (registry, _) = conlang_language::ontology::OntologyRegistry::build(&[language]);
            for (index, trait_def) in language.traits.iter().enumerate() {
                let probe = SignDef {
                    id: conlang_language::SignId::synthetic(),
                    name: "__slot_rename_probe".to_owned(),
                    items: vec![SignItem::Belongs(trait_def.name.clone())],
                };
                let inherited_owner = registry
                    .inheritance_order(&probe)
                    .iter()
                    .filter_map(|source| {
                        registry.node(&source.trait_name).and_then(|node| {
                            node.items
                                .iter()
                                .any(
                                    |item| matches!(item, SignItem::Slot(slot) if slot.name == old),
                                )
                                .then_some(source.trait_name.clone())
                        })
                    })
                    .next_back();
                // Rewrite only consumers whose effective slot is the node
                // being renamed.  A descendant may inherit through an
                // intermediate trait that shadows the same slot name; merely
                // checking its own body would incorrectly rewrite that
                // descendant's references to the shadowing declaration.
                if inherited_owner.as_deref() == Some(target.as_str()) {
                    scope.trait_indices.insert(index);
                }
            }
            for (index, sign) in language.signs.iter().enumerate() {
                let local_shadow = sign
                    .items
                    .iter()
                    .any(|item| matches!(item, SignItem::Slot(slot) if slot.name == old));
                let inheritance = registry.inheritance_order(sign);
                let inherited_owner = inheritance
                    .iter()
                    .filter_map(|source| {
                        registry.node(&source.trait_name).and_then(|node| {
                            node.items
                                .iter()
                                .any(
                                    |item| matches!(item, SignItem::Slot(slot) if slot.name == old),
                                )
                                .then_some(source.trait_name.clone())
                        })
                    })
                    .next_back();
                if !local_shadow && inherited_owner.as_deref() == Some(target.as_str()) {
                    scope.sign_indices.insert(index);
                    scope.callee_names.insert(sign.name.clone());
                }
            }
        }
        _ => return Err(field_mismatch(node, "slot owner")),
    }
    Ok(scope)
}

fn rewrite_slot_consumers(language: &mut Language, scope: &SlotRenameScope, old: &str, new: &str) {
    for index in &scope.trait_indices {
        for block in &mut language.traits[*index].blocks {
            rewrite_local_slot_refs_in_items(&mut block.items, old, new);
        }
    }
    for index in &scope.sign_indices {
        rewrite_local_slot_refs_in_items(&mut language.signs[*index].items, old, new);
    }
    for trait_def in &mut language.traits {
        for block in &mut trait_def.blocks {
            rewrite_application_parameters_in_items(
                &mut block.items,
                &scope.callee_names,
                old,
                new,
            );
        }
    }
    for sign in &mut language.signs {
        rewrite_application_parameters_in_items(&mut sign.items, &scope.callee_names, old, new);
    }
}

fn rewrite_local_slot_refs_in_items(items: &mut [SignItem], old: &str, new: &str) {
    for item in items {
        match item {
            // 義項/衍生邊不持 slot 引用。
            SignItem::Slot(_)
            | SignItem::FeatureDecl(_)
            | SignItem::FeatureValue(_)
            | SignItem::Sense(_)
            | SignItem::SenseEdge(_) => {}
            SignItem::SlotFeatureBinding(binding) => {
                if binding.slot == old {
                    binding.slot = new.to_owned();
                }
                binding.value = rewrite_slot_accesses(&binding.value, old, new);
            }
            SignItem::SlotMap(operation) => match operation {
                SlotMapOp::Preserve { slot }
                | SlotMapOp::Rename { slot, .. }
                | SlotMapOp::AutoFill { slot, .. }
                | SlotMapOp::Internalize { slot }
                | SlotMapOp::Optional { slot, .. }
                    if slot == old =>
                {
                    *slot = new.to_owned();
                }
                _ => {}
            },
            SignItem::RoleBinding(binding) if binding.slot == old => {
                binding.slot = new.to_owned();
            }
            SignItem::Constraint(constraint) => {
                constraint.left = rewrite_slot_operand(&constraint.left, old, new);
                constraint.right = rewrite_slot_operand(&constraint.right, old, new);
            }
            SignItem::Rule(rule) | SignItem::FeatureRule(rule) => {
                rule.body = rewrite_slot_accesses(&rule.body, old, new);
                for branch in &mut rule.else_chain {
                    *branch = rewrite_slot_accesses(branch, old, new);
                }
                for branch in &mut rule.then_chain {
                    *branch = rewrite_slot_accesses(branch, old, new);
                }
            }
            SignItem::Def(def) => {
                def.value = rewrite_slot_template(&def.value, old, new);
                def.value = rewrite_slot_accesses(&def.value, old, new);
            }
            SignItem::Realization(realization) => {
                // Typed realization case slot-renames flow through the shared
                // case-expression rewrite; the former flat branches are gone.
                if let Some(case) = &mut realization.expression {
                    rewrite_local_slot_refs_in_case(case, old, new);
                }
            }
            SignItem::SignExpression(expression) => {
                rewrite_local_slot_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::FeatureExpression(expression) => {
                rewrite_local_slot_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::RoleExpression(expression) => {
                rewrite_local_slot_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::TraitUse { .. }
            | SignItem::Belongs(_)
            | SignItem::RoleDecl(_)
            | SignItem::RoleBinding(_) => {}
        }
    }
}

fn rewrite_local_slot_refs_in_case(case: &mut TypedCase, old: &str, new: &str) {
    if let Some(scrutinee) = &mut case.scrutinee {
        *scrutinee = rewrite_slot_operand(scrutinee, old, new);
        *scrutinee = rewrite_slot_accesses(scrutinee, old, new);
    }
    for branch in &mut case.branches {
        if let conlang_language::CaseCondition::Guard(guard) = &mut branch.condition {
            *guard = rewrite_slot_accesses(guard, old, new);
        }
        rewrite_local_slot_refs_in_expression(&mut branch.result, old, new);
    }
}

fn rewrite_local_slot_refs_in_expression(expression: &mut Expression, old: &str, new: &str) {
    match expression {
        Expression::SignApplication(application) | Expression::PhonInterpolation(application) => {
            rewrite_local_slot_refs_in_application(application, old, new)
        }
        Expression::Projection { value, .. } => {
            rewrite_local_slot_refs_in_expression(value, old, new)
        }
        Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
            rewrite_local_slot_refs_in_items(items, old, new)
        }
        Expression::PhonTemplate(template) => *template = rewrite_slot_template(template, old, new),
        Expression::Slot(slot) if slot == old => *slot = new.to_owned(),
        Expression::Case(case) => rewrite_local_slot_refs_in_case(case, old, new),
        Expression::EnumValue(_) | Expression::SelfSign | Expression::Slot(_) => {}
    }
}

fn rewrite_local_slot_refs_in_application(application: &mut SignApplication, old: &str, new: &str) {
    for argument in &mut application.arguments {
        match &mut argument.value {
            SignArgumentValue::Slot(slot) if slot == old => *slot = new.to_owned(),
            SignArgumentValue::Application(nested) => {
                rewrite_local_slot_refs_in_application(nested, old, new)
            }
            SignArgumentValue::SelfSign | SignArgumentValue::Slot(_) => {}
        }
    }
}

fn rewrite_application_parameters_in_items(
    items: &mut [SignItem],
    callees: &BTreeSet<String>,
    old: &str,
    new: &str,
) {
    for item in items {
        match item {
            SignItem::SignExpression(expression) => rewrite_application_parameters_in_expression(
                &mut expression.expression,
                callees,
                old,
                new,
            ),
            SignItem::FeatureExpression(expression) => {
                rewrite_application_parameters_in_expression(
                    &mut expression.expression,
                    callees,
                    old,
                    new,
                )
            }
            SignItem::RoleExpression(expression) => rewrite_application_parameters_in_expression(
                &mut expression.expression,
                callees,
                old,
                new,
            ),
            SignItem::Realization(realization) => {
                if let Some(case) = &mut realization.expression {
                    for branch in &mut case.branches {
                        rewrite_application_parameters_in_expression(
                            &mut branch.result,
                            callees,
                            old,
                            new,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn rewrite_application_parameters_in_expression(
    expression: &mut Expression,
    callees: &BTreeSet<String>,
    old: &str,
    new: &str,
) {
    match expression {
        Expression::SignApplication(application) | Expression::PhonInterpolation(application) => {
            rewrite_application_parameters(application, callees, old, new)
        }
        Expression::Projection { value, .. } => {
            rewrite_application_parameters_in_expression(value, callees, old, new)
        }
        Expression::SignFragment(items) | Expression::DimFragment { items, .. } => {
            rewrite_application_parameters_in_items(items, callees, old, new)
        }
        Expression::Case(case) => {
            for branch in &mut case.branches {
                rewrite_application_parameters_in_expression(&mut branch.result, callees, old, new);
            }
        }
        Expression::PhonTemplate(_)
        | Expression::EnumValue(_)
        | Expression::SelfSign
        | Expression::Slot(_) => {}
    }
}

fn rewrite_application_parameters(
    application: &mut SignApplication,
    callees: &BTreeSet<String>,
    old: &str,
    new: &str,
) {
    if callees.contains(&application.callee) {
        for argument in &mut application.arguments {
            if argument.name.as_deref() == Some(old) {
                argument.name = Some(new.to_owned());
            }
        }
    }
    for argument in &mut application.arguments {
        if let SignArgumentValue::Application(nested) = &mut argument.value {
            rewrite_application_parameters(nested, callees, old, new);
        }
    }
}

fn rewrite_slot_operand(source: &str, old: &str, new: &str) -> String {
    if source == old {
        return new.to_owned();
    }
    source
        .strip_prefix(old)
        .filter(|rest| rest.starts_with('.'))
        .map(|rest| format!("{new}{rest}"))
        .unwrap_or_else(|| source.to_owned())
}

fn rewrite_slot_template(source: &str, old: &str, new: &str) -> String {
    source.replace(&format!("{{{old}}}"), &format!("{{{new}}}"))
}

fn rewrite_slot_accesses(source: &str, old: &str, new: &str) -> String {
    let needle = format!("$slot.{old}");
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(offset) = source[cursor..].find(&needle) {
        let start = cursor + offset;
        let end = start + needle.len();
        let boundary = source[end..]
            .chars()
            .next()
            .is_none_or(|character| character == '.' || !is_identifier_character(character));
        output.push_str(&source[cursor..start]);
        if boundary {
            output.push_str("$slot.");
            output.push_str(new);
        } else {
            output.push_str(&source[start..end]);
        }
        cursor = end;
    }
    output.push_str(&source[cursor..]);
    output
}

fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | ':' | '/')
}

fn rewrite_sign_refs(language: &mut Language, old: &str, new: &str) {
    for sign in &mut language.signs {
        rewrite_sign_refs_in_items(&mut sign.items, old, new);
    }
    for trait_def in &mut language.traits {
        for block in &mut trait_def.blocks {
            rewrite_sign_refs_in_items(&mut block.items, old, new);
        }
    }
}

fn rewrite_sign_refs_in_items(items: &mut [SignItem], old: &str, new: &str) {
    for item in items {
        match item {
            SignItem::SlotMap(SlotMapOp::AutoFill { filler, .. }) if filler == old => {
                *filler = new.to_owned()
            }
            SignItem::Def(def) if def.path == "origin" && def.value == format!("sign({old})") => {
                def.value = format!("sign({new})")
            }
            SignItem::SignExpression(expression) => {
                rewrite_sign_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::FeatureExpression(expression) => {
                rewrite_sign_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::RoleExpression(expression) => {
                rewrite_sign_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::Realization(realization) => {
                if let Some(case) = &mut realization.expression {
                    rewrite_sign_refs_in_case(case, old, new);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_sign_refs_in_expression(expression: &mut Expression, old: &str, new: &str) {
    match expression {
        Expression::SignApplication(application) | Expression::PhonInterpolation(application) => {
            rewrite_sign_refs_in_application(application, old, new)
        }
        Expression::Projection { value, .. } => rewrite_sign_refs_in_expression(value, old, new),
        Expression::Case(case) => rewrite_sign_refs_in_case(case, old, new),
        _ => {}
    }
}

fn rewrite_sign_refs_in_application(application: &mut SignApplication, old: &str, new: &str) {
    if application.callee == old {
        application.callee = new.to_owned();
    }
    for argument in &mut application.arguments {
        if let SignArgumentValue::Application(nested) = &mut argument.value {
            rewrite_sign_refs_in_application(nested, old, new);
        }
    }
}

fn rewrite_sign_refs_in_case(case: &mut TypedCase, old: &str, new: &str) {
    for branch in &mut case.branches {
        rewrite_sign_refs_in_expression(&mut branch.result, old, new);
    }
}

fn rewrite_trait_refs(language: &mut Language, old: &str, new: &str) {
    for sign in &mut language.signs {
        rewrite_trait_refs_in_items(&mut sign.items, old, new);
    }
    for trait_def in &mut language.traits {
        for block in &mut trait_def.blocks {
            rewrite_trait_refs_in_items(&mut block.items, old, new);
        }
    }
}

fn rewrite_trait_refs_in_items(items: &mut [SignItem], old: &str, new: &str) {
    for item in items {
        match item {
            SignItem::TraitUse { name, .. } | SignItem::Belongs(name) if name == old => {
                *name = new.to_owned()
            }
            SignItem::Slot(slot) => {
                if let SlotConstraint::Category(name) = &mut slot.constraint {
                    if name == old {
                        *name = new.to_owned();
                    }
                }
            }
            SignItem::RoleDecl(role) if role.constraint == old => role.constraint = new.to_owned(),
            SignItem::SignExpression(expression) => {
                rewrite_trait_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::FeatureExpression(expression) => {
                rewrite_trait_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::RoleExpression(expression) => {
                rewrite_trait_refs_in_expression(&mut expression.expression, old, new)
            }
            SignItem::Realization(realization) => {
                if let Some(case) = &mut realization.expression {
                    rewrite_trait_refs_in_case(case, old, new);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_trait_refs_in_expression(expression: &mut Expression, old: &str, new: &str) {
    match expression {
        Expression::Projection { value, .. } => rewrite_trait_refs_in_expression(value, old, new),
        Expression::Case(case) => rewrite_trait_refs_in_case(case, old, new),
        _ => {}
    }
}

fn rewrite_trait_refs_in_case(case: &mut TypedCase, old: &str, new: &str) {
    let compares_phon_category = case
        .scrutinee
        .as_deref()
        .and_then(|value| value.split_once('.'))
        .is_some_and(|(_, projection)| projection == "phon");
    for branch in &mut case.branches {
        match &mut branch.condition {
            conlang_language::CaseCondition::Guard(guard) => {
                *guard = rewrite_bracketed_category(guard, old, new);
            }
            conlang_language::CaseCondition::Equals(category)
                if compares_phon_category && category == old =>
            {
                *category = new.to_owned();
            }
            conlang_language::CaseCondition::Equals(_) | conlang_language::CaseCondition::Else => {}
        }
        for belongs in &mut branch.belongs {
            if belongs == old {
                *belongs = new.to_owned();
            }
        }
        rewrite_trait_refs_in_expression(&mut branch.result, old, new);
    }
}

fn rewrite_bracketed_category(source: &str, old: &str, new: &str) -> String {
    source
        .split("&&")
        .map(str::trim)
        .map(|conjunct| {
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
                })
                .map(str::trim);
            if category == Some(old) {
                match (conjunct.find('['), conjunct.rfind(']')) {
                    (Some(open), Some(close)) if open < close => {
                        format!("{}[{new}]{}", &conjunct[..open], &conjunct[close + 1..])
                    }
                    _ => conjunct.to_owned(),
                }
            } else {
                conjunct.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

fn snapshots(document: &LanguageDocument) -> BTreeMap<NodeId, NodeSnapshot> {
    document
        .identities()
        .nodes
        .iter()
        .map(|entry| {
            (
                entry.id.clone(),
                NodeSnapshot {
                    id: entry.id.clone(),
                    kind: entry.kind,
                    parent: entry.parent.clone(),
                    address: entry.address.clone(),
                    value: debug_value(document.language(), entry),
                },
            )
        })
        .collect()
}

fn snapshot_for(document: &LanguageDocument, reference: &NodeRef) -> Option<NodeSnapshot> {
    snapshot_by_id(document, &reference.id).filter(|snapshot| snapshot.kind == reference.expected)
}

fn snapshot_by_id(document: &LanguageDocument, id: &NodeId) -> Option<NodeSnapshot> {
    snapshots(document).remove(id)
}

fn debug_value(language: &Language, entry: &NodeEntryV1) -> String {
    match entry.address.0.as_slice() {
        [] => "Language".to_owned(),
        [AddressSegment::DslDeclarations(index)] => format!("{:?}", language.dsl_decls[*index]),
        [AddressSegment::Prosody] => format!("{:?}", language.prosody),
        [AddressSegment::Distribution(index)] => format!("{:?}", language.distribution[*index]),
        [AddressSegment::Traits(index)] => {
            let value = &language.traits[*index];
            format!("Trait(name={:?},global={})", value.name, value.global)
        }
        [AddressSegment::Signs(index)] => {
            format!("Sign(name={:?})", language.signs[*index].name)
        }
        [AddressSegment::Traits(_), AddressSegment::Blocks(_)] => "Block".to_owned(),
        address if item_at_address(language, &NodeAddress(address.to_vec())).is_some() => {
            match item_at_address(language, &NodeAddress(address.to_vec())).unwrap() {
                SignItem::Rule(rule) | SignItem::FeatureRule(rule) => format!(
                    "Rule(body={:?},stage={:?},dim={:?})",
                    rule.body, rule.stage, rule.dim
                ),
                SignItem::Realization(_) => "Realization".to_owned(),
                SignItem::SignExpression(expression) => expression_case(&expression.expression)
                    .map(case_header_value)
                    .unwrap_or_else(|| expression_value(&expression.expression)),
                SignItem::FeatureExpression(expression) => format!(
                    "FeatureExpression(dim={:?},name={:?},value={})",
                    expression.dim,
                    expression.name,
                    expression_value(&expression.expression)
                ),
                SignItem::RoleExpression(expression) => format!(
                    "RoleExpression(name={:?},value={})",
                    expression.name,
                    expression_value(&expression.expression)
                ),
                SignItem::Constraint(constraint) => constraint_value(constraint),
                item => format!("{item:?}"),
            }
        }
        address => {
            let parent = NodeAddress(address[..address.len() - 1].to_vec());
            match address.last() {
                Some(AddressSegment::RuleElse(index)) => format!(
                    "{:?}",
                    rule_at(language, &parent)
                        .ok()
                        .and_then(|rule| rule.else_chain.get(*index))
                ),
                Some(AddressSegment::RuleThen(index)) => format!(
                    "{:?}",
                    rule_at(language, &parent)
                        .ok()
                        .and_then(|rule| rule.then_chain.get(*index))
                ),
                Some(AddressSegment::CaseExpression) => case_at(language, &entry.address)
                    .map(case_header_value)
                    .unwrap_or_else(|| "<unavailable>".to_owned()),
                Some(AddressSegment::CaseBranches(_)) => case_branch_at(language, &entry.address)
                    .map(case_branch_value)
                    .unwrap_or_else(|_| "<unavailable>".to_owned()),
                Some(AddressSegment::CaseResult) => match entry.kind {
                    NodeKind::Application => application_at(language, &entry.address)
                        .map(application_value)
                        .unwrap_or_else(|_| "<unavailable>".to_owned()),
                    NodeKind::Case => case_at(language, &entry.address)
                        .map(case_header_value)
                        .unwrap_or_else(|| "<unavailable>".to_owned()),
                    _ => "<unavailable>".to_owned(),
                },
                Some(AddressSegment::ApplicationArguments(_)) => {
                    application_at(language, &entry.address)
                        .map(application_value)
                        .unwrap_or_else(|_| "<unavailable>".to_owned())
                }
                _ => "<unavailable>".to_owned(),
            }
        }
    }
}

fn constraint_value(constraint: &BinaryConstraint) -> String {
    format!(
        "Constraint(predicate={:?},left={:?},right={:?})",
        constraint.predicate, constraint.left, constraint.right
    )
}

fn case_header_value(case: &TypedCase) -> String {
    format!(
        "Case(selection={:?},expected={:?},scrutinee={:?})",
        case.selection, case.expected, case.scrutinee
    )
}

fn case_branch_value(branch: &CaseBranch) -> String {
    let result = expression_structure_value(&branch.result);
    format!(
        "CaseBranch(condition={:?},belongs={:?},result={result})",
        branch.condition, branch.belongs
    )
}

fn expression_structure_value(expression: &Expression) -> String {
    match expression {
        Expression::SignApplication(_) => "node(Application)".to_owned(),
        Expression::PhonInterpolation(_) => "PhonInterpolation(node(Application))".to_owned(),
        Expression::Projection { value, dimension } => format!(
            "Projection({dimension:?},{})",
            expression_structure_value(value)
        ),
        Expression::Case(_) => "node(Case)".to_owned(),
        _ => expression_value(expression),
    }
}

fn application_value(application: &SignApplication) -> String {
    let arguments = application
        .arguments
        .iter()
        .map(|argument| {
            let value = match &argument.value {
                SignArgumentValue::SelfSign => "$self".to_owned(),
                SignArgumentValue::Slot(slot) => format!("slot({slot:?})"),
                SignArgumentValue::Application(_) => "node(Application)".to_owned(),
            };
            format!("{:?}={value}", argument.name)
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "Application(callee={:?},arguments=[{arguments}])",
        application.callee
    )
}

fn expression_value(expression: &Expression) -> String {
    match expression {
        Expression::SignApplication(application) => application_value(application),
        Expression::PhonInterpolation(application) => {
            format!("PhonInterpolation({})", application_value(application))
        }
        Expression::Projection { value, dimension } => {
            format!("Projection({dimension:?},{})", expression_value(value))
        }
        Expression::SignFragment(items) => format!("SignContext(items={})", items.len()),
        Expression::DimFragment { dim, items } => {
            format!("{}Context(items={})", dim.keyword(), items.len())
        }
        Expression::PhonTemplate(value) => format!("PhonTemplate({value:?})"),
        Expression::EnumValue(value) => format!("EnumValue({value:?})"),
        Expression::SelfSign => "$self".to_owned(),
        Expression::Slot(value) => format!("slot({value:?})"),
        Expression::Case(case) => case_header_value(case),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_anchor_variants_are_distinct() {
        assert_ne!(Anchor::Start, Anchor::End);
    }
}
