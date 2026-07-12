//! `Action`:六原語的資料具現(執行語意 §1 步驟 2 的產物、步驟 3 的輸入)。
//!
//! Action 是**純資料**——它描述「要做什麼」,不含「怎麼寫入」;寫入語意集中於
//! `super::commit`(單一資訊源:commit 是唯一改變表徵的地方)。所有索引(`seq_idx`、
//! `node`、`target`)都指向**規則套用前的凍結快照**(I1/I2:parallel 一次定案,commit 重編)。
//!
//! 具名動詞(spread/dock/fill/merge/parse…)不新增 variant,而是產生**六原語**的序列
//! (基礎先行,M0 §1.1 原則 2);理論宏更只是資料表。六原語清單凍結(I9)。
//!
//! `SegRewrite` **不是**第七個原語(I12):它是音段層 rewrite 規則(`A => B / C _ D`,
//! 語法貼合 Lexurgy)的專屬通道——不供具名動詞組合,僅由音段規則匹配產生,
//! 走同一 commit 管線(執行語意 §1 對所有規則一體適用)。骨架長度不變;
//! 音段增/刪與其跨層連鎖留步驟 5(I10)。

use crate::repr::feature::FeatBits;
use crate::repr::intern::{SymId, ValId};
use crate::repr::melody::Links;
use crate::repr::prosody::{AnchorRef, Level};

/// 六原語 + 音段規則通道(I12)。前四者操作旋律 tier(`Autoseg`),
/// dominate/release 操作韻律結構(`Span`),`SegRewrite` 替換骨架音段。
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // ── 旋律原語(旋律層:時間乘客;D2 聯結動詞家族的原子)──
    /// 加一條聯結邊:`tier` 的第 `seq_idx` 個自體段 → `anchor`。冪等(邊已存在則 noop)。
    Associate {
        tier: SymId,
        seq_idx: usize,
        anchor: AnchorRef,
    },
    /// 減一條聯結邊。邊不存在則 noop(冪等)。刪至零邊 = 該自體段回浮游(D6/D14)。
    Delink {
        tier: SymId,
        seq_idx: usize,
        anchor: AnchorRef,
    },
    /// 造一個新自體段。`at=None` 追加序列尾;`at=Some(i)` 插在前狀態第 `i` 項之前。
    /// `links` 為空 = 浮游(tonogenesis 的 `insert (L)`);非空 = 生成即掛。
    Insert {
        tier: SymId,
        at: Option<usize>,
        val: ValId,
        links: Links,
        /// 浮游原位記憶(I11 v2;`insert … near X` 寫入 X)。
        origin: Option<u32>,
    },
    /// 刪一個自體段(連同其所有聯結邊)。commit 重建 `seq` 收攏後續索引(I10)。
    Delete { tier: SymId, seq_idx: usize },

    // ── 韻律原語(韻律層:時間軌道;D2 支配動詞家族的原子;§2 標 repair)──
    /// 結構修復:令 `level` 層第 `node` 個節點的 Span **涵蓋**下層索引 `target`
    /// (最小連續擴展:`lo'=min(lo,target)`、`hi'=max(hi,target+1)`)。
    /// 補償性延長即空莫拉 `dominate` 向元音重掛。`parse` = 批次 dominate。
    Dominate {
        level: Level,
        node: usize,
        target: u32,
    },
    /// 釋放:令 `level` 層第 `node` 個節點的 Span **不再涵蓋** `target`(端點縮一格)。
    /// `target` 在區間內部會使連續 Span 斷裂 → `EngineError::ReleaseWouldSplit`(I2)。
    Release {
        level: Level,
        node: usize,
        target: u32,
    },

    // ── 音段規則通道(I12;非原語,見模組說明)──
    /// 整段替換骨架第 `idx` 個音段(符號 + 特徵束;長度不變,錨點/聯結皆穩定)。
    /// 由音段層 rewrite 規則產生;`sym` 已由 Inventory 反查完畢(無對應=規則層 error)。
    SegRewrite {
        idx: usize,
        sym: SymId,
        feats: FeatBits,
    },
    /// 刪除骨架第 `idx` 個音段,觸發跨層連鎖重編(I13):Span 平移、無核心音節清理、
    /// 旋律 links 重映射、on-anchor-loss float(D14)、stale 標記。commit 最後套用。
    SegDelete { idx: usize },
}

impl Action {
    /// 旋律 Action 所屬的 tier;其餘回 `None`。commit 依此分組。
    pub fn tier(&self) -> Option<SymId> {
        match *self {
            Action::Associate { tier, .. }
            | Action::Delink { tier, .. }
            | Action::Insert { tier, .. }
            | Action::Delete { tier, .. } => Some(tier),
            Action::Dominate { .. }
            | Action::Release { .. }
            | Action::SegRewrite { .. }
            | Action::SegDelete { .. } => None,
        }
    }

    /// 是否為旋律層原語(operate on `Autoseg`)。
    pub fn is_melodic(&self) -> bool {
        self.tier().is_some()
    }

    /// 是否為韻律層原語(operate on `Span`;§2 的 repair 類)。
    pub fn is_prosodic(&self) -> bool {
        matches!(self, Action::Dominate { .. } | Action::Release { .. })
    }

    /// 是否為音段規則通道(I12/I13)。
    pub fn is_segmental(&self) -> bool {
        matches!(self, Action::SegRewrite { .. } | Action::SegDelete { .. })
    }
}
