//! primitives:六原語(M0 §1.1 原則 2「基礎先行」的基底)。
//!
//! 每個原語 = 一個建構 [`Action`] 的純函數 + 語意契約(本檔文件)+ 不變量(proptest,見 tests)。
//! 具名動詞(spread / dock / fill / merge / shift / parse…)一律由這六個組合
//! (`verbs/`,步驟 3+),不得另闢狀態;寫入語意唯一住在 `lifecycle::commit`
//! (單一資訊源)——本模組**不碰 `Word`**,只產生描述。
//!
//! | 原語 | 家族(D2) | 語意(§2) | 契約摘要 |
//! |---|---|---|---|
//! | `associate` | 聯結 | parallel | 加邊;冪等;不改自體段數 |
//! | `delink` | 聯結 | parallel | 減邊;冪等;刪至零邊=回浮游(D6/D14) |
//! | `insert` | 聯結 | parallel | 造自體段;`links` 空=浮游(D12 哲學:Ø=無邊) |
//! | `delete` | 聯結 | parallel | 刪自體段連同其邊;commit 收攏索引(I10) |
//! | `dominate` | 支配 | parallel(repair) | Span 最小連續擴展涵蓋 target;不觸發 reparse(A3) |
//! | `release` | 支配 | parallel(repair) | Span 端點縮一格;內部 target=錯誤(I2 連續性) |
//!
//! 所有索引參數(`seq_idx`/`at`/`node`/`target`)一律指**規則套用前的凍結快照**(I1/I2)。
//! I9:骨架(`Seg`)的符號/特徵改動不屬六原語,屬音段層規則(步驟 3+)。

use crate::lifecycle::Action;
use crate::repr::intern::{SymId, ValId};
use crate::repr::melody::Links;
use crate::repr::prosody::{AnchorRef, Level};

// ── 旋律原語(操作 Autoseg)──

/// 加一條聯結邊:`tier` 第 `seq_idx` 個自體段 → `anchor`。
/// 冪等;`spread` = 迭代 associate、`dock` = 條件 associate(步驟 3+)。
pub fn associate(tier: SymId, seq_idx: usize, anchor: AnchorRef) -> Action {
    Action::Associate {
        tier,
        seq_idx,
        anchor,
    }
}

/// 減一條聯結邊。冪等;刪至零邊 = 該自體段回浮游(D6 keep-in-place)。
pub fn delink(tier: SymId, seq_idx: usize, anchor: AnchorRef) -> Action {
    Action::Delink {
        tier,
        seq_idx,
        anchor,
    }
}

/// 造一個新自體段。`at=None` 追加尾端;`links` 空 = 浮游。
/// `fill` = 逐 Ø 的 insert+associate(步驟 3+)。
pub fn insert(tier: SymId, at: Option<usize>, val: ValId, links: Links) -> Action {
    Action::Insert {
        tier,
        at,
        val,
        links,
        origin: None,
    }
}

/// insert 浮游 + 原位記憶(I11 v2:`insert … near X`)。
pub fn insert_floating_at(tier: SymId, at: Option<usize>, val: ValId, origin: u32) -> Action {
    Action::Insert {
        tier,
        at,
        val,
        links: Links::new(),
        origin: Some(origin),
    }
}

/// 便利:insert 浮游自體段(8.1 tonogenesis 的 `insert (L)`)。
pub fn insert_floating(tier: SymId, at: Option<usize>, val: ValId) -> Action {
    insert(tier, at, val, Links::new())
}

/// 刪一個自體段(連同所有邊)。`merge` = delete+associate(步驟 3+)。
pub fn delete(tier: SymId, seq_idx: usize) -> Action {
    Action::Delete { tier, seq_idx }
}

// ── 韻律原語(操作 Span;repair 類,永不觸發 implicit reparse,A3)──

/// 令 `level` 層第 `node` 節點涵蓋下層索引 `target`(最小連續擴展)。
/// `parse` = 批次 dominate(步驟 5+)。補償性延長 = 空莫拉 dominate 元音。
pub fn dominate(level: Level, node: usize, target: u32) -> Action {
    Action::Dominate {
        level,
        node,
        target,
    }
}

/// 令 `level` 層第 `node` 節點不再涵蓋 `target`(端點縮一格;可縮成空節點=合法暫態)。
pub fn release(level: Level, node: usize, target: u32) -> Action {
    Action::Release {
        level,
        node,
        target,
    }
}

#[cfg(test)]
mod tests {
    //! 六原語不變量(M0 §7.1)。proptest 在隨機良構詞上驗證;
    //! 測試名對齊不變量描述(CLAUDE.md §7 命名規約)。

    use proptest::prelude::*;

    use super::*;
    use crate::lifecycle::{commit, EngineError};
    use crate::repr::feature::FeatBits;
    use crate::repr::invariant::{check_word, Severity};
    use crate::repr::melody::{Autoseg, MelodyTier};
    use crate::repr::prosody::Span;
    use crate::repr::word::{Seg, Word};

    const TONE: SymId = SymId(100);

    /// 造隨機良構詞:n_seg 音段、每音段一莫拉、單一全覆蓋音節、
    /// tone tier(alphabet {0,1,2})載 autosegs=(val, 已排序莫拉聯結)。
    fn build_word(n_seg: usize, autosegs: Vec<(u32, Vec<u32>)>) -> Word {
        let mut w = Word::new();
        for i in 0..n_seg {
            w.skeleton.push(Seg::new(SymId(i as u32), FeatBits::EMPTY));
        }
        w.prosody.syllables.push(Span::new(0, n_seg as u32));
        for i in 0..n_seg as u32 {
            w.prosody.moras.push(Span::new(i, i + 1));
        }
        let mut tier = MelodyTier::new(TONE, Level::Mora, vec![ValId(0), ValId(1), ValId(2)]);
        for (val, links) in autosegs {
            tier.seq.push(Autoseg::linked(
                ValId(val),
                links.into_iter().map(|m| AnchorRef::new(Level::Mora, m)),
            ));
        }
        w.melodies.push(tier);
        w
    }

    /// 隨機 (詞, 合法 seq_idx, 合法莫拉 idx)。
    fn arb_setup() -> impl Strategy<Value = (Word, usize, u32)> {
        (1usize..=5)
            .prop_flat_map(|n_seg| {
                let autoseg = (
                    0u32..3,
                    proptest::sample::subsequence(
                        (0..n_seg as u32).collect::<Vec<_>>(),
                        0..=n_seg.min(2),
                    ),
                );
                (
                    Just(n_seg),
                    proptest::collection::vec(autoseg, 1..=4),
                    0u32..n_seg as u32,
                )
            })
            .prop_flat_map(|(n_seg, autosegs, mora_idx)| {
                let n_auto = autosegs.len();
                (Just(build_word(n_seg, autosegs)), 0..n_auto, Just(mora_idx))
            })
    }

    fn floating_count(w: &Word) -> usize {
        w.tier(TONE).unwrap().floating_indices().len()
    }

    fn no_error_issues(w: &Word) -> bool {
        check_word(w).iter().all(|i| i.severity() != Severity::Error)
    }

    proptest! {
        /// commit 是純函數:不動 before(I1 快照語意的前提)。
        #[test]
        fn commit_does_not_mutate_before((w, si, mi) in arb_setup()) {
            let before = w.clone();
            let anchor = AnchorRef::new(Level::Mora, mi);
            let _ = commit(&w, &[associate(TONE, si, anchor)]).unwrap();
            prop_assert_eq!(w, before);
        }

        /// associate 後自體段數守恆;浮游數減 1(原浮游)或不變(原已掛/邊已存在);
        /// 合法錨點下不引入 error 級問題(commit 後 NCC 檢查完備,warn 級容許)。
        #[test]
        fn associate_conserves_autoseg_count_and_floating((w, si, mi) in arb_setup()) {
            let anchor = AnchorRef::new(Level::Mora, mi);
            let was_floating = w.tier(TONE).unwrap().seq[si].is_floating();
            let f0 = floating_count(&w);
            let after = commit(&w, &[associate(TONE, si, anchor)]).unwrap();

            prop_assert_eq!(after.tier(TONE).unwrap().seq.len(), w.tier(TONE).unwrap().seq.len());
            prop_assert!(after.tier(TONE).unwrap().seq[si].links.contains(&anchor));
            let expected = if was_floating { f0 - 1 } else { f0 };
            prop_assert_eq!(floating_count(&after), expected);
            prop_assert!(no_error_issues(&after));
        }

        /// delink 是 associate 的逆(邊原不存在時):roundtrip = identity。
        #[test]
        fn delink_inverts_associate((w, si, mi) in arb_setup()) {
            let anchor = AnchorRef::new(Level::Mora, mi);
            prop_assume!(!w.tier(TONE).unwrap().seq[si].links.contains(&anchor));
            let there = commit(&w, &[associate(TONE, si, anchor)]).unwrap();
            let back = commit(&there, &[delink(TONE, si, anchor)]).unwrap();
            prop_assert_eq!(back, w);
        }

        /// insert 後 seq 長度 +1;插入者就位、其餘保序;浮游 insert 浮游數 +1。
        #[test]
        fn insert_grows_seq_preserving_order((w, _si, _mi) in arb_setup(), pos_frac in 0.0f64..=1.0) {
            let len = w.tier(TONE).unwrap().seq.len();
            let at = ((len as f64) * pos_frac) as usize; // 0..=len
            let f0 = floating_count(&w);
            let after = commit(&w, &[insert_floating(TONE, Some(at), ValId(1))]).unwrap();

            let old = &w.tier(TONE).unwrap().seq;
            let new = &after.tier(TONE).unwrap().seq;
            prop_assert_eq!(new.len(), len + 1);
            prop_assert_eq!(new[at].val, ValId(1));
            prop_assert!(new[at].is_floating());
            prop_assert_eq!(floating_count(&after), f0 + 1);
            // 其餘保序:去掉插入者 = 原序列
            let mut rest = new.clone();
            rest.remove(at);
            prop_assert_eq!(&rest, old);
        }

        /// delete 後 seq 長度 -1;倖存者 = 原序列去掉被刪者(保序,I10 收攏)。
        #[test]
        fn delete_removes_exactly_one_preserving_order((w, si, _mi) in arb_setup()) {
            let after = commit(&w, &[delete(TONE, si)]).unwrap();
            let mut expect = w.tier(TONE).unwrap().seq.clone();
            expect.remove(si);
            prop_assert_eq!(&after.tier(TONE).unwrap().seq, &expect);
            prop_assert!(no_error_issues(&after));
        }

        /// dominate 後該節點涵蓋 target;Span 單調不縮;節點數不變(I10:不增刪節點)。
        #[test]
        fn dominate_covers_target_monotonically((w, _si, mi) in arb_setup(), node_frac in 0.0f64..1.0) {
            let n = w.prosody.moras.len();
            let node = ((n as f64) * node_frac) as usize; // 0..n
            let s0 = w.prosody.moras[node];
            let after = commit(&w, &[dominate(Level::Mora, node, mi)]).unwrap();
            let s1 = after.prosody.moras[node];

            prop_assert!(s1.contains_idx(mi));
            prop_assert!(s1.lo <= s0.lo && s1.hi >= s0.hi); // 只擴不縮
            prop_assert_eq!(after.prosody.moras.len(), n);
        }

        /// release:端點 target 縮一格且不再涵蓋;區間外 = 冪等 noop;內部 = ReleaseWouldSplit。
        #[test]
        fn release_shrinks_endpoint_or_errors_interior((w, _si, mi) in arb_setup(), node_frac in 0.0f64..1.0) {
            let n = w.prosody.moras.len();
            let node = ((n as f64) * node_frac) as usize;
            let s0 = w.prosody.moras[node];
            let r = commit(&w, &[release(Level::Mora, node, mi)]);
            if !s0.contains_idx(mi) {
                prop_assert_eq!(r.unwrap(), w); // noop
            } else if mi == s0.lo || mi + 1 == s0.hi {
                let s1 = r.unwrap().prosody.moras[node];
                prop_assert_eq!(s1.len(), s0.len() - 1);
                prop_assert!(!s1.contains_idx(mi));
            } else {
                let is_split_err = matches!(r.unwrap_err(), EngineError::ReleaseWouldSplit { .. });
                prop_assert!(is_split_err, "interior release must error as ReleaseWouldSplit");
            }
        }

        /// dominate 與 release 是修復對:release 右端後 dominate 同一 target 還原原 Span。
        #[test]
        fn dominate_inverts_endpoint_release((w, _si, _mi) in arb_setup(), node_frac in 0.0f64..1.0) {
            let n = w.prosody.moras.len();
            let node = ((n as f64) * node_frac) as usize;
            let s0 = w.prosody.moras[node];
            let target = s0.hi - 1; // 右端(build_word 保證每莫拉非空)
            let shrunk = commit(&w, &[release(Level::Mora, node, target)]).unwrap();
            let restored = commit(&shrunk, &[dominate(Level::Mora, node, target)]).unwrap();
            prop_assert_eq!(restored.prosody.moras[node], s0);
        }
    }

    /// 冪等邊角(proptest 之外的定點確認):重複 associate / delink 無二次效果。
    #[test]
    fn associate_and_delink_are_idempotent() {
        let w = build_word(2, vec![(0, vec![])]);
        let a = AnchorRef::new(Level::Mora, 1);
        let once = commit(&w, &[associate(TONE, 0, a)]).unwrap();
        let twice = commit(&once, &[associate(TONE, 0, a)]).unwrap();
        assert_eq!(once, twice);
        assert_eq!(once.tier(TONE).unwrap().seq[0].links.len(), 1);

        let gone = commit(&twice, &[delink(TONE, 0, a)]).unwrap();
        let gone2 = commit(&gone, &[delink(TONE, 0, a)]).unwrap();
        assert_eq!(gone, gone2);
        assert!(gone.tier(TONE).unwrap().seq[0].is_floating()); // 零邊=回浮游(D6/D14)
    }
}
