//! 表徵模組:`Word` 快照與其組成(《M0 實作參照》§3)。
//!
//! 單一資訊源(原則 3)在此模組的落實:
//! - 字串 ↔ id 對應只存在於 [`Env`] 的 interner
//! - 聯結邊只存在於 [`melody::Autoseg::links`]
//! - 序列位置只是 [`melody::MelodyTier::seq`] 的索引
//! - 韻律結構只存在於 [`prosody::ProsodyLayers`] 的 Span 序列
//! - stale 旗標只存在於 [`prosody::StaleFlags`]

pub mod feature;
pub mod intern;
pub mod invariant;
pub mod melody;
pub mod notation;
pub mod prosody;
pub mod word;

pub use feature::{FeatBit, FeatBits, FeatureRegistry};
pub use intern::{SymId, SymTable, ValId, ValTable};
pub use invariant::{check_word, InvariantIssue, Severity};
pub use melody::{Autoseg, MelodyTier, OnAnchorLoss, OnStray, TierPolicies, Visibility};
pub use prosody::{AnchorRef, Level, ProsodyLayers, Span, StaleFlags};
pub use word::{Bracket, MorphUnit, Seg, Word};

/// 表徵層錯誤。核心不 panic(可移植性規範),一律回傳本型別。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReprError {
    /// 特徵原子超過 64 個(MVP 以 u64 bitset 承載;超出時未來換 u128/Vec<u64>)。
    FeatureSpaceExhausted,
    /// 引用了不存在的錨點。
    DanglingAnchor(prosody::AnchorRef),
    /// 對 Segment 層請求下層(Segment 是骨架,無下層)。
    NoLowerLevel(prosody::Level),
}

impl core::fmt::Display for ReprError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReprError::FeatureSpaceExhausted => {
                write!(f, "feature space exhausted (64 atoms max in MVP)")
            }
            ReprError::DanglingAnchor(a) => write!(f, "dangling anchor: {:?}", a),
            ReprError::NoLowerLevel(l) => write!(f, "level {:?} has no lower level", l),
        }
    }
}

/// 專案級環境:所有 interner 與特徵註冊表的單一存放處。
/// `Word` 快照只存整數 id,字串進出僅在 parse 入口與 spell-out 出口(I1/interning)。
#[derive(Debug, Default, Clone)]
pub struct Env {
    pub syms: SymTable,
    pub vals: ValTable,
    pub feats: FeatureRegistry,
}

impl Env {
    pub fn new() -> Self {
        Self::default()
    }
}
