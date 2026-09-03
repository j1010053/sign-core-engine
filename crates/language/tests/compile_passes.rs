//! 步驟 10 出口:compile ①–④ 每 pass 一份 dump golden(P21)+ 錯誤案例。
//! I22 語法(colon+縮排+維度區塊);修補07 P38 v0.2:所有 trait 存續。

use conlang_language::compile::{self, CompileError};
use conlang_language::{Block, Def, Language, SignItem, TraitDef};

/// ① Source:global trait(phon 兩規則)、雙 block trait(`==`,Name[n] 引用)、
/// 同 path Def 後者勝(trait 提供 → sign 覆寫)、亂序 stage。
const SOURCE: &str = "\
Feature voice(+voice, -voice)

Prosody mora < syllable < foot < pword < phrase < iphrase < utterance

global trait CorePhonology:
    phon:
        a => ə / _# @stage word
        n => m / _# @stage stem

trait VerbCommon:
    syn:
        provides = VERB
    ==
    entrenchment = 0.5

sign go:
    VerbCommon[0]
    VerbCommon[1]
    syn:
        provides = VERB.motion
    phon:
        /go/
        g => k / #_ @stage stem
        o => ə / _# @stage phrase
    entrenchment = 0.8
";

fn source() -> Language {
    Language::parse(SOURCE).expect("fixture must parse")
}

#[test]
fn per_pass_dump_goldens() {
    let p = compile::compile(&source()).unwrap();
    for (name, lang) in [
        ("pass2_expanded", &p.expanded),
        ("pass3_resolved", &p.resolved),
        ("pass4_ordered", &p.ordered),
    ] {
        let dump = lang.dump();
        insta::assert_snapshot!(name, dump);
        assert_eq!(
            Language::parse(&dump).unwrap().dump(),
            dump,
            "{name} 必須 round-trip 恆等"
        );
    }
    insta::assert_snapshot!("trait_index", format!("{:#?}", p.trait_index));
}

/// ②:全部 `TraitUse` 消去(inline);**所有 trait 存續**(P38 v0.2 分類節點)。
#[test]
fn expansion_inlines_traituse_and_keeps_all_traits() {
    let src = source();
    let e = compile::expand_traits(&src).unwrap();
    assert!(e.signs.iter().flat_map(|s| &s.items).all(|i| !matches!(
        i,
        SignItem::TraitMount {
            kind: conlang_language::TraitMountKind::Whole
                | conlang_language::TraitMountKind::Block(_),
            ..
        }
    )));
    assert!(e.trait_named("CorePhonology").is_some());
    assert!(e.trait_named("VerbCommon").is_some());
    // inline 於引用位置:go 前兩項為 VerbCommon 兩 block 的內容
    let go = e.sign_named("go").unwrap();
    assert!(
        matches!(&go.items[0], SignItem::Def(d) if d.path == "syn.provides" && d.value == "VERB")
    );
    assert!(
        matches!(&go.items[1], SignItem::Def(d) if d.path == "entrenchment" && d.value == "0.5")
    );
}

/// ③:同 path Def 文件序後者勝。
#[test]
fn resolution_last_def_wins() {
    let p = compile::compile(&source()).unwrap();
    let go = p.resolved.sign_named("go").unwrap();
    let vals = |path: &str| -> Vec<String> {
        go.items
            .iter()
            .filter_map(|i| match i {
                SignItem::Def(d) if d.path == path => Some(d.value.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(vals("syn.provides"), ["VERB.motion"], "只留最後一次出現");
    assert_eq!(vals("entrenchment"), ["0.8"]);
}

/// ④:Rule 依 stem→word→phrase 穩定排序。
#[test]
fn stage_sort_orders_rules() {
    use conlang_language::Stage::*;
    let p = compile::compile(&source()).unwrap();
    let go = p.ordered.sign_named("go").unwrap();
    let stages: Vec<_> = go
        .items
        .iter()
        .filter_map(|i| match i {
            SignItem::Rule(r) => Some(r.stage),
            _ => None,
        })
        .collect();
    assert_eq!(stages, [Stem, Phrase]); // go 有 stem(g=>k)、phrase(o=>ə)
    let core = p.ordered.trait_named("CorePhonology").unwrap();
    let ts: Vec<_> = core.blocks[0]
        .items
        .iter()
        .filter_map(|i| match i {
            SignItem::Rule(r) => Some(r.stage),
            _ => None,
        })
        .collect();
    assert_eq!(ts, [Stem, Word], "global blocks 展平 + 排序");
}

#[test]
fn stage_sort_keeps_feature_rules_in_the_same_dispatch_stream() {
    use conlang_language::Stage::*;

    let source = Language::parse(
        "sign staged:\n    syn:\n        feature:\n            mark = enum(on, off)\n            mark => on @stage phrase\n        tam.past => ready @stage stem\n",
    )
    .unwrap();
    let ordered = compile::compile(&source).unwrap().ordered;
    let staged = ordered.sign_named("staged").unwrap();
    let sequence = staged
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::Rule(rule) => Some(("rule", rule.stage, rule.body.as_str())),
            SignItem::FeatureRule(rule) => Some(("feature", rule.stage, rule.body.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sequence,
        [
            ("rule", Stem, "tam.past => ready"),
            ("feature", Phrase, "mark => on")
        ]
    );
}

#[test]
fn passes_are_pure_and_idempotent() {
    let src = source();
    let p = compile::compile(&src).unwrap();
    assert_eq!(compile::expand_traits(&src).unwrap(), p.expanded);
    assert_eq!(compile::resolve(&p.expanded), p.resolved);
    assert_eq!(compile::order_stages(&p.resolved), p.ordered);
    assert_eq!(compile::expand_traits(&p.expanded).unwrap(), p.expanded);
    assert_eq!(compile::resolve(&p.resolved), p.resolved);
    assert_eq!(compile::order_stages(&p.ordered), p.ordered);
    let q = compile::compile(&src).unwrap();
    assert_eq!(q.ordered.dump(), p.ordered.dump());
}

// ── 錯誤案例 ──

#[test]
fn error_unknown_trait() {
    let l = Language::parse("sign x:\n    Missing[1]\n").unwrap();
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::UnknownTrait {
            sign: "x".into(),
            name: "Missing".into()
        }
    );
}

#[test]
fn error_block_out_of_range() {
    // 0 起算:1-block trait 的合法索引只有 0;`T[1]` 越界。
    let l =
        Language::parse("trait T:\n    syn:\n        tam.past = 1\n\nsign x:\n    T[1]\n").unwrap();
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::BlockOutOfRange {
            sign: "x".into(),
            name: "T".into(),
            block: 1,
            blocks: 1
        }
    );
}

/// P5:全 block 強制顯式——2-block trait 只用 `T[0]` 而漏 block 1 = 編譯錯誤。
#[test]
fn error_incomplete_trait_use_p5() {
    let l = Language::parse(
        "trait T:\n    syn:\n        tam.past = 1\n    ==\n    syn:\n        tam.present = 2\n\nsign x:\n    T[0]\n",
    )
    .unwrap();
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::IncompleteTraitUse {
            sign: "x".into(),
            name: "T".into(),
            missing: 1
        }
    );
}

/// 裸 `T`(整個 trait):**分塊 trait 裸引用 = 全 block 依序 inline**(無 P5 缺塊錯);
/// 未分塊 trait 寫 `T[]` = 那唯一 block。兩者皆「整個 trait」。
#[test]
fn bare_and_empty_brackets_inline_whole_trait() {
    // 分塊 trait(2 block)裸引用 → 兩 block 全展開,無 IncompleteTraitUse
    let split = Language::parse(
        "trait T:\n    syn:\n        tam.past = 1\n    ==\n    syn:\n        tam.present = 2\n\nsign x:\n    T\n",
    )
    .unwrap();
    let e = compile::expand_traits(&split).unwrap();
    let x = e.sign_named("x").unwrap();
    let paths: Vec<&str> = x
        .items
        .iter()
        .filter_map(|i| match i {
            SignItem::Def(d) => Some(d.path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        paths,
        ["syn.tam.past", "syn.tam.present"],
        "裸 T 展開全部 block"
    );
    assert!(!x.items.iter().any(|i| matches!(
        i,
        SignItem::TraitMount {
            kind: conlang_language::TraitMountKind::Whole
                | conlang_language::TraitMountKind::Block(_),
            ..
        }
    )));

    // 未分塊 trait(1 block)寫 `T[]` → 整個 trait(等同裸 T),canonical 印為裸 `T`
    let unsplit =
        Language::parse("trait T:\n    syn:\n        tam.past = 1\n\nsign y:\n    T[]\n").unwrap();
    assert!(
        unsplit.dump().contains("    T\n"),
        "T[] 正規化為裸 T:\n{}",
        unsplit.dump()
    );
    let y = compile::expand_traits(&unsplit).unwrap();
    let yp: Vec<&str> = y
        .sign_named("y")
        .unwrap()
        .items
        .iter()
        .filter_map(|i| match i {
            SignItem::Def(d) => Some(d.path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(yp, ["syn.tam.past"]);
}

#[test]
fn error_duplicate_names() {
    let mut l = Language::new();
    let t = TraitDef {
        name: "T".into(),
        global: false,
        marker: false,
        type_params: vec![],
        blocks: vec![Block::default()],
    };
    l.add_trait(t.clone());
    l.add_trait(t);
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::DuplicateTrait("T".into())
    );

    let mut l = Language::new();
    l.add_sign(
        "x",
        vec![SignItem::Def(Def {
            path: "phon".into(),
            value: "/a/".into(),
        })],
    );
    l.add_sign("x", vec![]);
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::DuplicateSign("x".into())
    );
}
