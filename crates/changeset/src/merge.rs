//! 步驟 16 ⑤a —— **多親機械合併的分析層**(《修補11》P61 §6)。
//!
//! ## 為什麼合併只是「算出計畫」,不直接產文件
//!
//! 合併結果要成為一份 `LanguageDocument`,而該型別的欄位是私有的、沒有
//! 「(Language, identity manifest) → document」的建構子——把來自不同 parent 的
//! sign 拼成一份帶正確穩定 id 的 manifest,需要 `language` 側新的 API。
//! 故本模組**只算計畫與衝突**(純函數、好測),物化留給 ⑤b。
//!
//! ## 三態規則(§6.3)
//!
//! 以**最近共同祖先為基準**做 3-way;無共同祖先時基準為空,規則自然退化成聯集,
//! 故兩種情形共用同一段程式(同 git `--allow-unrelated-histories`)。
//!
//! | 情況 | 結果 |
//! |---|---|
//! | 僅一邊相對基準改過 | 自動取該邊 |
//! | 多邊改成相同值 | 自動,取該值 |
//! | 多邊改成不同值 | **衝突** |
//!
//! **必須 3-way 而非兩兩比對**:2-way 只看得到「兩邊不同」,分不出「一邊改過」與
//! 「兩邊都改過」。有共同祖先的場景下絕大多數 sign 是「僅一邊改過」,2-way 會產生
//! 數量級的假衝突,機制不可用。
//!
//! ## 對齊鍵 = `SignId`
//!
//! 承 docs/06 §6.1(diff 的對齊方式):「同 id = 同一 sign 的兩個演化階段」。
//! 合併與 diff 因此**共用同一套對齊**,不是兩套;且能正確處理「一邊改了名」
//! ——用名字對齊會把它誤判成一生一滅。

use conlang_language::{IdentityNamespace, LanguageDocument, NodeId, SignDef};
use std::collections::{BTreeMap, BTreeSet};

/// 合併計畫裡的一筆:某個 `SignId` 最終取哪一份內容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignPick {
    pub id: NodeId,
    /// `None` = 各方都沒動,沿用基準;`Some(i)` = 取第 i 個 parent 的版本。
    pub from: Option<usize>,
}

/// 沒有對齊鍵、只能整塊比的區段(§6.2)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeBlock {
    /// 不透明 verbatim 行(I15-a),合併器看不進內容。
    DslDecls,
    /// 單一條七層鏈——**沒有「聯集」可言**,只能擇一或重寫。
    Prosody,
    /// 分佈覆寫。
    Distribution,
    /// trait 容器。
    Traits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeConflict {
    /// 多方相對基準改成**不同**的值。
    Content { id: NodeId, sides: Vec<usize> },
    /// 一邊刪掉、另一邊改動(git 的 delete/modify)。
    DeleteModify {
        id: NodeId,
        deleted_by: Vec<usize>,
        modified_by: Vec<usize>,
    },
    /// 同一個 `SignId` 出現在多方、但**基準沒有**——合法共有的 id 只可能來自共同
    /// 祖先,故這是各自獨立配發後撞號,不是同一個 sign。空基準時任何共有 id 皆屬此類。
    /// 這正是 `add_root` 的 namespace 守門擋不住的那一半(跨家族 fork 撞號)。
    IdCollision { id: NodeId, sides: Vec<usize> },
    /// 合併結果裡有兩個**不同 id** 的 sign 同名。`.lang` 名字唯一,故非法。
    /// 融合不同源語言時這是主要工作量,而非語意分歧(§6.2)。
    NameCollision { name: String, ids: Vec<NodeId> },
    /// 無對齊鍵的區段各方改成不同值(§6.2:須人工指定)。
    Block(MergeBlock),
}

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    /// sign 的 id 不在文件命名空間裡 ⇒ 跨文件無法對齊。
    #[error("MERGE_UNSTABLE_SIGN_ID: sign {name:?} has a non-document id {id}")]
    UnstableSignId { name: String, id: NodeId },
    /// 至少要兩個 parent 才叫合併。
    #[error("MERGE_TOO_FEW_PARENTS: merging needs at least two parents")]
    TooFewParents,
}

/// 合併計畫。`conflicts` 非空時**不得建節點**(§6.4:有衝突就建不出來,
/// 不存在「先建起來之後再解」的中間狀態)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePlan {
    /// 依 `SignId` 排序,故計畫本身是決定性的(P26)。
    pub signs: Vec<SignPick>,
    /// 各無鍵區段取自哪一方(`None` = 沿用基準);有衝突時該項不出現。
    pub blocks: Vec<(MergeBlock, Option<usize>)>,
    pub conflicts: Vec<MergeConflict>,
}

impl MergePlan {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// 算出合併計畫。`base` 為最近共同祖先的 snapshot;`None` = 無共同祖先(空基準)。
pub fn plan_merge(
    base: Option<&LanguageDocument>,
    sides: &[&LanguageDocument],
) -> Result<MergePlan, MergeError> {
    if sides.len() < 2 {
        return Err(MergeError::TooFewParents);
    }
    let base_signs = base.map(signs_by_id).transpose()?.unwrap_or_default();
    let side_signs = sides
        .iter()
        .map(|document| signs_by_id(document))
        .collect::<Result<Vec<_>, _>>()?;

    let mut universe: BTreeSet<&NodeId> = base_signs.keys().collect();
    for map in &side_signs {
        universe.extend(map.keys());
    }

    let mut signs = Vec::new();
    let mut conflicts = Vec::new();
    for id in universe {
        let present: Vec<usize> = side_signs
            .iter()
            .enumerate()
            .filter(|(_, map)| map.contains_key(id))
            .map(|(index, _)| index)
            .collect();
        match base_signs.get(id) {
            // 基準沒有這個 id。
            None => {
                if present.len() > 1 {
                    conflicts.push(MergeConflict::IdCollision {
                        id: id.clone(),
                        sides: present,
                    });
                } else {
                    // 恰好一方新增 → 直接納入。(`present` 不可能為空:id 來自聯集。)
                    signs.push(SignPick {
                        id: id.clone(),
                        from: Some(present[0]),
                    });
                }
            }
            // 基準有這個 id:比對誰改過、誰刪了。
            Some(inherited) => {
                let deleted: Vec<usize> = (0..side_signs.len())
                    .filter(|index| !side_signs[*index].contains_key(id))
                    .collect();
                let changed: Vec<usize> = present
                    .iter()
                    .copied()
                    .filter(|index| side_signs[*index][id] != *inherited)
                    .collect();
                if !deleted.is_empty() && !changed.is_empty() {
                    conflicts.push(MergeConflict::DeleteModify {
                        id: id.clone(),
                        deleted_by: deleted,
                        modified_by: changed,
                    });
                } else if !deleted.is_empty() {
                    // 有人刪、其餘沒動 → 採用刪除(不進計畫)。
                } else if changed.is_empty() {
                    signs.push(SignPick {
                        id: id.clone(),
                        from: None,
                    });
                } else {
                    let first = side_signs[changed[0]][id];
                    if changed.iter().all(|index| side_signs[*index][id] == first) {
                        signs.push(SignPick {
                            id: id.clone(),
                            from: Some(changed[0]),
                        });
                    } else {
                        conflicts.push(MergeConflict::Content {
                            id: id.clone(),
                            sides: changed,
                        });
                    }
                }
            }
        }
    }

    conflicts.extend(name_collisions(&signs, &base_signs, &side_signs));

    let mut blocks = Vec::new();
    merge_block(
        MergeBlock::DslDecls,
        base.map(|d| &d.language().dsl_decls),
        &sides
            .iter()
            .map(|d| &d.language().dsl_decls)
            .collect::<Vec<_>>(),
        &mut blocks,
        &mut conflicts,
    );
    merge_block(
        MergeBlock::Prosody,
        base.map(|d| &d.language().prosody),
        &sides
            .iter()
            .map(|d| &d.language().prosody)
            .collect::<Vec<_>>(),
        &mut blocks,
        &mut conflicts,
    );
    merge_block(
        MergeBlock::Distribution,
        base.map(|d| &d.language().distribution),
        &sides
            .iter()
            .map(|d| &d.language().distribution)
            .collect::<Vec<_>>(),
        &mut blocks,
        &mut conflicts,
    );
    merge_block(
        MergeBlock::Traits,
        base.map(|d| &d.language().traits),
        &sides
            .iter()
            .map(|d| &d.language().traits)
            .collect::<Vec<_>>(),
        &mut blocks,
        &mut conflicts,
    );

    Ok(MergePlan {
        signs,
        blocks,
        conflicts,
    })
}

/// 無對齊鍵的區段:整塊套同一套三態規則。
///
/// **為什麼不是「各方必須完全一致」**:那會讓「只有一支動過韻律」也變成衝突,
/// 而有共同祖先的合併裡這是常態——等於每次合併都要人工處理。整塊 3-way 一樣機械、
/// 一樣不默默近似,只是把「只有一邊動過」正確地判成不衝突。
fn merge_block<T: PartialEq>(
    block: MergeBlock,
    base: Option<&T>,
    sides: &[&T],
    blocks: &mut Vec<(MergeBlock, Option<usize>)>,
    conflicts: &mut Vec<MergeConflict>,
) {
    let changed: Vec<usize> = match base {
        Some(inherited) => (0..sides.len())
            .filter(|index| sides[*index] != inherited)
            .collect(),
        // 空基準:視同「每一方都改過」,故只有全體一致才不衝突。
        None => (0..sides.len()).collect(),
    };
    if changed.is_empty() {
        blocks.push((block, None));
        return;
    }
    let first = sides[changed[0]];
    if changed.iter().all(|index| sides[*index] == first) {
        blocks.push((block, Some(changed[0])));
    } else {
        conflicts.push(MergeConflict::Block(block));
    }
}

/// `.lang` 的 sign 名字必須唯一。合併把兩支的詞庫並在一起時,**不同 id 同名**
/// 就是硬性錯誤——這是融合不同源語言的主要工作量(§6.2/§6.3)。
fn name_collisions(
    picks: &[SignPick],
    base: &BTreeMap<NodeId, &SignDef>,
    sides: &[BTreeMap<NodeId, &SignDef>],
) -> Vec<MergeConflict> {
    let mut by_name: BTreeMap<&str, Vec<NodeId>> = BTreeMap::new();
    for pick in picks {
        let sign = match pick.from {
            Some(index) => sides[index][&pick.id],
            None => base[&pick.id],
        };
        by_name.entry(&sign.name).or_default().push(pick.id.clone());
    }
    by_name
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(name, ids)| MergeConflict::NameCollision {
            name: name.to_owned(),
            ids,
        })
        .collect()
}

/// `SignId` → sign。**id 必須在文件命名空間裡**——`Ephemeral` 的 id 只在單一
/// `Language` 內有意義,跨文件拿它對齊會把不相干的 sign 湊成一對。故硬錯,不略過。
fn signs_by_id(document: &LanguageDocument) -> Result<BTreeMap<NodeId, &SignDef>, MergeError> {
    let mut map = BTreeMap::new();
    for sign in &document.language().signs {
        if !matches!(sign.id.0.namespace, IdentityNamespace::Document(_)) {
            return Err(MergeError::UnstableSignId {
                name: sign.name.clone(),
                id: sign.id.0.clone(),
            });
        }
        map.insert(sign.id.0.clone(), sign);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    //! 無鍵區段的三態分支**只能在模組內測**——改 `prosody`/`dsl_decls` 沒有 `.chg`
    //! 路徑,從文件層構造不出「只有一邊改過區塊」的輸入。而那正是 3-way 相對 2-way
    //! 的唯一差別所在;不測它,`merge_block` 可以退化成「全體一致才不衝突」而整合
    //! 測試全綠(每次合併都假衝突,但沒有測試看得到)。

    use super::*;

    fn block(
        base: Option<&Vec<&str>>,
        sides: &[&Vec<&str>],
    ) -> (Vec<(MergeBlock, Option<usize>)>, Vec<MergeConflict>) {
        let (mut blocks, mut conflicts) = (Vec::new(), Vec::new());
        merge_block(
            MergeBlock::Prosody,
            base,
            sides,
            &mut blocks,
            &mut conflicts,
        );
        (blocks, conflicts)
    }

    #[test]
    fn a_block_changed_by_one_side_only_is_taken() {
        // **3-way 的判別分支**。B 還是祖先的樣子 ⇒ 不是分歧,是 A 單方面改動。
        let (base, changed, same) = (vec!["mu"], vec!["sigma"], vec!["mu"]);
        let (blocks, conflicts) = block(Some(&base), &[&changed, &same]);
        assert_eq!(blocks, vec![(MergeBlock::Prosody, Some(0))]);
        assert!(conflicts.is_empty(), "{conflicts:?}");
    }

    #[test]
    fn a_block_nobody_touched_stays_on_the_base() {
        let base = vec!["mu"];
        let (blocks, conflicts) = block(Some(&base), &[&base, &base]);
        assert_eq!(blocks, vec![(MergeBlock::Prosody, None)]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn a_block_changed_to_the_same_value_is_clean() {
        let (base, left, right) = (vec!["mu"], vec!["sigma"], vec!["sigma"]);
        let (blocks, conflicts) = block(Some(&base), &[&left, &right]);
        assert_eq!(blocks, vec![(MergeBlock::Prosody, Some(0))]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn a_block_changed_differently_conflicts() {
        let (base, left, right) = (vec!["mu"], vec!["sigma"], vec!["foot"]);
        let (blocks, conflicts) = block(Some(&base), &[&left, &right]);
        assert!(blocks.is_empty(), "衝突時不得留下取值");
        assert_eq!(conflicts, vec![MergeConflict::Block(MergeBlock::Prosody)]);
    }

    #[test]
    fn an_empty_base_treats_every_side_as_changed() {
        // 無共同祖先 ⇒ 沒有「誰動過」可言 ⇒ 只有全體一致才不衝突。
        let (left, right) = (vec!["mu"], vec!["sigma"]);
        let (_, conflicts) = block(None, &[&left, &right]);
        assert_eq!(conflicts, vec![MergeConflict::Block(MergeBlock::Prosody)]);

        let same = vec!["mu"];
        let (blocks, conflicts) = block(None, &[&left, &same]);
        assert_eq!(blocks, vec![(MergeBlock::Prosody, Some(0))]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn a_documents_signs_all_carry_document_scoped_ids() {
        // `signs_by_id` 對 `Ephemeral` 的 id 硬錯——那種 id 只在單一 `Language` 內
        // 有意義,拿它跨文件對齊會把不相干的 sign 湊成一對。
        //
        // **誠實標記**:該拒絕分支從本 crate **構造不出來**——`LanguageDocument` 的
        // 欄位私有,而所有建構路徑(`import_new_root`/`fork`)都會把 id 綁進文件命名
        // 空間。此測試只能證明正常路徑不會誤擋;拒絕分支是對 `language` 側日後變動的
        // 防線,目前無法以突變測試觀測。
        let document = LanguageDocument::import_new_root(
            "sign a:\n    syn:\n        category = noun\n",
            "evo:probe",
        )
        .unwrap();
        let map = signs_by_id(&document).expect("正常文件必須通過");
        assert!(map
            .keys()
            .all(|id| matches!(id.namespace, IdentityNamespace::Document(_))));
    }
}
