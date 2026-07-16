//! 步驟 9 出口:source→AST golden + **round-trip 恆等**(P21:text→IR→text)。

use conlang_language::{Language, SignItem, Stage};

/// canonical 樣例(步驟 8 golden 的超集:加 else 鏈與 sign 內規則)。
const CANONICAL: &str = "\
Feature voice(+voice, -voice)
Symbol m [+sonorant]
Class vowel {a, e, i, o, u}

prosody = μ σ Ft ω φ ι U

distribution {
    /k/ = 0.15
    /t/ = 0.20
}

global trait CorePhonology {
    a => ə / _# @stage word
        else ɐ / _[+cons]
        else e
    ==
    n => m / _[+labial] @stage stem
}

trait VerbCommon {
    syn.provides = VERB
    syn.requires = [agent, patient]
    ==
    valence => 2 / _[+move] @stage word
}

sign go {
    VerbCommon[1]
    phon = /go/
    sem.senses = [ sense s1 { concept = GO } ]
    VerbCommon[2]
    entrenchment = 0.8
    o => ə / _# @stage phrase
}
";

/// round-trip 恆等(P21):canonical 輸入 → parse → dump → 逐位元相同。
#[test]
fn roundtrip_identity_on_canonical_p21() {
    let lang = Language::parse(CANONICAL).expect("canonical must parse");
    assert_eq!(lang.dump(), CANONICAL);
    // 冪等:再 parse 再 dump 仍恆等(無隱藏狀態)
    let again = Language::parse(&lang.dump()).unwrap();
    assert_eq!(again.dump(), CANONICAL);
    assert_eq!(again, lang); // id 依文件序決定性再生(I15-b/P26)
}

/// 非 canonical 輸入(亂序容器、省略 @stage、雜空白)→ parse → dump = canonical 正規化。
#[test]
fn non_canonical_input_normalizes() {
    let messy = "\
Feature voice(+voice, -voice)

sign zz {
    phon = /z/
}

trait Beta {
    x => y
}

global trait Alpha {
    a => b @stage stem
}
";
    let lang = Language::parse(messy).unwrap();
    let dump = lang.dump();
    // global 區段先於 trait、canonical 補 @stage word
    let ia = dump.find("global trait Alpha").unwrap();
    let ib = dump.find("trait Beta").unwrap();
    let iz = dump.find("sign zz").unwrap();
    assert!(ia < ib && ib < iz);
    assert!(dump.contains("x => y @stage word"));
    // 正規化後達成不動點:再 round-trip 恆等
    assert_eq!(Language::parse(&dump).unwrap().dump(), dump);
}

/// source→AST golden:結構斷言(容器/塊/else/引用位置)。
#[test]
fn source_to_ast_shape() {
    let lang = Language::parse(CANONICAL).unwrap();
    let core = lang.trait_named("CorePhonology").unwrap();
    assert!(core.global);
    assert_eq!(core.blocks.len(), 2); // == 切 Block(P27)
    let conlang_language::Item::Rule(r) = &core.blocks[0].items[0] else {
        panic!("expected rule");
    };
    assert_eq!(r.body, "a => ə / _#");
    assert_eq!(r.else_chain, vec!["ɐ / _[+cons]", "e"]); // P22 else 鏈
    assert_eq!(r.stage, Stage::Word);

    let go = lang.sign_named("go").unwrap();
    assert!(matches!(
        &go.items[0],
        SignItem::TraitUse { name, block: 1 } if name == "VerbCommon"
    ));
    assert!(matches!(
        &go.items[3],
        SignItem::TraitUse { name, block: 2 } if name == "VerbCommon"
    )); // 引用位置有語意(P5),保序
    assert!(matches!(&go.items[5], SignItem::Rule(r) if r.stage == Stage::Phrase));
    insta::assert_snapshot!("ast_debug", format!("{lang:#?}"));
}
