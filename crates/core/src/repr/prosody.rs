//! 韻律層(時間軌道):Span 序列,非一般圖(I2)。
//!
//! # I8:支配拓撲修正(相對執行語意規格的「全覆蓋」措辭)
//! 標準韻律理論中,音節首輔音不屬於任何莫拉(onset 直接受音節支配),故:
//! - `Syllable` 與 `Mora` 都直接以 **Segment** 為下層(而非 σ→μ→seg 的單鏈);
//! - 音節層對骨架**全覆蓋、不重疊**(D24 排除 ambisyllabicity);
//! - 莫拉層對骨架**部分覆蓋**(onset 不入莫拉),且**允許重疊**
//!   (長元音 = 兩莫拉共享同一音段);
//! - `Foot` 以 Syllable 為下層、`Pword` 以 Foot 為下層,皆不強制全覆蓋
//!   (degenerate 音節可不入音步)。
//! - 空節點(`lo == hi`)= 暫態病理結構(如補償性延長的空莫拉),
//!   合法存在、由 `dominate` 修復;invariant 檢查以 info 級回報。

use super::feature::FeatBits;

/// 韻律層級。`Segment` 指骨架本身(無 Span 向量)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    Segment,
    Mora,
    Syllable,
    Foot,
    Pword,
}

impl Level {
    /// 該層 Span 的索引所指向的下層(I8 拓撲)。
    pub fn lower(self) -> Option<Level> {
        match self {
            Level::Segment => None,
            Level::Mora => Some(Level::Segment),
            Level::Syllable => Some(Level::Segment),
            Level::Foot => Some(Level::Syllable),
            Level::Pword => Some(Level::Foot),
        }
    }

    /// notation 用字符(對齊規格記法:μ σ Ft ω;骨架槽 = x)。
    pub fn glyph(self) -> &'static str {
        match self {
            Level::Segment => "x",
            Level::Mora => "μ",
            Level::Syllable => "σ",
            Level::Foot => "Ft",
            Level::Pword => "ω",
        }
    }

    fn stale_idx(self) -> Option<usize> {
        match self {
            Level::Segment => None,
            Level::Mora => Some(0),
            Level::Syllable => Some(1),
            Level::Foot => Some(2),
            Level::Pword => Some(3),
        }
    }
}

/// 對下層的連續區間 `[lo, hi)` + 該層級特徵(如音節的 [+heavy])。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub lo: u32,
    pub hi: u32,
    pub feats: FeatBits,
}

impl Span {
    pub fn new(lo: u32, hi: u32) -> Self {
        Span {
            lo,
            hi,
            feats: FeatBits::EMPTY,
        }
    }
    /// 空節點:暫態病理結構(見模組說明)。
    pub fn is_empty_node(&self) -> bool {
        self.lo == self.hi
    }
    pub fn len(&self) -> u32 {
        self.hi - self.lo
    }
    pub fn contains_idx(&self, i: u32) -> bool {
        self.lo <= i && i < self.hi
    }
    pub fn overlaps(&self, other: &Span) -> bool {
        self.lo < other.hi && other.lo < self.hi
    }
    /// self 是否完全落在 other 之內(莫拉⊆音節檢查)。
    pub fn within(&self, other: &Span) -> bool {
        other.lo <= self.lo && self.hi <= other.hi
    }
}

/// 四個韻律層的 Span 序列(韻律結構的單一存放處)。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProsodyLayers {
    pub moras: Vec<Span>,
    pub syllables: Vec<Span>,
    pub feet: Vec<Span>,
    pub pwords: Vec<Span>,
}

impl ProsodyLayers {
    pub fn level(&self, l: Level) -> Option<&Vec<Span>> {
        match l {
            Level::Segment => None,
            Level::Mora => Some(&self.moras),
            Level::Syllable => Some(&self.syllables),
            Level::Foot => Some(&self.feet),
            Level::Pword => Some(&self.pwords),
        }
    }

    pub fn level_mut(&mut self, l: Level) -> Option<&mut Vec<Span>> {
        match l {
            Level::Segment => None,
            Level::Mora => Some(&mut self.moras),
            Level::Syllable => Some(&mut self.syllables),
            Level::Foot => Some(&mut self.feet),
            Level::Pword => Some(&mut self.pwords),
        }
    }
}

/// 錨點引用 = 層級 + 索引(I2:commit 為序列點,規則內索引凍結、commit 時重編)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnchorRef {
    pub level: Level,
    pub index: u32,
}

impl AnchorRef {
    pub fn new(level: Level, index: u32) -> Self {
        AnchorRef { level, index }
    }
}

/// 韻律結構過期旗標(lazy reparse,執行語意 §3;stale 的單一存放處)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaleFlags {
    flags: [bool; 4], // Mora, Syllable, Foot, Pword
}

impl StaleFlags {
    pub fn mark_all(&mut self) {
        self.flags = [true; 4];
    }
    pub fn mark(&mut self, level: Level) {
        if let Some(i) = level.stale_idx() {
            self.flags[i] = true;
        }
    }
    pub fn clear(&mut self, level: Level) {
        if let Some(i) = level.stale_idx() {
            self.flags[i] = false;
        }
    }
    pub fn is_stale(&self, level: Level) -> bool {
        level.stale_idx().map(|i| self.flags[i]).unwrap_or(false)
    }
    pub fn any(&self) -> bool {
        self.flags.iter().any(|&b| b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_geometry() {
        let a = Span::new(0, 2);
        let b = Span::new(1, 3);
        let c = Span::new(2, 4);
        let e = Span::new(2, 2);
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
        assert!(e.is_empty_node());
        assert!(Span::new(1, 2).within(&Span::new(0, 3)));
        assert!(!Span::new(1, 4).within(&Span::new(0, 3)));
    }

    #[test]
    fn stale_flags_roundtrip() {
        let mut s = StaleFlags::default();
        assert!(!s.any());
        s.mark(Level::Mora);
        assert!(s.is_stale(Level::Mora));
        assert!(!s.is_stale(Level::Syllable));
        s.clear(Level::Mora);
        assert!(!s.any());
        s.mark_all();
        assert!(s.is_stale(Level::Pword));
        // Segment 無 stale 概念
        assert!(!s.is_stale(Level::Segment));
    }

    #[test]
    fn level_topology_i8() {
        assert_eq!(Level::Mora.lower(), Some(Level::Segment));
        assert_eq!(Level::Syllable.lower(), Some(Level::Segment)); // I8:非 σ→μ 單鏈
        assert_eq!(Level::Foot.lower(), Some(Level::Syllable));
        assert_eq!(Level::Segment.lower(), None);
    }
}
