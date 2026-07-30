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
    let dump = resolved.dump();
    assert!(dump.contains(".global = true"), "dump:\n{dump}");

    // dump→parse→resolve→dump 穩定。
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump);

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
