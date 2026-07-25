//! P 系列取徑 B:rule `@name <label>` → keyed 定址 `rule["label"]`(穩定、非序數)。

use conlang_changeset::{change_set_prelude, ChangeInterpreter, ReplayError, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol d
Symbol o
Symbol g

trait LocalNoun:

sign dog:
    belongs LocalNoun
    syn:
        class => transitive / [Verb] @name classify
    phon:
        /dog/
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").unwrap()
}

#[test]
fn a_named_rule_survives_import_and_round_trip() {
    // `@name` 進 IR 並在 canonical dump 保留。
    let doc = base();
    assert!(doc.source().contains("@name classify"), "{}", doc.source());
}

#[test]
fn rule_addressed_by_label_receives_an_else_branch() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:named").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into sign(\"dog\").rule[\"classify\"] at end:\n            else class => other\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    // label 定址 → stable node(rule,@…);round-trip 穩定。
    let dump = resolved.dump();
    assert!(dump.contains("insert into node(rule, @"));
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump);

    let doc = ChangeInterpreter::new(base, spec, "evo:named")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    let rendered = doc.source();
    assert!(rendered.contains("else class => other"));
    assert!(rendered.contains("@name classify"), "label 保留");
}

#[test]
fn unknown_rule_label_is_rejected() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:badlabel").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into sign(\"dog\").rule[\"nope\"] at end:\n            else class => other\n",
    );
    let err = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap_err();
    assert!(matches!(err, ReplayError::Selector(_)), "got {err}");
}
