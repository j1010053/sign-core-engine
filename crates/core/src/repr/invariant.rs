//! 表徵不變量檢查(proptest 與 Validation 階段共用;執行語意 §1 步驟 4 的基礎)。
//!
//! 回報為資料非列印(可移植性規範);嚴重度對齊執行語意 §7 分級。
//! NCC 為軟約束(D7):此處只偵測,升級與否由呼叫端依 `strict-ncc` 決定。

use super::intern::SymId;
use super::melody::MelodyTier;
use super::prosody::{AnchorRef, Level, Span};
use super::word::Word;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warn,
    Info,
}

/// 檢查發現的問題。`(seq, anchor)` 皆為索引對。
#[derive(Debug, Clone, PartialEq)]
pub enum InvariantIssue {
    /// NCC 交叉(D7,預設 warn):兩條邊 (seq_a, anchor_a) 與 (seq_b, anchor_b) 交叉。
    NccCrossing {
        tier: SymId,
        a: (usize, u32),
        b: (usize, u32),
    },
    /// 聯結指向不存在的錨點(error:表徵損壞)。
    DanglingLink {
        tier: SymId,
        seq_idx: usize,
        anchor: AnchorRef,
    },
    /// 聯結層級與 tier 的 anchor 層不符(error)。
    AnchorLevelMismatch {
        tier: SymId,
        seq_idx: usize,
        expected: Level,
        got: Level,
    },
    /// 自體段值不在該層字母表(error)。
    ValueNotInAlphabet { tier: SymId, seq_idx: usize },
    /// 空韻律節點(info:暫態病理結構,I8;等待 dominate 修復)。
    EmptyProsodyNode { level: Level, index: usize },
    /// 某層 Span 未依 (lo, hi) 排序(error)。
    NotSorted { level: Level, index: usize },
    /// 音節重疊(error:D24 排除 ambisyllabicity)。
    SyllableOverlap { a: usize, b: usize },
    /// 音節層對骨架覆蓋有缺口(warn:剖析未完成或骨架已變動)。
    SyllableCoverageGap { at: u32 },
    /// 非空莫拉未落在任何音節內(warn:I8 包含關係)。
    MoraOutsideSyllable { mora: usize },
    /// 上層 Span 超出下層長度(error)。
    SpanOutOfRange { level: Level, index: usize },
}

impl InvariantIssue {
    pub fn severity(&self) -> Severity {
        use InvariantIssue::*;
        match self {
            NccCrossing { .. } => Severity::Warn, // D7 預設;strict-ncc 由呼叫端升級
            DanglingLink { .. }
            | AnchorLevelMismatch { .. }
            | ValueNotInAlphabet { .. }
            | NotSorted { .. }
            | SyllableOverlap { .. }
            | SpanOutOfRange { .. } => Severity::Error,
            SyllableCoverageGap { .. } | MoraOutsideSyllable { .. } => Severity::Warn,
            EmptyProsodyNode { .. } => Severity::Info,
        }
    }
}

/// 全詞不變量檢查。
pub fn check_word(w: &Word) -> Vec<InvariantIssue> {
    let mut out = Vec::new();
    check_prosody(w, &mut out);
    for tier in &w.melodies {
        check_melody(w, tier, &mut out);
    }
    out
}

fn check_melody(w: &Word, tier: &MelodyTier, out: &mut Vec<InvariantIssue>) {
    // 邊表:(seq_idx, anchor_index),只收層級正確且未懸空的邊
    let mut edges: Vec<(usize, u32)> = Vec::new();

    for (si, a) in tier.seq.iter().enumerate() {
        if !tier.in_alphabet(a.val) {
            out.push(InvariantIssue::ValueNotInAlphabet {
                tier: tier.name,
                seq_idx: si,
            });
        }
        for &link in &a.links {
            if link.level != tier.anchor {
                out.push(InvariantIssue::AnchorLevelMismatch {
                    tier: tier.name,
                    seq_idx: si,
                    expected: tier.anchor,
                    got: link.level,
                });
                continue;
            }
            if !w.anchor_exists(link) {
                out.push(InvariantIssue::DanglingLink {
                    tier: tier.name,
                    seq_idx: si,
                    anchor: link,
                });
                continue;
            }
            edges.push((si, link.index));
        }
    }

    // NCC:邊 (s1,a1)、(s2,a2) 交叉 iff (s1<s2 ∧ a1>a2) ∨ (s1>s2 ∧ a1<a2)。
    // 同一自體段多邊(延展)s 相同、同錨多承載(輪廓)a 相同,皆不構成交叉——
    // 正好使 D27 共同著陸與 spread 延展天然合法。詞極小,O(n²) 足矣。
    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            let (s1, a1) = edges[i];
            let (s2, a2) = edges[j];
            let crossing = (s1 < s2 && a1 > a2) || (s1 > s2 && a1 < a2);
            if crossing {
                out.push(InvariantIssue::NccCrossing {
                    tier: tier.name,
                    a: edges[i],
                    b: edges[j],
                });
            }
        }
    }
}

fn check_prosody(w: &Word, out: &mut Vec<InvariantIssue>) {
    let seg_len = w.skeleton.len() as u32;

    check_layer_common(Level::Mora, &w.prosody.moras, seg_len, out);
    check_layer_common(Level::Syllable, &w.prosody.syllables, seg_len, out);
    check_layer_common(
        Level::Foot,
        &w.prosody.feet,
        w.prosody.syllables.len() as u32,
        out,
    );
    check_layer_common(
        Level::Pword,
        &w.prosody.pwords,
        w.prosody.feet.len() as u32,
        out,
    );

    // 音節層特有:不重疊(D24)+ 對骨架全覆蓋(有音節存在時才要求)
    let syls = &w.prosody.syllables;
    for i in 1..syls.len() {
        if syls[i - 1].overlaps(&syls[i]) {
            out.push(InvariantIssue::SyllableOverlap { a: i - 1, b: i });
        }
    }
    if !syls.is_empty() {
        let mut expect = 0u32;
        for s in syls {
            if s.lo > expect {
                out.push(InvariantIssue::SyllableCoverageGap { at: expect });
            }
            expect = expect.max(s.hi);
        }
        if expect < seg_len {
            out.push(InvariantIssue::SyllableCoverageGap { at: expect });
        }
    }

    // 莫拉層特有(I8):部分覆蓋合法、重疊合法(長元音);
    // 非空莫拉須落在某音節內(有音節存在時才要求)。
    if !syls.is_empty() {
        for (i, m) in w.prosody.moras.iter().enumerate() {
            if m.is_empty_node() {
                continue;
            }
            let inside = syls.iter().any(|s| m.within(s));
            if !inside {
                out.push(InvariantIssue::MoraOutsideSyllable { mora: i });
            }
        }
    }
}

/// 各層共通:排序、範圍、空節點回報。
fn check_layer_common(
    level: Level,
    spans: &[Span],
    lower_len: u32,
    out: &mut Vec<InvariantIssue>,
) {
    for (i, s) in spans.iter().enumerate() {
        if i > 0 {
            let p = &spans[i - 1];
            if (s.lo, s.hi) < (p.lo, p.hi) {
                out.push(InvariantIssue::NotSorted { level, index: i });
            }
        }
        if s.hi > lower_len || s.lo > s.hi {
            out.push(InvariantIssue::SpanOutOfRange { level, index: i });
        }
        if s.is_empty_node() {
            out.push(InvariantIssue::EmptyProsodyNode { level, index: i });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::feature::FeatBits;
    use crate::repr::intern::{SymId, ValId};
    use crate::repr::melody::{Autoseg, MelodyTier};
    use crate::repr::word::Seg;

    fn seg(i: u32) -> Seg {
        Seg::new(SymId(i), FeatBits::EMPTY)
    }

    /// 造一個 2 音段、1 音節、1 莫拉的乾淨詞("ba" 形)。
    fn clean_word() -> Word {
        let mut w = Word::new();
        w.skeleton.push(seg(0)); // b
        w.skeleton.push(seg(1)); // a
        w.prosody.syllables.push(Span::new(0, 2));
        w.prosody.moras.push(Span::new(1, 2)); // onset 不入莫拉(I8)
        w
    }

    fn tone_tier() -> MelodyTier {
        MelodyTier::new(SymId(100), Level::Mora, vec![ValId(0), ValId(1)])
    }

    #[test]
    fn clean_word_has_no_issues() {
        let w = clean_word();
        assert!(check_word(&w).is_empty());
    }

    #[test]
    fn ncc_detects_crossing_and_permits_spread_and_contour() {
        let mut w = clean_word();
        // 擴成兩個莫拉:μ0=[1,2) μ1=[1,2)(長元音式重疊,I8 合法)
        w.prosody.moras.push(Span::new(1, 2));
        let mut t = tone_tier();
        // 交叉:seq0→μ1, seq1→μ0
        t.seq.push(Autoseg::linked(
            ValId(0),
            vec![AnchorRef::new(Level::Mora, 1)],
        ));
        t.seq.push(Autoseg::linked(
            ValId(1),
            vec![AnchorRef::new(Level::Mora, 0)],
        ));
        w.melodies.push(t);
        let issues = check_word(&w);
        assert!(issues
            .iter()
            .any(|i| matches!(i, InvariantIssue::NccCrossing { .. })));

        // 延展(同段兩邊)與輪廓(兩段同錨)皆不觸發 NCC
        let mut w2 = clean_word();
        w2.prosody.moras.push(Span::new(1, 2));
        let mut t2 = tone_tier();
        t2.seq.push(Autoseg::linked(
            ValId(0),
            vec![
                AnchorRef::new(Level::Mora, 0),
                AnchorRef::new(Level::Mora, 1),
            ],
        )); // 延展
        t2.seq.push(Autoseg::linked(
            ValId(1),
            vec![AnchorRef::new(Level::Mora, 1)],
        )); // 與延展尾端同錨 = 輪廓
        w2.melodies.push(t2);
        let issues2 = check_word(&w2);
        assert!(!issues2
            .iter()
            .any(|i| matches!(i, InvariantIssue::NccCrossing { .. })));
    }

    #[test]
    fn dangling_mismatch_and_alphabet() {
        let mut w = clean_word();
        let mut t = tone_tier();
        t.seq.push(Autoseg::linked(
            ValId(0),
            vec![AnchorRef::new(Level::Mora, 9)], // 懸空
        ));
        t.seq.push(Autoseg::linked(
            ValId(1),
            vec![AnchorRef::new(Level::Syllable, 0)], // 層級不符
        ));
        t.seq.push(Autoseg::floating(ValId(7))); // 字母表外
        w.melodies.push(t);
        let issues = check_word(&w);
        assert!(issues
            .iter()
            .any(|i| matches!(i, InvariantIssue::DanglingLink { .. })));
        assert!(issues
            .iter()
            .any(|i| matches!(i, InvariantIssue::AnchorLevelMismatch { .. })));
        assert!(issues
            .iter()
            .any(|i| matches!(i, InvariantIssue::ValueNotInAlphabet { .. })));
    }

    #[test]
    fn empty_mora_is_info_not_error() {
        let mut w = clean_word();
        // 模擬 coda 脫落後:空莫拉(暫態病理結構)
        w.prosody.moras.push(Span::new(2, 2));
        let issues = check_word(&w);
        let empties: Vec<_> = issues
            .iter()
            .filter(|i| matches!(i, InvariantIssue::EmptyProsodyNode { .. }))
            .collect();
        assert_eq!(empties.len(), 1);
        assert_eq!(empties[0].severity(), Severity::Info);
    }

    #[test]
    fn syllable_coverage_and_overlap() {
        let mut w = Word::new();
        for i in 0..4 {
            w.skeleton.push(seg(i));
        }
        w.prosody.syllables.push(Span::new(0, 2));
        w.prosody.syllables.push(Span::new(3, 4)); // 缺口 at 2
        let issues = check_word(&w);
        assert!(issues
            .iter()
            .any(|i| matches!(i, InvariantIssue::SyllableCoverageGap { at: 2 })));

        let mut w2 = Word::new();
        for i in 0..3 {
            w2.skeleton.push(seg(i));
        }
        w2.prosody.syllables.push(Span::new(0, 2));
        w2.prosody.syllables.push(Span::new(1, 3)); // 重疊
        let issues2 = check_word(&w2);
        assert!(issues2
            .iter()
            .any(|i| matches!(i, InvariantIssue::SyllableOverlap { .. })));
    }

    #[test]
    fn mora_outside_syllable_is_flagged() {
        let mut w = Word::new();
        for i in 0..3 {
            w.skeleton.push(seg(i));
        }
        w.prosody.syllables.push(Span::new(0, 2)); // 覆蓋缺口另計
        w.prosody.moras.push(Span::new(2, 3)); // 在音節外
        let issues = check_word(&w);
        assert!(issues
            .iter()
            .any(|i| matches!(i, InvariantIssue::MoraOutsideSyllable { mora: 0 })));
    }
}
