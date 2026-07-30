//! P46 S4 — phon `propagate`(解 L3)。引擎 `.qy` 有兩處 propagate 修飾詞:
//! **header**(`name propagate:` = 整條 rule 迭代到 fixpoint → `Rule.propagate`)與
//! **boundary**(`Then propagate:` = 只重複它引入的那個 block element →
//! `PhonBlock::Propagate`)。此前 `.lang` 兩者皆無,且 `PhonBlock::Propagate` 會被
//! printer/codegen **靜默丟棄**(語意流失)——本檔釘住修好的行為。

use conlang_language::{codegen, Language, PhonBlock, SignItem};

const SYMS: &str = "Symbol a\nSymbol b\nSymbol c\nSymbol d\n";

fn lang(body: &str) -> Language {
    Language::parse(&format!("{SYMS}\nglobal trait Core:\n    phon:\n{body}")).expect("parse")
}

fn qy(l: &Language) -> String {
    codegen::compile_full(l)
        .expect("engine accepts generated phon source")
        .grammar
        .phon_source
}

fn only_rule(l: &Language) -> &conlang_language::Rule {
    l.traits[0]
        .blocks
        .iter()
        .flat_map(|b| &b.items)
        .find_map(|item| match item {
            SignItem::Rule(r) => Some(r),
            _ => None,
        })
        .expect("one rule")
}

// ── boundary propagate ────────────────────────────────────────────────────

#[test]
fn boundary_propagate_parses_into_a_propagate_element() {
    let l = lang(
        "        r:\n            a => b\n            Then propagate:\n                b => c\n",
    );
    let block = only_rule(&l).phon_block.as_ref().expect("phon block");
    // Then([Leaf(a=>b), Propagate(Leaf(b=>c))]) — the modifier wraps *only* the
    // element its boundary introduces.
    match block {
        PhonBlock::Then(elements) => {
            assert_eq!(elements.len(), 2);
            assert!(
                matches!(&elements[0], PhonBlock::Leaf(v) if v == &["a => b".to_owned()]),
                "leading leaf unwrapped, got {:?}",
                elements[0]
            );
            assert!(
                matches!(&elements[1], PhonBlock::Propagate(inner)
                    if matches!(inner.as_ref(), PhonBlock::Leaf(v) if v == &["b => c".to_owned()])),
                "second element carries Propagate, got {:?}",
                elements[1]
            );
        }
        other => panic!("expected Then, got {other:?}"),
    }
}

#[test]
fn boundary_propagate_survives_print_and_codegen() {
    // Regression for the silent-loss bug: both layers used to drop the modifier.
    let l = lang(
        "        r:\n            a => b\n            Then propagate:\n                b => c\n",
    );
    let dumped = l.dump();
    assert!(
        dumped.contains("Then propagate:"),
        "printer keeps the modifier:\n{dumped}"
    );
    assert_eq!(
        Language::parse(&dumped).unwrap().dump(),
        dumped,
        "round-trip stable"
    );
    let s = qy(&l);
    assert!(
        s.contains("Then propagate:"),
        "codegen keeps the modifier:\n{s}"
    );
}

#[test]
fn a_plain_boundary_emits_no_propagate() {
    // Near-miss: the modifier must not leak onto an unmarked boundary.
    let l = lang("        r:\n            a => b\n            Then:\n                b => c\n");
    assert!(!l.dump().contains("propagate"), "{}", l.dump());
    assert!(!qy(&l).contains("propagate"), "{}", qy(&l));
    assert!(matches!(
        only_rule(&l).phon_block.as_ref().unwrap(),
        PhonBlock::Then(e) if matches!(e[1], PhonBlock::Leaf(_))
    ));
}

// ── header (rule-level) propagate ─────────────────────────────────────────

#[test]
fn header_propagate_sets_the_rule_flag_and_round_trips() {
    let l = lang("        r propagate:\n            a => b\n            Else: c => d\n");
    assert!(only_rule(&l).propagate, "rule-level flag set");
    let dumped = l.dump();
    assert!(
        dumped.contains("r propagate:"),
        "printer emits the header modifier:\n{dumped}"
    );
    assert_eq!(Language::parse(&dumped).unwrap().dump(), dumped);
    let s = qy(&l);
    assert!(
        s.contains("r propagate:"),
        "codegen emits the header modifier:\n{s}"
    );
}

#[test]
fn header_and_boundary_propagate_are_independent() {
    // Header set, boundary not: only the header modifier appears.
    let l = lang(
        "        r propagate:\n            a => b\n            Then:\n                b => c\n",
    );
    let s = qy(&l);
    assert!(s.contains("r propagate:"), "{s}");
    assert!(!s.contains("Then propagate:"), "{s}");
    assert!(only_rule(&l).propagate);
    assert!(matches!(
        only_rule(&l).phon_block.as_ref().unwrap(),
        PhonBlock::Then(e) if matches!(e[1], PhonBlock::Leaf(_))
    ));
}

#[test]
fn a_rule_named_like_a_propagate_suffix_is_not_a_modifier() {
    // Near-miss: `xpropagate:` is a plain rule name, not `x` + modifier.
    let l = lang("        xpropagate:\n            a => b\n");
    let rule = only_rule(&l);
    assert_eq!(rule.name.as_deref(), Some("xpropagate"));
    assert!(!rule.propagate, "no modifier parsed");
}

// ── propagate + brace grouping (slice 4 interplay) ────────────────────────

#[test]
fn propagate_on_a_nested_element_emits_a_braced_group() {
    let l = lang(
        "        r:\n            a => b\n            Then propagate:\n                b => c\n                Else:\n                    c => d\n",
    );
    let s = qy(&l);
    assert!(
        s.contains("Then propagate: {"),
        "modifier and brace group combine:\n{s}"
    );
}

// ── 顯式拒絕:無處可掛的 propagate ─────────────────────────────────────────

#[test]
fn a_leading_propagate_element_is_rejected_not_silently_dropped() {
    // `Then([Propagate(Leaf), Leaf])` is only reachable via an S3 move. The
    // leading element precedes every boundary, so `.qy` has nowhere to hang the
    // modifier — codegen must reject rather than silently drop the semantics.
    let mut l = lang("        r:\n            a => b\n");
    let block = PhonBlock::Then(vec![
        PhonBlock::Propagate(Box::new(PhonBlock::Leaf(vec!["a => b".to_owned()]))),
        PhonBlock::Leaf(vec!["b => c".to_owned()]),
    ]);
    for item in &mut l.traits[0].blocks[0].items {
        if let SignItem::Rule(r) = item {
            r.phon_block = Some(block.clone());
        }
    }
    let err = codegen::compile_full(&l).expect_err("must be rejected");
    let text = format!("{err}");
    assert!(
        text.contains("leading block element cannot carry `propagate`"),
        "explicit rejection, got: {text}"
    );
}
