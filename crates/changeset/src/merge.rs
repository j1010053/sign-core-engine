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
//! 「兩邊都改過」。有共同祖先的場景下絕大多數項目是「僅一邊改過」,2-way 會產生
//! 數量級的假衝突,機制不可用。
//!
//! ## 逐項 vs 整塊:看有沒有對齊鍵
//!
//! 三態規則要能**逐項**套用,前提是兩邊的項目對得起來。故分兩類:
//!
//! | 區段 | 對齊鍵 | 作法 |
//! |---|---|---|
//! | `signs` | 穩定 id | **逐項** |
//! | `traits` | 穩定 id(向 identity manifest 取) | **逐項** |
//! | `distribution` | 鍵字串 | **逐項** |
//! | `prosody` | 無——單一條鏈,沒有「第幾項對第幾項」 | 整塊 |
//! | `dsl_decls` | 無——不透明 verbatim(I15-a),看不進內容 | 整塊 |
//!
//! 逐項的三個**共用同一段泛型程式**(`merge_keyed`)。寫三份幾乎一樣的程式會走鐘:
//! 日後補一個角落只改到其中一份,詞與文法的合併行為就悄悄不一致了。
//!
//! **顆粒度到「一個項目」為止,不再往下**:A 改了某個 sign 的發音、B 改了同一個
//! sign 的語意,仍算衝突。降到維度/規則層是另一個議題,本刀不做(owner 裁定)。
//!
//! ## 對齊鍵 = 穩定 id
//!
//! 承 docs/06 §6.1(diff 的對齊方式):「同 id = 同一 sign 的兩個演化階段」。
//! 合併與 diff 因此**共用同一套對齊**,不是兩套;且能正確處理「一邊改了名」
//! ——用名字對齊會把它誤判成一生一滅。

use conlang_language::{IdentityNamespace, LanguageDocument, NodeId, SignDef, TraitDef};
use std::collections::{BTreeMap, BTreeSet};

/// 逐項合併的三個區段。衝突必須說得出是**哪個區段**,否則 trait 衝突與 sign 衝突
/// 在回報上無從分辨。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeCollection {
    Signs,
    Traits,
    Distribution,
}

/// 逐項合併的對齊鍵。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MergeKey {
    /// `signs` / `traits`:穩定 id(docs/06 §6.1)。
    Node(NodeId),
    /// `distribution`:鍵字串本身就是身分。
    Name(String),
}

/// 合併計畫裡的一筆:某個鍵最終取哪一份內容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub key: MergeKey,
    /// `None` = 各方都沒動,沿用基準;`Some(i)` = 取第 i 個 parent 的版本。
    pub from: Option<usize>,
}

/// 沒有對齊鍵、只能整塊比的區段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeBlock {
    /// 不透明 verbatim 行(I15-a),合併器看不進內容。
    DslDecls,
    /// 單一條七層鏈——**沒有「聯集」可言**,只能擇一或重寫。
    Prosody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeConflict {
    /// 多方相對基準改成**不同**的值。
    Content {
        collection: MergeCollection,
        key: MergeKey,
        sides: Vec<usize>,
    },
    /// 一邊刪掉、另一邊改動(git 的 delete/modify)。
    DeleteModify {
        collection: MergeCollection,
        key: MergeKey,
        deleted_by: Vec<usize>,
        modified_by: Vec<usize>,
    },
    /// 同一個鍵出現在多方、但**基準沒有**——合法共有的鍵只可能來自共同祖先,
    /// 故這是各自獨立配發後撞號,不是同一個項目。空基準時任何共有鍵皆屬此類。
    /// 這正是 `add_root` 的 namespace 守門擋不住的那一半(跨家族 fork 撞號)。
    IdCollision {
        collection: MergeCollection,
        key: MergeKey,
        sides: Vec<usize>,
    },
    /// 合併結果裡有兩個**不同鍵**的項目同名。`.lang` 名字唯一,故非法。
    /// 融合不同源語言時這是主要工作量,而非語意分歧(§6.2)。
    NameCollision {
        collection: MergeCollection,
        name: String,
        keys: Vec<MergeKey>,
    },
    /// 無對齊鍵的區段各方改成不同值。
    Block(MergeBlock),
}

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    /// sign 的 id 不在文件命名空間裡 ⇒ 跨文件無法對齊。
    #[error("MERGE_UNSTABLE_SIGN_ID: sign {name:?} has a non-document id {id}")]
    UnstableSignId { name: String, id: NodeId },
    /// trait 在 identity manifest 裡沒有對應條目 ⇒ 沒有對齊鍵。
    #[error("MERGE_UNIDENTIFIED_TRAIT: trait {name:?} has no identity entry")]
    UnidentifiedTrait { name: String },
    /// 至少要兩個 parent 才叫合併。
    #[error("MERGE_TOO_FEW_PARENTS: merging needs at least two parents")]
    TooFewParents,
}

/// 合併計畫。`conflicts` 非空時**不得建節點**(§6.4:有衝突就建不出來,
/// 不存在「先建起來之後再解」的中間狀態)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePlan {
    /// 三個逐項區段各自的取值,依鍵排序,故計畫是決定性的(P26)。
    pub signs: Vec<Pick>,
    pub traits: Vec<Pick>,
    pub distribution: Vec<Pick>,
    /// 無鍵區段取自哪一方(`None` = 沿用基準);有衝突時該項不出現。
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
    let mut conflicts = Vec::new();

    // ── signs(逐項,以穩定 id 對齊)──
    let base_signs = base.map(signs_by_id).transpose()?.unwrap_or_default();
    let side_signs = sides
        .iter()
        .map(|document| signs_by_id(document))
        .collect::<Result<Vec<_>, _>>()?;
    let merged = merge_keyed(&base_signs, &side_signs);
    let signs = label(
        MergeCollection::Signs,
        merged,
        &mut conflicts,
        MergeKey::Node,
    );
    conflicts.extend(name_collisions(
        MergeCollection::Signs,
        &signs,
        &base_signs,
        &side_signs,
        |sign: &&SignDef| sign.name.as_str(),
        MergeKey::Node,
    ));

    // ── traits(逐項;`TraitDef` 不自帶 id,向 identity manifest 取)──
    let base_traits = base.map(traits_by_id).transpose()?.unwrap_or_default();
    let side_traits = sides
        .iter()
        .map(|document| traits_by_id(document))
        .collect::<Result<Vec<_>, _>>()?;
    let merged = merge_keyed(&base_traits, &side_traits);
    let traits = label(
        MergeCollection::Traits,
        merged,
        &mut conflicts,
        MergeKey::Node,
    );
    conflicts.extend(name_collisions(
        MergeCollection::Traits,
        &traits,
        &base_traits,
        &side_traits,
        |item: &&TraitDef| item.name.as_str(),
        MergeKey::Node,
    ));

    // ── distribution(逐項;鍵字串即身分,故不可能有「不同鍵同名」)──
    let base_distribution = base.map(distribution_by_key).unwrap_or_default();
    let side_distribution: Vec<_> = sides.iter().map(|d| distribution_by_key(d)).collect();
    let merged = merge_keyed(&base_distribution, &side_distribution);
    let distribution = label(
        MergeCollection::Distribution,
        merged,
        &mut conflicts,
        MergeKey::Name,
    );

    // ── 無鍵區段(整塊)──
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

    Ok(MergePlan {
        signs,
        traits,
        distribution,
        blocks,
        conflicts,
    })
}

/// 一次逐項合併的**原始結果**(還沒貼上區段標籤)。
///
/// 泛型函式刻意不碰 `MergeConflict`——那樣它才能同時服務三個區段;貼標籤由 `label` 做。
struct KeyedMerge<K> {
    picks: Vec<(K, Option<usize>)>,
    content: Vec<(K, Vec<usize>)>,
    delete_modify: Vec<(K, Vec<usize>, Vec<usize>)>,
    collisions: Vec<(K, Vec<usize>)>,
}

/// **逐項三態規則**——signs / traits / distribution 共用這一段。
///
/// 它不需要知道自己在合併什麼,只需要:每個項目有個**鍵**,兩個項目能比**相等**。
/// 寫三份幾乎一樣的程式會走鐘(補一個角落只改到其中一份),故泛型化。
fn merge_keyed<K: Ord + Clone, V: PartialEq>(
    base: &BTreeMap<K, V>,
    sides: &[BTreeMap<K, V>],
) -> KeyedMerge<K> {
    let mut universe: BTreeSet<&K> = base.keys().collect();
    for map in sides {
        universe.extend(map.keys());
    }
    let mut result = KeyedMerge {
        picks: Vec::new(),
        content: Vec::new(),
        delete_modify: Vec::new(),
        collisions: Vec::new(),
    };
    for key in universe {
        let present: Vec<usize> = (0..sides.len())
            .filter(|index| sides[*index].contains_key(key))
            .collect();
        match base.get(key) {
            // 基準沒有這個鍵。
            None => {
                if present.len() > 1 {
                    result.collisions.push((key.clone(), present));
                } else {
                    // 恰好一方新增 → 直接納入。(`present` 不可能為空:鍵來自聯集。)
                    result.picks.push((key.clone(), Some(present[0])));
                }
            }
            // 基準有這個鍵:比對誰改過、誰刪了。
            Some(inherited) => {
                let deleted: Vec<usize> = (0..sides.len())
                    .filter(|index| !sides[*index].contains_key(key))
                    .collect();
                let changed: Vec<usize> = present
                    .iter()
                    .copied()
                    .filter(|index| sides[*index][key] != *inherited)
                    .collect();
                if !deleted.is_empty() && !changed.is_empty() {
                    result.delete_modify.push((key.clone(), deleted, changed));
                } else if !deleted.is_empty() {
                    // 有人刪、其餘沒動 → 採用刪除(不進計畫)。
                } else if changed.is_empty() {
                    result.picks.push((key.clone(), None));
                } else {
                    let first = &sides[changed[0]][key];
                    if changed.iter().all(|index| &sides[*index][key] == first) {
                        result.picks.push((key.clone(), Some(changed[0])));
                    } else {
                        result.content.push((key.clone(), changed));
                    }
                }
            }
        }
    }
    result
}

/// 把泛型結果貼上區段標籤,轉成公開型別。
fn label<K>(
    collection: MergeCollection,
    merged: KeyedMerge<K>,
    conflicts: &mut Vec<MergeConflict>,
    into_key: impl Fn(K) -> MergeKey,
) -> Vec<Pick> {
    for (key, sides) in merged.content {
        conflicts.push(MergeConflict::Content {
            collection,
            key: into_key(key),
            sides,
        });
    }
    for (key, deleted_by, modified_by) in merged.delete_modify {
        conflicts.push(MergeConflict::DeleteModify {
            collection,
            key: into_key(key),
            deleted_by,
            modified_by,
        });
    }
    for (key, sides) in merged.collisions {
        conflicts.push(MergeConflict::IdCollision {
            collection,
            key: into_key(key),
            sides,
        });
    }
    merged
        .picks
        .into_iter()
        .map(|(key, from)| Pick {
            key: into_key(key),
            from,
        })
        .collect()
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

/// `.lang` 的名字必須唯一。合併把兩支的內容並在一起時,**不同鍵同名**就是硬性錯誤
/// ——這是融合不同源語言的主要工作量(§6.2/§6.3)。
fn name_collisions<K: Ord + Clone, V>(
    collection: MergeCollection,
    picks: &[Pick],
    base: &BTreeMap<K, V>,
    sides: &[BTreeMap<K, V>],
    name_of: impl Fn(&V) -> &str,
    into_key: impl Fn(K) -> MergeKey,
) -> Vec<MergeConflict> {
    let mut by_name: BTreeMap<String, Vec<MergeKey>> = BTreeMap::new();
    let keys: BTreeSet<&K> = base
        .keys()
        .chain(sides.iter().flat_map(|map| map.keys()))
        .collect();
    for key in keys {
        let candidate = into_key(key.clone());
        let Some(pick) = picks.iter().find(|pick| pick.key == candidate) else {
            continue; // 衝突或被刪掉的項目不進計畫,也就不參與命名檢查。
        };
        let value = match pick.from {
            Some(index) => &sides[index][key],
            None => &base[key],
        };
        by_name
            .entry(name_of(value).to_owned())
            .or_default()
            .push(candidate);
    }
    by_name
        .into_iter()
        .filter(|(_, keys)| keys.len() > 1)
        .map(|(name, keys)| MergeConflict::NameCollision {
            collection,
            name,
            keys,
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

/// trait 的對齊鍵。`TraitDef` 本身**不帶 id**(不像 `SignDef`),故向 identity manifest
/// 取——這正是 sidecar 的用意:身分不住在 `.lang` 文字裡。
fn traits_by_id(document: &LanguageDocument) -> Result<BTreeMap<NodeId, &TraitDef>, MergeError> {
    let mut map = BTreeMap::new();
    for item in &document.language().traits {
        let reference =
            document
                .ref_for_trait(&item.name)
                .ok_or_else(|| MergeError::UnidentifiedTrait {
                    name: item.name.clone(),
                })?;
        map.insert(reference.id, item);
    }
    Ok(map)
}

/// distribution 的鍵字串**就是**身分,不需要 sidecar。
fn distribution_by_key(document: &LanguageDocument) -> BTreeMap<String, &String> {
    document
        .language()
        .distribution
        .iter()
        .map(|(key, value)| (key.clone(), value))
        .collect()
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
    fn the_keyed_rule_is_shared_by_every_collection() {
        // 泛型函式不碰任何區段專屬型別——這正是「一段程式用三次」的證據:
        // 餵它純 `&str`,若它偷偷依賴 sign/trait 的型別,這裡就編不過。
        let base: BTreeMap<u8, &str> = [(1, "a"), (2, "b")].into_iter().collect();
        let left: BTreeMap<u8, &str> = [(1, "A"), (2, "b")].into_iter().collect();
        let right: BTreeMap<u8, &str> = [(1, "a"), (2, "b")].into_iter().collect();
        let merged = merge_keyed(&base, &[left, right]);
        assert_eq!(merged.picks, vec![(1, Some(0)), (2, None)]);
        assert!(merged.content.is_empty());
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
