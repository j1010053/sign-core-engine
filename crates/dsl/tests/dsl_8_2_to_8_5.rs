//! M0 步驟 5 出口:範例 8.2–8.5 各自的規則檔經 DSL 管線端到端。
//!
//! 詞彙給定的旋律(8.2 的 +nasal、8.3 的 H、8.5 的 +ATR;docs/03 §3.5)與
//! 型態括號(8.5;docs/03 §4)、weight-by-position 莫拉(8.4;Parse 宣告留步驟 6+)
//! 由測試端注入——這些屬詞條載入層,非規則引擎職責。

use conlang_core::lifecycle::has_error;
use conlang_core::repr::melody::Autoseg;
use conlang_core::repr::prosody::{AnchorRef, Level, Span};
use conlang_core::repr::word::{Bracket, MorphUnit, Seg, Word};
use conlang_core::repr::{notation, FeatBits, ValId};
use conlang_dsl::{build_word, compile, run_program, Program};

fn val_named(p: &Program, name: &str) -> ValId {
    (0..p.env.vals.len() as u32)
        .map(ValId)
        .find(|&v| p.env.vals.resolve(v) == Some(name))
        .unwrap_or_else(|| panic!("value {name} not interned"))
}

fn render(p: &Program, w: &Word) -> String {
    let mut s = notation::render_skeleton(w, &p.env.syms);
    for t in &w.melodies {
        let tier = notation::render_tier(t, &p.env.vals);
        if !tier.is_empty() {
            s.push_str("  |  ");
            s.push_str(&tier);
        }
    }
    s
}

fn derive(p: &Program, label: &str, w: Word, transcript: &mut String) -> Word {
    transcript.push_str(&format!("{label}\n  {:<18} {}\n", "input", render(p, &w)));
    let steps = run_program(p, w).expect("derivation");
    for s in &steps {
        assert!(!has_error(&s.issues), "rule {}: {:?}", s.rule, s.issues);
        transcript.push_str(&format!("  {:<18} {}\n", s.rule, render(p, &s.word)));
    }
    transcript.push('\n');
    steps.last().unwrap().word.clone()
}

/// 8.2:spread 只填 Ø、遇 [-sonorant] 阻塞停下;私有特徵無負值(D12)。
#[test]
fn dsl_8_2_nasal_harmony() {
    let p = compile(include_str!("../../../examples/8_2_nasal_harmony.dsl")).unwrap();
    let mut w = build_word(&p, "mata").unwrap();
    // 詞彙給定:+nasal 掛 m(seg0)
    let nasal = p.tier_named("nasal").unwrap();
    let plus = val_named(&p, "+nasal");
    w.tier_mut(nasal)
        .unwrap()
        .seq
        .push(Autoseg::linked(plus, vec![AnchorRef::new(Level::Segment, 0)]));

    let mut t = String::new();
    let out = derive(&p, "*mata (+nasal on m)", w, &mut t);
    insta::assert_snapshot!("dsl_8_2", t);

    let links = &out.tier(nasal).unwrap().seq[0].links;
    assert!(links.contains(&AnchorRef::new(Level::Segment, 1))); // a 被鼻化(延展)
    assert!(!links.contains(&AnchorRef::new(Level::Segment, 2))); // t 阻塞
    assert!(!links.contains(&AnchorRef::new(Level::Segment, 3))); // 其後不達
}

/// 8.3:詞尾元音脫落 → 無核心音節亡(I13)→ 調浮游原位(D14/D6)→ redock 左掛。
#[test]
fn dsl_8_3_stability_and_redock() {
    let p = compile(include_str!("../../../examples/8_3_stability.dsl")).unwrap();
    let mut w = build_word(&p, "tapa").unwrap();
    let tone = p.tier_named("tone").unwrap();
    let h = val_named(&p, "H");
    // 詞彙給定:H 掛末莫拉 μ1;μ0 無調
    w.tier_mut(tone)
        .unwrap()
        .seq
        .push(Autoseg::linked(h, vec![AnchorRef::new(Level::Mora, 1)]));

    let mut t = String::new();
    let out = derive(&p, "*tapá (H on final mora)", w, &mut t);
    insta::assert_snapshot!("dsl_8_3", t);

    let tier = out.tier(tone).unwrap();
    assert_eq!(
        notation::render_tier(tier, &p.env.vals),
        "H~μ0",
        "調在錨點消失後浮游、再左掛存活莫拉(連調)"
    );
    assert_eq!(notation::render_skeleton(&out, &p.env.syms), "t a p");
}

/// 8.4:coda 脫落 → 空莫拉存活(keep-empty;音節仍有核心,I13)→ dominate 修復 → 長元音。
#[test]
fn dsl_8_4_compensatory_lengthening() {
    let p = compile(include_str!("../../../examples/8_4_compensatory.dsl")).unwrap();
    // 手動建構 weight-by-position:a k;μ0=核心、μ1=coda
    let mut w = Word::new();
    let a = (0..p.env.syms.len() as u32)
        .map(conlang_core::repr::intern::SymId)
        .find(|&s| p.env.syms.resolve(s) == Some("a"))
        .unwrap();
    let k = (0..p.env.syms.len() as u32)
        .map(conlang_core::repr::intern::SymId)
        .find(|&s| p.env.syms.resolve(s) == Some("k"))
        .unwrap();
    w.skeleton.push(Seg::new(a, FeatBits::EMPTY));
    w.skeleton.push(Seg::new(k, FeatBits::EMPTY));
    w.prosody.syllables.push(Span::new(0, 2));
    w.prosody.moras.push(Span::new(0, 1));
    w.prosody.moras.push(Span::new(1, 2));

    let mut t = String::new();
    let out = derive(&p, "*ak (weight-by-position)", w, &mut t);
    insta::assert_snapshot!("dsl_8_4", t);

    assert_eq!(notation::render_prosody(&out), "σ0[0,1) μ0[0,1) μ1[0,1)");
    assert_eq!(
        out.prosody.moras.iter().filter(|m| m.contains_idx(0)).count(),
        2,
        "a 承兩莫拉 = 長元音(spell-out 渲染 aː 是步驟 6 的事)"
    );
}

/// 8.5:+ATR 自詞根雙向擴散,within stem 出括號即停;anchor mora 等價表達(I13 附註)。
#[test]
fn dsl_8_5_atr_bidirectional_within_stem() {
    let p = compile(include_str!("../../../examples/8_5_atr.dsl")).unwrap();
    let mut w = build_word(&p, "papapa").unwrap(); // μ0,μ1,μ2
    let atr = p.tier_named("atr").unwrap();
    let plus = val_named(&p, "+ATR");
    // 型態括號:stem = 前兩音節(segments [0,4));μ2 在括號外
    w.morph.push(Bracket {
        unit: MorphUnit::Stem,
        lo: 0,
        hi: 4,
    });
    // 詞彙給定:+ATR 掛詞根中莫拉 μ1
    w.tier_mut(atr)
        .unwrap()
        .seq
        .push(Autoseg::linked(plus, vec![AnchorRef::new(Level::Mora, 1)]));

    let mut t = String::new();
    let out = derive(&p, "*papapa (stem=[0,4), +ATR on μ1)", w, &mut t);
    insta::assert_snapshot!("dsl_8_5", t);

    let tier = out.tier(atr).unwrap();
    assert!(tier.seq[0].links.contains(&AnchorRef::new(Level::Mora, 0))); // 左擴 ✓
    assert!(tier.seq[0].links.contains(&AnchorRef::new(Level::Mora, 1)));
    assert!(
        !tier.seq[0].links.contains(&AnchorRef::new(Level::Mora, 2)),
        "μ2 在 stem 括號外,不擴(within stem)"
    );
}
