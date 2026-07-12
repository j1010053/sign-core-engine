//! locality:局部可達上界(執行語意 §5 的唯一實作處)。
//!
//! 一條 tier 內規則自當前錨點能唯讀觸及的範圍 = 沿聯結+支配的**可達閉包**,取:
//! (a) 錨點所屬 `locality` 韻律成分之內(預設 syllable)
//! (b) 沿該旋律層**錨定層**前後各相鄰一格
//! 的**聯集**。這是可達性邊界、不是距離度量(B7);超出者一律走 spread 的
//! within 或 Scan——locality 邊界與「誰該用 Scan」的邊界重合。
//!
//! M0 步驟 5:提供計算函數與測試;對一般 selector 的強制執行隨 DSL 通用環境
//! (步驟 6 Scan/selector 泛化)接上。現有動詞的讀取(onset 述語=音節內、
//! rewrite 環境=骨架相鄰一格)皆天然落在上界內。

use crate::repr::prosody::{AnchorRef, Level};
use crate::repr::word::Word;

/// 錨點的骨架區間 [lo, hi)。
fn seg_range(w: &Word, level: Level, idx: u32) -> (u32, u32) {
    match level {
        Level::Segment => (idx, idx.saturating_add(1)),
        _ => w
            .prosody
            .level(level)
            .and_then(|v| v.get(idx as usize))
            .map(|s| (s.lo, s.hi))
            .unwrap_or((0, 0)),
    }
}

/// 自 `from` 出發、`locality` 成分為界的可達骨架範圍(聯集後的最小涵蓋區間)。
/// 回傳 [lo, hi);`from` 懸空(不在任何成分內)時退化為錨點自身區間 ∪ 相鄰一格。
pub fn reachable_segments(w: &Word, from: AnchorRef, locality: Level) -> (u32, u32) {
    let (flo, fhi) = seg_range(w, from.level, from.index);
    let (mut lo, mut hi) = (flo, fhi);

    // (a) locality 成分之內:含 from 區間的成分
    if let Some(spans) = w.prosody.level(locality) {
        for s in spans {
            if s.lo <= flo && fhi <= s.hi && !s.is_empty_node() {
                lo = lo.min(s.lo);
                hi = hi.max(s.hi);
            }
        }
    }

    // (b) 錨定層相鄰一格(前後各一)
    if from.index > 0 {
        let (alo, _) = seg_range(w, from.level, from.index - 1);
        lo = lo.min(alo);
    }
    let n = match from.level {
        Level::Segment => w.skeleton.len() as u32,
        _ => w
            .prosody
            .level(from.level)
            .map(|v| v.len() as u32)
            .unwrap_or(0),
    };
    if from.index + 1 < n {
        let (_, ahi) = seg_range(w, from.level, from.index + 1);
        hi = hi.max(ahi);
    }

    (lo, hi.min(w.skeleton.len() as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::feature::FeatBits;
    use crate::repr::intern::SymId;
    use crate::repr::prosody::Span;
    use crate::repr::word::{Seg, Word};

    /// 六莫拉同音節:全部可達(B7 的反例免疫——成分內經支配互相可達)。
    #[test]
    fn six_moras_in_one_syllable_all_reachable_b7() {
        let mut w = Word::new();
        for i in 0..6u32 {
            w.skeleton.push(Seg::new(SymId(i), FeatBits::EMPTY));
            w.prosody.moras.push(Span::new(i, i + 1));
        }
        w.prosody.syllables.push(Span::new(0, 6));
        let r = reachable_segments(&w, AnchorRef::new(Level::Mora, 0), Level::Syllable);
        assert_eq!(r, (0, 6));
    }

    /// 相鄰一格越過成分界:CV.CV 的 μ0 可達第二音節的 μ1(聯集語意)。
    #[test]
    fn adjacent_anchor_extends_past_constituent() {
        let mut w = Word::new();
        for i in 0..4u32 {
            w.skeleton.push(Seg::new(SymId(i), FeatBits::EMPTY));
        }
        w.prosody.syllables.push(Span::new(0, 2));
        w.prosody.syllables.push(Span::new(2, 4));
        w.prosody.moras.push(Span::new(1, 2));
        w.prosody.moras.push(Span::new(3, 4));
        let r = reachable_segments(&w, AnchorRef::new(Level::Mora, 0), Level::Syllable);
        // σ0 全域 [0,2) ∪ 相鄰莫拉 μ1 的 [3,4) → 涵蓋區間 [0,4)
        assert_eq!(r, (0, 4));
    }
}
