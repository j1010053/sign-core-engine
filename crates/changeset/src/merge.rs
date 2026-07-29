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

use conlang_language::{
    AddressSegment, IdentityAllocatorV2, IdentityError, IdentityNamespace, Language,
    LanguageDocument, NodeAddress, NodeEntryV1, NodeId, SignDef, TraitDef,
};
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
    /// 有衝突就建不出節點(§6.4):不存在「先建起來之後再解」的中間狀態。
    #[error("MERGE_UNRESOLVED_CONFLICTS: {0} conflict(s) must be resolved first")]
    UnresolvedConflicts(usize),
    /// 合併結果的命名空間與某個 parent 相同 ⇒ 新配發的 id 會撞上繼承來的 id。
    #[error("MERGE_NAMESPACE_IN_USE: {0} is already used by a parent")]
    NamespaceInUse(String),
    /// 計畫指向不存在的 parent,或該項在來源裡找不到。
    #[error("MERGE_SOURCE_MISSING: {0}")]
    SourceMissing(String),
    /// 合併後的文件出現預期外的頂層位址(語言模型長出新東西而合併沒跟上)。
    /// **硬錯而非沿用新 id**——沿用等於默默丟掉繼承的身分。
    #[error("MERGE_UNEXPECTED_ADDRESS: {0:?}")]
    UnexpectedAddress(NodeAddress),
    #[error(transparent)]
    Identity(#[from] IdentityError),
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

/// **物化**:把乾淨的合併計畫變成一份真正的 `LanguageDocument`(⑤b)。
///
/// ## 為什麼要繞一圈「先 import 再換 id」
///
/// 難處全在 **identity manifest**(身分清單):它記的位址是**位置式**的
/// (`Signs(3)`、`Signs(3)/Items(0)`…)。合併後 sign 會按名字重排,於是**每一個的
/// 「第幾個」都變了**,整份清單要重算——但**編號必須原封不動**(編號變了就等於換了
/// 一個 sign)。而且一個 sign 不是一筆,是**一整棵子樹**,每層位址都要跟著挪。
///
/// 與其自己重算位址(得複製 `language` 內部的節點列舉規則,兩份規則必然走鐘),
/// 這裡走一條更短的路:
///
/// 1. 先把挑好的內容組成 `Language`,`dump()` 後 `import_new_root` ——
///    **正確的形狀免費拿到**,只是 id 全是新配的;
/// 2. 再把每個節點的新 id **換回它繼承來的 id**(依位址對應回來源文件);
/// 3. `from_edit_parts` 驗證整份清單與 `.lang` 內容**逐位址相符**,不符當場爆。
///
/// 故位址算錯不可能靜默通過。**唯一不在該驗證範圍內的是配號器**,見下。
///
/// ## 配號器必須取各方最大值
///
/// 每個命名空間記著「下一個號碼發到幾」。只拿其中一方的 → 日後新增節點會**發出
/// 已經用過的號碼**,兩個不同節點共用一個 id,**不報錯**。故取聯集後的最大值。
///
/// `namespace` 必須是**沒被任何 parent 用過**的新命名空間,否則新配的 id 會撞上
/// 繼承來的 id。
pub fn materialize(
    plan: &MergePlan,
    base: Option<&LanguageDocument>,
    sides: &[&LanguageDocument],
    namespace: &str,
) -> Result<LanguageDocument, MergeError> {
    if !plan.is_clean() {
        return Err(MergeError::UnresolvedConflicts(plan.conflicts.len()));
    }
    // 比對的是**配號器**的命名空間,不是 `root_namespace`——後代文件的
    // `root_namespace` 仍是祖先的,而 id 實際上住在各次 fork 自己的命名空間裡。
    // 只看 `root_namespace` 會漏掉那些,讓新配的 id 撞上繼承來的 id。
    for document in base.iter().copied().chain(sides.iter().copied()) {
        if document
            .identities()
            .allocators
            .iter()
            .any(|allocator| allocator.namespace == namespace)
        {
            return Err(MergeError::NamespaceInUse(namespace.to_owned()));
        }
    }

    // ① 依計畫挑出內容,組成 Language。
    let mut language = Language::new();
    for (block, from) in &plan.blocks {
        let source = source_of(base, sides, *from)?;
        match block {
            MergeBlock::DslDecls => language.dsl_decls = source.language().dsl_decls.clone(),
            MergeBlock::Prosody => language.prosody = source.language().prosody.clone(),
        }
    }
    let mut origins = Origins::default();
    for pick in &plan.distribution {
        let MergeKey::Name(key) = &pick.key else {
            return Err(MergeError::SourceMissing(format!("{:?}", pick.key)));
        };
        let source = source_of(base, sides, pick.from)?;
        let value = source
            .language()
            .distribution
            .iter()
            .find(|(candidate, _)| candidate == key)
            .ok_or_else(|| MergeError::SourceMissing(format!("distribution {key:?}")))?;
        origins.distribution.insert(key.clone(), pick.from);
        language.distribution.push(value.clone());
    }
    for pick in &plan.traits {
        let source = source_of(base, sides, pick.from)?;
        let item = traits_by_id(source)?
            .remove(node_key(&pick.key)?)
            .ok_or_else(|| MergeError::SourceMissing(format!("trait {:?}", pick.key)))?;
        origins.traits.insert(item.name.clone(), pick.from);
        language.traits.push(item.clone());
    }
    for pick in &plan.signs {
        let source = source_of(base, sides, pick.from)?;
        let sign = signs_by_id(source)?
            .remove(node_key(&pick.key)?)
            .ok_or_else(|| MergeError::SourceMissing(format!("sign {:?}", pick.key)))?;
        origins.signs.insert(sign.name.clone(), pick.from);
        language.signs.push(sign.clone());
    }

    // ② 形狀免費拿:dump → import。id 全新,下一步再換回繼承的。
    let fresh = LanguageDocument::import_new_root(&language.dump(), namespace)?;
    let (fresh_language, mut manifest) = fresh.into_edit_parts();

    // ③ 新 id → 繼承 id。先建對照表,再一次套用到 `id` 與 `parent`,
    //    否則改到一半的清單會出現指向舊 id 的 parent。
    let by_address: Vec<BTreeMap<&NodeAddress, &NodeEntryV1>> = base
        .iter()
        .copied()
        .chain(sides.iter().copied())
        .map(|document| {
            document
                .identities()
                .nodes
                .iter()
                .map(|entry| (&entry.address, entry))
                .collect()
        })
        .collect();
    let mut remap: BTreeMap<NodeId, NodeId> = BTreeMap::new();
    for entry in &manifest.nodes {
        if let Some(inherited) = inherited_id(
            &entry.address,
            plan,
            &origins,
            &fresh_language,
            base,
            sides,
            &by_address,
        )? {
            remap.insert(entry.id.clone(), inherited);
        }
    }
    for entry in &mut manifest.nodes {
        if let Some(replacement) = remap.get(&entry.id) {
            entry.id = replacement.clone();
        }
        if let Some(parent) = entry.parent.as_ref().and_then(|id| remap.get(id)).cloned() {
            entry.parent = Some(parent);
        }
    }

    // ④ 配號器取各方最大值(不在 from_edit_parts 的驗證範圍內,漏了會靜默撞號)。
    let mut allocators: BTreeMap<String, u64> = BTreeMap::new();
    for source in base
        .iter()
        .copied()
        .chain(sides.iter().copied())
        .map(LanguageDocument::identities)
        .chain(std::iter::once(&manifest as &_))
    {
        for allocator in &source.allocators {
            let slot = allocators.entry(allocator.namespace.clone()).or_default();
            *slot = (*slot).max(allocator.next_ordinal);
        }
    }
    manifest.allocators = allocators
        .into_iter()
        .map(|(namespace, next_ordinal)| IdentityAllocatorV2 {
            namespace,
            next_ordinal,
        })
        .collect();

    Ok(LanguageDocument::from_edit_parts(fresh_language, manifest)?)
}

/// 某個合併後的位址,其身分該從哪裡繼承。`None` = 保留新配的 id(只有 Language 根)。
/// 合併後的每個項目**來自哪個 parent**,以名字/鍵為索引。
///
/// 不能用位置索引:`MergePlan` 的項目依**鍵**排序,而 canonical printer(P21/I15-d)
/// 把具名容器依**名字**排序——兩者順序不同,用位置去查會系統性地對錯人。
#[derive(Default)]
struct Origins {
    signs: BTreeMap<String, Option<usize>>,
    traits: BTreeMap<String, Option<usize>>,
    distribution: BTreeMap<String, Option<usize>>,
}

fn inherited_id(
    address: &NodeAddress,
    plan: &MergePlan,
    origins: &Origins,
    merged: &Language,
    base: Option<&LanguageDocument>,
    sides: &[&LanguageDocument],
    by_address: &[BTreeMap<&NodeAddress, &NodeEntryV1>],
) -> Result<Option<NodeId>, MergeError> {
    let Some(head) = address.0.first() else {
        return Ok(None); // Language 根:新文件的根,命名空間必須是新的那個。
    };
    // 頂層段 → (來自哪個 parent, 在該 parent 裡的頂層段)
    let (from, source_head) = match head {
        AddressSegment::Signs(index) => {
            let name = &merged
                .signs
                .get(*index)
                .ok_or_else(|| MergeError::UnexpectedAddress(address.clone()))?
                .name;
            let from = *origins
                .signs
                .get(name)
                .ok_or_else(|| MergeError::SourceMissing(format!("sign {name:?}")))?;
            let source = source_of(base, sides, from)?;
            let position = source
                .language()
                .signs
                .iter()
                .position(|sign| &sign.name == name)
                .ok_or_else(|| MergeError::SourceMissing(format!("sign {name:?}")))?;
            (from, AddressSegment::Signs(position))
        }
        AddressSegment::Traits(index) => {
            let name = &merged
                .traits
                .get(*index)
                .ok_or_else(|| MergeError::UnexpectedAddress(address.clone()))?
                .name;
            let from = *origins
                .traits
                .get(name)
                .ok_or_else(|| MergeError::SourceMissing(format!("trait {name:?}")))?;
            let source = source_of(base, sides, from)?;
            let position = source
                .language()
                .traits
                .iter()
                .position(|item| &item.name == name)
                .ok_or_else(|| MergeError::SourceMissing(format!("trait {name:?}")))?;
            (from, AddressSegment::Traits(position))
        }
        AddressSegment::Distribution(index) => {
            let key = &merged
                .distribution
                .get(*index)
                .ok_or_else(|| MergeError::UnexpectedAddress(address.clone()))?
                .0;
            let from = *origins
                .distribution
                .get(key)
                .ok_or_else(|| MergeError::SourceMissing(format!("distribution {key:?}")))?;
            let source = source_of(base, sides, from)?;
            let position = source
                .language()
                .distribution
                .iter()
                .position(|(candidate, _)| candidate == key)
                .ok_or_else(|| MergeError::SourceMissing(format!("distribution {key:?}")))?;
            (from, AddressSegment::Distribution(position))
        }
        // 整塊搬過來的,內容逐字相同 ⇒ 位址一一對應,不必換算。
        AddressSegment::DslDeclarations(_) => {
            (block_source(plan, MergeBlock::DslDecls)?, head.clone())
        }
        AddressSegment::Prosody => (block_source(plan, MergeBlock::Prosody)?, head.clone()),
        _ => return Err(MergeError::UnexpectedAddress(address.clone())),
    };
    let mut source_address = vec![source_head];
    source_address.extend(address.0.iter().skip(1).cloned());
    let source_address = NodeAddress(source_address);
    // `by_address` 的排列是 base(若有)在前、其餘依 sides 順序。
    let slot = match from {
        None => 0,
        Some(index) => usize::from(base.is_some()) + index,
    };
    Ok(by_address
        .get(slot)
        .and_then(|map| map.get(&source_address))
        .map(|entry| entry.id.clone()))
}

fn block_source(plan: &MergePlan, block: MergeBlock) -> Result<Option<usize>, MergeError> {
    plan.blocks
        .iter()
        .find(|(candidate, _)| *candidate == block)
        .map(|(_, from)| *from)
        .ok_or_else(|| MergeError::SourceMissing(format!("block {block:?}")))
}

fn source_of<'a>(
    base: Option<&'a LanguageDocument>,
    sides: &[&'a LanguageDocument],
    from: Option<usize>,
) -> Result<&'a LanguageDocument, MergeError> {
    match from {
        None => base.ok_or_else(|| MergeError::SourceMissing("base".to_owned())),
        Some(index) => sides
            .get(index)
            .copied()
            .ok_or_else(|| MergeError::SourceMissing(format!("parent #{index}"))),
    }
}

fn node_key(key: &MergeKey) -> Result<&NodeId, MergeError> {
    match key {
        MergeKey::Node(id) => Ok(id),
        MergeKey::Name(name) => Err(MergeError::SourceMissing(format!(
            "expected node id, got {name:?}"
        ))),
    }
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
