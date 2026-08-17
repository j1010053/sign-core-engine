//! Explicit identity reconciliation for externally edited `.lang` source.
//!
//! Exact `LanguageDocument::open` deliberately remains digest-strict. This
//! module is the separate, auditable recovery path: old identities are reused
//! only when a hint or a mutually unique semantic signal proves the match.

use crate::{debug_value, item_at_address, render_case_branch_block, render_item_block};
use conlang_language::{
    AddressSegment, IdentityError, IdentityNamespace, Language, LanguageDocument, NodeAddress,
    NodeEntryV1, NodeId, NodeKind, NodeRef, RefBindingV1, RefTargetV1, SignItem,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileHint {
    pub previous: NodeRef,
    pub edited_address: NodeAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReconcileHints {
    pub matches: Vec<ReconcileHint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciledIdentity {
    pub previous: NodeRef,
    pub edited_address: NodeAddress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileAmbiguity {
    pub edited_address: NodeAddress,
    pub kind: NodeKind,
    pub candidates: Vec<NodeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReconcileReport {
    pub matched: Vec<ReconciledIdentity>,
    pub inserted: Vec<NodeRef>,
    pub deleted: Vec<NodeRef>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("IDENTITY_RECONCILE_HINT_INVALID: {0}")]
    InvalidHint(String),
    #[error("IDENTITY_RECONCILE_AMBIGUOUS: {0:?}")]
    Ambiguous(Vec<ReconcileAmbiguity>),
}

/// Reconcile an externally edited source against a known document lineage.
///
/// Replay inserts must run from `before.fork(edit_namespace)` (the normal
/// ChangeSet interpreter does this automatically), because fresh nodes are
/// allocated in the explicit edit namespace returned here.
pub fn reconcile_edited_source(
    before: &LanguageDocument,
    edited_source: &str,
    edit_namespace: &str,
    hints: &ReconcileHints,
) -> Result<(LanguageDocument, ReconcileReport), ReconcileError> {
    let fresh = LanguageDocument::import_new_root(edited_source, edit_namespace)?;
    let old = entries(before);
    let edited = entries(&fresh);
    let mut matched: BTreeMap<NodeId, NodeId> = BTreeMap::new();
    let mut used_old = BTreeSet::new();

    apply_hints(before, &fresh, hints, &mut matched, &mut used_old)?;

    let old_root = old
        .values()
        .find(|entry| entry.parent.is_none() && entry.kind == NodeKind::Language)
        .expect("validated document has a root");
    let fresh_root = edited
        .values()
        .find(|entry| entry.parent.is_none() && entry.kind == NodeKind::Language)
        .expect("fresh import has a root");
    if let Some(hinted) = matched.get(&fresh_root.id) {
        if hinted != &old_root.id {
            return Err(ReconcileError::InvalidHint(
                "the edited Language root must match the previous Language root".to_owned(),
            ));
        }
    } else {
        matched.insert(fresh_root.id.clone(), old_root.id.clone());
        used_old.insert(old_root.id.clone());
    }

    loop {
        let mut progress = false;
        progress |= match_unique_pass(
            before,
            &fresh,
            &old,
            &edited,
            &mut matched,
            &mut used_old,
            MatchSignal::ExplicitName,
        );
        progress |= match_unique_pass(
            before,
            &fresh,
            &old,
            &edited,
            &mut matched,
            &mut used_old,
            MatchSignal::AnonymousSubtree,
        );
        progress |= match_unique_pass(
            before,
            &fresh,
            &old,
            &edited,
            &mut matched,
            &mut used_old,
            MatchSignal::ExactSubtree,
        );
        if !progress {
            break;
        }
    }

    let ambiguities = collect_ambiguities(&fresh, &old, &edited, &matched, &used_old);
    if !ambiguities.is_empty() {
        return Err(ReconcileError::Ambiguous(ambiguities));
    }

    materialize_reconciled(before, fresh, edit_namespace, matched, used_old)
}

fn entries(document: &LanguageDocument) -> BTreeMap<NodeId, NodeEntryV1> {
    document
        .identities()
        .nodes
        .iter()
        .map(|entry| (entry.id.clone(), entry.clone()))
        .collect()
}

fn apply_hints(
    before: &LanguageDocument,
    fresh: &LanguageDocument,
    hints: &ReconcileHints,
    matched: &mut BTreeMap<NodeId, NodeId>,
    used_old: &mut BTreeSet<NodeId>,
) -> Result<(), ReconcileError> {
    for hint in &hints.matches {
        let old = before.node(&hint.previous).ok_or_else(|| {
            ReconcileError::InvalidHint(format!(
                "unknown previous {:?} {}",
                hint.previous.expected, hint.previous.id
            ))
        })?;
        let edited = fresh.node_at(&hint.edited_address).ok_or_else(|| {
            ReconcileError::InvalidHint(format!("no edited node at {:?}", hint.edited_address))
        })?;
        if old.kind != edited.kind {
            return Err(ReconcileError::InvalidHint(format!(
                "kind mismatch: previous {:?}, edited {:?}",
                old.kind, edited.kind
            )));
        }
        if used_old.contains(&old.id) || matched.contains_key(&edited.id) {
            return Err(ReconcileError::InvalidHint(
                "a hint target may appear only once".to_owned(),
            ));
        }
        matched.insert(edited.id.clone(), old.id.clone());
        used_old.insert(old.id.clone());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum MatchSignal {
    ExplicitName,
    AnonymousSubtree,
    ExactSubtree,
}

fn match_unique_pass(
    before: &LanguageDocument,
    fresh: &LanguageDocument,
    old: &BTreeMap<NodeId, NodeEntryV1>,
    edited: &BTreeMap<NodeId, NodeEntryV1>,
    matched: &mut BTreeMap<NodeId, NodeId>,
    used_old: &mut BTreeSet<NodeId>,
    signal: MatchSignal,
) -> bool {
    let mut proposals: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    for entry in edited
        .values()
        .filter(|entry| !matched.contains_key(&entry.id))
    {
        let Some(old_parent) = entry.parent.as_ref().and_then(|parent| matched.get(parent)) else {
            continue;
        };
        let value = match signal_value(fresh, entry, signal) {
            Some(value) => value,
            None => continue,
        };
        let candidates = old
            .values()
            .filter(|candidate| !used_old.contains(&candidate.id))
            .filter(|candidate| candidate.parent.as_ref() == Some(old_parent))
            .filter(|candidate| candidate.kind == entry.kind)
            .filter(|candidate| signal_value(before, candidate, signal).as_ref() == Some(&value))
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            proposals
                .entry(candidates[0].clone())
                .or_default()
                .push(entry.id.clone());
        }
    }
    let accepted = proposals
        .into_iter()
        .filter_map(|(old_id, edited_ids)| {
            (edited_ids.len() == 1).then(|| (edited_ids[0].clone(), old_id))
        })
        .collect::<Vec<_>>();
    let changed = !accepted.is_empty();
    for (edited_id, old_id) in accepted {
        matched.insert(edited_id, old_id.clone());
        used_old.insert(old_id);
    }
    changed
}

fn signal_value(
    document: &LanguageDocument,
    entry: &NodeEntryV1,
    signal: MatchSignal,
) -> Option<String> {
    match signal {
        MatchSignal::ExplicitName => explicit_name(document, entry),
        MatchSignal::AnonymousSubtree => Some(subtree_fingerprint(document, entry, true)),
        MatchSignal::ExactSubtree => Some(subtree_fingerprint(document, entry, false)),
    }
}

fn explicit_name(document: &LanguageDocument, entry: &NodeEntryV1) -> Option<String> {
    let language = document.language();
    match entry.address.0.as_slice() {
        [AddressSegment::Distribution(index)] => {
            return language
                .distribution
                .get(*index)
                .map(|(key, _)| key.clone())
        }
        [AddressSegment::Traits(index)] => {
            return language.traits.get(*index).map(|value| value.name.clone())
        }
        [AddressSegment::Signs(index)] => {
            return language.signs.get(*index).map(|value| value.name.clone())
        }
        _ => {}
    }
    if let Some(item) = item_at_address(language, &entry.address) {
        return match item {
            SignItem::TraitMount { name, kind: conlang_language::TraitMountKind::Whole | conlang_language::TraitMountKind::Block(_) } | SignItem::TraitMount { name: name, kind: conlang_language::TraitMountKind::Declaration } => Some(name.clone()),
            SignItem::Slot(value) => Some(value.name.clone()),
            SignItem::FeatureDecl(value) => Some(value.name.clone()),
            SignItem::FeatureValue(value) => Some(value.name.clone()),
            SignItem::FeatureExpression(value) => Some(value.name.clone()),
            SignItem::RoleDecl(value) => Some(value.name.clone()),
            SignItem::RoleBinding(value) => Some(value.name.clone()),
            SignItem::RoleExpression(value) => Some(value.name.clone()),
            SignItem::Sense(value) => Some(value.name.clone()),
            SignItem::Def(value) => Some(value.path.clone()),
            SignItem::Rule(value) | SignItem::FeatureRule(value) => value.name.clone(),
            _ => None,
        };
    }
    match entry.kind {
        NodeKind::Case => {
            crate::case_at(language, &entry.address).and_then(|case| case.name.clone())
        }
        NodeKind::CaseBranch => crate::case_branch_at(language, &entry.address)
            .ok()
            .and_then(|branch| branch.name.clone()),
        _ => None,
    }
}

fn subtree_fingerprint(
    document: &LanguageDocument,
    root: &NodeEntryV1,
    anonymize_root_name: bool,
) -> String {
    let mut values = document
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.address.starts_with(&root.address))
        .map(|entry| {
            let suffix = &entry.address.0[root.address.0.len()..];
            let value = if entry.id == root.id && anonymize_root_name {
                anonymous_node_value(document, entry)
            } else {
                semantic_node_value(document, entry)
            };
            format!("{suffix:?}|{:?}|{value}", entry.kind)
        })
        .collect::<Vec<_>>();
    values.sort();
    values.join("\n")
}

fn anonymous_node_value(document: &LanguageDocument, entry: &NodeEntryV1) -> String {
    let language = document.language();
    match entry.address.0.as_slice() {
        [AddressSegment::Traits(index)] => {
            let mut value = language.traits[*index].clone();
            value.name = "<identity-name>".to_owned();
            let mut fragment = Language::new();
            fragment.traits.push(value);
            fragment.dump()
        }
        [AddressSegment::Signs(index)] => {
            let mut value = language.signs[*index].clone();
            value.name = "<identity-name>".to_owned();
            let mut fragment = Language::new();
            fragment.signs.push(value);
            fragment.dump()
        }
        _ => semantic_node_value(document, entry),
    }
}

fn semantic_node_value(document: &LanguageDocument, entry: &NodeEntryV1) -> String {
    if let Some(item) = item_at_address(document.language(), &entry.address) {
        return render_item_block(item);
    }
    if entry.kind == NodeKind::CaseBranch {
        if let Ok(branch) = crate::case_branch_at(document.language(), &entry.address) {
            return render_case_branch_block(branch);
        }
    }
    debug_value(document.language(), entry)
}

fn collect_ambiguities(
    fresh: &LanguageDocument,
    old: &BTreeMap<NodeId, NodeEntryV1>,
    edited: &BTreeMap<NodeId, NodeEntryV1>,
    matched: &BTreeMap<NodeId, NodeId>,
    used_old: &BTreeSet<NodeId>,
) -> Vec<ReconcileAmbiguity> {
    let mut ambiguities = Vec::new();
    for entry in edited
        .values()
        .filter(|entry| !matched.contains_key(&entry.id))
    {
        let Some(old_parent) = entry.parent.as_ref().and_then(|parent| matched.get(parent)) else {
            continue;
        };
        let mut candidates = old
            .values()
            .filter(|candidate| !used_old.contains(&candidate.id))
            .filter(|candidate| candidate.parent.as_ref() == Some(old_parent))
            .filter(|candidate| candidate.kind == entry.kind)
            .map(|candidate| NodeRef::new(candidate.id.clone(), candidate.kind))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.id.cmp(&right.id));
        if !candidates.is_empty() {
            ambiguities.push(ReconcileAmbiguity {
                edited_address: fresh
                    .node_at(&entry.address)
                    .expect("entry came from fresh")
                    .address
                    .clone(),
                kind: entry.kind,
                candidates,
            });
        }
    }
    ambiguities.sort_by(|left, right| left.edited_address.cmp(&right.edited_address));
    ambiguities
}

fn materialize_reconciled(
    before: &LanguageDocument,
    fresh: LanguageDocument,
    edit_namespace: &str,
    mut matched: BTreeMap<NodeId, NodeId>,
    used_old: BTreeSet<NodeId>,
) -> Result<(LanguageDocument, ReconcileReport), ReconcileError> {
    let (_, mut identities) = before.clone().into_edit_parts();
    if !identities
        .allocators
        .iter()
        .any(|allocator| allocator.namespace == edit_namespace)
    {
        let fork = before.fork(edit_namespace.to_owned())?;
        identities = fork.into_edit_parts().1;
    }
    identities.active_namespace = edit_namespace.to_owned();
    let allocator = identities
        .allocators
        .iter_mut()
        .find(|allocator| allocator.namespace == edit_namespace)
        .expect("existing or forked allocator");
    let mut next = allocator.next_ordinal;

    let (language, fresh_manifest) = fresh.into_edit_parts();
    let mut fresh_entries = fresh_manifest.nodes.clone();
    fresh_entries.sort_by(|left, right| left.address.cmp(&right.address));
    let mut inserted = Vec::new();
    for entry in &fresh_entries {
        if matched.contains_key(&entry.id) {
            continue;
        }
        let id = NodeId::new(IdentityNamespace::Document(edit_namespace.to_owned()), next);
        next = next.checked_add(1).ok_or_else(|| {
            IdentityError::InvalidManifest("identity allocator exhausted".to_owned())
        })?;
        matched.insert(entry.id.clone(), id.clone());
        inserted.push(NodeRef::new(id, entry.kind));
    }
    allocator.next_ordinal = next;

    identities.nodes = fresh_entries
        .iter()
        .map(|entry| NodeEntryV1 {
            id: matched[&entry.id].clone(),
            kind: entry.kind,
            parent: entry.parent.as_ref().map(|parent| matched[parent].clone()),
            address: entry.address.clone(),
        })
        .collect();
    identities.refs = fresh_manifest
        .refs
        .iter()
        .map(|binding| RefBindingV1 {
            owner: matched[&binding.owner].clone(),
            field: binding.field.clone(),
            target: match &binding.target {
                RefTargetV1::Local { target } => RefTargetV1::Local {
                    target: NodeRef::new(matched[&target.id].clone(), target.expected),
                },
                RefTargetV1::External { spelling, expected } => RefTargetV1::External {
                    spelling: spelling.clone(),
                    expected: *expected,
                },
            },
        })
        .collect();
    identities.source_sha256 = fresh_manifest.source_sha256;

    let document = LanguageDocument::from_edit_parts(language, identities)?;
    let old = entries(before);
    let mut report = ReconcileReport {
        matched: fresh_entries
            .iter()
            .filter_map(|entry| {
                let final_id = &matched[&entry.id];
                old.get(final_id).map(|old_entry| ReconciledIdentity {
                    previous: NodeRef::new(final_id.clone(), old_entry.kind),
                    edited_address: entry.address.clone(),
                })
            })
            .collect(),
        inserted,
        deleted: old
            .values()
            .filter(|entry| !used_old.contains(&entry.id))
            .map(|entry| NodeRef::new(entry.id.clone(), entry.kind))
            .collect(),
    };
    report
        .matched
        .sort_by(|left, right| left.edited_address.cmp(&right.edited_address));
    report
        .inserted
        .sort_by(|left, right| left.id.cmp(&right.id));
    report.deleted.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((document, report))
}
