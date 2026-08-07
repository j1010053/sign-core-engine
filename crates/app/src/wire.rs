//! **出境契約**:前端看到的形狀(裁定 ② 丙,擁有者 2026-08-06)。
//!
//! # 為什麼是「加 `Serialize` + 明確版本化」而不是另定一套 DTO
//!
//! 三個選項當時是:
//!
//! | | 作法 | 代價 |
//! |---|---|---|
//! | 甲 | query 型別直接出境 | 內部形狀變成 wire contract,改欄位即 breaking 而**沒人會發現** |
//! | 乙 | 另定一套 DTO | 雙份維護,欄位容易漂移 |
//! | **丙** | 出境,但**帶 schema 標記** | 多一層命名紀律 |
//!
//! 丙的理由是 repo 已有先例:`SemanticDocumentV1` 就是明確版本化的出境 DTO
//! (`SEMANTIC_SCHEMA_V1` + `#[serde(deny_unknown_fields)]`)。照同一個模式做,
//! 不算新發明,而且**版本欄位讓「形狀變了」這件事在資料上看得見**——
//! 這正是甲缺的那一半。
//!
//! # 什麼時候要 bump [`UI_SCHEMA_V1`]
//!
//! **任何出境形狀改變時**:本模組的欄位增刪、其嵌入的 `conlang-query` 型別的
//! 欄位增刪、列舉變體改名。純文件與內部實作不算。
//!
//! 與 `COMPILER_SEMANTICS_VERSION` 同樣是**約定不是機制**——沒有東西強制你
//! bump。旁證是本模組的 golden 測試:形狀一變 JSON 就 churn,審查時看得到。

use conlang_changeset::state::EvolutionState;
use conlang_query::{Grouping, Lexicon};
use serde::{Deserialize, Serialize};

pub const UI_SCHEMA_V1: &str = "conlang.ui/v1";

/// 一條入邊。
///
/// `kind` 區分主幹與引用:**引用邊不是世系鄰接**(donor 借詞、克里奧爾合併),
/// 分群不沿它切,UI 也該畫得不一樣(例如虛線)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeEdgeV1 {
    pub from: String,
    /// `"trunk"` | `"reference"`。
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeNodeV1 {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 空 = root。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<TreeEdgeV1>,
}

/// 演化樹視圖。
///
/// **只給 parents,不給 children**——`EvolutionGraph` 本身就沒有 children
/// 索引(節點 id 由 parents 的 id 算出),前端要畫樹自己反轉一次即可。
/// 在這裡假造一個 children 欄位,等於讓出境形狀宣稱一個資料層沒有的關係。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionTreeV1 {
    pub schema: String,
    /// 依 id 排序(決定性)。
    pub nodes: Vec<TreeNodeV1>,
    /// 目前開著的節點。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

/// 一個節點的詞典。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexiconViewV1 {
    pub schema: String,
    pub node: String,
    pub lexicon: Lexicon,
}

/// 節點編輯頁要的東西:**全部是雜湊外的**(P64),改它們不動任何 replay 產物。
///
/// 對映 `NodeMetadataCommand` 的三個槽位:`config`(label)、`state`、`annotation/`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeDetailV1 {
    pub schema: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub state: EvolutionState,
    /// 旁註檔名,排序後。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<String>,
    /// 這個節點有幾個詞——編輯頁的概況,不是完整詞典。
    pub sign_count: usize,
}

/// 方言分群視圖。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupingViewV1 {
    pub schema: String,
    pub grouping: Grouping,
}
