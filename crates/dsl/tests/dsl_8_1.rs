//! M0 步驟 4 出口:8.1 規則檔**全文**經 lexer→parser→lowering→executor 端到端,
//! 四詞推導與步驟 3(Rust API 直呼,`core/tests/tonogenesis_8_1.rs`)語意一致。
//! devoicing 在此已是 DSL 音段規則(I12 通道),不再手動操作。

use conlang_core::repr::notation;
use conlang_dsl::{build_word, compile, run_program, Program};

const RULES: &str = include_str!("../../../examples/8_1_tonogenesis.dsl");

fn render(p: &Program, w: &conlang_core::repr::word::Word) -> String {
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
fn dsl_8_1_end_to_end() {
    let p = compile(RULES).expect("8.1 rule file must compile");
    assert_eq!(p.rules.len(), 5);
    assert_eq!(p.rules[0].name, "tonogenesis");
    assert_eq!(p.rules[0].stmts.len(), 2); // 平行子項(B5 共享一次 match)

    let mut transcript = String::new();
    let mut finals = Vec::new();
    for word in ["pa", "ba", "baba", "a"] {
        let w = build_word(&p, word).expect("build word");
        transcript.push_str(&format!("*{word}\n  {:<14} {}\n", "input", render(&p, &w)));
        let steps = run_program(&p, w).expect("derivation");
        for s in &steps {
            assert!(
                !conlang_core::lifecycle::has_error(&s.issues),
                "rule {}: {:?}",
                s.rule,
                s.issues
            );
            transcript.push_str(&format!("  {:<14} {}\n", s.rule, render(&p, &s.word)));
        }
        transcript.push('\n');
        finals.push(steps.last().unwrap().word.clone());
    }

    insta::assert_snapshot!("dsl_8_1", transcript);

    // 硬斷言(與步驟 3 測試同錨點):
    let tone_id = p.tier_named("tone").unwrap();
    let tone = |w: &conlang_core::repr::word::Word| {
        notation::render_tier(w.tier(tone_id).unwrap(), &p.env.vals)
    };
    // 對立轉移:pa/ba 骨架相同,調 H vs L
    assert_eq!(
        notation::render_skeleton(&finals[0], &p.env.syms),
        notation::render_skeleton(&finals[1], &p.env.syms)
    );
    assert_eq!(tone(&finals[0]), "H~μ0");
    assert_eq!(tone(&finals[1]), "L~μ0");
    // baba:devoicing 出於 DSL;OCP 合併為延展
    assert_eq!(notation::render_skeleton(&finals[2], &p.env.syms), "p a p a");
    assert_eq!(tone(&finals[2]), "L~μ0~μ1");
    // a:fill 補 M
    assert_eq!(tone(&finals[3]), "M~μ0");
}

/// `stage:` 標記(P3/I14)可解析且預設 word。
#[test]
fn stage_marker_parses_with_default_word() {
    let p = compile(RULES).unwrap();
    use conlang_dsl::lower::Stage;
    assert_eq!(p.rules[2].name, "devoicing");
    assert_eq!(p.rules[2].stage, Stage::Word); // 顯式標記
    assert_eq!(p.rules[0].stage, Stage::Word); // 預設
}
