//! `update <selector>.<field> = <value>` — 擴充欄位詞彙(對稱 update_for / dump_update)。

use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol d
Symbol o
Symbol g

trait LocalNoun:

sign dog:
    belongs LocalNoun
    phon:
        /dog/
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").unwrap()
}

#[test]
fn promotes_a_trait_to_global_and_round_trips() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:global").unwrap();
    source.push_str("\n    statement 0:\n        update trait(\"LocalNoun\").global = true\n");

    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let dump = resolved.dump().expect("dump");
    assert!(dump.contains(".global = true"), "dump:\n{dump}");

    // dump→parse→resolve→dump 穩定。
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump().expect("dump"), dump);

    // apply：LocalNoun 變 global（printer 以 `global trait` 輸出）。
    let doc = ChangeInterpreter::new(base, spec, "evo:global")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert!(doc.source().contains("global trait LocalNoun"));
}

#[test]
fn invalid_bool_for_global_is_rejected() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:badbool").unwrap();
    source.push_str("\n    statement 0:\n        update trait(\"LocalNoun\").global = maybe\n");

    let err = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap_err();
    assert!(
        format!("{err}").contains("true") || format!("{err}").contains("false"),
        "expected bool rejection, got {err}"
    );
}

#[test]
fn marker_and_type_params_updates_round_trip_through_chg() {
    let root = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&root, &spec, "evo:trait-header").unwrap();
    source.push_str(
        "\n    statement 0:\n        update trait(\"LocalNoun\").type_params = \"C: Nominal, T\"\n",
    );

    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&root, &spec)
        .unwrap();
    let dump = resolved.dump().expect("dump");
    assert!(
        dump.contains(".type_params = \"C: Nominal, T\""),
        "dump:\n{dump}"
    );
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&root, &spec)
        .unwrap();
    assert_eq!(round.dump().expect("dump"), dump);

    let document = ChangeInterpreter::new(root, spec, "evo:trait-header".to_owned())
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert!(document
        .source()
        .contains("trait LocalNoun<C: Nominal, T>:"));

    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:marker").unwrap();
    source.push_str("\n    statement 0:\n        update trait(\"LocalNoun\").marker = true\n");
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert!(resolved.dump().expect("dump").contains(".marker = true"));
    let document = ChangeInterpreter::new(base, spec, "evo:marker".to_owned())
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert!(document.source().contains("marker trait LocalNoun:"));
}

#[test]
fn an_empty_type_params_string_clears_the_parameter_list() {
    let base = LanguageDocument::import_new_root(
        "trait Schema<C: Nominal, T>:\n    pass\n",
        "evo:generic-root",
    )
    .expect("generic root parses");
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:generic-clear").unwrap();
    source.push_str("\n    statement 0:\n        update trait(\"Schema\").type_params = \"\"\n");
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let dump = resolved.dump().expect("dump");
    assert!(dump.contains(".type_params = \"\""), "dump:\n{dump}");
    let reparsed = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let document = ChangeInterpreter::new(base, spec, "evo:generic-clear".to_owned())
        .unwrap()
        .run(&reparsed)
        .unwrap()
        .document;
    assert!(document.source().contains("trait Schema:\n"));
    assert!(!document.source().contains("Schema<"));
}

#[test]
fn a_trait_cannot_be_global_and_marker_at_once() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:bad-trait-kind").unwrap();
    source.push_str(
        "\n    statement 0:\n        update trait(\"LocalNoun\").marker = true\n        update trait(\"LocalNoun\").global = true\n",
    );
    let error = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .expect_err("conflicting trait kinds must fail validation");
    assert!(
        format!("{error:?}").contains("TRAIT_GLOBAL_MARKER_CONFLICT"),
        "{error:?}"
    );
}

#[test]
fn unknown_field_on_kind_is_rejected() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:badfield").unwrap();
    // `stage` 不是 Trait 的欄位。
    source.push_str("\n    statement 0:\n        update trait(\"LocalNoun\").stage = word\n");

    let err = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap_err();
    assert!(
        format!("{err}").contains("not editable"),
        "expected not-editable, got {err}"
    );
}
