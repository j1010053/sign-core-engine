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
    detached_at, Anchor, DetachedNode, EditError, LanguageDocument, NodeUpdate, PrimitiveEdit,
};
use conlang_language::{
    AddressSegment, IdentityError, NodeEntryV1, NodeId, NodeKind, NodeRef, SignItem,
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

    // ── ① 更新:兩邊都有的,比淺層欄位 ──
    for (id, old_entry) in &old {
        let Some(new_entry) = new.get(id) else {
            continue;
        };
        // Language 根沒有淺層可編輯欄位,也沒有「拆下來」的形式。
        if old_entry.address.0.is_empty() {
            continue;
        }
        let old_node = detached_at(before.language(), old_entry)?;
        let new_node = detached_at(after.language(), new_entry)?;
        for change in shallow_updates(&old_node, &new_node)? {
            edits.push(PrimitiveEdit::Update {
                node: NodeRef::new(id.clone(), old_entry.kind),
                change,
            });
        }
    }

    // ── ② 移動:parent 換了 ──
    //
    // 只處理「換父」。**同父的重排不在此處**:具名容器(sign/trait/distribution)的
    // 順序是正規印出的結果而非語意(`sibling_ranks` 對此已有同樣的判斷),而
    // 序列性容器的重排要靠錨點細算,留待往返性質指出需要時再補。
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
        let parent_kind = new
            .values()
            .find(|entry| entry.id == parent)
            .map(|entry| entry.kind)
            .unwrap_or(NodeKind::Language);
        edits.push(PrimitiveEdit::Move {
            node: NodeRef::new(id.clone(), old_entry.kind),
            new_parent: NodeRef::new(parent, parent_kind),
            anchor: anchor_for(&new, new_entry),
        });
    }

    // ── ③ 新增:只發**最上層**的,帶完整子樹 ──
    //
    // 後代隨父節點的子樹一起進來,再單獨插一次會變成插兩份(承 P16 的「優先一次
    // 完整 Insert」)。
    let added: BTreeSet<&NodeId> = new.keys().filter(|id| !old.contains_key(*id)).collect();
    let mut topmost: Vec<&NodeEntryV1> = new
        .values()
        .filter(|entry| added.contains(&entry.id))
        .filter(|entry| !has_ancestor_in(entry, &new, &added))
        .collect();
    topmost.sort_by(|left, right| left.address.cmp(&right.address));
    for entry in topmost {
        let parent = entry.parent.clone();
        let parent_kind = parent
            .as_ref()
            .and_then(|id| new.values().find(|candidate| &candidate.id == id))
            .map(|candidate| candidate.kind)
            .unwrap_or(NodeKind::Language);
        let parent_ref = NodeRef::new(parent.unwrap_or_else(|| root_id(after)), parent_kind);
        edits.push(PrimitiveEdit::Insert {
            parent: parent_ref,
            anchor: anchor_for(&new, entry),
            subtree: detached_at(after.language(), entry)?,
        });
    }

    // ── ④ 刪除:只發最上層的(後代隨父節點消失)──
    let removed: BTreeSet<&NodeId> = old.keys().filter(|id| !new.contains_key(*id)).collect();
    let mut dying: Vec<&NodeEntryV1> = old
        .values()
        .filter(|entry| removed.contains(&entry.id))
        .filter(|entry| !has_ancestor_in(entry, &old, &removed))
        .collect();
    dying.sort_by(|left, right| right.address.cmp(&left.address));
    for entry in dying {
        edits.push(PrimitiveEdit::Delete {
            node: NodeRef::new(entry.id.clone(), entry.kind),
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

/// 節點在**目標狀態**裡的落點。
///
/// 取「前一個兄弟之後」而非一律 `End`:序列性容器的順序有語意(P5:trait 引用位置
/// 有語意),一律 End 會把順序弄丟。第一個孩子用 `Start`。
fn anchor_for(all: &BTreeMap<NodeId, NodeEntryV1>, entry: &NodeEntryV1) -> Anchor {
    let Some((last, _)) = entry.address.0.split_last() else {
        return Anchor::End;
    };
    // **具名無序容器只接受 `End`**(sign / trait / distribution):它們的順序是正規印出
    // 的結果而非語意,故編輯層明令 `EDIT_ANCHOR_INVALID: canonical unordered
    // collections accept only End`。`sibling_ranks` 對此已有同樣的判斷(跳過 tag 3–5)。
    if matches!(
        last,
        AddressSegment::Distribution(_) | AddressSegment::Traits(_) | AddressSegment::Signs(_)
    ) {
        return Anchor::End;
    }
    let previous = all.values().find(|candidate| {
        candidate.parent == entry.parent
            && candidate
                .address
                .0
                .split_last()
                .is_some_and(|(segment, prefix)| {
                    prefix == &entry.address.0[..entry.address.0.len() - 1]
                        && index_of(segment).zip(index_of(last)).is_some_and(
                            |(candidate_index, own_index)| candidate_index + 1 == own_index,
                        )
                })
    });
    match previous {
        Some(sibling) => Anchor::After(NodeRef::new(sibling.id.clone(), sibling.kind)),
        None => Anchor::Start,
    }
}

fn index_of(segment: &AddressSegment) -> Option<usize> {
    match segment {
        AddressSegment::DslDeclarations(index)
        | AddressSegment::Distribution(index)
        | AddressSegment::Traits(index)
        | AddressSegment::Signs(index)
        | AddressSegment::Blocks(index)
        | AddressSegment::Items(index)
        | AddressSegment::RuleElse(index)
        | AddressSegment::RuleThen(index)
        | AddressSegment::PhonLeaf(index)
        | AddressSegment::PhonThen(index)
        | AddressSegment::PhonElse(index)
        | AddressSegment::RealizationBranches(index)
        | AddressSegment::CaseBranches(index)
        | AddressSegment::ApplicationArguments(index) => Some(*index),
        _ => None,
    }
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
        (DetachedNode::Prosody(old), DetachedNode::Prosody(new)) => {
            if old != new {
                updates.push(NodeUpdate::Prosody(new.clone()));
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
            if old.body != new.body {
                updates.push(NodeUpdate::RuleBody(new.body.clone()));
            }
            if old.stage != new.stage {
                updates.push(NodeUpdate::RuleStage(new.stage));
            }
            if old.dim != new.dim {
                updates.push(NodeUpdate::RuleDimension(new.dim));
            }
            if old.propagate != new.propagate {
                updates.push(NodeUpdate::Propagate(new.propagate));
            }
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
            if old != new {
                updates.push(NodeUpdate::FeatureDeclaration(new.clone()));
            }
        }
        (SignItem::FeatureValue(old), SignItem::FeatureValue(new)) => {
            if old != new {
                updates.push(NodeUpdate::FeatureValue(new.clone()));
            }
        }
        (SignItem::SlotFeatureBinding(old), SignItem::SlotFeatureBinding(new)) => {
            if old != new {
                updates.push(NodeUpdate::SlotFeatureBinding(new.clone()));
            }
        }
        (SignItem::SlotMap(old), SignItem::SlotMap(new)) => {
            if old != new {
                updates.push(NodeUpdate::SlotMap(new.clone()));
            }
        }
        (SignItem::RoleDecl(old), SignItem::RoleDecl(new)) => {
            if old != new {
                updates.push(NodeUpdate::RoleDeclaration(new.clone()));
            }
        }
        (SignItem::RoleBinding(old), SignItem::RoleBinding(new)) => {
            if old != new {
                updates.push(NodeUpdate::RoleBinding(new.clone()));
            }
        }
        (SignItem::Constraint(old), SignItem::Constraint(new)) => {
            if old != new {
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
