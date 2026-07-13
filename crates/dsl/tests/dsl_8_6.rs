//! M0 步驟 6 出口:範例 8.6(Bantu 三連發,Scan)端到端 = M0 最後一案。

use conlang_core::lifecycle::has_error;
use conlang_core::repr::{notation, Word};
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

#[test]
fn dsl_8_6_bantu_scan() {
    let p = compile(include_str!("../../../examples/8_6_bantu.dsl")).unwrap();
    let tone = p.tier_named("tone").unwrap();
    let mut transcript = String::new();
    let mut finals = Vec::new();
    for word in ["bapa", "bababa"] {
        let w = build_word(&p, word).unwrap();
        transcript.push_str(&format!("*{word}\n  {:<28} {}\n", "input", render(&p, &w)));
        let steps = run_program(&p, w).unwrap();
        for s in &steps {
            assert!(!has_error(&s.issues), "rule {}: {:?}", s.rule, s.issues);
            transcript.push_str(&format!("  {:<28} {}\n", s.rule, render(&p, &s.word)));
        }
        transcript.push('\n');
        finals.push(steps.last().unwrap().word.clone());
    }
    insta::assert_snapshot!("dsl_8_6", transcript);

    // *bapa:tonogenesis H~μ1 +(a)倒數第二 σ 指派 H~μ0 → 相鄰 HH →(b)Meeussen 後者變 L
    let t0 = finals[0].tier(tone).unwrap();
    let r0 = notation::render_tier(t0, &p.env.vals);
    assert!(r0.contains("H~μ0") && r0.contains("L~μ1"), "Meeussen: {r0}");
    // *bababa:全 Ø →(a)H~μ1(倒數第二)→(c)第一個 Ø 莫拉 μ0 指派 H;μ2 保持 Ø
    let t1 = finals[1].tier(tone).unwrap();
    let r1 = notation::render_tier(t1, &p.env.vals);
    assert!(r1.contains("H~μ1") && r1.contains("H~μ0"), "(a)+(c): {r1}");
    assert!(!r1.contains("μ2"), "μ2 保持 Ø(fill 才落值): {r1}");
}
