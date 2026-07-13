//! M0 步驟 5 出口:範例 8.2–8.5 各自的規則檔經 DSL 管線端到端。
//!
//! 特徵(+nasal / tone / +ATR)一律由規則檔內的 `insert … near` + `dock` 產生
//! (擁有者要求:範例自足,不靠測試注入詞彙旋律;I11 v2 原位記憶使其可行)。
//! 型態括號(8.5)由測試注入(詞條載入層);8.4 的 WBP 莫拉自步驟 7 起由 Parse 宣告產生。

use conlang_core::lifecycle::has_error;
use conlang_core::repr::prosody::{AnchorRef, Level};
use conlang_core::repr::word::{Bracket, MorphUnit, Word};
use conlang_core::repr::notation;
use conlang_dsl::{build_word, compile, run_program, Program};

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

/// 8.2:+nasal 自鼻音聲母規則生成 → dock → spread 只填 Ø、遇 [-sonorant] 阻塞。
#[test]
fn dsl_8_2_nasal_harmony() {
    let p = compile(include_str!("../../../examples/8_2_nasal_harmony.dsl")).unwrap();
    let w = build_word(&p, "mata").unwrap();

    let mut t = String::new();
    let out = derive(&p, "*mata", w, &mut t);
    insta::assert_snapshot!("dsl_8_2", t);

    let nasal = p.tier_named("nasal").unwrap();
    let links = &out.tier(nasal).unwrap().seq[0].links;
    assert!(links.contains(&AnchorRef::new(Level::Segment, 0))); // 源:m(dock)
    assert!(links.contains(&AnchorRef::new(Level::Segment, 1))); // a 被鼻化(延展)
    assert!(!links.contains(&AnchorRef::new(Level::Segment, 2))); // t 阻塞
    assert!(!links.contains(&AnchorRef::new(Level::Segment, 3))); // 其後不達
}

/// 8.3:tonogenesis 生調(σ1 濁聲母 → L~μ1)→ 詞尾元音脫落 → 無核心音節亡(I13)
/// → 調浮游原位(D14/D6,origin 記憶)→ redock 左掛存活莫拉。
#[test]
fn dsl_8_3_stability_and_redock() {
    let p = compile(include_str!("../../../examples/8_3_stability.dsl")).unwrap();
    let w = build_word(&p, "ada").unwrap(); // σ0=a(無聲母) σ1=da(濁)

    let mut t = String::new();
    let out = derive(&p, "*adá (L from tonogenesis)", w, &mut t);
    insta::assert_snapshot!("dsl_8_3", t);

    let tone = p.tier_named("tone").unwrap();
    assert_eq!(notation::render_skeleton(&out, &p.env.syms), "a d");
    assert_eq!(
        notation::render_tier(out.tier(tone).unwrap(), &p.env.vals),
        "L~μ0",
        "調在錨點消失後浮游、再左掛存活莫拉(連調)"
    );
}

/// 8.4:coda 脫落 → 空莫拉存活(keep-empty,I13)→ dominate 修復 → 長元音。
/// WBP 莫拉由 `Parse mora: @vowel | @vowel :: @cons` 宣告產生(步驟 7,全 DSL)。
#[test]
fn dsl_8_4_compensatory_lengthening() {
    let p = compile(include_str!("../../../examples/8_4_compensatory.dsl")).unwrap();
    let w = build_word(&p, "ak").unwrap();
    assert_eq!(
        notation::render_prosody(&w),
        "σ0[0,2) μ0[0,1) μ1[1,2)",
        "Parse 宣告應產生 WBP 莫拉"
    );

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

/// 8.5:+ATR 自 [+atrsrc] 核心規則生成(μ1 的 i)→ dock → 雙向擴散,within stem 出括號即停。
#[test]
fn dsl_8_5_atr_bidirectional_within_stem() {
    let p = compile(include_str!("../../../examples/8_5_atr.dsl")).unwrap();
    let mut w = build_word(&p, "papipa").unwrap(); // μ0(a) μ1(i,+atrsrc) μ2(a)
    // 型態括號(詞條載入層):stem = 前兩音節(segments [0,4));μ2 在括號外
    w.morph.push(Bracket {
        unit: MorphUnit::Stem,
        lo: 0,
        hi: 4,
    });

    let mut t = String::new();
    let out = derive(&p, "*papipa (stem=[0,4))", w, &mut t);
    insta::assert_snapshot!("dsl_8_5", t);

    let atr = p.tier_named("atr").unwrap();
    let tier = out.tier(atr).unwrap();
    assert!(tier.seq[0].links.contains(&AnchorRef::new(Level::Mora, 1))); // 源(dock 於 i 的莫拉)
    assert!(tier.seq[0].links.contains(&AnchorRef::new(Level::Mora, 0))); // 左擴 ✓
    assert!(
        !tier.seq[0].links.contains(&AnchorRef::new(Level::Mora, 2)),
        "μ2 在 stem 括號外,不擴(within stem)"
    );
}
