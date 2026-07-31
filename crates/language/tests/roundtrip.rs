//! 步驟 9 出口(+ I22 語法):source→AST + **round-trip 恆等**(P21:text→IR→text)。

use conlang_language::{Language, SignItem, Stage};

/// 新語法樣例(colon+縮排+維度區塊;else 鏈;`==` block;Name[n])。
const SRC: &str = "\
Feature voice(+voice, -voice)
Symbol m [+sonorant]
Class vowel {a, e, i, o, u}

Prosody mora < syllable < foot < pword < phrase < iphrase < utterance

distribution:
    /k/ = 0.15
    /t/ = 0.20

global trait CorePhonology:
    phon:
        a => ə / _# @stage word
            else ɐ / _[+cons]
            else e
    ==
    phon:
        n => m / _[+labial] @stage stem

trait VerbCommon:
    syn:
        provides = VERB
        requires = [agent, patient]
    ==
    syn:
        arity = 2

sign go:
    VerbCommon[0]
    VerbCommon[1]
    entrenchment = 0.8
    sem:
        senses = [ sense s1 { concept = GO } ]
    phon:
        /go/
        o => ə / _# @stage phrase
";

/// round-trip 恆等(P21):canonical(= 一次正規化的產物)→ parse → dump 逐位元相同;
/// id 依文件序決定性再生(I15-b/P26)。
#[test]
fn roundtrip_identity_on_canonical_p21() {
    let canon = Language::parse(SRC).expect("parse").dump();
    assert_eq!(
        Language::parse(&canon).unwrap().dump(),
        canon,
        "canonical 為不動點"
    );
    let a = Language::parse(&canon).unwrap();
    let b = Language::parse(&canon).unwrap();
    assert_eq!(a, b, "決定性 id");
}

/// 非 canonical(亂序容器、省略 @stage)→ 正規化不動點。
#[test]
fn non_canonical_input_normalizes() {
    let messy = "\
Feature voice(+voice, -voice)

sign zz:
    phon:
        /z/

trait Beta:
    phon:
        x => y

global trait Alpha:
    phon:
        a => b @stage stem
";
    let dump = Language::parse(messy).unwrap().dump();
    let ia = dump.find("global trait Alpha").unwrap();
    let ib = dump.find("trait Beta").unwrap();
    let iz = dump.find("sign zz").unwrap();
    assert!(ia < ib && ib < iz, "區段序:global → trait → sign");
    assert!(
        dump.contains("x => y @stage word"),
        "canonical 補 @stage word"
    );
    assert_eq!(Language::parse(&dump).unwrap().dump(), dump, "不動點");
}

/// source→AST 結構斷言 + golden。
#[test]
fn source_to_ast_shape() {
    let lang = Language::parse(SRC).unwrap();
    let core = lang.trait_named("CorePhonology").unwrap();
    assert!(core.global);
    assert_eq!(core.blocks.len(), 2, "== 切 Block(P27)");
    let SignItem::Rule(r) = &core.blocks[0].items[0] else {
        panic!("expected phon rule");
    };
    assert_eq!(r.body, "a => ə / _#");
    assert_eq!(r.else_chain, vec!["ɐ / _[+cons]", "e"], "P22 else 鏈");
    assert_eq!(r.stage, Stage::Word);

    let go = lang.sign_named("go").unwrap();
    assert!(
        matches!(&go.items[0], SignItem::TraitUse { name, block: Some(0) } if name == "VerbCommon")
    );
    assert!(
        matches!(&go.items[1], SignItem::TraitUse { name, block: Some(1) } if name == "VerbCommon")
    );
    assert!(go
        .items
        .iter()
        .any(|i| matches!(i, SignItem::Rule(r) if r.stage == Stage::Phrase)));
    insta::assert_snapshot!("ast_debug", format!("{lang:#?}"));
}

#[test]
fn slot_features_are_canonical_and_roundtrip() {
    let source = r#"sign Pair:
    syn:
        feature:
            case = enum(nominative, accusative)
            number = enum(singular, plural)
        slots:
            target [Nominal]
            source [Nominal]
        slot_features:
            target.case = nominative
            target.number = $slot.source.syn.number
"#;

    let language = Language::parse(source).expect("parse slot features");
    let pair = language.sign_named("Pair").expect("Pair sign");
    assert!(matches!(
        pair.items.as_slice(),
        [
            SignItem::FeatureDecl(_),
            SignItem::FeatureDecl(_),
            SignItem::Slot(_),
            SignItem::Slot(_),
            SignItem::SlotFeatureBinding(first),
            SignItem::SlotFeatureBinding(second),
        ] if first.slot == "target"
            && first.feature == "case"
            && first.value == "nominative"
            && first.source.line == 10
            && second.slot == "target"
            && second.feature == "number"
            && second.value == "$slot.source.syn.number"
            && second.source.line == 11
    ));

    let canonical = language.dump();
    assert_eq!(
        canonical,
        r#"sign Pair:
    syn:
        slots:
            target [Nominal]
            source [Nominal]
        slot_features:
            target.case = nominative
            target.number = $slot.source.syn.number
        feature:
            case = enum(nominative, accusative)
            number = enum(singular, plural)
"#
    );
    assert_eq!(Language::parse(&canonical).unwrap().dump(), canonical);
}
