//! 步驟 12 出口:**詞根+詞綴組合 → 循環 → 表層;surface sandhi 首測**。
//!
//! 規則語意錨定上游 stages.rs 已驗證模式(獨立 oracle);組合/cophonology/
//! 驅動等價性/括號穿越(協作規範契約 2)為本 crate 新斷言。

use conlang_language::codegen::compile_full;
use conlang_language::word::{self, Component, PhraseSpec, WordError};
use conlang_language::Language;
use tshiatun_core::repr::word::MorphUnit;
use tshiatun_dsl::run_program;

fn artifacts() -> conlang_language::codegen::Artifacts {
    let src = include_str!("fixtures/compose.lang");
    compile_full(&Language::parse(src).expect("fixture parses")).expect("①–⑤")
}

fn ring(names: &[&str]) -> Component {
    Component::Ring(names.iter().copied().map(Component::sign).collect())
}

/// 🔑 出口案例:詞根+詞綴組合成一個 ω、加第二個 ω 成片語;
/// stem 環(詞幹內)→ word(跨詞幹縫)→ phrase(跨詞縫 sandhi)逐段命中。
#[test]
fn root_affix_composition_through_cycles_to_surface() {
    let a = artifacts();
    let spec = PhraseSpec(vec![ring(&["root_pa", "suffix_ap"]), Component::sign("root_pa")]);
    let d = word::derive(&a, &spec).expect("derive");

    assert_eq!(d.input_text, "pa+ap pa");
    // stem:`p a => b a` 只在詞幹內命中(pa→ba;跨縫 p+a 不命中)
    // word:`a => x / _ + a` 跨詞幹縫命中(ba+ap → bx+ap)
    // phrase:`p => b / _ ## b` 跨詞縫 sandhi(…ap ba → …ab ba)
    assert_eq!(d.surface, "bx+ab ba", "input={}", d.input_text);
    assert_eq!(d.steps_per_stage, [1, 1, 1]);

    // 括號穿越(協作規範契約 2):跑完規則後成分邊界不錯位
    let mut words: Vec<_> = d
        .word
        .morph
        .iter()
        .filter(|b| b.unit == MorphUnit::Word)
        .map(|b| (b.lo, b.hi))
        .collect();
    words.sort_unstable();
    assert_eq!(words, vec![(0, 4), (4, 6)]);
    let mut stems: Vec<_> = d
        .word
        .morph
        .iter()
        .filter(|b| b.unit == MorphUnit::Stem)
        .map(|b| (b.lo, b.hi))
        .collect();
    stems.sort_unstable();
    assert_eq!(stems, vec![(0, 2), (2, 4), (4, 6)]);
}

/// 近失敗負例:同素材不組合(單 sign 兩個 ω、無詞幹縫)→ word 規則不命中、
/// sandhi 條件不同 → 表層不同。證明命中確實來自組合結構,非規則湊巧。
#[test]
fn without_composition_the_cycle_rules_do_not_fire() {
    let a = artifacts();
    let spec = PhraseSpec(vec![Component::sign("root_pa"), Component::sign("root_pa")]);
    let d = word::derive(&a, &spec).expect("derive");
    assert_eq!(d.input_text, "pa pa");
    // stem:pa→ba(兩詞);word:無縫不命中;phrase:junction a##b 不合 p _ ## b
    assert_eq!(d.surface, "ba ba");
}

/// cophonology(P3/P4):sign 局部 stem 規則只作用於自己的葉——
/// clitic 的 `a => x / # _` 改寫 clitic 自身,root 完全不受影響。
#[test]
fn cophonology_applies_to_its_own_sign_only() {
    let a = artifacts();
    let with_clitic = PhraseSpec(vec![ring(&["root_ap", "clitic_x"])]);
    let d = word::derive(&a, &with_clitic).expect("derive");
    assert_eq!(d.input_text, "ap+xp", "clitic 葉上 a→x;root 的 a 不受影響");

    // 控制組:同 UR 但無局部規則的 suffix → 無 x
    let control = PhraseSpec(vec![ring(&["root_ap", "suffix_ap"])]);
    let c = word::derive(&a, &control).expect("derive");
    assert_eq!(c.input_text, "ap+ap");
    assert_ne!(d.surface, c.surface);
}

/// 驅動等價性(metamorphic):對展平組合,三 stage 切片串跑 ≡ 單趟
/// `run_program`(④ 已排序)。表層與末狀態逐位元一致。
#[test]
fn staged_driver_equals_single_pass_on_flat_composition() {
    let a = artifacts();
    for spec in [
        PhraseSpec(vec![ring(&["root_pa", "suffix_ap"]), Component::sign("root_pa")]),
        PhraseSpec(vec![ring(&["root_ap", "clitic_x"])]),
        PhraseSpec(vec![Component::sign("root_ap")]),
    ] {
        let d = word::derive(&a, &spec).expect("staged");
        let w0 = word::build_word(&a, &spec).expect("build");
        let single = run_program(&a.grammar.program, w0.clone()).expect("single pass");
        let last = single.last().map(|s| &s.word).unwrap_or(&w0);
        assert_eq!(
            format!("{:?}", d.word),
            format!("{last:?}"),
            "切片驅動與單趟必須同末狀態"
        );
    }
}

/// P1:同一組合重複導出,結果決定性且無狀態殘留(Word 用後即棄)。
#[test]
fn derivation_is_deterministic_and_stateless() {
    let a = artifacts();
    let spec = PhraseSpec(vec![ring(&["root_pa", "suffix_ap"])]);
    let d1 = word::derive(&a, &spec).expect("first");
    let d2 = word::derive(&a, &spec).expect("second");
    assert_eq!(d1.surface, d2.surface);
    assert_eq!(format!("{:?}", d1.word), format!("{:?}", d2.word));
}

// ── 顯式拒絕/錯誤定位 ──

#[test]
fn unknown_sign_and_missing_or_malformed_ur_are_located_errors() {
    let a = artifacts();
    let e = word::derive(&a, &PhraseSpec(vec![Component::sign("ghost")])).unwrap_err();
    assert!(matches!(e, WordError::UnknownSign(n) if n == "ghost"));

    let e = word::derive(&a, &PhraseSpec(vec![Component::sign("no_ur")])).unwrap_err();
    assert!(matches!(e, WordError::UrMissing(n) if n == "no_ur"));

    let e = word::derive(&a, &PhraseSpec(vec![Component::sign("bad_ur")])).unwrap_err();
    assert!(matches!(e, WordError::UrMalformed { sign, .. } if sign == "bad_ur"));
}

/// P3:cophonology 只界定於 stem 層——word 層局部規則顯式拒絕。
#[test]
fn non_stem_sign_rule_is_rejected() {
    let a = artifacts();
    let e = word::derive(&a, &PhraseSpec(vec![Component::sign("clitic_word_rule")])).unwrap_err();
    assert!(
        matches!(e, WordError::UnsupportedSignRuleStage { ref sign, stage: "word" } if sign == "clitic_word_rule"),
        "{e:?}"
    );
}

/// I18:cophonology M1 子集 = 音段效果;留下旋律殘留(浮游調)顯式拒絕。
#[test]
fn melodic_cophonology_is_rejected_explicitly() {
    let a = artifacts();
    let e = word::derive(&a, &PhraseSpec(vec![Component::sign("clitic_tonal")])).unwrap_err();
    assert!(
        matches!(e, WordError::CophonologyNonSegmental { ref sign } if sign == "clitic_tonal"),
        "{e:?}"
    );
}
