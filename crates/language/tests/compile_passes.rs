//! 步驟 10 出口:compile ①–④ 每 pass 一份 dump golden(P21)+ 錯誤案例。
//! ①–④ 全是合法 Language:每份 golden 都能 re-parse 且 round-trip 恆等。

use conlang_language::compile::{self, CompileError};
use conlang_language::{Block, Item, Language, SignItem, TraitDef};

/// ① Source 樣例:global trait(存續)、雙 block trait(引用位置分插)、
/// 同 path Def 覆寫(trait 提供 → sign 本體改寫)、亂序 stage(phrase 先寫)。
const SOURCE: &str = "\
Feature voice(+voice, -voice)

prosody = μ σ Ft ω φ ι U

global trait CorePhonology {
    a => ə / _# @stage word
        else e
    ==
    n => m / _[+labial] @stage stem
}

trait VerbCommon {
    syn.provides = VERB
    valence => 2 / _[+move] @stage word
    ==
    entrenchment = 0.5
}

sign go {
    o => ə / _# @stage phrase
    VerbCommon[1]
    phon = /go/
    VerbCommon[2]
    syn.provides = VERB.motion
    g => k / #_ @stage stem
    entrenchment = 0.8
}
";

fn source() -> Language {
    Language::parse(SOURCE).expect("fixture must parse")
}

/// 每 pass 一份 dump golden;且每份產物皆為合法 Language(re-parse 恆等)。
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
        // ①–④ 全是合法 Language(P21 progressive lowering)
        assert_eq!(
            Language::parse(&dump).unwrap().dump(),
            dump,
            "{name} 必須 round-trip 恆等"
        );
    }
    insta::assert_snapshot!("trait_index", format!("{:#?}", p.trait_index));
}

/// ②(I16-a):非 global trait 與全部 TraitUse 消失;global trait 原樣存續。
#[test]
fn expansion_eliminates_traits_but_keeps_global() {
    let src = source();
    let e = compile::expand_traits(&src).unwrap();
    assert!(e.traits.iter().all(|t| t.global));
    assert!(e
        .signs
        .iter()
        .flat_map(|s| &s.items)
        .all(|i| !matches!(i, SignItem::TraitUse { .. })));
    // global trait 展開恆等
    assert_eq!(
        e.trait_named("CorePhonology"),
        src.trait_named("CorePhonology")
    );
    // inline 位於引用位置(P5 位置語意):VerbCommon[1] 的 Def 出現在 phrase 規則之後
    let go = e.sign_named("go").unwrap();
    assert!(matches!(&go.items[0], SignItem::Rule(r) if r.stage == conlang_language::Stage::Phrase));
    assert!(matches!(&go.items[1], SignItem::Def(d) if d.path == "syn.provides" && d.value == "VERB"));
}

/// ③(I16-c):同 path Def 文件序後者勝(位置語意);Rule 不合併不去重。
#[test]
fn resolution_last_def_wins() {
    let p = compile::compile(&source()).unwrap();
    let go = p.resolved.sign_named("go").unwrap();
    let provides: Vec<&str> = go
        .items
        .iter()
        .filter_map(|i| match i {
            SignItem::Def(d) if d.path == "syn.provides" => Some(d.value.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(provides, ["VERB.motion"], "只留最後一次出現");
    let entrench: Vec<&str> = go
        .items
        .iter()
        .filter_map(|i| match i {
            SignItem::Def(d) if d.path == "entrenchment" => Some(d.value.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(entrench, ["0.8"]);
    // Rule 一條不少
    let n_rules = |l: &Language| {
        l.sign_named("go")
            .unwrap()
            .items
            .iter()
            .filter(|i| matches!(i, SignItem::Rule(_)))
            .count()
    };
    assert_eq!(n_rules(&p.resolved), n_rules(&p.expanded));
}

/// ④(I16-d):Rule 槽位間 stem→word→phrase 穩定排序,Def 原地不動;
/// global trait blocks 展平為單 block。
#[test]
fn stage_sort_permutes_rules_only() {
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
    use conlang_language::Stage::*;
    assert_eq!(stages, [Stem, Word, Phrase]);
    // Def 槽位不動:與 ③ 逐位置比對,非 Rule 項目完全相同
    let before = &p.resolved.sign_named("go").unwrap().items;
    for (a, b) in before.iter().zip(&go.items) {
        if !matches!(a, SignItem::Rule(_)) {
            assert_eq!(a, b, "Def 槽位在 ④ 不得移動");
        } else {
            assert!(matches!(b, SignItem::Rule(_)), "槽位種類不得改變");
        }
    }
    let core = p.ordered.trait_named("CorePhonology").unwrap();
    assert_eq!(core.blocks.len(), 1, "global blocks 展平");
    let trait_stages: Vec<_> = core.blocks[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Rule(r) => Some(r.stage),
            _ => None,
        })
        .collect();
    assert_eq!(trait_stages, [Stem, Word]);
}

/// P21 純函數/決定性:pass 冪等;單獨跑 = 管線內跑;compile 兩次逐位元相同。
#[test]
fn passes_are_pure_and_idempotent() {
    let src = source();
    let p = compile::compile(&src).unwrap();
    assert_eq!(compile::expand_traits(&src).unwrap(), p.expanded);
    assert_eq!(compile::resolve(&p.expanded), p.resolved);
    assert_eq!(compile::order_stages(&p.resolved), p.ordered);
    // 冪等(不動點)
    assert_eq!(compile::expand_traits(&p.expanded).unwrap(), p.expanded);
    assert_eq!(compile::resolve(&p.resolved), p.resolved);
    assert_eq!(compile::order_stages(&p.ordered), p.ordered);
    let q = compile::compile(&src).unwrap();
    assert_eq!(q.ordered.dump(), p.ordered.dump());
    assert_eq!(q.trait_index, p.trait_index);
}

// ── 錯誤案例 ──

#[test]
fn error_unknown_trait() {
    let src = "sign x {\n    Missing[1]\n}\n";
    let l = Language::parse(src).unwrap();
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
    let src = "\
trait T {
    a = 1
}

sign x {
    T[2]
}
";
    let l = Language::parse(src).unwrap();
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::BlockOutOfRange {
            sign: "x".into(),
            name: "T".into(),
            block: 2,
            blocks: 1
        }
    );
}

/// P5:全 block 強制顯式——只用 T[1] 而漏 T[2] 是編譯錯誤。
#[test]
fn error_incomplete_trait_use_p5() {
    let src = "\
trait T {
    a = 1
    ==
    b = 2
}

sign x {
    T[1]
}
";
    let l = Language::parse(src).unwrap();
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::IncompleteTraitUse {
            sign: "x".into(),
            name: "T".into(),
            missing: 2
        }
    );
}

#[test]
fn error_duplicate_names() {
    let mut l = Language::new();
    let t = TraitDef {
        name: "T".into(),
        global: false,
        dim: None,
        blocks: vec![Block::default()],
    };
    l.add_trait(t.clone());
    l.add_trait(t);
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::DuplicateTrait("T".into())
    );

    let mut l = Language::new();
    l.add_sign("x", vec![]);
    l.add_sign("x", vec![]);
    assert_eq!(
        compile::expand_traits(&l).unwrap_err(),
        CompileError::DuplicateSign("x".into())
    );
}
