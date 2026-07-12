//! Commit(執行語意 §1 步驟 3):把一批 `Action` **一次**寫入凍結快照,產生新 `Word`。
//!
//! 這是**唯一**改變表徵的地方(單一資訊源)。核心保證:
//! - **凍結語意**:所有 Action 的索引(`seq_idx`/`at`/`node`)都指向**套用前**的狀態;
//!   commit 內不讓先套的 Action 位移後套 Action 的索引(parallel 不自我餵食,B5)。
//! - **重建收攏**:同 tier 的 `Delete` 造成的序列空洞,於重建階段一次收攏;`Associate`/`Delink`
//!   在收攏前以前狀態索引就地改邊,故三者索引一致(I10)。
//! - **韻律穩定**:`Dominate`/`Release` 只擴縮既有節點 Span、不增刪節點,`AnchorRef` 索引不變(I10)。
//!
//! 跨層連鎖重編(音段增刪 → 上層 Span / 旋律 link 位移)不在此步,留步驟 5(I10)。

use crate::repr::melody::{Autoseg, Links};
use crate::repr::prosody::AnchorRef;
use crate::repr::word::{Seg, Word};

use super::action::Action;
use super::error::EngineError;

/// 在凍結快照 `before` 上套用 `actions`,回傳新 `Word`。任一 Action 索引越界 → `EngineError`。
pub fn commit(before: &Word, actions: &[Action]) -> Result<Word, EngineError> {
    let mut after = before.clone(); // I1:快照 = clone
    // 全部 Action 以前狀態索引;SegRewrite(長度不變,I12)/韻律/旋律先套用,
    // SegDelete 的跨層連鎖(I13)最後跑並重映射一切。
    apply_segmental(&mut after, actions)?;
    apply_prosodic(&mut after, actions)?;
    apply_melodic(&mut after, actions)?;
    apply_seg_deletes(&mut after, actions)?;
    Ok(after)
}

/// SegDelete 連鎖(I13):骨架移除 → Span 平移 → 空節點政策(mora keep-empty、
/// 其餘 delete)→ 無核心音節清理 → 孤兒空莫拉清理 → 旋律 links 重映射
/// (錨點消失 = on-anchor-loss float,D14;序列原位不動,D6)→ stale 標記。
fn apply_seg_deletes(after: &mut Word, actions: &[Action]) -> Result<(), EngineError> {
    use crate::repr::prosody::Level;

    let mut deleted: Vec<usize> = actions
        .iter()
        .filter_map(|a| match *a {
            Action::SegDelete { idx } => Some(idx),
            _ => None,
        })
        .collect();
    if deleted.is_empty() {
        return Ok(());
    }
    deleted.sort_unstable();
    deleted.dedup();
    let len = after.skeleton.len();
    if let Some(&bad) = deleted.iter().find(|&&i| i >= len) {
        return Err(EngineError::SegIndexOutOfRange { idx: bad, len });
    }

    // (a) 骨架移除 + 音段新舊索引映射(None = 已刪)
    let seg_map: Vec<Option<u32>> = {
        let mut map = Vec::with_capacity(len);
        let mut new_idx = 0u32;
        for i in 0..len {
            if deleted.binary_search(&i).is_ok() {
                map.push(None);
            } else {
                map.push(Some(new_idx));
                new_idx += 1;
            }
        }
        map
    };
    let mut kept = 0usize;
    after.skeleton.retain(|_| {
        let keep = seg_map[kept].is_some();
        kept += 1;
        keep
    });

    // Span 平移:lo/hi 各減去其前方被刪的數量
    let shift = |x: u32| -> u32 {
        let cut = deleted.iter().take_while(|&&d| (d as u32) < x).count() as u32;
        x - cut
    };
    for m in after.prosody.moras.iter_mut() {
        let (lo, hi) = (shift(m.lo), shift(m.hi));
        m.lo = lo;
        m.hi = hi;
    }
    for s in after.prosody.syllables.iter_mut() {
        let (lo, hi) = (shift(s.lo), shift(s.hi));
        s.lo = lo;
        s.hi = hi;
    }
    // 自定域(I14):M0 假設 skeleton 為下層,僅平移;政策細化隨步驟 6+ reparse
    for (_, spans) in after.prosody.extra.iter_mut() {
        for s in spans.iter_mut() {
            let (lo, hi) = (shift(s.lo), shift(s.hi));
            s.lo = lo;
            s.hi = hi;
        }
    }

    // (b)(c) 空節點政策 + 無核心音節清理(先算音節去留,再算莫拉去留)
    // 音節刪除條件:變空,或內部已無任何非空莫拉(無核心)。
    let syl_alive: Vec<bool> = after
        .prosody
        .syllables
        .iter()
        .map(|s| {
            !s.is_empty_node()
                && after
                    .prosody
                    .moras
                    .iter()
                    .any(|m| !m.is_empty_node() && m.within(s))
        })
        .collect();
    // 莫拉刪除條件:空節點且(不落於任何存活音節內)——落於存活音節內的空莫拉
    // 依 keep-empty 存活(8.4);隨死亡音節或孤兒化者刪(8.3)。
    let mora_alive: Vec<bool> = after
        .prosody
        .moras
        .iter()
        .map(|m| {
            if !m.is_empty_node() {
                return true;
            }
            after
                .prosody
                .syllables
                .iter()
                .zip(&syl_alive)
                .any(|(s, &alive)| alive && m.within(s))
        })
        .collect();

    // 各層新舊索引映射 + 過濾
    let make_map = |alive: &[bool]| -> Vec<Option<u32>> {
        let mut n = 0u32;
        alive
            .iter()
            .map(|&a| {
                if a {
                    let v = Some(n);
                    n += 1;
                    v
                } else {
                    None
                }
            })
            .collect()
    };
    let mora_map = make_map(&mora_alive);
    let syl_map = make_map(&syl_alive);
    let mut i = 0;
    after.prosody.moras.retain(|_| {
        let k = mora_alive[i];
        i += 1;
        k
    });
    let mut i = 0;
    after.prosody.syllables.retain(|_| {
        let k = syl_alive[i];
        i += 1;
        k
    });

    // 音步(over 音節)/韻詞(over 音步):依音節映射平移;變空即刪(on-daughter-loss delete)。
    remap_upper_layer(&mut after.prosody.feet, &syl_map);
    let feet_alive: Vec<bool> = after.prosody.feet.iter().map(|f| !f.is_empty_node()).collect();
    let feet_map = make_map(&feet_alive);
    after.prosody.feet.retain(|f| !f.is_empty_node());
    remap_upper_layer(&mut after.prosody.pwords, &feet_map);
    after.prosody.pwords.retain(|p| !p.is_empty_node());

    // (d) 旋律 links 重映射;錨點消失 → 邊移除(on-anchor-loss float,D14)
    for tier in after.melodies.iter_mut() {
        let map: &dyn Fn(u32) -> Option<u32> = match tier.anchor {
            Level::Segment => &|i| seg_map.get(i as usize).copied().flatten(),
            Level::Mora => &|i| mora_map.get(i as usize).copied().flatten(),
            Level::Syllable => &|i| syl_map.get(i as usize).copied().flatten(),
            Level::Foot => &|i| feet_map.get(i as usize).copied().flatten(),
            Level::Pword => &|i| Some(i), // pword 極少作 anchor;M0 不映射
            Level::Custom(_) => &|i| Some(i), // 自定域(I14):M0 僅平移標 stale,錨點索引不映射
        };
        // 錨點軸座標平移量:舊索引 → 其下方存活數(= 新座標)
        let coord = |old: u32| -> u32 { (0..old).filter(|&i| map(i).is_some()).count() as u32 };
        for a in tier.seq.iter_mut() {
            let mut new_links = crate::repr::melody::Links::new();
            let mut dropped_at: Option<u32> = None;
            for l in a.links.iter() {
                if let Some(ni) = map(l.index) {
                    new_links.push(AnchorRef::new(l.level, ni));
                } else {
                    dropped_at.get_or_insert(coord(l.index));
                }
            }
            if new_links.is_empty() && !a.links.is_empty() {
                // 浮游化(D14):寫入原位記憶(I11 v2,重編後座標)
                a.origin = dropped_at;
            } else if let Some(o) = a.origin {
                a.origin = Some(coord(o)); // 既有原位同步重映射
            }
            a.links = new_links; // 全消失 = 回浮游,序列原位不動(D6)
        }
    }

    // (e) stale 標記(D23):骨架已變動,韻律結構視為過期(重剖機制留步驟 6+)
    after.stale.mark(Level::Mora);
    after.stale.mark(Level::Syllable);
    if !after.prosody.feet.is_empty() {
        after.stale.mark(Level::Foot);
    }
    Ok(())
}

/// 上層(音步/韻詞)Span 依下層新舊映射重編:端點取存活下層節點的新索引範圍。
fn remap_upper_layer(spans: &mut [crate::repr::prosody::Span], lower_map: &[Option<u32>]) {
    for s in spans.iter_mut() {
        // [lo,hi) 內存活者的新索引 min/max
        let mut new_lo = None;
        let mut new_hi = None;
        for old in s.lo..s.hi {
            if let Some(&Some(ni)) = lower_map.get(old as usize) {
                new_lo = Some(new_lo.map_or(ni, |v: u32| v.min(ni)));
                new_hi = Some(new_hi.map_or(ni + 1, |v: u32| v.max(ni + 1)));
            }
        }
        match (new_lo, new_hi) {
            (Some(lo), Some(hi)) => {
                s.lo = lo;
                s.hi = hi;
            }
            _ => {
                // 全滅:壓成空節點,呼叫端隨後過濾
                let base = lower_map
                    .get(s.lo as usize)
                    .and_then(|&m| m)
                    .unwrap_or(0);
                s.lo = base;
                s.hi = base;
            }
        }
    }
}

/// 套用所有音段規則通道(SegRewrite):整段替換,骨架長度與所有索引不變(I12)。
/// 同批多個 SegRewrite 指向同一音段時依 Action 序後者為準(parallel 同拍衝突的確定性解)。
fn apply_segmental(after: &mut Word, actions: &[Action]) -> Result<(), EngineError> {
    for a in actions {
        if let Action::SegRewrite { idx, sym, feats } = *a {
            let len = after.skeleton.len();
            let seg = after
                .skeleton
                .get_mut(idx)
                .ok_or(EngineError::SegIndexOutOfRange { idx, len })?;
            *seg = Seg::new(sym, feats);
        }
    }
    Ok(())
}

/// 套用所有韻律原語(dominate/release):就地擴縮 Span,節點數不變。
fn apply_prosodic(after: &mut Word, actions: &[Action]) -> Result<(), EngineError> {
    for a in actions {
        match *a {
            Action::Dominate {
                level,
                node,
                target,
            } => {
                let spans = after
                    .prosody
                    .level_mut(level)
                    .ok_or(EngineError::NotAProsodyLevel(level))?;
                let len = spans.len();
                let s = spans
                    .get_mut(node)
                    .ok_or(EngineError::NodeOutOfRange { level, node, len })?;
                // 最小連續擴展以涵蓋 target(補償性延長:空莫拉向元音重掛)。
                s.lo = s.lo.min(target);
                s.hi = s.hi.max(target + 1);
            }
            Action::Release {
                level,
                node,
                target,
            } => {
                let spans = after
                    .prosody
                    .level_mut(level)
                    .ok_or(EngineError::NotAProsodyLevel(level))?;
                let len = spans.len();
                let s = spans
                    .get_mut(node)
                    .ok_or(EngineError::NodeOutOfRange { level, node, len })?;
                if !s.contains_idx(target) {
                    // 已不支配 target:冪等 noop。
                } else if target == s.lo {
                    s.lo += 1; // 縮左端(可縮成空節點,合法暫態)
                } else if target + 1 == s.hi {
                    s.hi -= 1; // 縮右端
                } else {
                    return Err(EngineError::ReleaseWouldSplit {
                        level,
                        node,
                        target,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// 套用所有旋律原語,逐 tier 重建序列。
fn apply_melodic(after: &mut Word, actions: &[Action]) -> Result<(), EngineError> {
    // 涉及的 tier,保序去重(讓重建對每個 tier 恰跑一次)。
    let mut tiers: Vec<_> = Vec::new();
    for a in actions {
        if let Some(t) = a.tier() {
            if !tiers.contains(&t) {
                tiers.push(t);
            }
        }
    }
    for tier in tiers {
        apply_tier(after, tier, actions)?;
    }
    Ok(())
}

fn apply_tier(
    after: &mut Word,
    tier: crate::repr::intern::SymId,
    actions: &[Action],
) -> Result<(), EngineError> {
    let ti = after
        .melodies
        .iter()
        .position(|m| m.name == tier)
        .ok_or(EngineError::TierNotFound(tier))?;
    let seq_len = after.melodies[ti].seq.len();

    // ── 階段 A:就地改邊(associate/delink),索引指前狀態,不改長度 ──
    for a in actions {
        match *a {
            Action::Associate {
                tier: t,
                seq_idx,
                anchor,
            } if t == tier => {
                if seq_idx >= seq_len {
                    return Err(EngineError::SeqIndexOutOfRange {
                        tier,
                        idx: seq_idx,
                        len: seq_len,
                    });
                }
                add_link(&mut after.melodies[ti].seq[seq_idx].links, anchor);
            }
            Action::Delink {
                tier: t,
                seq_idx,
                anchor,
            } if t == tier => {
                if seq_idx >= seq_len {
                    return Err(EngineError::SeqIndexOutOfRange {
                        tier,
                        idx: seq_idx,
                        len: seq_len,
                    });
                }
                after.melodies[ti].seq[seq_idx]
                    .links
                    .retain(|x| *x != anchor);
            }
            _ => {}
        }
    }

    // ── 階段 B:收集 delete/insert(仍以前狀態索引)──
    let mut deleted = vec![false; seq_len];
    // ins_before[i] = 要插在前狀態第 i 項之前的新自體段;i==seq_len 表示尾端。
    let mut ins_before: Vec<Vec<Autoseg>> = (0..=seq_len).map(|_| Vec::new()).collect();
    for a in actions {
        match a {
            Action::Delete { tier: t, seq_idx } if *t == tier => {
                if *seq_idx >= seq_len {
                    return Err(EngineError::SeqIndexOutOfRange {
                        tier,
                        idx: *seq_idx,
                        len: seq_len,
                    });
                }
                deleted[*seq_idx] = true;
            }
            Action::Insert {
                tier: t,
                at,
                val,
                links,
                origin,
            } if *t == tier => {
                let pos = at.unwrap_or(seq_len);
                if pos > seq_len {
                    return Err(EngineError::InsertPosOutOfRange {
                        tier,
                        at: pos,
                        len: seq_len,
                    });
                }
                let mut links: Links = links.clone();
                normalize_links(&mut links);
                ins_before[pos].push(Autoseg {
                    val: *val,
                    links,
                    origin: *origin,
                });
            }
            _ => {}
        }
    }

    // ── 階段 C:一次重建 seq(過濾 deleted、插入 inserts)──
    let old = std::mem::take(&mut after.melodies[ti].seq);
    let mut new_seq: Vec<Autoseg> = Vec::with_capacity(old.len());
    for (i, seg) in old.into_iter().enumerate() {
        new_seq.append(&mut ins_before[i]);
        if !deleted[i] {
            new_seq.push(seg);
        }
    }
    new_seq.append(&mut ins_before[seq_len]); // at == len 的追加
    after.melodies[ti].seq = new_seq;
    Ok(())
}

/// 加一條邊:冪等(已存在則 noop),維持依 index 遞增(melody 註解的排序不變量)。
fn add_link(links: &mut Links, anchor: AnchorRef) {
    if !links.contains(&anchor) {
        links.push(anchor);
        links.sort_by(|a, b| a.index.cmp(&b.index));
    }
}

/// 新自體段的 links 正規化:排序 + 去重(與 `add_link` 同不變量)。
fn normalize_links(links: &mut Links) {
    links.sort_by(|a, b| a.index.cmp(&b.index));
    links.dedup();
}
