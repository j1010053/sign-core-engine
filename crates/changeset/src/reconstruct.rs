//! **從兩份狀態還原成四原語**(#8;docs/12「手改 → diff → changeset」的後半段)。
//!
//! ## 為什麼不沿用 `LanguageDiff`
//!
//! `LanguageDiff` 的 `NodeSnapshot.value` 是 **`debug_value` 產生的 Debug 字串**
//! (`Sign(name="x")`),它是**給人看的**。要從它反推 `NodeUpdate` 就得剖析 Debug 格式
//! ——而 P57 的鐵律正是「分類**只依變體**,不 `match` 訊息字串(改字不該改行為)」。
//! 有人為了 debug 好讀改一下格式,還原就會**靜默產生錯的 changeset**。
//!
//! 故這裡直接比**型別化的節點**(`DetachedNode`)。
//!
//! ## 比對必須是「淺的」
//!
//! `DetachedNode::Sign` 裡含整個 `SignDef`(包括 items)。直接比會讓子節點的改動
//! **在父節點上重複算一次**。而 `NodeUpdate` 的變體集合正好就是「每個節點自己的
//! 可編輯欄位」——所以淺層比對 = 逐 `NodeUpdate` 變體比一次。
//!
//! ## 漏比欄位是主要風險,靠往返性質擋
//!
//! 少寫一個欄位比較**不是型別錯誤**,編譯器抓不到,後果是「那種改動永遠不出現在
//! changeset 裡」——靜默丟改動。故驗證的主力是**往返性質**:
//!
//! ```text
//! before ──(已知 .chg)──► after
//! before ──(本模組)────► edits
//!            apply(before, edits) == after      ← 漏比任何欄位,這裡就不等
//! ```
//!
//! ## 未支援的節點種類**明確拒絕**
//!
//! 沒實作淺層比對的種類回 `Unsupported`,**不回空**。回空等於默默宣稱「這裡沒有
//! 改動」,而那是本專案一路在避免的靜默近似。

use crate::{
    application_at, apply_structural, case_at, detached_at, item_at_address, Anchor, DetachedNode,
    EditError, LanguageDocument, NodeUpdate, PrimitiveEdit,
};
use conlang_language::{
    AddressSegment, CaseBranch, Expression, IdentityError, NodeEntryV1, NodeId, NodeKind, NodeRef,
    PhonBlock, Realization, Rule, SignApplication, SignArgumentValue, SignItem, TypedCase,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, thiserror::Error)]
pub enum ReconstructError {
    /// 這種節點的淺層欄位比對尚未實作——**明確拒絕,不默默漏掉改動**。
    #[error("RECONSTRUCT_UNSUPPORTED: {kind:?} ({detail})")]
    Unsupported { kind: NodeKind, detail: String },
    #[error(transparent)]
    Edit(#[from] EditError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconstructCapability {
    Immutable,
    StructuralContainer,
    TypedUpdate,
    Unsupported,
}

/// Exhaustive capability map for every persisted `NodeKind`. Adding a new
/// addressable kind therefore forces reconstruct to make an explicit choice.
fn capability(kind: NodeKind) -> ReconstructCapability {
    match kind {
        // `pass` 沒有欄位:改變它只能靠 delete/insert,沒有 typed update 可言。
        NodeKind::Pass => ReconstructCapability::Immutable,
        NodeKind::Language => ReconstructCapability::Immutable,
        NodeKind::Block => ReconstructCapability::StructuralContainer,
        NodeKind::RealizationBranch => ReconstructCapability::Unsupported,
        NodeKind::DslDeclaration
        | NodeKind::Distribution
        | NodeKind::Trait
        | NodeKind::Sign
        | NodeKind::TraitUse
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
        | NodeKind::FeatureRule
        | NodeKind::Definition
        | NodeKind::Rule
        | NodeKind::RuleElseBranch
        | NodeKind::RuleThenBranch
        | NodeKind::PhonStatement
        | NodeKind::PhonBlockNode
        | NodeKind::Application
        | NodeKind::Case
        | NodeKind::CaseBranch
        | NodeKind::Constraint => ReconstructCapability::TypedUpdate,
    }
}

#[derive(Debug, Clone)]
enum ShallowNode {
    Detached(DetachedNode),
    ExpressionItem(SignItem),
    Realization(Realization),
    Case(TypedCase),
    CaseBranch(CaseBranch),
    Application(SignApplication),
}

/// 兩份**都有身分**的文件 → 還原成四原語序列。
///
/// **前提:兩邊的 id 出自同一血脈**。手改出來的 `.lang` 是一段沒有 id 的文字,
/// 得先「認親」(配 id)才能餵進來——那是另一件事,不在本模組。
///
/// 發出順序:**更新 → 移動 → 新增 → 刪除**。新增排在刪除之前,是為了讓錨點
/// (`Anchor::After(兄弟)`)在套用當下一定還指得到人。
pub fn reconstruct(
    before: &LanguageDocument,
    after: &LanguageDocument,
) -> Result<Vec<PrimitiveEdit>, ReconstructError> {
    let old = entries(before);
    let new = entries(after);
    let mut edits = Vec::new();
    let mut working = before.clone();
    let mut target_to_working: BTreeMap<NodeId, NodeId> = old
        .keys()
        .filter(|id| new.contains_key(*id))
        .map(|id| (id.clone(), id.clone()))
        .collect();
    let mut covered_added = BTreeSet::new();
    let mut covered_removed = BTreeSet::new();

    // ── ① 更新:兩邊都有的,比淺層欄位 ──
    for (id, old_entry) in &old {
        let Some(new_entry) = new.get(id) else {
            continue;
        };
        match capability(old_entry.kind) {
            ReconstructCapability::Immutable | ReconstructCapability::StructuralContainer => {
                continue;
            }
            ReconstructCapability::Unsupported => {
                return Err(ReconstructError::Unsupported {
                    kind: old_entry.kind,
                    detail: "persisted node kind has no reconstruction contract".to_owned(),
                });
            }
            ReconstructCapability::TypedUpdate => {}
        }
        if old_entry.kind != new_entry.kind {
            return Err(ReconstructError::Unsupported {
                kind: new_entry.kind,
                detail: format!(
                    "node kind changed under stable identity from {:?}",
                    old_entry.kind
                ),
            });
        }
        let old_node = shallow_at(before, old_entry)?;
        let new_node = shallow_at(after, new_entry)?;
        for change in shallow_state_updates(&old_node, &new_node)? {
            let edit = PrimitiveEdit::Update {
                node: NodeRef::new(id.clone(), old_entry.kind),
                change,
            };
            let previous = working.clone();
            working = apply_structural(working, &edit)?;
            map_updated_descendants(
                &previous,
                &working,
                after,
                old_entry,
                new_entry,
                &old,
                &new,
                &mut target_to_working,
                &mut covered_added,
                &mut covered_removed,
            )?;
            edits.push(edit);
        }
    }

    // ── ② 換父移動:先落到新父群組末端,精確順序留給 LCS 階段 ──
    for (id, old_entry) in &old {
        let Some(new_entry) = new.get(id) else {
            continue;
        };
        if old_entry.parent == new_entry.parent {
            continue;
        }
        let Some(parent) = new_entry.parent.clone() else {
            return Err(ReconstructError::Unsupported {
                kind: new_entry.kind,
                detail: "moved to the document root".to_owned(),
            });
        };
        let working_parent = target_to_working.get(&parent).cloned().ok_or_else(|| {
            ReconstructError::Unsupported {
                kind: new_entry.kind,
                detail: "moved below a parent that does not yet exist".to_owned(),
            }
        })?;
        let parent_kind = entry_by_id(&working, &working_parent)
            .map(|entry| entry.kind)
            .unwrap_or(NodeKind::Language);
        let edit = PrimitiveEdit::Move {
            node: NodeRef::new(id.clone(), old_entry.kind),
            new_parent: NodeRef::new(working_parent, parent_kind),
            anchor: Anchor::End,
        };
        working = apply_structural(working, &edit)?;
        edits.push(edit);
    }

    // ── ③ 新增:只發**最上層**的,帶完整子樹 ──
    //
    // 優先錨在目標序列中下一個已存在 sibling 之前；之後 sibling 被移動時，
    // 新節點仍黏在正確一側。實際配出的新 id 會映回目標 sidecar。
    let added: BTreeSet<&NodeId> = new
        .keys()
        .filter(|id| !old.contains_key(*id) && !covered_added.contains(*id))
        .collect();
    let mut topmost: Vec<&NodeEntryV1> = new
        .values()
        .filter(|entry| added.contains(&entry.id))
        .filter(|entry| !has_ancestor_in(entry, &new, &added))
        .collect();
    topmost.sort_by(|left, right| left.address.cmp(&right.address));
    for entry in topmost {
        let target_parent = entry.parent.clone().unwrap_or_else(|| root_id(after));
        let working_parent = target_to_working
            .get(&target_parent)
            .cloned()
            .ok_or_else(|| ReconstructError::Unsupported {
                kind: entry.kind,
                detail: "inserted below a parent that does not yet exist".to_owned(),
            })?;
        let parent_kind = entry_by_id(&working, &working_parent)
            .map(|candidate| candidate.kind)
            .unwrap_or(NodeKind::Language);
        let anchor = insertion_anchor(after, &working, entry, &target_to_working, &working_parent);
        let edit = PrimitiveEdit::Insert {
            parent: NodeRef::new(working_parent.clone(), parent_kind),
            anchor,
            subtree: detached_at(after.language(), entry)?,
        };
        let before_ids = working
            .identities()
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        let next = apply_structural(working, &edit)?;
        map_inserted_subtree(
            after,
            &next,
            entry,
            &working_parent,
            &before_ids,
            &mut target_to_working,
        )?;
        working = next;
        edits.push(edit);
    }

    // ── ④ 同父有序重排:每個 logical sequence 各自做 LCS ──
    for target in target_sequences(after, &target_to_working) {
        if target.nodes.len() < 2 {
            continue;
        }
        let target_set = target.nodes.iter().cloned().collect::<BTreeSet<_>>();
        let current = current_sequence(&working, &target.parent, target.key, &target_set);
        if current == target.nodes {
            continue;
        }
        let keep = lcs_keep(&current, &target.nodes);
        for index in (0..target.nodes.len()).rev() {
            let id = &target.nodes[index];
            if keep.contains(id) {
                continue;
            }
            let node = entry_by_id(&working, id).cloned().ok_or_else(|| {
                ReconstructError::Unsupported {
                    kind: NodeKind::Language,
                    detail: format!("reorder node {id} is missing"),
                }
            })?;
            let parent = entry_by_id(&working, &target.parent)
                .cloned()
                .ok_or_else(|| ReconstructError::Unsupported {
                    kind: node.kind,
                    detail: "reorder parent is missing".to_owned(),
                })?;
            let anchor = target
                .nodes
                .get(index + 1)
                .and_then(|next| entry_by_id(&working, next))
                .map(|next| Anchor::Before(NodeRef::new(next.id.clone(), next.kind)))
                .unwrap_or(Anchor::End);
            let edit = PrimitiveEdit::Move {
                node: NodeRef::new(node.id, node.kind),
                new_parent: NodeRef::new(parent.id, parent.kind),
                anchor,
            };
            working = apply_structural(working, &edit)?;
            edits.push(edit);
        }
    }

    // ── ⑤ 刪除:只發最上層的(後代隨父節點消失)──
    let removed: BTreeSet<&NodeId> = old
        .keys()
        .filter(|id| !new.contains_key(*id) && !covered_removed.contains(*id))
        .collect();
    let mut dying: Vec<&NodeEntryV1> = old
        .values()
        .filter(|entry| removed.contains(&entry.id))
        .filter(|entry| !has_ancestor_in(entry, &old, &removed))
        .collect();
    dying.sort_by(|left, right| right.address.cmp(&left.address));
    for entry in dying {
        let edit = PrimitiveEdit::Delete {
            node: NodeRef::new(entry.id.clone(), entry.kind),
        };
        working = apply_structural(working, &edit)?;
        edits.push(edit);
    }

    if working.source() != after.source() {
        return Err(ReconstructError::Unsupported {
            kind: NodeKind::Language,
            detail: "planned edits do not reproduce the target canonical source".to_owned(),
        });
    }

    Ok(edits)
}

/// 某個節點的祖先是否也在 `set` 裡(用來只取最上層)。
fn has_ancestor_in(
    entry: &NodeEntryV1,
    all: &BTreeMap<NodeId, NodeEntryV1>,
    set: &BTreeSet<&NodeId>,
) -> bool {
    let mut cursor = entry.parent.clone();
    while let Some(id) = cursor {
        if set.contains(&id) {
            return true;
        }
        cursor = all.get(&id).and_then(|parent| parent.parent.clone());
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SequenceKey {
    tag: u8,
    item_group: u16,
}

#[derive(Debug)]
struct TargetSequence {
    parent: NodeId,
    key: SequenceKey,
    nodes: Vec<NodeId>,
}

fn sequence_key(document: &LanguageDocument, entry: &NodeEntryV1) -> Option<SequenceKey> {
    let (tag, item_group) = match entry.address.0.last()? {
        AddressSegment::DslDeclarations(_) => (1, 0),
        AddressSegment::Blocks(_) => (6, 0),
        AddressSegment::Items(_) => (
            7,
            item_at_address(document.language(), &entry.address)
                .map(crate::item_group)
                .unwrap_or(0),
        ),
        AddressSegment::RuleElse(_) => (8, 0),
        AddressSegment::RuleThen(_) => (9, 0),
        AddressSegment::RealizationBranches(_) => (10, 0),
        AddressSegment::CaseBranches(_) => (12, 0),
        AddressSegment::PhonLeaf(_) => (15, 0),
        AddressSegment::PhonThen(_) => (16, 0),
        AddressSegment::PhonElse(_) => (17, 0),
        // Canonical unordered collections and expression-only singleton /
        // argument positions are intentionally not Move sequences.
        AddressSegment::Distribution(_)
        | AddressSegment::Traits(_)
        | AddressSegment::Signs(_)
        | AddressSegment::CaseExpression
        | AddressSegment::CaseResult
        | AddressSegment::ApplicationArguments(_) => return None,
    };
    Some(SequenceKey { tag, item_group })
}

fn entry_by_id<'a>(document: &'a LanguageDocument, id: &NodeId) -> Option<&'a NodeEntryV1> {
    document
        .identities()
        .nodes
        .iter()
        .find(|entry| &entry.id == id)
}

#[allow(clippy::too_many_arguments)]
fn map_updated_descendants(
    previous: &LanguageDocument,
    working: &LanguageDocument,
    target: &LanguageDocument,
    old_root: &NodeEntryV1,
    target_root: &NodeEntryV1,
    old: &BTreeMap<NodeId, NodeEntryV1>,
    new: &BTreeMap<NodeId, NodeEntryV1>,
    mapping: &mut BTreeMap<NodeId, NodeId>,
    covered_added: &mut BTreeSet<NodeId>,
    covered_removed: &mut BTreeSet<NodeId>,
) -> Result<(), ReconstructError> {
    let previous_ids = previous
        .identities()
        .nodes
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<BTreeSet<_>>();
    for target_entry in target
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.address.starts_with(&target_root.address))
    {
        let Some(actual) = working.identities().nodes.iter().find(|candidate| {
            candidate.address == target_entry.address && candidate.kind == target_entry.kind
        }) else {
            continue;
        };
        if let Some(expected) = mapping.get(&target_entry.id) {
            if expected != &actual.id {
                return Err(ReconstructError::Unsupported {
                    kind: target_entry.kind,
                    detail: "typed update would rebind an existing descendant identity".to_owned(),
                });
            }
        } else if !old.contains_key(&target_entry.id) && !previous_ids.contains(&actual.id) {
            mapping.insert(target_entry.id.clone(), actual.id.clone());
            covered_added.insert(target_entry.id.clone());
        }
    }
    for old_entry in old
        .values()
        .filter(|entry| entry.address.starts_with(&old_root.address))
        .filter(|entry| !new.contains_key(&entry.id))
    {
        if entry_by_id(previous, &old_entry.id).is_some()
            && entry_by_id(working, &old_entry.id).is_none()
        {
            covered_removed.insert(old_entry.id.clone());
        }
    }
    Ok(())
}

fn insertion_anchor(
    target: &LanguageDocument,
    working: &LanguageDocument,
    entry: &NodeEntryV1,
    target_to_working: &BTreeMap<NodeId, NodeId>,
    working_parent: &NodeId,
) -> Anchor {
    let Some(key) = sequence_key(target, entry) else {
        return Anchor::End;
    };
    let mut successors = target
        .identities()
        .nodes
        .iter()
        .filter(|candidate| candidate.parent == entry.parent)
        .filter(|candidate| sequence_key(target, candidate) == Some(key))
        .filter(|candidate| candidate.address > entry.address)
        .collect::<Vec<_>>();
    successors.sort_by(|left, right| left.address.cmp(&right.address));
    successors
        .into_iter()
        .filter_map(|candidate| target_to_working.get(&candidate.id))
        .filter_map(|id| entry_by_id(working, id))
        .find(|candidate| candidate.parent.as_ref() == Some(working_parent))
        .map(|candidate| Anchor::Before(NodeRef::new(candidate.id.clone(), candidate.kind)))
        .unwrap_or(Anchor::End)
}

fn map_inserted_subtree(
    target: &LanguageDocument,
    working: &LanguageDocument,
    target_root: &NodeEntryV1,
    working_parent: &NodeId,
    previous_ids: &BTreeSet<NodeId>,
    mapping: &mut BTreeMap<NodeId, NodeId>,
) -> Result<(), ReconstructError> {
    let working_root = working
        .identities()
        .nodes
        .iter()
        .filter(|entry| !previous_ids.contains(&entry.id))
        .find(|entry| {
            entry.parent.as_ref() == Some(working_parent) && entry.kind == target_root.kind
        })
        .ok_or_else(|| ReconstructError::Unsupported {
            kind: target_root.kind,
            detail: "insert did not allocate an addressable root".to_owned(),
        })?;
    let target_prefix = &target_root.address.0;
    let working_prefix = &working_root.address.0;
    let allocated = working
        .identities()
        .nodes
        .iter()
        .filter(|entry| !previous_ids.contains(&entry.id))
        .collect::<Vec<_>>();
    for target_entry in target
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.address.starts_with(&target_root.address))
    {
        let suffix = &target_entry.address.0[target_prefix.len()..];
        let matched = allocated.iter().find(|candidate| {
            candidate.kind == target_entry.kind
                && candidate.address.0.starts_with(working_prefix)
                && &candidate.address.0[working_prefix.len()..] == suffix
        });
        let Some(matched) = matched else {
            return Err(ReconstructError::Unsupported {
                kind: target_entry.kind,
                detail: "inserted subtree identity shape differs from target".to_owned(),
            });
        };
        mapping.insert(target_entry.id.clone(), matched.id.clone());
    }
    Ok(())
}

fn target_sequences(
    target: &LanguageDocument,
    mapping: &BTreeMap<NodeId, NodeId>,
) -> Vec<TargetSequence> {
    let mut groups: BTreeMap<(NodeId, SequenceKey), Vec<(&NodeEntryV1, NodeId)>> = BTreeMap::new();
    for entry in &target.identities().nodes {
        let (Some(parent), Some(mapped), Some(key)) = (
            entry.parent.as_ref().and_then(|id| mapping.get(id)),
            mapping.get(&entry.id),
            sequence_key(target, entry),
        ) else {
            continue;
        };
        groups
            .entry((parent.clone(), key))
            .or_default()
            .push((entry, mapped.clone()));
    }
    groups
        .into_iter()
        .map(|((parent, key), mut entries)| {
            entries.sort_by(|(left, _), (right, _)| left.address.cmp(&right.address));
            TargetSequence {
                parent,
                key,
                nodes: entries.into_iter().map(|(_, mapped)| mapped).collect(),
            }
        })
        .collect()
}

fn current_sequence(
    working: &LanguageDocument,
    parent: &NodeId,
    key: SequenceKey,
    target_set: &BTreeSet<NodeId>,
) -> Vec<NodeId> {
    let mut entries = working
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.parent.as_ref() == Some(parent))
        .filter(|entry| target_set.contains(&entry.id))
        .filter(|entry| sequence_key(working, entry) == Some(key))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.address.cmp(&right.address));
    entries.into_iter().map(|entry| entry.id.clone()).collect()
}

fn lcs_keep(current: &[NodeId], target: &[NodeId]) -> BTreeSet<NodeId> {
    let mut lengths = vec![vec![0usize; target.len() + 1]; current.len() + 1];
    for left in (0..current.len()).rev() {
        for right in (0..target.len()).rev() {
            lengths[left][right] = if current[left] == target[right] {
                lengths[left + 1][right + 1] + 1
            } else {
                lengths[left + 1][right].max(lengths[left][right + 1])
            };
        }
    }
    let (mut left, mut right) = (0, 0);
    let mut keep = BTreeSet::new();
    while left < current.len() && right < target.len() {
        if current[left] == target[right] {
            keep.insert(current[left].clone());
            left += 1;
            right += 1;
        } else if lengths[left + 1][right] >= lengths[left][right + 1] {
            left += 1;
        } else {
            right += 1;
        }
    }
    keep
}

fn root_id(document: &LanguageDocument) -> NodeId {
    document
        .identities()
        .nodes
        .iter()
        .find(|entry| entry.parent.is_none() && entry.kind == NodeKind::Language)
        .map(|entry| entry.id.clone())
        .expect("a document always has a Language root")
}

fn entries(document: &LanguageDocument) -> BTreeMap<NodeId, NodeEntryV1> {
    document
        .identities()
        .nodes
        .iter()
        .map(|entry| (entry.id.clone(), entry.clone()))
        .collect()
}

fn shallow_at(
    document: &LanguageDocument,
    entry: &NodeEntryV1,
) -> Result<ShallowNode, ReconstructError> {
    let language = document.language();
    let expression_item = matches!(entry.address.0.last(), Some(AddressSegment::Items(_)))
        .then(|| item_at_address(language, &entry.address))
        .flatten()
        .filter(|item| {
            matches!(
                item,
                SignItem::SignExpression(_)
                    | SignItem::FeatureExpression(_)
                    | SignItem::RoleExpression(_)
            )
        });
    match entry.kind {
        NodeKind::Application => match expression_item {
            Some(item) => Ok(ShallowNode::ExpressionItem(item.clone())),
            None => Ok(ShallowNode::Application(
                application_at(language, &entry.address)?.clone(),
            )),
        },
        NodeKind::Case => match expression_item {
            Some(item) => Ok(ShallowNode::ExpressionItem(item.clone())),
            None => Ok(ShallowNode::Case(
                case_at(language, &entry.address)
                    .ok_or_else(|| ReconstructError::Unsupported {
                        kind: NodeKind::Case,
                        detail: format!("case address is stale: {:?}", entry.address),
                    })?
                    .clone(),
            )),
        },
        NodeKind::CaseBranch => Ok(ShallowNode::CaseBranch(
            crate::case_branch_at(language, &entry.address)?.clone(),
        )),
        NodeKind::Realization => match item_at_address(language, &entry.address) {
            Some(SignItem::Realization(value)) => Ok(ShallowNode::Realization(value.clone())),
            _ => Err(ReconstructError::Unsupported {
                kind: NodeKind::Realization,
                detail: format!("realization address is stale: {:?}", entry.address),
            }),
        },
        _ => Ok(ShallowNode::Detached(detached_at(language, entry)?)),
    }
}

fn shallow_state_updates(
    before: &ShallowNode,
    after: &ShallowNode,
) -> Result<Vec<NodeUpdate>, ReconstructError> {
    match (before, after) {
        (ShallowNode::Detached(old), ShallowNode::Detached(new)) => shallow_updates(old, new),
        (ShallowNode::ExpressionItem(old), ShallowNode::ExpressionItem(new)) => {
            expression_item_updates(old, new)
        }
        (ShallowNode::Realization(old), ShallowNode::Realization(new)) => {
            Ok(match (&old.expression, &new.expression) {
                (None, None) | (Some(_), Some(_)) => Vec::new(),
                _ => vec![NodeUpdate::Realization(new.clone())],
            })
        }
        (ShallowNode::Case(old), ShallowNode::Case(new)) => Ok(case_updates(old, new)),
        (ShallowNode::CaseBranch(old), ShallowNode::CaseBranch(new)) => {
            Ok((!case_branch_shallow_eq(old, new))
                .then(|| NodeUpdate::CaseBranch(new.clone()))
                .into_iter()
                .collect())
        }
        (ShallowNode::Application(old), ShallowNode::Application(new)) => {
            Ok(application_updates(old, new))
        }
        (old, new) => Err(ReconstructError::Unsupported {
            kind: match new {
                ShallowNode::Detached(value) => value.kind(),
                ShallowNode::ExpressionItem(value) => crate::item_kind(value),
                ShallowNode::Realization(_) => NodeKind::Realization,
                ShallowNode::Case(_) => NodeKind::Case,
                ShallowNode::CaseBranch(_) => NodeKind::CaseBranch,
                ShallowNode::Application(_) => NodeKind::Application,
            },
            detail: format!("shallow representation changed from {old:?}"),
        }),
    }
}

fn expression_item_updates(
    before: &SignItem,
    after: &SignItem,
) -> Result<Vec<NodeUpdate>, ReconstructError> {
    let (metadata_equal, old_expression, new_expression) = match (before, after) {
        (SignItem::SignExpression(old), SignItem::SignExpression(new)) => {
            (true, &old.expression, &new.expression)
        }
        (SignItem::FeatureExpression(old), SignItem::FeatureExpression(new)) => (
            old.dim == new.dim && old.name == new.name,
            &old.expression,
            &new.expression,
        ),
        (SignItem::RoleExpression(old), SignItem::RoleExpression(new)) => {
            (old.name == new.name, &old.expression, &new.expression)
        }
        _ => {
            return Err(ReconstructError::Unsupported {
                kind: crate::item_kind(after),
                detail: "expression owner variant changed under a stable id".to_owned(),
            })
        }
    };
    if !metadata_equal || !expression_shell_eq(old_expression, new_expression) {
        return Ok(vec![NodeUpdate::ExpressionItem(after.clone())]);
    }
    match (
        expression_base(old_expression),
        expression_base(new_expression),
    ) {
        (
            Expression::SignApplication(old) | Expression::PhonInterpolation(old),
            Expression::SignApplication(new) | Expression::PhonInterpolation(new),
        ) => Ok(application_updates(old, new)),
        (Expression::Case(old), Expression::Case(new)) => Ok(case_updates(old, new)),
        _ => Ok(Vec::new()),
    }
}

fn expression_base(expression: &Expression) -> &Expression {
    match expression {
        Expression::Projection { value, .. } => expression_base(value),
        other => other,
    }
}

/// Compare only wrappers owned by the enclosing addressable node. Nested
/// applications, cases, and fragment items have their own identities.
fn expression_shell_eq(before: &Expression, after: &Expression) -> bool {
    match (before, after) {
        (
            Expression::Projection {
                value: old,
                dimension: old_dim,
            },
            Expression::Projection {
                value: new,
                dimension: new_dim,
            },
        ) => old_dim == new_dim && expression_shell_eq(old, new),
        (Expression::SignApplication(_), Expression::SignApplication(_))
        | (Expression::PhonInterpolation(_), Expression::PhonInterpolation(_))
        | (Expression::Case(_), Expression::Case(_))
        | (Expression::SignFragment(_), Expression::SignFragment(_)) => true,
        (
            Expression::DimFragment { dim: old_dim, .. },
            Expression::DimFragment { dim: new_dim, .. },
        ) => old_dim == new_dim,
        (Expression::PhonTemplate(old), Expression::PhonTemplate(new))
        | (Expression::EnumValue(old), Expression::EnumValue(new))
        | (Expression::Slot(old), Expression::Slot(new)) => old == new,
        (Expression::SelfSign, Expression::SelfSign) => true,
        _ => false,
    }
}

fn case_updates(before: &TypedCase, after: &TypedCase) -> Vec<NodeUpdate> {
    if before.selection == after.selection
        && before.expected == after.expected
        && before.scrutinee == after.scrutinee
        && before.name == after.name
    {
        Vec::new()
    } else {
        vec![NodeUpdate::CaseHeader {
            selection: after.selection,
            expected: after.expected.clone(),
            scrutinee: after.scrutinee.clone(),
            name: after.name.clone(),
        }]
    }
}

fn case_branch_shallow_eq(before: &CaseBranch, after: &CaseBranch) -> bool {
    before.condition == after.condition
        && before.belongs == after.belongs
        && before.name == after.name
        && expression_shell_eq(&before.result, &after.result)
}

fn application_updates(before: &SignApplication, after: &SignApplication) -> Vec<NodeUpdate> {
    (!application_shallow_eq(before, after))
        .then(|| NodeUpdate::SignApplication(after.clone()))
        .into_iter()
        .collect()
}

fn application_shallow_eq(before: &SignApplication, after: &SignApplication) -> bool {
    before.callee == after.callee
        && before.arguments.len() == after.arguments.len()
        && before
            .arguments
            .iter()
            .zip(&after.arguments)
            .all(|(old, new)| {
                old.name == new.name
                    && match (&old.value, &new.value) {
                        (SignArgumentValue::SelfSign, SignArgumentValue::SelfSign) => true,
                        (SignArgumentValue::Slot(old), SignArgumentValue::Slot(new)) => old == new,
                        (SignArgumentValue::Application(_), SignArgumentValue::Application(_)) => {
                            true
                        }
                        _ => false,
                    }
            })
}

/// 一個節點**自己的**欄位改了哪些(不含子節點)。
///
/// 對應關係就是 `NodeUpdate` 的變體集合——它本來就是「每個節點的可編輯欄位」的列舉。
fn shallow_updates(
    before: &DetachedNode,
    after: &DetachedNode,
) -> Result<Vec<NodeUpdate>, ReconstructError> {
    if before == after {
        return Ok(Vec::new());
    }
    let mut updates = Vec::new();
    match (before, after) {
        (DetachedNode::DslDeclaration(old), DetachedNode::DslDeclaration(new)) => {
            if old != new {
                updates.push(NodeUpdate::DslDeclaration(new.clone()));
            }
        }
        (
            DetachedNode::Distribution {
                key: old_key,
                value: old_value,
            },
            DetachedNode::Distribution { key, value },
        ) => {
            if old_key != key || old_value != value {
                updates.push(NodeUpdate::Distribution {
                    key: key.clone(),
                    value: value.clone(),
                });
            }
        }
        // trait / sign 的子節點(blocks / items)各自有 entry,故這裡**只比自己的欄位**。
        (DetachedNode::Trait(old), DetachedNode::Trait(new)) => {
            if old.name != new.name {
                updates.push(NodeUpdate::Rename(new.name.clone()));
            }
            if old.global != new.global {
                updates.push(NodeUpdate::TraitGlobal(new.global));
            }
        }
        (DetachedNode::Sign(old), DetachedNode::Sign(new)) => {
            if old.name != new.name {
                updates.push(NodeUpdate::Rename(new.name.clone()));
            }
        }
        // Block 只有 items,items 各自有 entry ⇒ 淺層無欄位可改。
        (DetachedNode::Block(_), DetachedNode::Block(_)) => {}
        (DetachedNode::Item(old), DetachedNode::Item(new)) => {
            updates.extend(item_updates(old, new)?);
        }
        (DetachedNode::RuleThenBranch(old), DetachedNode::RuleThenBranch(new)) => {
            if old != new {
                updates.push(NodeUpdate::RuleBranchBody(new.clone()));
            }
        }
        (DetachedNode::RuleElseBranch(old), DetachedNode::RuleElseBranch(new)) => {
            if old != new {
                updates.push(NodeUpdate::RuleBranchBody(new.clone()));
            }
        }
        (DetachedNode::PhonStatement(old), DetachedNode::PhonStatement(new)) => {
            if old != new {
                updates.push(NodeUpdate::RuleBranchBody(new.clone()));
            }
        }
        (DetachedNode::PhonBlockNode(old), DetachedNode::PhonBlockNode(new)) => {
            updates.extend(phon_block_updates(old, new)?);
        }
        (DetachedNode::CaseBranch(old), DetachedNode::CaseBranch(new)) => {
            if old != new {
                updates.push(NodeUpdate::CaseBranch(new.clone()));
            }
        }
        // 只有**種類對不上**或真的沒實作才走到這裡。
        //
        // 已知種類一律用**無 guard 的 arm**:帶 guard(`if old.x != new.x`)的 arm 在
        // 「欄位相同但節點不等」時會掉進這裡誤報 Unsupported —— 而那正是
        // `Sense`/`SenseEdge` 的處境:它們帶 `source: SourceLocation`,且它進了
        // `PartialEq`,所以上面插一行就讓行號變、節點不等,語意卻沒變。
        (old, new) => {
            return Err(ReconstructError::Unsupported {
                kind: new.kind(),
                detail: format!("no shallow comparison from {:?}", old.kind()),
            })
        }
    }
    Ok(updates)
}

fn phon_block_updates(
    before: &PhonBlock,
    after: &PhonBlock,
) -> Result<Vec<NodeUpdate>, ReconstructError> {
    let (old_propagate, old_bare) = split_propagate(before);
    let (new_propagate, new_bare) = split_propagate(after);
    if phon_block_kind(old_bare) != phon_block_kind(new_bare) {
        return Err(ReconstructError::Unsupported {
            kind: NodeKind::PhonBlockNode,
            detail: "phon block structural kind changed under a stable id".to_owned(),
        });
    }
    Ok((old_propagate != new_propagate)
        .then_some(NodeUpdate::Propagate(new_propagate))
        .into_iter()
        .collect())
}

fn split_propagate(block: &PhonBlock) -> (bool, &PhonBlock) {
    match block {
        PhonBlock::Propagate(inner) => (true, inner),
        other => (false, other),
    }
}

fn phon_block_kind(block: &PhonBlock) -> &'static str {
    match block {
        PhonBlock::Leaf(_) => "leaf",
        PhonBlock::Then(_) => "then",
        PhonBlock::Else(_) => "else",
        PhonBlock::Propagate(_) => "propagate",
    }
}

/// sign 項目自己的欄位。
fn item_updates(before: &SignItem, after: &SignItem) -> Result<Vec<NodeUpdate>, ReconstructError> {
    let mut updates = Vec::new();
    match (before, after) {
        (SignItem::Belongs(old), SignItem::Belongs(new)) => {
            if old != new {
                updates.push(NodeUpdate::Belongs(new.clone()));
            }
        }
        (
            SignItem::TraitUse {
                name: old_name,
                block: old_block,
            },
            SignItem::TraitUse { name, block },
        ) => {
            if old_name != name || old_block != block {
                updates.push(NodeUpdate::TraitUse {
                    name: name.clone(),
                    block: *block,
                });
            }
        }
        (SignItem::Def(old), SignItem::Def(new)) => {
            if old.path != new.path {
                updates.push(NodeUpdate::DefinitionPath(new.path.clone()));
            }
            if old.value != new.value {
                updates.push(NodeUpdate::DefinitionValue(new.value.clone()));
            }
        }
        // rule 的 then/else 分支各自有 entry ⇒ 這裡只比 rule 自己的欄位。
        (SignItem::Rule(old), SignItem::Rule(new)) => {
            updates.extend(rule_updates(old, new, NodeKind::Rule)?);
        }
        (SignItem::FeatureRule(old), SignItem::FeatureRule(new)) => {
            updates.extend(rule_updates(old, new, NodeKind::FeatureRule)?);
        }
        // `source` 刻意不比:它是行號,上面插一行就變,語意卻沒變。
        (SignItem::Sense(old), SignItem::Sense(new)) => {
            if old.gloss != new.gloss {
                updates.push(NodeUpdate::SenseGloss(new.gloss.clone()));
            }
        }
        (SignItem::SenseEdge(old), SignItem::SenseEdge(new)) => {
            if old.kind != new.kind {
                updates.push(NodeUpdate::SenseEdgeKind(new.kind));
            }
            if old.transparency != new.transparency {
                updates.push(NodeUpdate::SenseEdgeTransparency(new.transparency));
            }
        }
        (SignItem::Slot(old), SignItem::Slot(new)) => {
            if old.name != new.name {
                updates.push(NodeUpdate::SlotName(new.name.clone()));
            }
            if old.constraint != new.constraint {
                updates.push(NodeUpdate::SlotConstraint(new.constraint.clone()));
            }
            if old.optional != new.optional {
                updates.push(NodeUpdate::SlotOptional(new.optional));
            }
        }
        (SignItem::FeatureDecl(old), SignItem::FeatureDecl(new)) => {
            if old.dim != new.dim || old.name != new.name || old.values != new.values {
                updates.push(NodeUpdate::FeatureDeclaration(new.clone()));
            }
        }
        (SignItem::FeatureValue(old), SignItem::FeatureValue(new)) => {
            if old.dim != new.dim || old.name != new.name || old.value != new.value {
                updates.push(NodeUpdate::FeatureValue(new.clone()));
            }
        }
        (SignItem::SlotFeatureBinding(old), SignItem::SlotFeatureBinding(new)) => {
            if old.slot != new.slot || old.feature != new.feature || old.value != new.value {
                updates.push(NodeUpdate::SlotFeatureBinding(new.clone()));
            }
        }
        (SignItem::SlotMap(old), SignItem::SlotMap(new)) => {
            if old != new {
                updates.push(NodeUpdate::SlotMap(new.clone()));
            }
        }
        (SignItem::RoleDecl(old), SignItem::RoleDecl(new)) => {
            if old.name != new.name
                || old.constraint != new.constraint
                || old.optional != new.optional
            {
                updates.push(NodeUpdate::RoleDeclaration(new.clone()));
            }
        }
        (SignItem::RoleBinding(old), SignItem::RoleBinding(new)) => {
            if old.name != new.name || old.slot != new.slot {
                updates.push(NodeUpdate::RoleBinding(new.clone()));
            }
        }
        (SignItem::Constraint(old), SignItem::Constraint(new)) => {
            if old.predicate != new.predicate || old.left != new.left || old.right != new.right {
                updates.push(NodeUpdate::Constraint(new.clone()));
            }
        }
        (_, new) => {
            return Err(ReconstructError::Unsupported {
                kind: crate::item_kind(new),
                detail: "no shallow comparison for this item".to_owned(),
            })
        }
    }
    Ok(updates)
}

fn rule_updates(
    before: &Rule,
    after: &Rule,
    kind: NodeKind,
) -> Result<Vec<NodeUpdate>, ReconstructError> {
    let mut updates = Vec::new();
    if before.name != after.name {
        updates.push(NodeUpdate::RuleName(after.name.clone()));
    }
    if before.body != after.body {
        updates.push(NodeUpdate::RuleBody(after.body.clone()));
    }
    if before.stage != after.stage {
        updates.push(NodeUpdate::RuleStage(after.stage));
    }
    if before.dim != after.dim {
        updates.push(NodeUpdate::RuleDimension(after.dim));
    }
    if before.propagate != after.propagate {
        updates.push(NodeUpdate::Propagate(after.propagate));
    }
    match (&before.phon_block, &after.phon_block) {
        (Some(old), Some(new)) => {
            let (_, old_bare) = split_propagate(old);
            let (_, new_bare) = split_propagate(new);
            if phon_block_kind(old_bare) != phon_block_kind(new_bare) {
                updates.push(NodeUpdate::PhonBlockRoot(new.clone()));
            }
        }
        (None, None) => {}
        (None, Some(new)) => updates.push(NodeUpdate::PhonBlockRoot(new.clone())),
        (Some(_), None) => {
            return Err(ReconstructError::Unsupported {
                kind,
                detail: "structured-to-flat phon reconstruction needs an explicit flat root update"
                    .to_owned(),
            })
        }
    }
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phon_block_shape_changes_are_explicitly_unsupported() {
        let error = phon_block_updates(
            &PhonBlock::Leaf(vec!["a => b".to_owned()]),
            &PhonBlock::Then(vec![PhonBlock::Leaf(vec!["a => b".to_owned()])]),
        )
        .expect_err("a stable block id cannot silently change structural kind");
        assert!(matches!(
            error,
            ReconstructError::Unsupported {
                kind: NodeKind::PhonBlockNode,
                ..
            }
        ));
    }

    #[test]
    fn phon_block_children_are_not_duplicated_as_parent_updates() {
        let updates = phon_block_updates(
            &PhonBlock::Leaf(vec!["a => b".to_owned()]),
            &PhonBlock::Leaf(vec!["a => c".to_owned()]),
        )
        .expect("child statements have their own identities");
        assert!(updates.is_empty());
    }
}
