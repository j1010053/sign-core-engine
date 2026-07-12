//! lifecycle:規則生命週期(執行語意 §1 的**唯一**實作處)。
//!
//! ```text
//! Rule
//!   ├─ 1. Parallel Match      凍結前狀態,一次找出所有匹配        ← verbs/scan(步驟 3+)供給 actions
//!   ├─ 2. Action Evaluation   算出每個匹配的 Action(iterative 在此展開) ← 同上
//!   ├─ 3. Commit              一次寫入 → 新 Word                    ← `commit`
//!   ├─ 4. Validation          新狀態良構檢查(NCC…),產生分級診斷    ← `validate`
//!   └─ 5. Lazy Reparse Mark   query 類遇 stale 觸發重剖;repair 跳過   ← `needs_reparse`
//! ```
//!
//! M0 步驟 2 實作步驟 3–5(有實質邏輯處);步驟 1–2 尚無 selector,`actions` 由呼叫端提供
//! (測試、未來 verbs/scan)。單一資訊源:改變表徵只在 `commit`,良構判斷只在 `validate`。

pub mod action;
pub mod commit;
pub mod error;

pub use action::Action;
pub use commit::commit;
pub use error::EngineError;

use crate::repr::invariant::{check_word, InvariantIssue, Severity};
use crate::repr::word::Word;

/// 動詞執行語意類別(執行語意 §2/§3;決定步驟 5 的 reparse 行為)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbClass {
    /// 讀韻律結構的查詢類(讀 `<syl>`、序數…):commit 後遇 stale 結構觸發 lazy 重剖。
    Query,
    /// 修復類(dominate/release):**不**觸發 reparse,故看見 stale 舊結構(A3)。
    Repair,
    /// 純旋律變動(associate/insert…):不讀韻律,不涉重剖。
    Melodic,
}

/// 步驟 4:Validation。分級良構檢查(NCC 等),回報為資料(可移植性規範,非列印)。
pub fn validate(word: &Word) -> Vec<InvariantIssue> {
    check_word(word)
}

/// 是否含 error 級問題——呼叫端據此決定「停止該詞演化」(執行語意 §7)。
/// NCC 等 warn/info 不在此列(D7 軟約束:中間態放任,出口才由 `strict-ncc` 升級)。
pub fn has_error(issues: &[InvariantIssue]) -> bool {
    issues.iter().any(|i| i.severity() == Severity::Error)
}

/// 步驟 5:Lazy Reparse Mark。回傳「本步驟是否需要重剖」。
/// repair 類永遠跳過(A3);query 類遇 stale 才需重剖(重剖本身 = 批次 dominate,屬步驟 5+)。
pub fn needs_reparse(word: &Word, class: VerbClass) -> bool {
    matches!(class, VerbClass::Query) && word.stale.any()
}

/// 一條規則跑完步驟 3–5 的產出。
#[derive(Debug, Clone)]
pub struct Outcome {
    pub word: Word,
    pub issues: Vec<InvariantIssue>,
    /// query 類遇 stale:步驟 5+ 需先重剖再查(步驟 2 只標記,不執行重剖)。
    pub reparse_pending: bool,
}

/// 執行語意 §1 步驟 3→4→5 的編排。步驟 1–2 由呼叫端算好 `actions` 傳入(見模組說明)。
pub fn run(before: &Word, actions: &[Action], class: VerbClass) -> Result<Outcome, EngineError> {
    let word = commit(before, actions)?; // 步驟 3
    let issues = validate(&word); // 步驟 4
    let reparse_pending = needs_reparse(&word, class); // 步驟 5
    Ok(Outcome {
        word,
        issues,
        reparse_pending,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::feature::FeatBits;
    use crate::repr::intern::{SymId, ValId};
    use crate::repr::melody::{Autoseg, MelodyTier};
    use crate::repr::notation;
    use crate::repr::prosody::{AnchorRef, Level, Span};
    use crate::repr::word::Seg;

    const TONE: SymId = SymId(100);

    /// 造 "a"(1 音段、1 音節、1 莫拉)+ 空 tone tier(alphabet {H=0, L=1, M=2})。
    fn word_with_tier() -> (Word, crate::repr::intern::ValTable) {
        let mut vals = crate::repr::intern::ValTable::default();
        let _h = vals.intern("H");
        let _l = vals.intern("L");
        let _m = vals.intern("M");
        let mut w = Word::new();
        w.skeleton.push(Seg::new(SymId(0), FeatBits::EMPTY));
        w.prosody.syllables.push(Span::new(0, 1));
        w.prosody.moras.push(Span::new(0, 1));
        w.melodies.push(MelodyTier::new(
            TONE,
            Level::Mora,
            vec![ValId(0), ValId(1), ValId(2)],
        ));
        (w, vals)
    }

    #[test]
    fn insert_floating_then_associate_matches_8_1() {
        let (w, vals) = word_with_tier();
        // insert 浮游 L(val 1)
        let w = commit(
            &w,
            &[Action::Insert {
                tier: TONE,
                at: None,
                val: ValId(1),
                links: Default::default(),
            }],
        )
        .unwrap();
        assert_eq!(notation::render_tier(w.tier(TONE).unwrap(), &vals), "(L)@0");
        // associate seq0 → μ0(dock)
        let w = commit(
            &w,
            &[Action::Associate {
                tier: TONE,
                seq_idx: 0,
                anchor: AnchorRef::new(Level::Mora, 0),
            }],
        )
        .unwrap();
        assert_eq!(notation::render_tier(w.tier(TONE).unwrap(), &vals), "L~μ0");
    }

    /// I10 核心:同批 delete seq0 + associate seq2,parallel 讀前狀態,commit 後重建收攏索引。
    #[test]
    fn delete_and_associate_reindex_by_frozen_state() {
        let (mut w, _vals) = word_with_tier();
        // 前狀態 seq = [H(0), L(1), M(2)] 全浮游
        {
            let t = w.tier_mut(TONE).unwrap();
            t.seq.push(Autoseg::floating(ValId(0)));
            t.seq.push(Autoseg::floating(ValId(1)));
            t.seq.push(Autoseg::floating(ValId(2)));
        }
        let m0 = AnchorRef::new(Level::Mora, 0);
        let w = commit(
            &w,
            &[
                Action::Associate {
                    tier: TONE,
                    seq_idx: 2, // 指前狀態的 M
                    anchor: m0,
                },
                Action::Delete {
                    tier: TONE,
                    seq_idx: 0, // 指前狀態的 H
                },
            ],
        )
        .unwrap();
        let t = w.tier(TONE).unwrap();
        // 刪 H 後剩 [L, M];M 現於 index 1 且帶 μ0 邊
        assert_eq!(t.seq.len(), 2);
        assert_eq!(t.seq[0].val, ValId(1)); // L
        assert_eq!(t.seq[1].val, ValId(2)); // M
        assert!(t.seq[1].links.contains(&m0));
        assert!(t.seq[0].is_floating());
    }

    #[test]
    fn insert_at_position_respects_frozen_index() {
        let (mut w, _vals) = word_with_tier();
        {
            let t = w.tier_mut(TONE).unwrap();
            t.seq.push(Autoseg::floating(ValId(0))); // H @0
            t.seq.push(Autoseg::floating(ValId(2))); // M @1
        }
        // 在前狀態第 1 項(M)之前插入 L
        let w = commit(
            &w,
            &[Action::Insert {
                tier: TONE,
                at: Some(1),
                val: ValId(1),
                links: Default::default(),
            }],
        )
        .unwrap();
        let vals: Vec<_> = w.tier(TONE).unwrap().seq.iter().map(|a| a.val).collect();
        assert_eq!(vals, vec![ValId(0), ValId(1), ValId(2)]); // H L M
    }

    /// 8.4:空莫拉 dominate 向元音重掛 → 涵蓋 seg 0(長元音的第二莫拉)。
    #[test]
    fn dominate_empty_mora_covers_vowel() {
        let mut w = Word::new();
        w.skeleton.push(Seg::new(SymId(0), FeatBits::EMPTY)); // a
        w.prosody.syllables.push(Span::new(0, 1));
        w.prosody.moras.push(Span::new(0, 1)); // μ0 核心
        w.prosody.moras.push(Span::new(1, 1)); // μ1 空節點(coda 脫落後)
        let w = commit(
            &w,
            &[Action::Dominate {
                level: Level::Mora,
                node: 1,
                target: 0, // 向左重掛到 a
            }],
        )
        .unwrap();
        assert_eq!(notation::render_prosody(&w), "σ0[0,1) μ0[0,1) μ1[0,1)");
        // a 現有兩個莫拉承載 → 長元音
        assert_eq!(
            w.prosody.moras.iter().filter(|m| m.contains_idx(0)).count(),
            2
        );
    }

    #[test]
    fn release_shrinks_end_and_errors_on_interior() {
        let mut w = Word::new();
        for i in 0..3 {
            w.skeleton.push(Seg::new(SymId(i), FeatBits::EMPTY));
        }
        w.prosody.moras.push(Span::new(0, 3)); // 涵蓋 seg 0,1,2
        // 縮右端:release target 2 → [0,3)→[0,2)
        let w2 = commit(
            &w,
            &[Action::Release {
                level: Level::Mora,
                node: 0,
                target: 2,
            }],
        )
        .unwrap();
        assert_eq!(w2.prosody.moras[0], Span::new(0, 2));
        // 內部 target 1 → 斷裂錯誤
        let err = commit(
            &w,
            &[Action::Release {
                level: Level::Mora,
                node: 0,
                target: 1,
            }],
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::ReleaseWouldSplit { .. }));
    }

    #[test]
    fn out_of_range_and_missing_tier_error() {
        let (w, _vals) = word_with_tier();
        // tier 不存在
        assert!(matches!(
            commit(
                &w,
                &[Action::Delete {
                    tier: SymId(999),
                    seq_idx: 0
                }]
            ),
            Err(EngineError::TierNotFound(_))
        ));
        // seq 越界(tier 空)
        assert!(matches!(
            commit(
                &w,
                &[Action::Associate {
                    tier: TONE,
                    seq_idx: 0,
                    anchor: AnchorRef::new(Level::Mora, 0)
                }]
            ),
            Err(EngineError::SeqIndexOutOfRange { .. })
        ));
        // 對 Segment 層 dominate
        assert!(matches!(
            commit(
                &w,
                &[Action::Dominate {
                    level: Level::Segment,
                    node: 0,
                    target: 0
                }]
            ),
            Err(EngineError::NotAProsodyLevel(_))
        ));
    }

    #[test]
    fn repair_class_never_triggers_reparse() {
        let (mut w, _vals) = word_with_tier();
        w.stale.mark(Level::Mora); // 結構 stale
        assert!(!needs_reparse(&w, VerbClass::Repair)); // A3:repair 看 stale 舊結構
        assert!(!needs_reparse(&w, VerbClass::Melodic));
        assert!(needs_reparse(&w, VerbClass::Query)); // query 遇 stale 才重剖
    }
}
