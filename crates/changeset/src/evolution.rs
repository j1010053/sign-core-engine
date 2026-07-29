//! 步驟 16 ④ —— **演化圖節點模型**(《修補11》P56/P58/P59;docs/06 §1–§5)。
//!
//! ## 節點 = 結果,邊 = 過程(P56)
//!
//! 前一版把 changeset 存在節點上、狀態靠 replay 現算,並以 `cache` + `stale` 傳播
//! 加速。該模型有結構性缺陷(《修補11》§0):`.chg` 的 digest 釘死 base,所以「改了
//! 祖先」與「後代仍可求值」**互斥**——stale 傳播標記完後代,後代必然因 digest 不符
//! 而失敗。兩個機制互相打架,不是實作 bug。
//!
//! 現模型:
//!
//! - **`Node` 持 snapshot**(物化狀態),**`Edge` 持 changeset**(`.chg` 原文);
//! - **兩者皆不可變**。改 changeset 不是就地改邊,而是**生成新邊 + 新節點**;
//! - 因果不變:snapshot **永遠**由 `apply(parent.snapshot, edge.changeset)` 產生,
//!   **不可手動編寫**。故 docs/06 §5「不存副本(單一資訊源)」的**精神保留**——
//!   誰決定內容仍是 changeset,改的只是「果要不要留下來」。
//!
//! 不可變性使「副本與真相不同步」**不可能發生**(兩者都不會在腳下改變),且不變式
//! `apply(trunk.from.snapshot, trunk.changeset) == snapshot` **隨時可查**
//! (`verify`,git `fsck` 的對應物)。故物化不是失去單一資訊源,是把果快取成一等資料。
//!
//! **紅利**:讀節點狀態是 **O(1)**(`snapshot()`),不必 replay 祖先鏈;replay 只在
//! **建立**節點時發生一次。先前量到的「replay 一棵樹會放大成本」對讀取直接消失,
//! 連帶 `cache`/`replay_count`/`invalidate` 三個機制一起不需要了。
//!
//! ## 內容定址(P58)
//!
//! `NodeId = sha256(snapshot ‖ parents ‖ nativization)`,同構 git commit hash,
//! 滿足 P26(決定性、禁隨機/時間戳)。兩個推論:
//!
//! - **無環是結構保證,不是檢查**:節點 id 由其 parents 的 id 算出,故「成環」需要
//!   一個雜湊包含自己——不可能。前一版的 `check_acyclic` 因此整個移除,
//!   而非改寫(docs/06 §2 的無環約束由型別而非執行期斷言達成)。
//! - **重複提交是冪等的**:內容全同 ⇒ 同一個 id ⇒ 同一個物件(git 語意)。
//!   故前一版的 `Duplicate` 錯誤消失。

use crate::merge::{plan_merge, MergeError, MergePlan};
use crate::{
    identity_manifest_digest, ChangeInterpreter, LanguageDocument, ReplayError, UnresolvedChangeSet,
};
use conlang_language::{sha256_hex, LibrarySpec};
use std::collections::{BTreeMap, BTreeSet};

/// 節點識別 = **內容雜湊**(P58)。
///
/// 欄位私有:id **只能由內容算出**,不能自取——這正是「身分含出身」與無環保證的來源。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(String);

impl NodeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// pidgin/creole 判定(docs/06 §4)。**社會語言學的分界(有無母語者),不由入邊決定**,
/// 故是節點的獨立屬性。一般造語手動填【M】;multi-agent 的湧現偵測屬【M+】。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Nativization {
    #[default]
    None,
    /// 接觸簡化系統,無母語者。
    Pidgin,
    /// pidgin 被某代當母語習得,獲完整語法。
    Creole { generation: u32 },
}

impl Nativization {
    /// 進入 NodeId 雜湊的正規形(P58 決定性)。
    fn canonical(self) -> String {
        match self {
            Nativization::None => "none".to_owned(),
            Nativization::Pidgin => "pidgin".to_owned(),
            Nativization::Creole { generation } => format!("creole:{generation}"),
        }
    }
}

/// 演化樹的**邊**(P56):不可變,承載「過程」。
///
/// `changeset` **只有主幹邊(`parents[0]`)帶**;其餘 parent 是**引用邊**
/// (docs/06 §5「其餘 parent 僅供條目引用取材」),必須為 `None`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: NodeId,
    pub changeset: Option<String>,
}

impl Edge {
    /// 主幹邊:帶 `.chg` 原文。
    pub fn trunk(from: NodeId, changeset: impl Into<String>) -> Edge {
        Edge {
            from,
            changeset: Some(changeset.into()),
        }
    }

    /// 引用邊:融合時的非主幹 parent。
    pub fn reference(from: NodeId) -> Edge {
        Edge {
            from,
            changeset: None,
        }
    }
}

/// 演化圖的一個節點(P56):**不可變**。
///
/// 欄位私有:`snapshot` 尤其不可被外部寫入——它永遠是 replay 的結果,手動編寫會
/// 破壞 §2.2 的因果契約。
#[derive(Debug, Clone)]
pub struct Node {
    parents: Vec<Edge>,
    snapshot: LanguageDocument,
    nativization: Nativization,
    label: Option<String>,
}

impl Node {
    /// 入邊;主幹在 `[0]`。root 為空。
    pub fn parents(&self) -> &[Edge] {
        &self.parents
    }

    /// 物化的語言狀態。
    pub fn snapshot(&self) -> &LanguageDocument {
        &self.snapshot
    }

    pub fn nativization(&self) -> Nativization {
        self.nativization
    }

    /// 人類可讀名字。**不是身分**(P58/P45):不進雜湊。
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// 主幹邊(root 沒有)。
    pub fn trunk(&self) -> Option<&Edge> {
        self.parents.first()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EvolutionError {
    #[error("EVOLUTION_UNKNOWN_NODE: {0}")]
    UnknownNode(NodeId),
    /// root 由 `add_root` 建立;其餘節點必須有 parent。
    #[error("EVOLUTION_NO_PARENT: a committed node needs at least one parent")]
    NoParent,
    /// 兩個 root 共用 identity namespace ⇒ 它們的穩定 id 會撞,合併會靜默併掉
    /// 無關的 sign(見 `add_root`)。
    #[error("EVOLUTION_DUPLICATE_ROOT_NAMESPACE: {0} is already used by another root")]
    DuplicateRootNamespace(String),
    /// 同一份 `.lang`、不同 identity namespace ⇒ 內容雜湊相同但身分不同。
    /// 見 `add_root` 的說明。
    #[error(
        "EVOLUTION_ROOT_IDENTITY_CONFLICT: same source already rooted at {kept}, not {incoming}"
    )]
    RootIdentityConflict { kept: String, incoming: String },
    /// 主幹邊必須帶 changeset——否則 snapshot 無從產生(P56 因果契約)。
    #[error("EVOLUTION_TRUNK_WITHOUT_CHANGESET: parents[0] must carry a changeset")]
    TrunkWithoutChangeset,
    /// 引用邊不得帶 changeset(P56:只有主幹邊帶)。
    #[error("EVOLUTION_REFERENCE_EDGE_WITH_CHANGESET: only parents[0] may carry a changeset")]
    ReferenceEdgeWithChangeset,
    /// fsck:節點 id 不等於其內容雜湊(P58)。
    #[error("EVOLUTION_CORRUPT_ID: {stored} stores content hashing to {computed}")]
    CorruptId { stored: NodeId, computed: NodeId },
    /// fsck:snapshot ≠ replay(parent.snapshot, edge.changeset)(P56 §2.2 不變式)。
    #[error("EVOLUTION_SNAPSHOT_MISMATCH: {0} snapshot is not the replay of its trunk edge")]
    SnapshotMismatch(NodeId),
    /// rebase 找不到可重寫的 base digest 行——邊上的 `.chg` 不合格式。
    #[error("EVOLUTION_PRELUDE_WITHOUT_DIGESTS: the edge changeset has no base digest lines")]
    PreludeWithoutDigests,
    /// 有多個互不為祖先的共同祖先候選 —— **不自行挑一個**(§12.6)。
    #[error("EVOLUTION_AMBIGUOUS_MERGE_BASE: {0:?} are all lowest common ancestors")]
    AmbiguousMergeBase(Vec<NodeId>),
    #[error(transparent)]
    Merge(#[from] MergeError),
    #[error(transparent)]
    Replay(#[from] ReplayError),
}

/// 演化圖(P56):節點物化、邊承載 changeset,**全部不可變**。
///
/// 沒有 `cache`/`replay_count`/`invalidate`——snapshot 已是結果,快取層無事可做;
/// 也沒有 `check_acyclic`——內容定址使成環在型別上不可能(見模組說明)。
/// **多 root**(《修補11》§12.5):圖可以有數個彼此無血緣的起點語言。
///
/// 單 root 之下任兩個節點往上追必定相遇,故 P61 的「**無共同祖先 → 空基準**」路徑
/// 永遠走不到——而那正是真克里奧爾(法語 + 西非語言)的形狀。不是「做得出來但沒用」,
/// 是**造不出輸入去測它**,寫了也是改成什麼樣測試都不會紅的死碼。
#[derive(Debug)]
pub struct EvolutionGraph {
    libraries: LibrarySpec,
    nodes: BTreeMap<NodeId, Node>,
    roots: BTreeSet<NodeId>,
    /// 已用掉的 root namespace。見 `add_root` 的說明。
    root_namespaces: BTreeSet<String>,
}

impl EvolutionGraph {
    /// 建一張**空**圖。起點語言用 `add_root` 加(可以有多個)。
    pub fn new(libraries: LibrarySpec) -> EvolutionGraph {
        EvolutionGraph {
            libraries,
            nodes: BTreeMap::new(),
            roots: BTreeSet::new(),
            root_namespaces: BTreeSet::new(),
        }
    }

    /// 加一個起點語言:沒有入邊的節點,snapshot 直接就是給定的文件。
    ///
    /// ## 為什麼要擋重複的 root namespace
    ///
    /// namespace 決定該文件裡每個節點的穩定 id(`<namespace>:<n>`),而**合併是以穩定
    /// id 對齊的**(承 docs/06 §6.1,與 diff 同一套)。兩個 root 若共用 namespace,
    /// 法語的 `evo:root:5`(水)與沃洛夫語的 `evo:root:5`(樹)會被合併器**當成同一個
    /// 詞的兩個演化階段而靜默併掉**——不報錯、沒有任何跡象。故在最早的時點硬擋。
    ///
    /// **誠實界線**:這只保證兩個 root **自身**的 id 不撞。後代 fork 的 namespace
    /// 由 `.chg` 的 `changeset <ns>:` 自由指定,跨家族仍可能撞。完整的防線是合併當下
    /// 的檢查——**兩側都有、但共同基底沒有的 id 即為碰撞**(空基底時任何共有 id 都是
    /// 碰撞)。該檢查屬 P61,本刀不做。
    ///
    /// 內容定址下重加同一份 root 是冪等的(同內容 = 同節點),故先查 id 再查 namespace。
    ///
    /// ⚠️ **`NodeId` 雜湊的是 `snapshot.source()`,而 namespace 不在 `.lang` 文字裡**
    /// (身分是 sidecar)。故「同一份 `.lang`、不同 namespace」的兩個 root **id 相同**。
    /// 若讓它走冪等路徑,呼叫端指定的 namespace 會被**默默忽略**,之後對著它寫的
    /// changeset 才會因 `base_identities` 不符而失敗——錯誤離成因很遠。故在冪等路徑上
    /// 額外比對 identity manifest,不一致就當場硬錯。
    pub fn add_root(&mut self, document: LanguageDocument) -> Result<NodeId, EvolutionError> {
        let id = node_id(&document, &[], Nativization::None);
        if let Some(existing) = self.nodes.get(&id) {
            let (kept, incoming) = (
                identity_manifest_digest(existing.snapshot())?,
                identity_manifest_digest(&document)?,
            );
            if kept != incoming {
                return Err(EvolutionError::RootIdentityConflict {
                    kept: existing.snapshot().identities().root_namespace.clone(),
                    incoming: document.identities().root_namespace.clone(),
                });
            }
            return Ok(id);
        }
        let namespace = document.identities().root_namespace.clone();
        if !self.root_namespaces.insert(namespace.clone()) {
            return Err(EvolutionError::DuplicateRootNamespace(namespace));
        }
        self.nodes.insert(
            id.clone(),
            Node {
                parents: Vec::new(),
                snapshot: document,
                nativization: Nativization::None,
                label: None,
            },
        );
        self.roots.insert(id.clone());
        Ok(id)
    }

    /// 全部的起點語言(彼此無血緣)。
    pub fn roots(&self) -> impl Iterator<Item = &NodeId> {
        self.roots.iter()
    }

    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &NodeId> {
        self.nodes.keys()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// 讀一個節點的語言狀態:**O(1)**(P56 紅利)——snapshot 已物化,不 replay。
    pub fn snapshot(&self, id: &NodeId) -> Result<&LanguageDocument, EvolutionError> {
        self.nodes
            .get(id)
            .map(Node::snapshot)
            .ok_or_else(|| EvolutionError::UnknownNode(id.clone()))
    }

    /// 建一個節點:套用主幹邊的 changeset 於其來源節點的 snapshot,物化結果。
    ///
    /// **replay 只在這裡發生一次**。內容定址使重複提交冪等(同內容 = 同節點)。
    ///
    /// 多親時 base 目前仍取 `parents[0]`(docs/06 §5 v0.1.1 的 MVP 語意);
    /// **全 parent 機械合併是 P61**,尚未實作——故引用邊現階段只登記來源,
    /// 不影響求值。此為**已知且已記錄的缺口**,不是預設行為。
    pub fn commit(
        &mut self,
        parents: Vec<Edge>,
        nativization: Nativization,
        label: Option<String>,
    ) -> Result<NodeId, EvolutionError> {
        let (trunk, references) = parents.split_first().ok_or(EvolutionError::NoParent)?;
        for edge in &parents {
            if !self.nodes.contains_key(&edge.from) {
                return Err(EvolutionError::UnknownNode(edge.from.clone()));
            }
        }
        if references.iter().any(|edge| edge.changeset.is_some()) {
            return Err(EvolutionError::ReferenceEdgeWithChangeset);
        }
        let changeset = trunk
            .changeset
            .clone()
            .ok_or(EvolutionError::TrunkWithoutChangeset)?;
        let base = self.snapshot(&trunk.from)?.clone();
        let snapshot = self.replay(&base, &changeset)?;
        let id = node_id(&snapshot, &parents, nativization);
        self.nodes.entry(id.clone()).or_insert(Node {
            parents,
            snapshot,
            nativization,
            label,
        });
        Ok(id)
    }

    /// **fsck**(P56 §2.2):驗證一個節點的兩條不變式——
    /// ① id 等於其內容雜湊(P58);② snapshot 等於主幹邊的 replay 結果。
    ///
    /// 這是「物化不等於失去單一資訊源」的可驗證性依據。
    pub fn verify(&self, id: &NodeId) -> Result<(), EvolutionError> {
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| EvolutionError::UnknownNode(id.clone()))?;
        let computed = node_id(&node.snapshot, &node.parents, node.nativization);
        if &computed != id {
            return Err(EvolutionError::CorruptId {
                stored: id.clone(),
                computed,
            });
        }
        let Some(trunk) = node.trunk() else {
            // root 沒有入邊,不變式 ② 不適用。
            return Ok(());
        };
        let changeset = trunk
            .changeset
            .as_deref()
            .ok_or(EvolutionError::TrunkWithoutChangeset)?;
        let base = self.snapshot(&trunk.from)?.clone();
        let replayed = self.replay(&base, changeset)?;
        if replayed.source() != node.snapshot.source() {
            return Err(EvolutionError::SnapshotMismatch(id.clone()));
        }
        Ok(())
    }

    /// 一個節點的全部祖先(**含自己**)。
    fn ancestors(&self, start: &NodeId) -> BTreeSet<NodeId> {
        let mut seen = BTreeSet::new();
        let mut frontier = vec![start.clone()];
        while let Some(current) = frontier.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if let Some(node) = self.nodes.get(&current) {
                for edge in &node.parents {
                    frontier.push(edge.from.clone());
                }
            }
        }
        seen
    }

    /// **最近共同祖先**——§6.3 三方合併的基準。
    ///
    /// | 回傳 | 意義 |
    /// |---|---|
    /// | `Ok(Some(id))` | 唯一的最近共同祖先 |
    /// | `Ok(None)` | **無共同祖先** ⇒ 空基準,規則退化為聯集(只有多 root 才可能) |
    /// | `Err(AmbiguousMergeBase)` | 多個互不為祖先的候選 |
    ///
    /// 多候選時**不自行挑一個**(§12.6):git 用遞迴合併處理,我們的 MVP 是報錯要求
    /// 人指定——挑錯基準會讓整份合併結果悄悄偏掉,比停下來糟得多。
    pub fn merge_base(&self, parents: &[NodeId]) -> Result<Option<NodeId>, EvolutionError> {
        for parent in parents {
            if !self.nodes.contains_key(parent) {
                return Err(EvolutionError::UnknownNode(parent.clone()));
            }
        }
        let mut sets = parents.iter().map(|id| self.ancestors(id));
        let Some(mut common) = sets.next() else {
            return Ok(None);
        };
        for set in sets {
            common.retain(|id| set.contains(id));
        }
        // 「最近」= 沒有別的共同祖先是它的後代。等價於:它不是任何其他共同祖先的祖先。
        let lowest: Vec<NodeId> = common
            .iter()
            .filter(|candidate| {
                !common
                    .iter()
                    .any(|other| other != *candidate && self.ancestors(other).contains(candidate))
            })
            .cloned()
            .collect();
        match lowest.len() {
            0 => Ok(None),
            1 => Ok(Some(lowest.into_iter().next().expect("len == 1"))),
            _ => Err(EvolutionError::AmbiguousMergeBase(lowest)),
        }
    }

    /// 算出多親合併的計畫(P61 §6)。`conflicts` 非空時**不得建節點**(§6.4)。
    ///
    /// 只算不做:物化成 `LanguageDocument` 需要 `language` 側新的建構 API,屬 ⑤b。
    pub fn merge_plan(&self, parents: &[NodeId]) -> Result<MergePlan, EvolutionError> {
        let base = self.merge_base(parents)?;
        let base_snapshot = base.as_ref().map(|id| self.snapshot(id)).transpose()?;
        let sides = parents
            .iter()
            .map(|id| self.snapshot(id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(plan_merge(base_snapshot, &sides)?)
    }

    /// 對全圖跑 fsck。
    pub fn verify_all(&self) -> Result<(), EvolutionError> {
        for id in self.nodes.keys() {
            self.verify(id)?;
        }
        Ok(())
    }

    /// **rebase**(P57):把 `node` 的主幹 changeset 改套到 `onto` 的 snapshot 上。
    ///
    /// 這是《修補11》§2.3 流程的第三步——改了祖先之後,後代要不要跟過去。
    /// 舊節點**完全不動**(不可變),成功時產生的是**並存的**新節點。
    ///
    /// ## 為什麼不需要「放寬 digest 驗證」
    ///
    /// §3.1 描述 rebase 為「放寬 digest 重新 resolve」。實作上更直接:**重寫 prelude
    /// 的兩行 base digest** 使其對上新 base——這同時達成放寬**且**產生正確的新邊。
    /// 若只放寬而不重寫,新節點的邊會帶著舊 digest,`verify`(fsck)當場就會失敗。
    /// 故重寫是**必需**的,並且涵蓋了放寬;`lib.rs` 的驗證路徑一行都不用改
    /// (P59:步驟 14 的契約語意一字不動)。
    ///
    /// library lock **刻意不重寫**——版本變動是另一類事件(§3.2「環境變動」),
    /// 應該浮出來讓人看見,不是被 rebase 默默吞掉。
    pub fn rebase(
        &mut self,
        node: &NodeId,
        onto: &NodeId,
    ) -> Result<RebaseOutcome, EvolutionError> {
        let source = self
            .nodes
            .get(node)
            .ok_or_else(|| EvolutionError::UnknownNode(node.clone()))?;
        let (nativization, label) = (source.nativization, source.label.clone());
        let changeset = source
            .trunk()
            .and_then(|edge| edge.changeset.clone())
            .ok_or(EvolutionError::TrunkWithoutChangeset)?;
        let base = self.snapshot(onto)?.clone();
        let rebased = rebase_prelude(&changeset, &base)?;

        match self.commit(
            vec![Edge::trunk(onto.clone(), rebased)],
            nativization,
            label,
        ) {
            Ok(id) => Ok(RebaseOutcome::Clean(id)),
            // 分類**只依變體**(P57 鐵律):訊息字串是給人看的,改字不該改行為。
            Err(EvolutionError::Replay(error)) => Ok(RebaseOutcome::classify(error)),
            // 圖層面的錯(未知節點等)不是 rebase 結果,照常往上拋。
            Err(other) => Err(other),
        }
    }

    fn replay(
        &self,
        base: &LanguageDocument,
        changeset: &str,
    ) -> Result<LanguageDocument, EvolutionError> {
        let parsed = UnresolvedChangeSet::parse(changeset)?;
        let namespace = parsed.namespace.clone();
        let resolved = parsed.resolve(base, &self.libraries)?;
        let interpreter = ChangeInterpreter::new(base.clone(), self.libraries.clone(), namespace)?;
        Ok(interpreter.run(&resolved)?.document)
    }
}

/// rebase 的三分結果(《修補11》§3.2)。
#[derive(Debug)]
pub enum RebaseOutcome {
    /// 乾淨:changeset 原封不動套上了新 base,產生新節點。
    Clean(NodeId),
    /// **衝突**:該筆編輯在新 base 上套不上去。`statement` 指出**哪一句**
    /// ——由 `ReplayError::Statement { ordinal }` 免費提供,不必逐句試探。
    Conflict {
        statement: Option<u64>,
        error: ReplayError,
    },
    /// **環境變動**(套件版本),不是節點衝突。
    Environment(ReplayError),
    /// changeset 本身壞了(輸入錯誤),不是衝突。
    Broken(ReplayError),
}

impl RebaseOutcome {
    /// 依**錯誤變體**分類(P57)。公開是為了讓分類表本身可測——經由 `rebase` 只走得到
    /// 一部分分支(`commit` 已驗過 changeset 可解析,故 `Parse`/`Schema` 到不了),
    /// 沒有這個入口,`Broken`/`Environment` 兩支就是測不到的死碼。
    pub fn classify(error: ReplayError) -> RebaseOutcome {
        match &error {
            // 語句層失敗:目標不存在、錨點失效、驗證不過、型別/欄位對不上……
            // `StatementSelector` 是「依名字定址找不到目標」——rebase 最常見的衝突。
            ReplayError::Statement { ordinal, .. }
            | ReplayError::StatementSelector { ordinal, .. } => RebaseOutcome::Conflict {
                statement: Some(*ordinal),
                error,
            },
            // 定址解不開 / 穩定 id 對不上 / 結果編不出系統:都是新 base 造成的衝突,
            // 只是沒有句號可指。
            ReplayError::Selector(_) | ReplayError::Identity(_) | ReplayError::Compile(_) => {
                RebaseOutcome::Conflict {
                    statement: None,
                    error,
                }
            }
            // 環境:套件版本/載入。**刻意不當成衝突**——不該要求人去改 changeset。
            ReplayError::LibraryLockMismatch(_) | ReplayError::Library(_) => {
                RebaseOutcome::Environment(error)
            }
            // 輸入本身壞掉。`BaseSourceMismatch`/`BaseIdentitiesMismatch` 落在這裡
            // 代表 prelude 重寫沒生效——那是本模組的 bug,不是使用者的衝突,
            // 歸為 Broken 讓它顯眼而不是被誤報成衝突。
            ReplayError::Parse(_)
            | ReplayError::Schema(_)
            | ReplayError::NamespaceMismatch(_)
            | ReplayError::BaseSourceMismatch
            | ReplayError::BaseIdentitiesMismatch => RebaseOutcome::Broken(error),
        }
    }

    pub fn is_clean(&self) -> bool {
        matches!(self, RebaseOutcome::Clean(_))
    }
}

/// 把 `.chg` 原文的兩行 base digest 改成對上 `base`,其餘**逐字保留**。
///
/// 不用「解析後重新 dump」是為了**保住原文**:dump 排出的是降階後的原語
/// (步驟 14 的 primitive-only 契約),`rewrite`/`clone` 這類授權糖會被抹平,
/// 使用者的書寫意圖就丟了。逐行替換只動兩行,其餘(含註解、library lock)不變。
fn rebase_prelude(changeset: &str, base: &LanguageDocument) -> Result<String, EvolutionError> {
    let source_digest = base.identities().source_sha256.clone();
    let identity_digest = identity_manifest_digest(base)?;
    let mut output = String::with_capacity(changeset.len());
    let (mut saw_source, mut saw_identities) = (false, false);
    for line in changeset.lines() {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        if trimmed.starts_with("base_source") {
            output.push_str(&format!("{indent}base_source = sha256:{source_digest}\n"));
            saw_source = true;
        } else if trimmed.starts_with("base_identities") {
            output.push_str(&format!(
                "{indent}base_identities = sha256:{identity_digest}\n"
            ));
            saw_identities = true;
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    if !saw_source || !saw_identities {
        return Err(EvolutionError::PreludeWithoutDigests);
    }
    Ok(output)
}

/// **P58 內容定址**:`sha256(snapshot ‖ parents ‖ nativization)`。
///
/// 每個組件先各自雜湊成定長 hex 再串接,故編碼是單射的(不會有「不同輸入拼出同一
/// 字串」的歧義),且無隨機/時間戳來源(P26)。
///
/// **`label` 不入雜湊**——P58「人類可讀名字是另一層,不是身分」(承 P45)。
///
/// **邊的 changeset 入雜湊**:P58 原文寫 `‖ parents` 而 `parents: Vec<Edge>`,
/// 字面上含 changeset;且其理由是「身分含出身,內容相同但**來歷**不同是不同節點」,
/// 而「怎麼走到這個狀態」正是來歷。若只雜湊 `from`,兩條不同 changeset 走到相同
/// 狀態的邊會摺疊成一個節點,**其中一份 changeset 會被靜默丟棄**——違反
/// docs/06「存事實(ChangeSet)」。故採含 changeset 的讀法。
fn node_id(snapshot: &LanguageDocument, parents: &[Edge], nativization: Nativization) -> NodeId {
    let mut buffer = String::from("conlang-node-v1\n");
    buffer.push_str(&format!(
        "snapshot {}\n",
        sha256_hex(snapshot.source().as_bytes())
    ));
    for edge in parents {
        buffer.push_str(&format!(
            "parent {} {} {}\n",
            edge.from.as_str(),
            u8::from(edge.changeset.is_some()),
            sha256_hex(edge.changeset.as_deref().unwrap_or("").as_bytes())
        ));
    }
    buffer.push_str(&format!("nativization {}\n", nativization.canonical()));
    NodeId(sha256_hex(buffer.as_bytes()))
}

/// 讓 rebase(P57,下一刀)與外部工具能取得節點 snapshot 的 identity digest。
/// 現在只有 `verify` 的姊妹用途;獨立出來避免日後在兩處重算。
pub fn snapshot_identity_digest(document: &LanguageDocument) -> Result<String, ReplayError> {
    identity_manifest_digest(document)
}

#[cfg(test)]
mod tests {
    //! fsck 的**失敗**分支只能在模組內測——`Node` 的欄位是私有的,外部無從偽造一個
    //! 壞節點。這正是想要的:偽造能力留在測試裡,不外洩成 API。
    //!
    //! 沒有這兩個測試,`verify` 的兩個比較都可以改成 `true` 而測試全綠(整合測試只
    //! 走得到成功路徑)——那是典型的假綠燈。

    use super::*;
    use crate::change_set_prelude;

    const ROOT: &str = "sign x:\n    syn:\n        category = noun\n";

    fn fixture() -> (EvolutionGraph, NodeId) {
        let root = LanguageDocument::import_new_root(ROOT, "evo:root").expect("root parses");
        let mut graph = EvolutionGraph::new(LibrarySpec::default());
        let root_id = graph.add_root(root).expect("root added");
        let base = graph.snapshot(&root_id).unwrap().clone();
        let mut changeset =
            change_set_prelude(&base, &LibrarySpec::default(), "evo:n1").expect("prelude");
        changeset
            .push_str("\n    #0:\n        update sign(\"x\").def[syn.category].value = verb\n");
        let id = graph
            .commit(
                vec![Edge::trunk(root_id, changeset)],
                Nativization::None,
                None,
            )
            .expect("n1 commits");
        (graph, id)
    }

    #[test]
    fn fsck_catches_a_hand_edited_snapshot() {
        // 威脅模型:有人手改 snapshot **並重算 id**,於是 id 檢查通過。只有
        // 「snapshot == replay(parent, changeset)」這條不變式擋得住(P56 §2.2)。
        let (mut graph, id) = fixture();
        let mut node = graph.nodes.remove(&id).expect("node exists");
        node.snapshot = LanguageDocument::import_new_root(
            "sign x:\n    syn:\n        category = adj\n",
            "evo:forged",
        )
        .expect("forged parses");
        let forged_id = node_id(&node.snapshot, &node.parents, node.nativization);
        graph.nodes.insert(forged_id.clone(), node);

        let err = graph
            .verify(&forged_id)
            .expect_err("fsck 必須擋下手改的 snapshot");
        assert!(
            matches!(err, EvolutionError::SnapshotMismatch(_)),
            "{err:?}"
        );
    }

    #[test]
    fn fsck_catches_a_node_stored_under_the_wrong_id() {
        let (mut graph, id) = fixture();
        let node = graph.nodes.remove(&id).expect("node exists");
        let wrong = NodeId("0".repeat(64));
        graph.nodes.insert(wrong.clone(), node);

        let err = graph.verify(&wrong).expect_err("fsck 必須擋下錯置的 id");
        assert!(matches!(err, EvolutionError::CorruptId { .. }), "{err:?}");
    }

    #[test]
    fn a_healthy_graph_passes_fsck() {
        let (graph, _) = fixture();
        graph.verify_all().expect("乾淨的圖必須通過");
    }

    /// 分類表的**死角分支**:經由 `rebase` 走不到(`commit` 已驗過 changeset 可解析,
    /// 故 `Parse`/`Schema` 到不了;library spec 在圖內固定,故 lock 也不會不符)。
    /// 沒有這個直接入口,`Environment`/`Broken` 兩支就是永遠測不到的死碼——
    /// 它們可以被改成任何東西而測試全綠。
    #[test]
    fn the_classifier_is_driven_by_variants_not_messages() {
        assert!(matches!(
            RebaseOutcome::classify(ReplayError::LibraryLockMismatch("v2".to_owned())),
            RebaseOutcome::Environment(_)
        ));
        assert!(matches!(
            RebaseOutcome::classify(ReplayError::Library("missing package".to_owned())),
            RebaseOutcome::Environment(_)
        ));
        assert!(matches!(
            RebaseOutcome::classify(ReplayError::Parse("bad".to_owned())),
            RebaseOutcome::Broken(_)
        ));
        assert!(matches!(
            RebaseOutcome::classify(ReplayError::Schema("v9".to_owned())),
            RebaseOutcome::Broken(_)
        ));
        // 入口信號落到這裡代表 prelude 重寫沒生效 = 本模組的 bug,
        // **不得**被誤報成使用者的衝突。
        assert!(matches!(
            RebaseOutcome::classify(ReplayError::BaseSourceMismatch),
            RebaseOutcome::Broken(_)
        ));
        assert!(matches!(
            RebaseOutcome::classify(ReplayError::Selector("unknown sign".to_owned())),
            RebaseOutcome::Conflict {
                statement: None,
                ..
            }
        ));
    }

    #[test]
    fn rebasing_a_changeset_without_digest_lines_is_rejected() {
        // 不合格式的邊要明確擋下,不能默默產生一份缺 digest 的 `.chg`。
        let root = LanguageDocument::import_new_root(ROOT, "evo:root").expect("root parses");
        let err = rebase_prelude("changeset evo:n:\n    schema = v1\n", &root)
            .expect_err("沒有 digest 行");
        assert!(
            matches!(err, EvolutionError::PreludeWithoutDigests),
            "{err:?}"
        );
    }
}
