//! P46 S4 — `propagate` 的四原語編輯面。`Propagate` 是**修飾詞不是層級**:它不佔
//! 位址節段,所以 `update <node>.propagate = …` 切換時,底下語句的穩定 id **不變**
//! (P25/P26)。rule-level(`name propagate:`)與 element-level(`Then propagate:`)
//! 各自可切換,互不干涉。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, ReplayError, ResolvedChangeSet, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol a
Symbol b
Symbol c
Symbol d

sign x:
    phon:
        /a/
        r:
            a => b
            Then propagate:
                b => c
                c => d
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("base parses")
}

fn resolve(chg: &str, ns: &str) -> Result<ResolvedChangeSet, ReplayError> {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(chg);
    UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
}

fn apply(chg: &str, ns: &str) -> LanguageDocument {
    let base = base();
    let spec = LibrarySpec::default();
    let resolved = resolve(chg, ns).expect("resolve");
    ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document
}

const TURN_OFF: &str =
    "\n    statement 0:\n        update sign(\"x\").rule[\"r\"].then[1].propagate = false\n";
const TURN_ON_RULE: &str =
    "\n    statement 0:\n        update sign(\"x\").rule[\"r\"].propagate = true\n";

#[test]
fn element_propagate_can_be_switched_off() {
    let doc = apply(TURN_OFF, "evo:off");
    let src = doc.source();
    assert!(!src.contains("Then propagate:"), "modifier removed:\n{src}");
    assert!(src.contains("Then:"), "boundary itself survives:\n{src}");
    // The wrapped statements are untouched.
    assert!(src.contains("b => c") && src.contains("c => d"), "{src}");
}

#[test]
fn element_propagate_can_be_switched_back_on() {
    // off → on returns to the original source (toggle is lossless).
    let off = apply(TURN_OFF, "evo:off2");
    assert!(!off.source().contains("propagate"));
    let base_src = base().source().to_owned();
    let spec = LibrarySpec::default();
    let mut chg = change_set_prelude(&off, &spec, "evo:on").unwrap();
    chg.push_str(
        "\n    statement 0:\n        update sign(\"x\").rule[\"r\"].then[1].propagate = true\n",
    );
    let resolved = UnresolvedChangeSet::parse(&chg)
        .unwrap()
        .resolve(&off, &spec)
        .unwrap();
    let back = ChangeInterpreter::new(off, spec, "evo:on")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert_eq!(
        back.source(),
        base_src,
        "off→on round-trips to the original"
    );
}

#[test]
fn rule_level_propagate_is_independent_of_the_element_modifier() {
    let doc = apply(TURN_ON_RULE, "evo:rule");
    let src = doc.source();
    assert!(src.contains("r propagate:"), "header modifier set:\n{src}");
    assert!(
        src.contains("Then propagate:"),
        "element modifier untouched:\n{src}"
    );
}

/// 這是 S4 把 `Propagate` 設計成**透明修飾詞**(不佔位址節段)的理由:
/// 切換 propagate 不得改變底下語句的穩定 id,否則 "同一條語句" 的身分會斷。
#[test]
fn toggling_propagate_keeps_child_statement_identities_stable() {
    let before = base();
    let stmt_ids = |doc: &LanguageDocument| {
        let mut ids = doc
            .identities()
            .nodes
            .iter()
            .filter(|n| n.kind == conlang_language::NodeKind::PhonStatement)
            .map(|n| (n.id.to_string(), n.address.clone()))
            .collect::<Vec<_>>();
        ids.sort();
        ids
    };
    let before_ids = stmt_ids(&before);
    assert!(!before_ids.is_empty(), "statements are enumerated");

    let after = apply(TURN_OFF, "evo:stable");
    assert_eq!(
        stmt_ids(&after),
        before_ids,
        "statement ids and addresses survive a propagate toggle"
    );
}

#[test]
fn statements_under_a_propagate_element_are_still_addressable() {
    // Transparency must not cost addressability: the wrapped Leaf's statements
    // keep the `then[1].leaf[k]` path (no extra segment for the modifier).
    let doc = apply(
        "\n    statement 0:\n        update sign(\"x\").rule[\"r\"].then[1].leaf[0].body = b => d\n",
        "evo:addr",
    );
    let src = doc.source();
    assert!(src.contains("b => d"), "nested statement edited:\n{src}");
    assert!(
        src.contains("Then propagate:"),
        "modifier preserved by an unrelated edit:\n{src}"
    );
}

#[test]
fn propagate_update_round_trips_through_dump() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:rt").unwrap();
    source.push_str(TURN_OFF);
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let dump = resolved.dump();
    assert!(dump.contains("propagate = false"), "field in dump:\n{dump}");
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump, "dump→parse→resolve stable");
}

/// Near-miss 負例:`propagate` 是 phon **block** 的修飾詞;扁平 else/then 鏈的 rule
/// 沒有對應的引擎表面,必須明確拒絕(不默默設一個永遠不會被排出的旗標)。
#[test]
fn propagate_on_a_flat_chain_rule_is_rejected() {
    const FLAT: &str = r#"Symbol a
Symbol b

sign y:
    phon:
        /a/
        flat: a => b
"#;
    let base = LanguageDocument::import_new_root(FLAT, "evo:flatroot").expect("flat base parses");
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:flat").unwrap();
    source.push_str(
        "\n    statement 0:\n        update sign(\"y\").rule[\"flat\"].propagate = true\n",
    );
    let err = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .expect_err("a flat-chain rule has no phon block to propagate");
    assert!(
        format!("{err:?}").contains("phon block"),
        "expected the phon-block guard, got: {err:?}"
    );
}
