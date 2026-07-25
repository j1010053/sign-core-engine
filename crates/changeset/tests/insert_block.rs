//! `insert into <target> at <pos>:` — 通用單一 payload 插入(trait / 單一 item),
//! block 為逐字 `.lang` fragment(重用 `.lang` parser + 維度驗證),降階為單一 `Insert`。

use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol d
Symbol o
Symbol g
Symbol k

trait LocalNoun:

sign dog:
    belongs LocalNoun
    phon:
        /dog/
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").unwrap()
}

fn resolve_and_run(chg_body: &str, ns: &str) -> LanguageDocument {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(chg_body);
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document
}

#[test]
fn inserts_a_new_trait_under_language() {
    let doc = resolve_and_run(
        "\n    statement 0:\n        insert into language at end:\n            trait Nocturnal:\n",
        "evo:trait",
    );
    assert!(doc.ref_for_trait("Nocturnal").is_some());
    assert!(doc.ref_for_trait("LocalNoun").is_some(), "既有 trait 不動");
}

#[test]
fn inserts_a_phon_tshiatun_rule_into_a_sign() {
    let doc = resolve_and_run(
        "\n    statement 0:\n        insert into sign(\"dog\") at end:\n            phon:\n                g => k / _ #\n",
        "evo:phon",
    );
    let rendered = doc.source();
    assert!(rendered.contains("g => k"), "phon 規則寫入 dog:\n{rendered}");
    assert!(rendered.contains("/dog/"), "既有 UR 模板保留");
}

#[test]
fn inserts_a_single_syn_slot_into_a_sign() {
    let doc = resolve_and_run(
        "\n    statement 0:\n        insert into sign(\"dog\") at end:\n            syn:\n                slots:\n                    agent [LocalNoun]\n",
        "evo:slot",
    );
    assert!(doc.source().contains("agent [LocalNoun]"));
}

#[test]
fn insert_block_round_trips_through_dump() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:rt").unwrap();
    source.push_str(concat!(
        "\n    statement 0:\n        insert into language at end:\n            trait Nocturnal:\n",
        "\n    statement 1:\n        insert into sign(\"dog\") at end:\n            phon:\n                g => k / _ #\n",
    ));
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let dump = resolved.dump();
    assert!(dump.contains("insert into node(language, @"));
    assert!(dump.contains("trait Nocturnal:"));
    assert!(dump.contains("g => k"));

    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump, "dump→parse→resolve→dump 穩定");
}

#[test]
fn multi_item_block_is_rejected() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:multi").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into sign(\"dog\") at end:\n            syn:\n                slots:\n                    agent [LocalNoun]\n                    theme [LocalNoun]\n",
    );
    let err = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap_err();
    assert!(
        format!("{err}").contains("exactly one item"),
        "expected single-item rejection, got {err}"
    );
}
