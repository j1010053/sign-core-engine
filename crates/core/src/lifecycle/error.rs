//! 引擎級錯誤(執行語意 §7 的 error 級之型別載體)。
//!
//! 可移植性規範:核心不 panic;結構性錯誤走 `Result<_, EngineError>`,
//! 而**良構診斷**(NCC 交叉、空節點…)不是錯誤,是 `validate` 回傳的資料(見 `super::validate`)。
//! 兩者分工:`EngineError` = 「這個 Action 無法套用」(索引越界、tier 不存在);
//! `InvariantIssue` = 「套用後表徵的健康狀態」(分級,多數可續跑)。

use crate::repr::intern::SymId;
use crate::repr::prosody::Level;
use crate::repr::ReprError;

/// 生命週期執行期錯誤:一個 Action 因指向不存在的目標或違反結構前提而無法 commit。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    /// 由表徵層上浮(如特徵空間耗盡)。
    #[error(transparent)]
    Repr(#[from] ReprError),

    /// Action 指名的 tier 不在 `Word::melodies` 內。
    #[error("tier {0:?} not found in word")]
    TierNotFound(SymId),

    /// 旋律 Action 的 `seq_idx` 超出該 tier 的自體段序列長度(索引指凍結前狀態)。
    #[error("seq index {idx} out of range in tier {tier:?} (len {len})")]
    SeqIndexOutOfRange {
        tier: SymId,
        idx: usize,
        len: usize,
    },

    /// 韻律 Action 指名的節點超出該層 Span 數。
    #[error("prosody node {node} out of range at level {level:?} (len {len})")]
    NodeOutOfRange {
        level: Level,
        node: usize,
        len: usize,
    },

    /// 韻律層級不接受支配 Action(對 `Segment` 這種無 Span 向量的層請求 dominate/release)。
    #[error("level {0:?} carries no prosodic node vector (cannot dominate/release)")]
    NotAProsodyLevel(Level),

    /// `Insert` 的插入位置 `at` 超出序列長度 +1(允許等於 len = append)。
    #[error("insert position {at} out of range in tier {tier:?} (len {len})")]
    InsertPosOutOfRange {
        tier: SymId,
        at: usize,
        len: usize,
    },

    /// `Release` 的 target 落在區間內部,縮減會把連續 Span 一分為二(韻律=連續區間,I2)。
    /// 中間挖洞非原語職責;呼叫端應改以兩個節點表達,或此為規則設計錯。
    #[error("release target {target} is interior to node {node} at level {level:?}; would split span")]
    ReleaseWouldSplit {
        level: Level,
        node: usize,
        target: u32,
    },

    /// `SegRewrite` 指向不存在的骨架音段(I12)。
    #[error("segment index {idx} out of range (skeleton len {len})")]
    SegIndexOutOfRange { idx: usize, len: usize },

    /// 音段規則改寫後的特徵束在音素庫無對應符號(I12,對齊 Lexurgy 行為)。
    #[error("no symbol in inventory for rewritten feature bundle at segment {idx}")]
    NoSymbolForBundle { idx: usize },
}
