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
use conlang_query::{DerivationDag, Grouping, IntelligibilityScore, Lexicon};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    ///
    /// **一律序列化,即使是空陣列。** 先前加了 `skip_serializing_if` 省那幾個
    /// 位元組,結果 root 節點在 JSON 裡沒有這個鍵,前端 `node.parents.length`
    /// 直接 `Cannot read properties of undefined`。
    ///
    /// 「沒有 parents」與「parents 為空」**不是兩件事**——省略它只是替消費端
    /// 憑空造一個 `undefined` 分支,沒有換到任何語意。
    /// (對照 `active: Option<String>`:那裡「沒有」與「有」確實不同,故保留。)
    #[serde(default)]
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

/// Launcher 與全域導覽列共用的專案概況。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSummaryV1 {
    pub schema: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 沒有 `project.toml`、以 fallback 套件開啟的既有 store。
    pub legacy: bool,
    pub graph_dirty: bool,
    pub has_pending: bool,
    pub node_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    pub packages: Vec<String>,
}

/// Current offline package catalog exposed to the desktop settings page.
///
/// `declared` means the package is a root in `project.toml`; `selected` also
/// includes packages pulled in transitively by `requires`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackageV1 {
    pub id: String,
    pub kind: String,
    pub version: String,
    pub source: String,
    pub enabled: bool,
    pub declared: bool,
    pub selected: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCatalogV1 {
    pub schema: String,
    pub packages: Vec<CatalogPackageV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightEntryV1 {
    pub segment: String,
    pub weight: f64,
    /// `manual` | `prior`; imported providers remain reserved for a later UI.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightConfigV1 {
    pub schema: String,
    pub declaration_source: String,
    pub manual: Vec<WeightEntryV1>,
    pub effective: Vec<WeightEntryV1>,
}

/// Structured authoring choice. Values are opaque engine identifiers; labels
/// are display-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringChoiceV1 {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringFieldV1 {
    pub name: String,
    pub label: String,
    /// `text` | `textarea` | `boolean` | `choice`.
    pub control: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<AuthoringChoiceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringNodeV1 {
    pub selector: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub kind: String,
    pub path: String,
    pub summary: String,
    pub deletable: bool,
    pub movable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<AuthoringFieldV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringSignV1 {
    pub name: String,
    pub selector: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringTraitV1 {
    pub name: String,
    pub global: bool,
    pub blocks: usize,
    /// `local` | `library`.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringCatalogV1 {
    pub schema: String,
    pub revision: String,
    pub nodes: Vec<AuthoringNodeV1>,
    pub signs: Vec<AuthoringSignV1>,
    pub traits: Vec<AuthoringTraitV1>,
    pub rule_homes: Vec<AuthoringChoiceV1>,
    pub body_containers: Vec<AuthoringChoiceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringMoveOptionV1 {
    pub parent: String,
    pub parent_label: String,
    /// `start` | `end` | `before` | `after`.
    pub position: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sibling: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringMoveOptionsV1 {
    pub schema: String,
    pub revision: String,
    pub target: String,
    pub placements: Vec<AuthoringMoveOptionV1>,
}

/// pending `.chg` 的可觀測狀態與 replay 預覽。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingChangeV1 {
    pub schema: String,
    pub source: String,
    pub statements: usize,
    pub diff: DiffSummaryV1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiffSummaryV1 {
    pub aligned: usize,
    pub born: usize,
    pub died: usize,
    pub phon: usize,
    pub syn: usize,
    pub sem: usize,
    pub prag: usize,
    pub structural: usize,
}

impl From<conlang_changeset::diff::DiffVector> for DiffSummaryV1 {
    fn from(value: conlang_changeset::diff::DiffVector) -> Self {
        Self {
            aligned: value.aligned,
            born: value.born,
            died: value.died,
            phon: value.phon,
            syn: value.syn,
            sem: value.sem,
            prag: value.prag,
            structural: value.structural,
        }
    }
}

/// 現行語言快照的 expert view。它是 canonical `.lang`，不是第二份格式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceViewV1 {
    pub schema: String,
    pub node: String,
    pub source: String,
}

/// 專家編輯器把 `.lang` 轉成 pending `.chg` 後的 identity 對帳報告。
///
/// 這是新的獨立 V1 DTO，不改動既有 `SourceViewV1` 的 wire shape。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReconcileV1 {
    pub schema: String,
    pub matched: usize,
    pub inserted: usize,
    pub deleted: usize,
    pub primitive_edits: usize,
    pub pending: PendingChangeV1,
}

/// Rebase 永遠先 preview；只有 `status == "clean"` 才能由確認命令套用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebasePreviewV1 {
    pub schema: String,
    pub node: String,
    pub onto: String,
    /// `clean` | `conflict` | `environment` | `broken`。
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentStatV1 {
    pub segment: String,
    pub count: f64,
}

/// 音素投影只是一份報表；`sampling_source` 永遠是 false。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatsViewV1 {
    pub schema: String,
    pub node: String,
    pub segmentation: String,
    pub sampling_source: bool,
    pub segments: Vec<SegmentStatV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalV1 {
    pub phon: String,
    pub score: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalsViewV1 {
    pub schema: String,
    pub node: String,
    pub proposals: Vec<ProposalV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntelligibilityViewV1 {
    pub schema: String,
    pub source: String,
    pub target: String,
    pub score: IntelligibilityScore,
    pub diff: DiffSummaryV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivationViewV1 {
    pub schema: String,
    pub node: String,
    pub family: DerivationDag,
}

/// 前端用來編輯 grouping override 的穩定形狀。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupingOverrideV1 {
    #[serde(default)]
    pub assignments: BTreeMap<String, String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}
