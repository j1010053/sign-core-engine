//! `insert into <target> at <pos>:` — 通用單一 payload 插入(trait / 單一 item),
//! block 為逐字 `.lang` fragment(重用 `.lang` parser + 維度驗證),降階為單一 `Insert`。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, DetachedNode, PrimitiveEdit, ReplayError,
    UnresolvedChangeSet,
};
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
    assert!(
        rendered.contains("g => k"),
        "phon 規則寫入 dog:\n{rendered}"
    );
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

/// §④:一個多 item block 展成 N 個 `Insert`(同 statement,只驗最終態),
/// 依來源序插入;dump→parse→resolve→dump 逐位元穩定(正規形 = 每 item 一 block)。
#[test]
fn multi_item_block_fans_out_to_ordered_inserts() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:fanout").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into sign(\"dog\") at end:\n            syn:\n                slots:\n                    agent [LocalNoun]\n                    theme [LocalNoun]\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    // 一個 operation → 兩個 primitive edit(fan-out)。
    assert_eq!(resolved.statements.len(), 1);
    assert_eq!(resolved.statements[0].edits.len(), 2);

    let doc = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:fanout")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    let rendered = doc.source();
    assert!(rendered.contains("agent [LocalNoun]"));
    assert!(rendered.contains("theme [LocalNoun]"));
    // 來源序保留:agent 在 theme 之前。
    assert!(
        rendered.find("agent").unwrap() < rendered.find("theme").unwrap(),
        "source order preserved"
    );

    // 正規形 = 每 item 一 block;round-trip 穩定。
    let dump = resolved.dump();
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump);
    assert_eq!(round.statements[0].edits.len(), 2, "正規形仍 2 個 edit");
}

/// 多 operation 同一 statement:update + insert 混排(皆定址 statement 起始態),
/// 一起套用、只驗最終態一次。
#[test]
fn a_statement_may_hold_multiple_operations() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:multiop").unwrap();
    source.push_str(concat!(
        "\n    statement 0:\n",
        "        update sign(\"dog\").def[phon].value = /dok/\n",
        "        insert into sign(\"dog\") at end:\n            syn:\n                slots:\n                    agent [LocalNoun]\n",
    ));
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(
        resolved.statements[0].edits.len(),
        2,
        "update + insert 同句"
    );
    let doc = ChangeInterpreter::new(base, spec, "evo:multiop")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    let rendered = doc.source();
    assert!(rendered.contains("/dok/"), "update 生效");
    assert!(rendered.contains("agent [LocalNoun]"), "insert 生效");
}

/// 廣度:通用 item insert 重用 `.lang` parser,故涵蓋整個 item 分類法
/// (feature 宣告 / sem Def path),不限 slot/rule。
#[test]
fn generic_item_insert_covers_feature_and_def_items() {
    let feat = resolve_and_run(
        "\n    statement 0:\n        insert into sign(\"dog\") at end:\n            syn:\n                feature:\n                    transitivity = enum(transitive, intransitive)\n",
        "evo:feat",
    );
    assert!(feat
        .source()
        .contains("transitivity = enum(transitive, intransitive)"));

    let def = resolve_and_run(
        "\n    statement 0:\n        insert into sign(\"dog\") at end:\n            sem:\n                time.present = 1\n",
        "evo:def",
    );
    assert!(def.source().contains("time.present = 1"));
}

#[test]
fn symbol_and_class_insert_replay_and_dump_round_trip() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:dsl").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into language at end:\n            Symbol a\n            Class front {a}\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(resolved.statements[0].edits.len(), 2);
    assert!(matches!(
        &resolved.statements[0].edits[0],
        PrimitiveEdit::Insert {
            subtree: DetachedNode::DslDeclaration(value),
            ..
        } if value == "Symbol a"
    ));
    assert!(matches!(
        &resolved.statements[0].edits[1],
        PrimitiveEdit::Insert {
            subtree: DetachedNode::DslDeclaration(value),
            ..
        } if value == "Class front {a}"
    ));

    let replayed = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:dsl")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    let symbol = replayed.source().find("Symbol a").unwrap();
    let class = replayed.source().find("Class front {a}").unwrap();
    assert!(symbol < class, "authored declaration order is retained");

    let dump = resolved.dump();
    assert!(dump.contains("Symbol a"));
    assert!(dump.contains("Class front {a}"));
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let replayed_round = ChangeInterpreter::new(base, spec, "evo:dsl")
        .unwrap()
        .run(&round)
        .unwrap()
        .document;
    assert_eq!(round.dump(), dump);
    assert_eq!(replayed_round.source(), replayed.source());
    assert_eq!(replayed_round.identities(), replayed.identities());
}

#[test]
fn symbol_and_class_insert_at_start_preserves_block_order() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:dsl-start").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into language at start:\n            Symbol a\n            Class front {a}\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let replayed = ChangeInterpreter::new(base, spec, "evo:dsl-start")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert_eq!(
        &replayed.language().dsl_decls[..2],
        ["Symbol a", "Class front {a}"]
    );
}

#[test]
fn symbol_insert_rejects_a_mixed_top_level_payload() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:dsl-invalid").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into language at end:\n            Symbol a\n            trait Mixed:\n",
    );
    let error = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap_err();
    assert!(matches!(error, ReplayError::Parse(_)));
}
