//! 規格記法渲染(語法規格 §3.2 記法):
//! 已聯結 `H~μ0~μ1`、浮游 `(H)@3`、骨架與韻律的可讀化。
//! 供 insta 快照測試與未來追蹤視圖;渲染是唯讀純函數。

use super::intern::{SymTable, ValTable};
use super::melody::MelodyTier;
use super::word::Word;

/// 渲染一條旋律層:依 seq 順序,空白分隔。
/// 例:`(L)@0` → dock 後 `L~μ0`;延展 `H~μ0~μ1`。
pub fn render_tier(tier: &MelodyTier, vals: &ValTable) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(tier.seq.len());
    for (i, a) in tier.seq.iter().enumerate() {
        let v = vals.resolve(a.val).unwrap_or("?");
        if a.is_floating() {
            parts.push(format!("({})@{}", v, i));
        } else {
            let mut s = String::from(v);
            for link in &a.links {
                s.push('~');
                s.push_str(link.level.glyph());
                s.push_str(&link.index.to_string());
            }
            parts.push(s);
        }
    }
    parts.join(" ")
}

/// 渲染骨架:音段符號以空白分隔,如 `b a`。
pub fn render_skeleton(w: &Word, syms: &SymTable) -> String {
    w.skeleton
        .iter()
        .map(|s| syms.resolve(s.sym).unwrap_or("?"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 渲染韻律(debug 導向):`σ0[0,2) μ0[1,2) μ1[2,2)∅`(∅ 標空節點)。
pub fn render_prosody(w: &Word) -> String {
    let mut parts: Vec<String> = Vec::new();
    let layers = [
        (crate::repr::Level::Syllable, &w.prosody.syllables),
        (crate::repr::Level::Mora, &w.prosody.moras),
        (crate::repr::Level::Foot, &w.prosody.feet),
        (crate::repr::Level::Pword, &w.prosody.pwords),
    ];
    for (level, spans) in layers {
        for (i, s) in spans.iter().enumerate() {
            let empty = if s.is_empty_node() { "∅" } else { "" };
            parts.push(format!("{}{}[{},{}){}", level.glyph(), i, s.lo, s.hi, empty));
        }
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::feature::FeatBits;
    use crate::repr::intern::{SymTable, ValTable};
    use crate::repr::melody::{Autoseg, MelodyTier};
    use crate::repr::prosody::{AnchorRef, Level, Span};
    use crate::repr::word::{Seg, Word};

    #[test]
    fn tier_notation_matches_spec() {
        let mut syms = SymTable::default();
        let mut vals = ValTable::default();
        let tone = syms.intern("tone");
        let h = vals.intern("H");
        let l = vals.intern("L");

        let mut t = MelodyTier::new(tone, Level::Mora, vec![h, l]);
        t.seq.push(Autoseg::linked(
            h,
            vec![
                AnchorRef::new(Level::Mora, 0),
                AnchorRef::new(Level::Mora, 1),
            ],
        ));
        t.seq.push(Autoseg::floating(l));
        assert_eq!(render_tier(&t, &vals), "H~μ0~μ1 (L)@1");
    }

    #[test]
    fn skeleton_and_prosody_notation() {
        let mut syms = SymTable::default();
        let b = syms.intern("b");
        let a = syms.intern("a");
        let mut w = Word::new();
        w.skeleton.push(Seg::new(b, FeatBits::EMPTY));
        w.skeleton.push(Seg::new(a, FeatBits::EMPTY));
        w.prosody.syllables.push(Span::new(0, 2));
        w.prosody.moras.push(Span::new(1, 2));
        w.prosody.moras.push(Span::new(2, 2)); // 空莫拉

        assert_eq!(render_skeleton(&w, &syms), "b a");
        assert_eq!(render_prosody(&w), "σ0[0,2) μ0[1,2) μ1[2,2)∅");
    }
}
