//! 步驟 8 出口:IR dump 格式 golden(P21)——修補05 §10.1 的 .lang 樣例
//! 以 builder 構造 → canonical printer → 快照釘住格式。
//! round-trip 恆等式(text→IR→text)於步驟 9 parser 落地後補上。

use conlang_language::*;

#[test]
fn lang_dump_golden_matches_patch05_sample() {
    let mut l = Language::new();

    // dsl 域宣告(Lexurgy 形,不透明;裁決 docs/13 §4-1)
    l.dsl_decls = vec![
        "Feature voice(+voice, -voice)".into(),
        "Feature sonorant(+sonorant)".into(),
        "Symbol m [+sonorant]".into(),
        "Class vowel {a, e, i, o, u}".into(),
    ];
    l.dsl_decls
        .push("Prosody mora < syllable < foot < pword".into());
    // ⑤ 分佈覆寫(故意亂序;canonical 應排序)
    l.distribution = vec![("/t/".into(), "0.20".into()), ("/k/".into(), "0.15".into())];

    // global trait CorePhonology(兩 block,P27)
    let r1 = l.rule("a => ə / _#", Stage::Word);
    let r2 = l.rule("n => m / _[+labial]", Stage::Stem);
    l.add_trait(TraitDef {
        name: "CorePhonology".into(),
        global: true,
        marker: false,
        blocks: vec![
            Block {
                items: vec![SignItem::Rule(r1)],
            },
            Block {
                items: vec![SignItem::Rule(r2)],
            },
        ],
    });

    // trait VerbCommon(Definition + syn 規則)
    let r3 = l.rule("valence => 2 / _[+move]", Stage::Word);
    l.add_trait(TraitDef {
        name: "VerbCommon".into(),
        global: false,
        marker: false,
        blocks: vec![
            Block {
                items: vec![
                    SignItem::Def(Def {
                        path: "syn.provides".into(),
                        value: "VERB".into(),
                    }),
                    SignItem::Def(Def {
                        path: "syn.requires".into(),
                        value: "[agent, patient]".into(),
                    }),
                ],
            },
            Block {
                items: vec![SignItem::Rule(r3)],
            },
        ],
    });

    // sign go:trait block 顯式插入位置有語意(P5)
    l.add_sign(
        "go",
        vec![
            SignItem::TraitUse {
                name: "VerbCommon".into(),
                block: Some(0),
            },
            SignItem::Def(Def {
                path: "phon".into(),
                value: "/go/".into(),
            }),
            SignItem::Def(Def {
                path: "sem.senses".into(),
                value: "[ sense s1 { concept = GO } ]".into(),
            }),
            SignItem::TraitUse {
                name: "VerbCommon".into(),
                block: Some(1),
            },
            SignItem::Def(Def {
                path: "entrenchment".into(),
                value: "0.8".into(),
            }),
        ],
    );

    let dump = l.dump();
    insta::assert_snapshot!("lang_dump", dump);

    // 冪等:dump 是純函數(P21 無隱藏狀態)
    assert_eq!(dump, l.dump());
}
