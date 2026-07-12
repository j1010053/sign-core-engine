//! 旋律層(時間乘客):自體段 + 聯結邊。
//!
//! 單一資訊源:聯結邊只存在於 `Autoseg::links`(自體段不另存「我掛在哪」的副本);
//! 序列位置(D6 keep-in-place 的「原位」)就是 `MelodyTier::seq` 的索引,不另存欄位。

use smallvec::SmallVec;

use super::intern::{SymId, ValId};
use super::prosody::{AnchorRef, Level};

/// 一個自體段的聯結邊集合。幾乎恆為 0–2 條(浮游 0、單掛 1、延展/多掛 2),
/// 故內聯儲存兩條免堆配置(I3:替換原 `Vec<AnchorRef>` 佔位)。
pub type Links = SmallVec<[AnchorRef; 2]>;

/// 可見性(B6/D6 相關;經架構書同步,為 extraprosodicity 預留的通用機制)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    /// 預設:selector 只見已聯結者,浮游須顯式 `&floating`。
    #[default]
    LinkedOnly,
    All,
}

/// 錨點被刪時的行為(D14)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnAnchorLoss {
    #[default]
    Float,
    Delete,
}

/// 浮游無宿主的行為(D6)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnStray {
    #[default]
    KeepInPlace,
    Delete,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierPolicies {
    pub on_anchor_loss: OnAnchorLoss,
    pub on_stray: OnStray,
    /// `ocp merge`:同層相鄰同值自動合併(預設關)。
    pub ocp_merge: bool,
}

impl Default for TierPolicies {
    fn default() -> Self {
        TierPolicies {
            on_anchor_loss: OnAnchorLoss::Float,
            on_stray: OnStray::KeepInPlace,
            ocp_merge: false,
        }
    }
}

/// 自體段。`links` 為空 = 浮游(合法、可長期,D6);多條 = 延展。
/// links 維持依 (level 固定, index 遞增) 排序,便於 NCC 檢查與 notation。
#[derive(Debug, Clone, PartialEq)]
pub struct Autoseg {
    pub val: ValId,
    pub links: Links,
    /// 原位記憶(I11 v2):浮游時在錨點軸上的原位——insert near 時寫入、
    /// 錨點刪除浮游化(D14)時由 commit 寫入舊位;是 D6「不漂移」的位置記憶,
    /// 非聯結資訊複本。dock 投影優先取此值。
    pub origin: Option<u32>,
}

impl Autoseg {
    pub fn floating(val: ValId) -> Self {
        Autoseg {
            val,
            links: Links::new(),
            origin: None,
        }
    }

    pub fn linked(val: ValId, anchors: impl IntoIterator<Item = AnchorRef>) -> Self {
        Autoseg {
            val,
            links: anchors.into_iter().collect(),
            origin: None,
        }
    }

    pub fn is_floating(&self) -> bool {
        self.links.is_empty()
    }

    /// 延展(一段多錨)。
    pub fn is_spread(&self) -> bool {
        self.links.len() > 1
    }
}

/// 一條旋律層。字母表為值集合;實際狀態空間 = 字母表 ∪ {Ø}(D12:Ø = 錨點無邊,不是符號)。
#[derive(Debug, Clone, PartialEq)]
pub struct MelodyTier {
    pub name: SymId,
    /// 乘客預設停靠的軌道層(anchor,語法規格 §6)。
    pub anchor: Level,
    pub alphabet: Vec<ValId>,
    /// 自體段的固有序列(浮游者留原位,D6)。
    pub seq: Vec<Autoseg>,
    pub visible: Visibility,
    pub policies: TierPolicies,
}

impl MelodyTier {
    pub fn new(name: SymId, anchor: Level, alphabet: Vec<ValId>) -> Self {
        MelodyTier {
            name,
            anchor,
            alphabet,
            seq: Vec::new(),
            visible: Visibility::default(),
            policies: TierPolicies::default(),
        }
    }

    pub fn in_alphabet(&self, v: ValId) -> bool {
        self.alphabet.contains(&v)
    }

    /// 指向某錨點的所有自體段(seq 索引);len>1 = 多承載(輪廓,D27 合法)。
    pub fn bearers_of(&self, anchor: AnchorRef) -> Vec<usize> {
        self.seq
            .iter()
            .enumerate()
            .filter(|(_, a)| a.links.contains(&anchor))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn floating_indices(&self) -> Vec<usize> {
        self.seq
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_floating())
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier() -> MelodyTier {
        // 測試不經 Env,直接造 id
        MelodyTier::new(SymId(0), Level::Mora, vec![ValId(0), ValId(1), ValId(2)])
    }

    #[test]
    fn floating_and_spread_states() {
        let mut t = tier();
        t.seq.push(Autoseg::floating(ValId(0))); // (H)@0
        t.seq.push(Autoseg::linked(
            ValId(1),
            vec![
                AnchorRef::new(Level::Mora, 0),
                AnchorRef::new(Level::Mora, 1),
            ],
        )); // L~μ0~μ1 延展
        assert!(t.seq[0].is_floating());
        assert!(!t.seq[1].is_floating());
        assert!(t.seq[1].is_spread());
        assert_eq!(t.floating_indices(), vec![0]);
    }

    #[test]
    fn contour_is_multi_bearer() {
        let mut t = tier();
        let m0 = AnchorRef::new(Level::Mora, 0);
        t.seq.push(Autoseg::linked(ValId(0), vec![m0])); // H~μ0
        t.seq.push(Autoseg::linked(ValId(2), vec![m0])); // L~μ0 → 輪廓(D27 合法)
        assert_eq!(t.bearers_of(m0), vec![0, 1]);
    }

    #[test]
    fn defaults_match_decisions() {
        let t = tier();
        assert_eq!(t.visible, Visibility::LinkedOnly); // B6
        assert_eq!(t.policies.on_anchor_loss, OnAnchorLoss::Float); // D14
        assert_eq!(t.policies.on_stray, OnStray::KeepInPlace); // D6
        assert!(!t.policies.ocp_merge);
    }
}
