//! scan:全域位置存取(D3 三道鎖;M0 步驟 6)。
//!
//! 枚舉:沿 `along` 韻律軌道逐刻度報告承載者(預設 linked-only,D4);
//! 空刻度以 Ø 入列(「第一個無調莫拉」可定址)。`along` 比 anchor 粗時每刻度
//! 是**不透明簇**(D18)。塊內「相鄰」= 沿掃描軸序列相鄰(D5);環境即過濾(D20)。
//! `within pword` = M0 全詞。塊內規則各走 §1 生命週期(產 Action 交 lifecycle)。

use crate::lifecycle::{Action, EngineError};
use crate::primitives;
use crate::repr::intern::{SymId, ValId};
use crate::repr::prosody::{AnchorRef, Level};
use crate::repr::word::Word;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanDir {
    FromLeft,
    FromRight,
}

/// 枚舉範圍(D4):預設 linked-only;`over all` 時浮游者按**原位**(I11 v2 origin)入列。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Enumerate {
    #[default]
    LinkedOnly,
    All,
}

/// 一個掃描刻度:`along` 層節點 + 其簇(有序 (seq_idx, val))+ 可停靠錨點。
#[derive(Debug, Clone)]
pub struct Tick {
    /// 簇:此刻度下所有聯結自體段(依錨點序);空 = Ø 刻度。
    pub cluster: Vec<(usize, ValId)>,
    /// 對此刻度 associate 時的停靠錨點(tier 錨定層座標;粗掃描取刻度內首錨)。
    pub dock: Option<u32>,
}

/// 枚舉刻度(linked-only,D4;`over all` 留步驟 7+)。
pub fn ticks(
    w: &Word,
    tier: SymId,
    along: Level,
    dir: ScanDir,
    over: Enumerate,
) -> Result<Vec<Tick>, EngineError> {
    let t = w.tier(tier).ok_or(EngineError::TierNotFound(tier))?;
    let n_along = w.anchor_count(along) as u32;
    let mut out = Vec::new();
    for i in 0..n_along {
        // 刻度 i 涵蓋的 tier 錨定層錨點
        let anchors: Vec<u32> = if along == t.anchor {
            vec![i]
        } else {
            // 粗掃描(D18):along 節點的骨架範圍內的錨定層節點
            let span = w
                .prosody
                .level(along)
                .and_then(|v| v.get(i as usize))
                .copied()
                .ok_or(EngineError::NodeOutOfRange {
                    level: along,
                    node: i as usize,
                    len: n_along as usize,
                })?;
            let cnt = w.anchor_count(t.anchor) as u32;
            (0..cnt)
                .filter(|&m| match t.anchor {
                    Level::Segment => span.contains_idx(m),
                    _ => w
                        .prosody
                        .level(t.anchor)
                        .and_then(|v| v.get(m as usize))
                        .is_some_and(|s| !s.is_empty_node() && s.within(&span)),
                })
                .collect()
        };
        let mut cluster = Vec::new();
        for &a in &anchors {
            for si in t.bearers_of(AnchorRef::new(t.anchor, a)) {
                cluster.push((si, t.seq[si].val));
            }
        }
        out.push(Tick {
            cluster,
            dock: anchors.first().copied(),
        });
    }
    // over all(D4):浮游者按原位入列(origin,I11 v2;無 origin 者以 seq 索引近似)
    if over == Enumerate::All {
        for (si, a) in t.seq.iter().enumerate() {
            if !a.is_floating() {
                continue;
            }
            let pos = a.origin.unwrap_or(si as u32);
            // 錨定層座標 → along 刻度(同層直用;粗掃描找涵蓋節點)
            let tick_i = if along == t.anchor {
                pos.min(n_along.saturating_sub(1))
            } else {
                let (plo, phi) = match t.anchor {
                    Level::Segment => (pos, pos + 1),
                    _ => w
                        .prosody
                        .level(t.anchor)
                        .and_then(|v| v.get(pos as usize))
                        .map(|s| (s.lo, s.hi))
                        .unwrap_or((0, 0)),
                };
                (0..n_along)
                    .find(|&i| {
                        w.prosody
                            .level(along)
                            .and_then(|v| v.get(i as usize))
                            .is_some_and(|s| s.lo <= plo && phi <= s.hi)
                    })
                    .unwrap_or(0)
            };
            if let Some(tk) = out.get_mut(tick_i as usize) {
                tk.cluster.push((si, a.val));
            }
        }
    }
    if dir == ScanDir::FromRight {
        out.reverse();
    }
    Ok(out)
}

/// 刻度目標:序數 `[n]`(1 起算,依掃描方向)或 `[first]` 滿足 Ø 述語者。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickSel {
    Nth(u32),
    FirstEmpty,
}

/// `associate <值> -> <目標>`:對選中刻度建新自體段並掛上(值來自字母表 = 生成即掛)。
pub fn assoc_at(
    w: &Word,
    tier: SymId,
    along: Level,
    dir: ScanDir,
    over: Enumerate,
    val: ValId,
    sel: TickSel,
) -> Result<Vec<Action>, EngineError> {
    let t = w.tier(tier).ok_or(EngineError::TierNotFound(tier))?;
    let list = ticks(w, tier, along, dir, over)?;
    let target = match sel {
        TickSel::Nth(n) => list.get((n.max(1) - 1) as usize),
        TickSel::FirstEmpty => list.iter().find(|tk| tk.cluster.is_empty()),
    };
    Ok(match target.and_then(|tk| tk.dock) {
        None => Vec::new(), // B8 noop
        Some(anchor) => {
            let links = std::iter::once(AnchorRef::new(t.anchor, anchor)).collect();
            vec![Action::Insert {
                tier,
                at: None,
                val,
                links,
                origin: Some(anchor),
            }]
        }
    })
}

/// 值改寫沿掃描軸:`FROM => TO / PRE _`(D20 環境即過濾;D5 序列相鄰)。
/// 簇匹配 = 簇恰為單值 [v](D18 不透明;複雜簇匹配 `:{…}` 留步驟 7+)。
/// 值改寫 = delete + insert(同位同聯結),parallel 以前狀態一次判斷。
pub fn value_rewrite(
    w: &Word,
    tier: SymId,
    along: Level,
    dir: ScanDir,
    over: Enumerate,
    from: ValId,
    to: ValId,
    pre: Option<ValId>,
) -> Result<Vec<Action>, EngineError> {
    let t = w.tier(tier).ok_or(EngineError::TierNotFound(tier))?;
    let list = ticks(w, tier, along, dir, over)?;
    let is_val = |tk: &Tick, v: ValId| tk.cluster.len() == 1 && tk.cluster[0].1 == v;
    let mut acts = Vec::new();
    for (i, tk) in list.iter().enumerate() {
        if !is_val(tk, from) {
            continue;
        }
        if let Some(p) = pre {
            if i == 0 || !is_val(&list[i - 1], p) {
                continue;
            }
        }
        let (si, _) = tk.cluster[0];
        let links = t.seq[si].links.clone();
        acts.push(primitives::delete(tier, si));
        acts.push(Action::Insert {
            tier,
            at: Some(si),
            val: to,
            links,
            origin: None,
        });
    }
    Ok(acts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::commit;
    use crate::repr::feature::FeatBits;
    use crate::repr::melody::{Autoseg, MelodyTier};
    use crate::repr::prosody::Span;
    use crate::repr::word::Seg;

    const TONE: SymId = SymId(100);
    const H: ValId = ValId(0);
    const L: ValId = ValId(1);

    /// CV×n 詞 + tone tier;`hs` 指定哪些莫拉掛 H。
    fn word(n: usize, hs: &[u32]) -> Word {
        let mut w = Word::new();
        for k in 0..n as u32 {
            w.skeleton.push(Seg::new(SymId(0), FeatBits::EMPTY));
            w.skeleton.push(Seg::new(SymId(1), FeatBits::EMPTY));
            w.prosody.syllables.push(Span::new(k * 2, k * 2 + 2));
            w.prosody.moras.push(Span::new(k * 2 + 1, k * 2 + 2));
        }
        let mut t = MelodyTier::new(TONE, Level::Mora, vec![H, L]);
        for &m in hs {
            t.seq
                .push(Autoseg::linked(H, vec![AnchorRef::new(Level::Mora, m)]));
        }
        w.melodies.push(t);
        w
    }

    /// 8.6(a):from right 序數 [2] = 倒數第二音節;粗掃描停靠其首莫拉(D16/D18)。
    #[test]
    fn assoc_penult_syllable_from_right_8_6a() {
        let w = word(3, &[]);
        let acts = assoc_at(
            &w,
            TONE,
            Level::Syllable,
            ScanDir::FromRight,
            Enumerate::LinkedOnly,
            H,
            TickSel::Nth(2),
        )
        .unwrap();
        let w2 = commit(&w, &acts).unwrap();
        let t = w2.tier(TONE).unwrap();
        assert!(t.seq[0].links.contains(&AnchorRef::new(Level::Mora, 1))); // 倒數第二 σ 的莫拉
    }

    /// 8.6(b):Meeussen——序列相鄰 H…H 後者變 L(D5 掃描軸相鄰;Ø 刻度不算相鄰對)。
    #[test]
    fn meeussen_value_rewrite_8_6b() {
        let w = word(2, &[0, 1]); // H~μ0 H~μ1
        let acts = value_rewrite(
            &w,
            TONE,
            Level::Mora,
            ScanDir::FromLeft,
            Enumerate::LinkedOnly,
            H,
            L,
            Some(H),
        )
        .unwrap();
        let w2 = commit(&w, &acts).unwrap();
        let t = w2.tier(TONE).unwrap();
        assert_eq!(t.seq[0].val, H);
        assert_eq!(t.seq[1].val, L);
        assert!(t.seq[1].links.contains(&AnchorRef::new(Level::Mora, 1))); // 聯結保留

        // 間隔 Ø:H Ø H 不觸發(掃描軸相鄰,非「有值相鄰」)
        let w3 = word(3, &[0, 2]);
        let acts3 = value_rewrite(
            &w3,
            TONE,
            Level::Mora,
            ScanDir::FromLeft,
            Enumerate::LinkedOnly,
            H,
            L,
            Some(H),
        )
        .unwrap();
        assert!(acts3.is_empty());
    }

    /// over all(D4):浮游者按原位入列——原位刻度不再是 Ø。
    #[test]
    fn over_all_inserts_floats_at_origin_d4() {
        let mut w = word(3, &[]);
        let mut fl = Autoseg::floating(H);
        fl.origin = Some(1); // 原位 μ1
        w.tier_mut(TONE).unwrap().seq.push(fl);
        // linked-only:μ1 是 Ø 刻度 → first-empty = μ0
        let l = ticks(&w, TONE, Level::Mora, ScanDir::FromLeft, Enumerate::LinkedOnly).unwrap();
        assert!(l[1].cluster.is_empty());
        // over all:μ1 刻度含浮游 H
        let a = ticks(&w, TONE, Level::Mora, ScanDir::FromLeft, Enumerate::All).unwrap();
        assert_eq!(a[1].cluster.len(), 1);
    }

    /// 8.6(c):第一個 Ø 莫拉可定址(空刻度入列,D4)。
    #[test]
    fn first_empty_mora_addressable_8_6c() {
        let w = word(3, &[1]); // μ0 Ø, μ1 H, μ2 Ø
        let acts = assoc_at(
            &w,
            TONE,
            Level::Mora,
            ScanDir::FromLeft,
            Enumerate::LinkedOnly,
            H,
            TickSel::FirstEmpty,
        )
        .unwrap();
        let w2 = commit(&w, &acts).unwrap();
        let t = w2.tier(TONE).unwrap();
        assert!(t
            .seq
            .iter()
            .any(|a| a.links.contains(&AnchorRef::new(Level::Mora, 0))));
    }
}
