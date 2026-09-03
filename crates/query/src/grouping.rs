//! 分群:**兩個不同的問題,兩個函式**(P72)。
//!
//! | 函式 | 比什麼 | 範圍 | 答什麼 |
//! |---|---|---|---|
//! | [`periods`] | 主幹邊(親↔子) | 全圖,N−1 次 | 何時該說它變成另一個語言了 |
//! | [`dialect_groups`] | 切片內任意兩點 | 一個[年代切片](Slice) | 這個時代誰跟誰互通 |
//!
//! # 為什麼一定要分開(P72)
//!
//! **方言是共時概念:母語言與子語言不可能同時出現。** 舊實作用親子邊算
//! 「方言群」,於是一條純鏈狀的專案(一個語言一路演化)會得到
//!
//! ```text
//! 古英語 ──> 中古英語 ──> 現代英語        舊輸出:2 個「方言群」
//! ```
//!
//! 那三者不是方言,是**歷時階段**;而真正的方言分群在這張圖上的答案應該是
//! **恆為一群**——任何時刻都只有一個語言活著。
//!
//! 沿主幹邊切出來的東西本身有用,它只是**不叫方言群**:它回答「什麼時候該說
//! 它變成另一個語言了」,也就是歷時分期。故 [`periods`] 保留原演算法與原語意,
//! 只把名字改對。
//!
//! # 兩者對 `演化圖本體論` §6.2 的關係
//!
//! > **方言連續體 = 樹上一組互通度高於某閾值的鄰近點**;閾值由觀察者/UI 設定,
//! > 非本質——資料層不做此離散判斷(鐵律)。
//!
//! [`periods`] 把「鄰近」讀成**邊相鄰**(距離 1)。[`dialect_groups`] 把它讀成
//! **同一個切片之內**——切片是反鏈,成員彼此沒有祖裔關係,兩兩都是同代的旁系。
//!
//! 舊模組說明拒絕「任意 pair」的理由是:會把**收斂**(非同源卻變像)也併進來,
//! 而收斂在本體論裡沒建模(`接觸痕跡與語言聯盟_v0.1.md`)。那條理由針對的是
//! **全域**任意對;切片內任意對仍然可能因收斂而併群(切片成員是同代旁系,
//! 各自漂移後可能變像)。**這一點沒有被解決,只是被限縮**:結果該讀成
//! 「現在看起來多像」,不是「同源」。語言聯盟仍未建模。
//!
//! # 閾值以外的鐵律沒變
//!
//! 兩個函式都不把離散判斷寫進資料層:切片是誰、閾值多少,都由呼叫端給。
//!
//! # 只看主幹邊
//!
//! `parents[0]` 是主幹(帶 changeset,是「這個語言由誰演化而來」);其餘是
//! **引用邊**——donor(借了幾個詞)或合併(克里奧爾)。把引用邊當世系鄰接,
//! 會讓「借了三個詞」和「同一支方言」變成同一回事。
//!
//! # 兩個函式共有的語意:群 = **連通分量**,不是「群內兩兩互通」
//!
//! 過閾值就把兩點併起來,群是併出來的分量。所以群內可以存在一對彼此低於閾值
//! 的成員(A–B 通、B–C 通,但 A–C 不通)。要「群內兩兩皆通」是**團**(clique)
//! 語意,那是另一個演算法,且無法用 union-find 表達。
//!
//! # 平行創新會低報互通度
//!
//! 對齊鍵是 `SignId` / `RuleId`(§6.1),而兩條分支各自新增的東西 id 不同
//! ——**兩邊做了同一件事**會被算成一生一滅。實測:兩個兄弟各做**完全相同**的
//! 九條音變,彼此互通度只有 `0.1429`,比各自對父的 `0.5714` 還低。
//!
//! 從族譜的角度那是對的(獨立造出來的同形詞不是同源詞);從「聽不聽得懂」的
//! 角度是錯的。這個張力繼承自 §6.2 把互通度定義成 diff 的派生函數,不是本模組
//! 能單獨修的——見 `intelligibility.rs` 與《分層差異向量 v0.2 裁定》。
//! **兩個函式都受它影響**,切片分群尤其:切片成員本來就是各自演化的旁系。
//!
//! # Override 是分類指派,不是 merge/split
//!
//! D-f2(擁有者 2026-08-04):指派是**函數**不是關係,故不可能互相矛盾,
//! 結果由建構保證唯一。管線因此只有三段,沒有 `validate consistency`:
//!
//! ```text
//! 1. strategy 算出基礎分群
//! 2. 套用 assignments(sparse 覆寫)
//! 3. 套用 labels(純顯示,不影響身分)
//! ```

use serde::{Deserialize, Serialize};
use crate::intelligibility::{intelligibility, IntelligibilityMeasure};
use conlang_changeset::evolution::{EvolutionGraph, NodeId};
use std::collections::BTreeMap;

/// 群組身分。MVP 用「該群裡最小的節點 id」當代表,故決定性且無需配號器。
pub type GroupId = String;

/// 使用者對分群結果的**詮釋層**覆寫(`邏輯分層` §1.2)。
///
/// 語言/方言界線本質是社會政治判斷(馬其頓語 vs 保加利亞語),不是語言距離
/// 能回答的。故資料層只存連續的差異向量,這裡是詮釋。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupingOverride {
    /// node → group。**sparse**:未列者一律用 strategy 算出的結果。
    ///
    /// 指派而非 merge/split:後者是關係運算,`A+B`、`B+C`、`A|C` 同時存在時
    /// 結果取決於套用順序、可能無解。
    pub assignments: BTreeMap<String, GroupId>,
    /// group → 顯示名。**純展示**,不影響群組身分。
    pub labels: BTreeMap<GroupId, String>,
}

/// 分群結果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grouping {
    /// node → group。**涵蓋範圍取決於誰算的**:[`periods`] 是全圖的每個節點,
    /// [`dialect_groups`] 只有切片成員。
    pub members: BTreeMap<String, GroupId>,
    /// group → 顯示名(只有被命名的才在)。
    pub labels: BTreeMap<GroupId, String>,
    /// 哪一套互通度算的——同 `IntelligibilityScore::measure_id` 的用意。
    pub measure_id: String,
    pub threshold: f64,
}

impl Grouping {
    /// 某群的成員,依節點 id 排序。
    pub fn members_of(&self, group: &str) -> Vec<&str> {
        self.members
            .iter()
            .filter(|(_, id)| id.as_str() == group)
            .map(|(node, _)| node.as_str())
            .collect()
    }

    /// 全部群組 id,排序後。
    pub fn groups(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self.members.values().map(String::as_str).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// 歷時分期的策略(P72)。**可替換**(同 §6.2 的「互通度是可替換函數」精神)。
pub trait PeriodizationStrategy: std::fmt::Debug {
    fn periods(
        &self,
        graph: &EvolutionGraph,
        measure: &dyn IntelligibilityMeasure,
        override_: &GroupingOverride,
    ) -> Grouping;
}

/// 沿**主幹邊**按閾值切斷;期 = 剩下的邊所連成的分量。
///
/// 只做 N−1 次互通度計算(每個非 root 節點與其主幹父各一次)。
///
/// **這算的是歷時分期,不是方言群**——它比的是同一支血脈的前後階段。要問
/// 「同一個時代誰跟誰互通」請用 [`dialect_groups`]。
#[derive(Debug, Clone)]
pub struct TreeEdgeCut {
    /// 低於此值即切斷。
    pub threshold: f64,
}

/// 極簡 union-find。節點數以「一個專案的語言數」為量級,不需要更精巧的東西。
#[derive(Debug)]
struct Union {
    parent: BTreeMap<String, String>,
}

impl Union {
    fn new(ids: impl Iterator<Item = String>) -> Union {
        Union {
            parent: ids.map(|id| (id.clone(), id)).collect(),
        }
    }

    fn find(&self, id: &str) -> String {
        let mut current = id.to_owned();
        while let Some(next) = self.parent.get(&current) {
            if next == &current {
                return current;
            }
            current = next.clone();
        }
        current
    }

    /// 併兩群,**代表一律取字典序較小者**——故結果與 union 的呼叫順序無關。
    fn union(&mut self, a: &str, b: &str) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let (keep, drop) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.parent.insert(drop, keep);
    }
}

impl PeriodizationStrategy for TreeEdgeCut {
    fn periods(
        &self,
        graph: &EvolutionGraph,
        measure: &dyn IntelligibilityMeasure,
        override_: &GroupingOverride,
    ) -> Grouping {
        let ids: Vec<&NodeId> = graph.ids().collect();
        let mut union = Union::new(ids.iter().map(|id| id.as_str().to_owned()));

        for id in &ids {
            let Some(node) = graph.node(id) else { continue };
            // 只看主幹邊;引用邊(donor / 合併)不是世系鄰接
            let Some(trunk) = node.parents().first() else {
                continue;
            };
            let (Ok(child), Ok(parent)) = (graph.snapshot(id), graph.snapshot(&trunk.from)) else {
                continue;
            };
            if intelligibility(parent, child, measure).value >= self.threshold {
                union.union(id.as_str(), trunk.from.as_str());
            }
        }

        let members = ids
            .iter()
            .map(|id| (id.as_str().to_owned(), union.find(id.as_str())))
            .collect();
        finish(members, override_, measure, self.threshold)
    }
}

/// 三段收尾:基礎分群 → 指派覆寫 → 顯示名。兩個函式共用,免得各自漂移。
fn finish(
    mut members: BTreeMap<String, GroupId>,
    override_: &GroupingOverride,
    measure: &dyn IntelligibilityMeasure,
    threshold: f64,
) -> Grouping {
    // 指派覆寫(sparse)。指向**不在本次結果裡**的節點的指派**靜靜略過**
    // ——view 檔可能比圖舊(節點被移除),或那個節點不在這個切片裡,
    // 而兩者都不該讓整個視圖失敗。
    for (node, group) in &override_.assignments {
        if let Some(slot) = members.get_mut(node) {
            *slot = group.clone();
        }
    }

    // 顯示名。只保留實際存在的群,免得 UI 列出空群。
    let live: std::collections::BTreeSet<&GroupId> = members.values().collect();
    let labels = override_
        .labels
        .iter()
        .filter(|(group, _)| live.contains(group))
        .map(|(group, label)| (group.clone(), label.clone()))
        .collect();

    Grouping {
        members,
        labels,
        measure_id: measure.id().to_owned(),
        threshold,
    }
}

/// 歷時分期的便利入口。
pub fn periods(
    graph: &EvolutionGraph,
    strategy: &dyn PeriodizationStrategy,
    measure: &dyn IntelligibilityMeasure,
    override_: &GroupingOverride,
) -> Grouping {
    strategy.periods(graph, measure, override_)
}

// ── 年代切片 ────────────────────────────────────────────────────────────────

/// 一個**年代切片**:一組**同時存在**的語言(P73)。
///
/// # 為什麼是拓撲條件而不是年代欄位(P73)
///
/// 「同時存在」在這個模型裡的可判定內容是:**沒有任何一個成員是另一個的祖先**
/// ——母語言與子語言不可能並存,那是同一支血脈的前後階段。這是圖的**反鏈**
/// (antichain),純由主幹邊算得出來。
///
/// `EvolutionState::time` 是**自由字串**,而且那裡明說「絕對年代只是給人看的
/// 標註……沒有任何運算依賴它」——若改用它來切片,就得處理「A 是 B 的祖先卻標了
/// 較晚年代」這種與圖拓撲矛盾的資料。切片走拓撲,那條理由就繼續成立。
///
/// # 祖裔只算主幹邊(P74)
///
/// 引用邊(donor / 合併)**不算**祖裔:克里奧爾與它的來源語**確實會並存**
/// (海地克里奧爾與法語),把引用邊當祖裔會把這種真實情形判成不可能。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slice {
    nodes: Vec<NodeId>,
}

/// 切片建不起來的兩種原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SliceError {
    /// 指定的節點不在圖裡。
    UnknownNode(NodeId),
    /// 兩個成員有祖裔關係——它們不可能同時存在。
    NotAnAntichain {
        ancestor: NodeId,
        descendant: NodeId,
    },
}

impl std::fmt::Display for SliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SliceError::UnknownNode(id) => write!(f, "SLICE_UNKNOWN_NODE: {id}"),
            SliceError::NotAnAntichain {
                ancestor,
                descendant,
            } => write!(
                f,
                "SLICE_NOT_AN_ANTICHAIN: {ancestor} is an ancestor of {descendant}; \
                 a parent language and its child cannot coexist"
            ),
        }
    }
}

impl std::error::Error for SliceError {}

impl Slice {
    /// **現存語言**:沒有任何主幹子節點的節點(圖的葉)。
    ///
    /// 這是最常要的那個切片,也是唯一一個不必問使用者就算得出來的
    /// ——「現在還活著的語言」。純鏈狀專案在這裡只會得到一個成員,
    /// 而那正是對的答案:一個語言一路演化,任何時刻只有一個語言活著。
    pub fn contemporary(graph: &EvolutionGraph) -> Slice {
        let with_children: std::collections::BTreeSet<&NodeId> = graph
            .ids()
            .filter_map(|id| graph.node(id))
            .filter_map(|node| node.trunk())
            .map(|edge| &edge.from)
            .collect();
        Slice {
            nodes: graph
                .ids()
                .filter(|id| !with_children.contains(id))
                .cloned()
                .collect(),
        }
    }

    /// 自選切片。**驗證反鏈**(P73:違反必須拒絕):任兩個成員不得有祖裔關係。
    ///
    /// 不驗的話,使用者一旦把拉丁文和法文放進同一個切片,分群就會回到舊實作
    /// 那個「母語言與子語言同群」的荒謬——而那正是切片要消滅的東西。
    pub fn new(
        graph: &EvolutionGraph,
        nodes: impl IntoIterator<Item = NodeId>,
    ) -> Result<Slice, SliceError> {
        let mut sorted: Vec<NodeId> = nodes.into_iter().collect();
        sorted.sort();
        sorted.dedup();
        for id in &sorted {
            if graph.node(id).is_none() {
                return Err(SliceError::UnknownNode(id.clone()));
            }
        }
        let members: std::collections::BTreeSet<&NodeId> = sorted.iter().collect();
        for id in &sorted {
            for ancestor in trunk_ancestors(graph, id) {
                if members.contains(&ancestor) {
                    return Err(SliceError::NotAnAntichain {
                        ancestor,
                        descendant: id.clone(),
                    });
                }
            }
        }
        Ok(Slice { nodes: sorted })
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// 一個節點沿**主幹邊**往上的全部祖先(不含自己)。P74:只有主幹算祖裔。
///
/// 主幹只有一條,故這是一條線而不是一棵樹;`seen` 仍然要有——圖由持久化還原
/// 時的環在載入層擋(`EVOLUTION_PERSISTED_GRAPH_CYCLE`),但這裡不該假設呼叫端
/// 一定走過那條路。
fn trunk_ancestors(graph: &EvolutionGraph, id: &NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut current = id.clone();
    while let Some(trunk) = graph.node(&current).and_then(|node| node.trunk()) {
        if !seen.insert(trunk.from.clone()) {
            break;
        }
        out.push(trunk.from.clone());
        current = trunk.from.clone();
    }
    out
}

/// **共時方言群**(P72):在一個年代切片(P73)內,兩兩比較互通度,群 = 連通分量。
///
/// # 為什麼是任意兩點而不是「只比兄弟」
///
/// 切片是反鏈,成員彼此**都是**同代旁系;親兄弟只是其中距離最近的一種。只比
/// 親兄弟會讓「同祖父的兩支」永遠不可能同群,而那沒有語言學上的理由。
///
/// # 成本
///
/// `C(L,2)` 次互通度計算,`L` = 切片大小。實測每次比較約 `0.8 µs × 詞數`
/// (release),故 400 詞、50 個語言的切片約 `1225 × 0.32 ms ≈ 0.4 s`。
/// 這一版**不做稀疏化**:路徑上界剪枝與 union-find 剪枝是另一刀,做之前先讓
/// 語意站對地方。呼叫端要注意這是**互動路徑**上的成本(桌面端的閾值 slider)。
pub fn dialect_groups(
    graph: &EvolutionGraph,
    slice: &Slice,
    threshold: f64,
    measure: &dyn IntelligibilityMeasure,
    override_: &GroupingOverride,
) -> Grouping {
    let mut union = Union::new(slice.nodes.iter().map(|id| id.as_str().to_owned()));

    for (index, left) in slice.nodes.iter().enumerate() {
        for right in &slice.nodes[index + 1..] {
            let (Ok(a), Ok(b)) = (graph.snapshot(left), graph.snapshot(right)) else {
                continue;
            };
            if intelligibility(a, b, measure).value >= threshold {
                union.union(left.as_str(), right.as_str());
            }
        }
    }

    let members = slice
        .nodes
        .iter()
        .map(|id| (id.as_str().to_owned(), union.find(id.as_str())))
        .collect();
    finish(members, override_, measure, threshold)
}
