//! 方言分群:**可替換 strategy**,MVP 只做 `TreeEdgeCut`。
//!
//! # 為什麼是「樹上鄰近」而不是任意 pair
//!
//! 這不是取捨,是規格明文。`演化圖本體論` §6.2:
//!
//! > **方言連續體 = 樹上一組互通度高於某閾值的鄰近點**;閾值由觀察者/UI 設定,
//! > 非本質——資料層不做此離散判斷(鐵律)。
//!
//! 「樹上」「鄰近點」直接就是沿演化邊切。取全部 pair 的連通分量會把
//! **收斂**(非同源卻變像)也併進來,而那在本體論裡**根本沒建模**——
//! 見 `docs/architecture/接觸痕跡與語言聯盟_v0.1.md`。
//!
//! # 只看主幹邊
//!
//! `parents[0]` 是主幹(帶 changeset,是「這個語言由誰演化而來」);其餘是
//! **引用邊**——donor(借了幾個詞)或合併(克里奧爾)。把引用邊當世系鄰接,
//! 會讓「借了三個詞」和「同一支方言」變成同一回事。
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

use crate::intelligibility::{intelligibility, IntelligibilityMeasure};
use conlang_changeset::evolution::{EvolutionGraph, NodeId};
use std::collections::BTreeMap;

/// 群組身分。MVP 用「該群裡最小的節點 id」當代表,故決定性且無需配號器。
pub type GroupId = String;

/// 使用者對分群結果的**詮釋層**覆寫(`邏輯分層` §1.2)。
///
/// 語言/方言界線本質是社會政治判斷(馬其頓語 vs 保加利亞語),不是語言距離
/// 能回答的。故資料層只存連續的差異向量,這裡是詮釋。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Grouping {
    /// node → group,全部節點都在。
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

pub trait DialectGroupingStrategy: std::fmt::Debug {
    fn group(
        &self,
        graph: &EvolutionGraph,
        measure: &dyn IntelligibilityMeasure,
        override_: &GroupingOverride,
    ) -> Grouping;
}

/// 沿**主幹邊**按閾值切斷;群 = 剩下的邊所連成的分量。
///
/// 只做 N−1 次互通度計算(每個非 root 節點與其主幹父各一次)。
/// 相對地,任意 pair 的連通分量要 O(N²) 次,而每次都要跑一趟 `diff_vector`。
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

impl DialectGroupingStrategy for TreeEdgeCut {
    fn group(
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

        // ① 基礎分群
        let mut members: BTreeMap<String, GroupId> = ids
            .iter()
            .map(|id| (id.as_str().to_owned(), union.find(id.as_str())))
            .collect();

        // ② 指派覆寫(sparse)。指向未知節點的指派**靜靜略過**——view 檔可能
        //    比圖舊(節點被移除),而那不該讓整個視圖失敗。
        for (node, group) in &override_.assignments {
            if let Some(slot) = members.get_mut(node) {
                *slot = group.clone();
            }
        }

        // ③ 顯示名。只保留實際存在的群,免得 UI 列出空群。
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
            threshold: self.threshold,
        }
    }
}

/// 便利入口。
pub fn dialect_groups(
    graph: &EvolutionGraph,
    strategy: &dyn DialectGroupingStrategy,
    measure: &dyn IntelligibilityMeasure,
    override_: &GroupingOverride,
) -> Grouping {
    strategy.group(graph, measure, override_)
}
