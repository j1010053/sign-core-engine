//! P46 L1 對接:工作台 `PhonBlock` codegen 出**大括號 `{ }` 巢狀 `.qy`**,接上引擎
//! (tshiatūn wuc-claudecode / PR #1)的 grouped-block parser。巢狀 Then/Else 端到端貫通:
//! 扁平單層仍**無括號**(與舊輸出逐字相同,零 golden churn),巢狀元素才包 `{ }`。

use conlang_language::{codegen, Language, PhonBlock, SignItem};

const SYMBOLS: &str = "Symbol a\nSymbol b\nSymbol c\nSymbol d\nSymbol e\nSymbol f\n";

fn phon_source(src: &str) -> String {
    let l = Language::parse(src).expect("parse");
    codegen::compile_full(&l)
        .expect("engine accepts generated phon source")
        .grammar
        .phon_source
}

#[test]
fn nested_then_else_codegen_uses_braces_and_engine_accepts() {
    // Then([Leaf(a=>b), Else([Leaf(c=>d), Leaf(e=>f)])]) — the inner Else is a
    // compound element, so it must be a braced group. compile_full succeeding
    // *is* the end-to-end proof that the engine parsed the braces.
    let src = format!(
        "{SYMBOLS}\nglobal trait Core:\n    phon:\n        nested:\n            a => b\n            Then:\n                c => d\n                Else:\n                    e => f\n"
    );
    let s = phon_source(&src);
    assert!(
        s.contains("Then: {"),
        "nested element is a braced group:\n{s}"
    );
    assert!(s.contains("Else:"), "inner boundary present:\n{s}");
    assert!(s.contains('}'), "group is closed:\n{s}");
}

#[test]
fn flat_single_level_block_stays_brace_free() {
    // Then([Leaf, Leaf]) — both elements are leaves, so no braces at all
    // (byte-compatible with the pre-brace codegen; guards against golden churn).
    let src = format!(
        "{SYMBOLS}\nglobal trait Core:\n    phon:\n        flat:\n            a => b\n            Then:\n                c => d\n"
    );
    let s = phon_source(&src);
    assert!(!s.contains('{'), "flat block emits no braces:\n{s}");
    assert!(s.contains("Then:"), "flat boundary present:\n{s}");
}

#[test]
fn nested_lang_round_trips() {
    let src = format!(
        "{SYMBOLS}\nglobal trait Core:\n    phon:\n        nested:\n            a => b\n            Then:\n                c => d\n                Else:\n                    e => f\n"
    );
    let l = Language::parse(&src).expect("parse");
    let dump = l.dump();
    assert_eq!(
        Language::parse(&dump).unwrap().dump(),
        dump,
        "`.lang` round-trip stable"
    );
}

#[test]
fn leading_compound_element_emits_an_opening_brace_group() {
    // element 0 compound is unreachable from `.lang` parse (a Then/Else always
    // leads with a Leaf), but S3 edits (move a sub-block to index 0) can produce
    // it. Build it directly:  Then([ Else([Leaf(c=>d), Leaf(e=>f)]), Leaf(a=>b) ]).
    let base =
        format!("{SYMBOLS}\nglobal trait Core:\n    phon:\n        r:\n            a => b\n");
    let mut l = Language::parse(&base).expect("parse");
    let block = PhonBlock::Then(vec![
        PhonBlock::Else(vec![
            PhonBlock::Leaf(vec!["c => d".to_owned()]),
            PhonBlock::Leaf(vec!["e => f".to_owned()]),
        ]),
        PhonBlock::Leaf(vec!["a => b".to_owned()]),
    ]);
    let mut set = false;
    for item in &mut l.traits[0].blocks[0].items {
        if let SignItem::Rule(rule) = item {
            rule.phon_block = Some(block.clone());
            set = true;
        }
    }
    assert!(set, "rule found");

    let s = codegen::compile_full(&l)
        .expect("engine accepts a leading `{` group")
        .grammar
        .phon_source;
    // The leading compound element is a bare `{` on its own line (GroupOpen),
    // and the trailing leaf sits after a `Then:` boundary.
    assert!(
        s.contains("    {\n"),
        "leading group opens with a brace line:\n{s}"
    );
    assert!(
        s.contains("Then:"),
        "trailing leaf keeps its boundary:\n{s}"
    );
}
