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
use crate::repr::word::Word;

use super::action::Action;
use super::error::EngineError;

/// 在凍結快照 `before` 上套用 `actions`,回傳新 `Word`。任一 Action 索引越界 → `EngineError`。
pub fn commit(before: &Word, actions: &[Action]) -> Result<Word, EngineError> {
    let mut after = before.clone(); // I1:快照 = clone
    // 韻律與旋律互不影響索引,順序任意;先韻律再旋律。
    apply_prosodic(&mut after, actions)?;
    apply_melodic(&mut after, actions)?;
    Ok(after)
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
                ins_before[pos].push(Autoseg { val: *val, links });
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
