//! `Word`:單一詞的完整表徵快照(I1:快照 = clone)。

use super::feature::FeatBits;
use super::intern::SymId;
use super::melody::MelodyTier;
use super::prosody::{AnchorRef, Level, ProsodyLayers, StaleFlags};

/// 骨架音段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seg {
    pub sym: SymId,
    pub feats: FeatBits,
}

impl Seg {
    pub fn new(sym: SymId, feats: FeatBits) -> Self {
        Seg { sym, feats }
    }
}

/// 型態單位(由型態層餵入,音系唯讀;語法規格 §7)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorphUnit {
    Root,
    Stem,
    Word,
}

/// 型態括號:骨架上的區間 `[lo, hi)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bracket {
    pub unit: MorphUnit,
    pub lo: u32,
    pub hi: u32,
}

/// 一個詞的完整表徵。詞極小(數十音段),clone 即快照(I1);
/// trace / undo / 演化樹回放皆由快照序列取得。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Word {
    pub skeleton: Vec<Seg>,
    pub prosody: ProsodyLayers,
    pub melodies: Vec<MelodyTier>,
    pub morph: Vec<Bracket>,
    pub stale: StaleFlags,
}

impl Word {
    pub fn new() -> Self {
        Self::default()
    }

    /// 快照 = clone(I1)。命名以彰顯語意;commit 之後由 lifecycle 呼叫。
    pub fn snapshot(&self) -> Word {
        self.clone()
    }

    pub fn tier(&self, name: SymId) -> Option<&MelodyTier> {
        self.melodies.iter().find(|t| t.name == name)
    }

    pub fn tier_mut(&mut self, name: SymId) -> Option<&mut MelodyTier> {
        self.melodies.iter_mut().find(|t| t.name == name)
    }

    /// 某層目前的錨點數(Segment = 骨架長度)。
    pub fn anchor_count(&self, level: Level) -> usize {
        match level {
            Level::Segment => self.skeleton.len(),
            _ => self.prosody.level(level).map(Vec::len).unwrap_or(0),
        }
    }

    pub fn anchor_exists(&self, a: AnchorRef) -> bool {
        (a.index as usize) < self.anchor_count(a.level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::intern::ValId;
    use crate::repr::melody::Autoseg;
    use crate::repr::prosody::Span;

    /// 快照獨立性:改動原詞不影響快照(I1 的核心保證)。
    #[test]
    fn snapshot_is_independent() {
        let mut w = Word::new();
        w.skeleton.push(Seg::new(SymId(0), FeatBits::EMPTY));
        w.prosody.moras.push(Span::new(0, 1));
        let mut tier = MelodyTier::new(SymId(1), Level::Mora, vec![ValId(0)]);
        tier.seq.push(Autoseg::floating(ValId(0)));
        w.melodies.push(tier);

        let snap = w.snapshot();
        w.skeleton.push(Seg::new(SymId(2), FeatBits::EMPTY));
        w.melodies[0].seq[0]
            .links
            .push(AnchorRef::new(Level::Mora, 0));
        w.stale.mark_all();

        assert_eq!(snap.skeleton.len(), 1);
        assert!(snap.melodies[0].seq[0].is_floating());
        assert!(!snap.stale.any());
        assert_eq!(w.skeleton.len(), 2);
    }

    #[test]
    fn anchor_counting() {
        let mut w = Word::new();
        w.skeleton.push(Seg::new(SymId(0), FeatBits::EMPTY));
        w.skeleton.push(Seg::new(SymId(1), FeatBits::EMPTY));
        w.prosody.moras.push(Span::new(1, 2));
        w.prosody.syllables.push(Span::new(0, 2));

        assert_eq!(w.anchor_count(Level::Segment), 2);
        assert_eq!(w.anchor_count(Level::Mora), 1);
        assert_eq!(w.anchor_count(Level::Syllable), 1);
        assert_eq!(w.anchor_count(Level::Foot), 0);
        assert!(w.anchor_exists(AnchorRef::new(Level::Mora, 0)));
        assert!(!w.anchor_exists(AnchorRef::new(Level::Mora, 1)));
    }
}
