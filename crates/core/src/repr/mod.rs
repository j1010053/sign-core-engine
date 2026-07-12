//! 表徵模組:`Word` 快照與其組成(《M0 實作參照》§3)。
//!
//! 單一資訊源(原則 3)在此模組的落實:
//! - 字串 ↔ id 對應只存在於 [`Env`] 的 interner
//! - 聯結邊只存在於 [`melody::Autoseg::links`]
//! - 序列位置只是 [`melody::MelodyTier::seq`] 的索引
//! - 韻律結構只存在於 [`prosody::ProsodyLayers`] 的 Span 序列
//! - stale 旗標只存在於 [`prosody::StaleFlags`]

pub mod domain;
pub mod feature;
pub mod intern;
pub mod invariant;
pub mod inventory;
pub mod melody;
pub mod notation;
pub mod prosody;
pub mod word;

pub use feature::{FeatBit, FeatBits, FeatureRegistry};
pub use intern::{SymId, SymTable, ValId, ValTable};
pub use domain::{DaughterLoss, DomainDef, DomainRegistry};
pub use inventory::Inventory;
pub use invariant::{check_word, InvariantIssue, Severity};
pub use melody::{Autoseg, Links, MelodyTier, OnAnchorLoss, OnStray, TierPolicies, Visibility};
pub use prosody::{AnchorRef, Level, ProsodyLayers, Span, StaleFlags};
pub use word::{Bracket, MorphUnit, Seg, Word};

/// 表徵層錯誤。核心不 panic(可移植性規範),一律回傳本型別。
/// thiserror 提供 `Display` 與 `std::error::Error`,使 `lifecycle::EngineError` 可 `#[from]` 之。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReprError {
    /// 特徵原子超過 64 個(MVP 以 u64 bitset 承載;超出時未來換 u128/Vec<u64>)。
    #[error("feature space exhausted (64 atoms max in MVP)")]
    FeatureSpaceExhausted,
    /// 引用了不存在的錨點。
    #[error("dangling anchor: {0:?}")]
    DanglingAnchor(prosody::AnchorRef),
    /// 對 Segment 層請求下層(Segment 是骨架,無下層)。
    #[error("level {0:?} has no lower level")]
    NoLowerLevel(prosody::Level),
}

/// 專案級環境:所有 interner、特徵註冊表與音素庫的單一存放處。
/// `Word` 快照只存整數 id,字串進出僅在 parse 入口與 spell-out 出口(I1/interning)。
#[derive(Debug, Default, Clone)]
pub struct Env {
    pub syms: SymTable,
    pub vals: ValTable,
    pub feats: FeatureRegistry,
    /// 音素庫(I12):符號 ↔ 特徵束;音段規則改寫後的反查來源。
    pub inv: Inventory,
    /// 韻律域註冊表(I14):內建+自定域定義的單一存放處。
    pub domains: DomainRegistry,
}

impl Env {
    pub fn new() -> Self {
        Self::default()
    }
}
