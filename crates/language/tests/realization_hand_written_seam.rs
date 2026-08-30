//! P96:**詞界一律來自顯式陳述**——`+` 由作者手寫在模板裡,引擎不推斷。
//!
//! 這兩條是該決策的迴歸守衛。否決自動發縫的理由見
//! `docs/implementation/realization收縮與規則分支_v0.1.md` §2 P96:從空白推斷
//! 是 P85 剛移除的「首段猜測」同一模式,而一律發縫對非串接形態可證明地錯
//! (阿拉伯語詞根三槽一詞素、中綴使一個詞素被切成兩截)。
use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::system::compile_system;
use conlang_language::Language;

const SRC: &str = r#"Feature Type(*cons, vowel)

Symbol a [vowel]
Symbol p [cons]
Symbol b [cons]
Symbol x [cons]

Class vowel {a}

global trait Core:
    phon:
        a => x / _ + a @stage word

trait Fillable:
    syn:
        feature:
            k = enum(one)

sign pa:
    belongs Fillable
    phon:
        /pa/

sign ab:
    belongs Fillable
    phon:
        /ab/

sign Joined:
    syn:
        slots:
            left [Fillable]
            right [Fillable]
    phon:
        /{$slot.left}+{$slot.right}/

sign Fused:
    syn:
        slots:
            left [Fillable]
            right [Fillable]
    phon:
        /{$slot.left}{$slot.right}/
"#;

fn surface(construction: &str) -> String {
    let language = Language::parse(SRC).expect("parse");
    let system = compile_system(language).expect("compile");
    system
        .derive(
            construction,
            &[
                SlotFiller::sign("left", "pa"),
                SlotFiller::sign("right", "ab"),
            ],
            &SlotMap::identity(),
        )
        .expect("derive")
        .surface
}

#[test]
fn a_hand_written_seam_survives_into_the_phon_pipeline() {
    // `expand_phon_template` 是純字串代換,`+` 原樣通過 → `build_phrase` 產生
    // Stem 括號 → `a => x / _ + a @stage word` 在縫上命中:pa+ab → px+ab
    let got = surface("Joined");
    println!("JOINED SURFACE: {got:?}");
    assert!(got.contains('x'), "縫上的 @stage word 規則應命中: {got:?}");
}

#[test]
fn without_a_seam_the_same_rule_cannot_fire() {
    // paab —— 沒有縫,同一條規則沒有環境可命中
    let got = surface("Fused");
    println!("FUSED SURFACE: {got:?}");
    assert!(!got.contains('x'), "無縫則不該命中: {got:?}");
}
