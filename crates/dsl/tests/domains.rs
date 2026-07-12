//! I14:`Prosody` 宣告鏈中的未知名註冊為自定韻律域;Melody 可錨定自定域。

use conlang_core::repr::prosody::Level;
use conlang_dsl::compile;

#[test]
fn prosody_chain_registers_custom_domain_i14() {
    let p = compile(
        "Prosody mora < syllable < colon < pword\nSymbol a\nClass vowel {a}\nMelody tone {H} anchor colon\n",
    )
    .unwrap();
    let colon = p.env.domains.by_name("colon").expect("custom domain registered");
    assert!(matches!(colon, Level::Custom(_)));
    assert_eq!(p.tiers[0].anchor, colon); // Melody 錨定自定域
    assert_eq!(p.env.domains.by_name("mora"), Some(Level::Mora)); // 內建不重複註冊
    assert_eq!(p.env.domains.glyph(colon), "c");
}

#[test]
fn stage_keyword_replaces_level() {
    // `level:` 已退役(I14):現為未知語句 → 規則頭解析失敗
    let err = compile("Symbol a\nClass vowel {a}\nr1:\n    level: word\n");
    assert!(err.is_err(), "old `level:` keyword must not parse");
    let ok = compile("Symbol a\nClass vowel {a}\nMelody tone {H} anchor mora\nr1:\n    stage: phrase\n    merge adjacent-equal\n");
    assert!(ok.is_ok());
    use conlang_dsl::lower::Stage;
    assert_eq!(ok.unwrap().rules[0].stage, Stage::Phrase);
}
