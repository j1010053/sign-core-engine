//! spellout:末端整合的**純函數**(C11;執行語意 §6 唯一權威)。
//!
//! `Representation → Surface`,僅 lookup / flatten / render——禁 rewrite/search/
//! spread/Scan,不產生 Representation'。宣告(C10):order(多層實現次序)、
//! empty(殘餘 Ø 落值)、floating(殘餘浮游處置)、contour(多承載線性化,
//! **D27 衝突檢查**:多值無對應 = error)。phrase-level 規則掛鉤(P3):M0 傳空集。
//! 韻律投影:雙莫拉音段 → 長音 `ː`(8.4 的收尾)。

use crate::lifecycle::EngineError;
use crate::repr::intern::{SymId, ValId};
use crate::repr::prosody::Level;
use crate::repr::word::Word;
use crate::repr::Env;

/// 殘餘浮游處置(§6;M0 支援 drop 與 error)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FloatingPolicy {
    #[default]
    Drop,
    Error,
}

/// Spell-out 宣告(C10)。
#[derive(Debug, Clone, Default)]
pub struct SpelloutSpec {
    /// 多層落同一音段的實現次序(必需;M0 依序渲染)。
    pub order: Vec<SymId>,
    /// 殘餘 Ø 落值:tier → 表層值(None = bare,不帶)。
    pub empty: Vec<(SymId, Option<ValId>)>,
    pub floating: FloatingPolicy,
    /// 多承載線性化:(tier, 有序值簇) → 表層名(D27 的唯一整合點)。
    pub contour: Vec<(SymId, Vec<ValId>, String)>,
    /// phrase-level 規則掛鉤(P3):M0 為空集,行為同無;步驟 8+ 接 Grammar Store。
    pub phrase_rules: Vec<()>,
}

/// 純函數拼讀:逐音段 render 符號 + 長度(多莫拉→`ː`)+ 各層實現(`[值]` 註記)。
pub fn spell_out(w: &Word, env: &Env, spec: &SpelloutSpec) -> Result<String, EngineError> {
    debug_assert!(spec.phrase_rules.is_empty()); // M0:掛鉤存在、集合為空
    if spec.floating == FloatingPolicy::Error {
        for t in &w.melodies {
            if let Some(&si) = t.floating_indices().first() {
                let _ = si;
                return Err(EngineError::TierNotFound(t.name)); // M0 佔位錯誤型別;diag 步驟 7+
            }
        }
    }
    let mut out = String::new();
    for (i, seg) in w.skeleton.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(env.syms.resolve(seg.sym).unwrap_or("?"));
        // 韻律投影:承載莫拉數 >1 → 長音
        let bearing = w
            .prosody
            .moras
            .iter()
            .filter(|m| m.contains_idx(i as u32))
            .count();
        if bearing > 1 {
            out.push('ː');
        }
        // 各層實現(依 order;僅列 order 中的層)
        for &tname in &spec.order {
            let Some(t) = w.tier(tname) else { continue };
            // 此音段的錨點集(segment 錨 = 自身;mora 錨 = 涵蓋此音段的莫拉)
            let mut vals: Vec<ValId> = Vec::new();
            match t.anchor {
                Level::Segment => {
                    for si in t.bearers_of(crate::repr::prosody::AnchorRef::new(
                        Level::Segment,
                        i as u32,
                    )) {
                        vals.push(t.seq[si].val);
                    }
                }
                _ => {
                    if let Some(spans) = w.prosody.level(t.anchor) {
                        for (mi, m) in spans.iter().enumerate() {
                            if m.contains_idx(i as u32) {
                                for si in t.bearers_of(crate::repr::prosody::AnchorRef::new(
                                    t.anchor, mi as u32,
                                )) {
                                    vals.push(t.seq[si].val);
                                }
                            }
                        }
                    }
                }
            }
            match vals.len() {
                0 => {
                    // 殘餘 Ø:查 empty 落值(bare = 不帶)
                    if let Some((_, Some(v))) =
                        spec.empty.iter().find(|(n, _)| *n == tname)
                    {
                        // 只在音段確實有該層錨點時落值(onset 無莫拉 = 無錨點,不帶)
                        let has_anchor = match t.anchor {
                            Level::Segment => true,
                            _ => w
                                .prosody
                                .level(t.anchor)
                                .is_some_and(|s| s.iter().any(|m| m.contains_idx(i as u32))),
                        };
                        if has_anchor {
                            out.push_str(&format!("[{}]", env.vals.resolve(*v).unwrap_or("?")));
                        }
                    }
                }
                1 => out.push_str(&format!("[{}]", env.vals.resolve(vals[0]).unwrap_or("?"))),
                _ => {
                    // 多承載:contour 查表;無對應 = error(D27)
                    let hit = spec
                        .contour
                        .iter()
                        .find(|(n, c, _)| *n == tname && *c == vals);
                    match hit {
                        Some((_, _, name)) => out.push_str(&format!("[{name}]")),
                        None => {
                            return Err(EngineError::ContourUnmapped {
                                tier: tname,
                                at: i,
                            })
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::feature::FeatBits;
    use crate::repr::melody::{Autoseg, MelodyTier};
    use crate::repr::prosody::{AnchorRef, Span};
    use crate::repr::word::Seg;

    fn setup() -> (Env, Word, SymId, ValId, ValId, ValId) {
        let mut env = Env::new();
        let a = env.syms.intern("a");
        let tone = env.syms.intern("tone");
        let h = env.vals.intern("H");
        let l = env.vals.intern("L");
        let m = env.vals.intern("M");
        let mut w = Word::new();
        w.skeleton.push(Seg::new(a, FeatBits::EMPTY));
        w.prosody.syllables.push(Span::new(0, 1));
        w.prosody.moras.push(Span::new(0, 1));
        w.melodies
            .push(MelodyTier::new(tone, Level::Mora, vec![h, l, m]));
        (env, w, tone, h, l, m)
    }

    /// 長元音渲染(8.4 收尾)+ 單值實現 + Ø 落值。
    #[test]
    fn renders_length_value_and_empty_policy() {
        let (env, mut w, tone, h, _l, m) = setup();
        w.prosody.moras.push(Span::new(0, 1)); // 雙莫拉 = 長元音
        w.tier_mut(tone)
            .unwrap()
            .seq
            .push(Autoseg::linked(h, vec![AnchorRef::new(Level::Mora, 0)]));
        let spec = SpelloutSpec {
            order: vec![tone],
            empty: vec![(tone, Some(m))],
            ..Default::default()
        };
        // μ0 有 H、μ1 Ø→兩莫拉:vals 收到 [H](μ1 無承載不湊值)…長音 ː + [H]
        assert_eq!(spell_out(&w, &env, &spec).unwrap(), "aː[H]");

        // 全 Ø → empty 落 M
        let (env2, w2, tone2, _h, _l, m2) = setup();
        let spec2 = SpelloutSpec {
            order: vec![tone2],
            empty: vec![(tone2, Some(m2))],
            ..Default::default()
        };
        assert_eq!(spell_out(&w2, &env2, &spec2).unwrap(), "a[M]");
    }

    /// D27:多承載無 contour 對應 = error;有對應即線性化。
    #[test]
    fn contour_lookup_and_d27_error() {
        let (env, mut w, tone, h, l, _m) = setup();
        let m0 = AnchorRef::new(Level::Mora, 0);
        w.tier_mut(tone).unwrap().seq.push(Autoseg::linked(h, vec![m0]));
        w.tier_mut(tone).unwrap().seq.push(Autoseg::linked(l, vec![m0]));
        let bare = SpelloutSpec {
            order: vec![tone],
            ..Default::default()
        };
        assert!(matches!(
            spell_out(&w, &env, &bare).unwrap_err(),
            EngineError::ContourUnmapped { .. }
        ));
        let spec = SpelloutSpec {
            order: vec![tone],
            contour: vec![(tone, vec![h, l], "falling".into())],
            ..Default::default()
        };
        assert_eq!(spell_out(&w, &env, &spec).unwrap(), "a[falling]");
    }
}
