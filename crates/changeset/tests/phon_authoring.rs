use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, NodeUpdate, PrimitiveEdit, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};

const FLAT: &str = r#"
Symbol a
Symbol b
Symbol c

global trait Core:
    phon:
        shift: a => b
"#;

fn source(namespace: &str, body: &str) -> (LanguageDocument, LibrarySpec, String) {
    let base = LanguageDocument::import_new_root(FLAT, "evo:phon-root").unwrap();
    let libraries = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &libraries, namespace).unwrap();
    source.push_str(body);
    (base, libraries, source)
}

#[test]
fn explicit_flat_to_structured_update_and_leaf_insert_round_trip_through_dump() {
    let (base, libraries, source) = source(
        "evo:phon-author",
        r#"
    #0:
        update trait("Core").block[0].rule["shift"].phon_block:
            a => b
            Then:
                b => c

    #1:
        insert into trait("Core").block[0].rule["shift"].then[1] at end:
            c => a

    #2:
        insert into trait("Core").block[0].rule["shift"] at end:
            Then propagate:
                c => b
"#,
    );
    let unresolved = UnresolvedChangeSet::parse(&source).unwrap();
    let resolved = unresolved.resolve(&base, &libraries).unwrap();
    assert!(matches!(
        resolved.statements[0].edits.as_slice(),
        [PrimitiveEdit::Update {
            change: NodeUpdate::PhonBlockRoot(_),
            ..
        }]
    ));
    let dump = resolved.dump();
    assert!(dump.contains(".phon_block:"));
    assert!(dump.contains("Then propagate:"));
    let reparsed = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &libraries)
        .unwrap();
    assert_eq!(reparsed.dump(), dump);

    let outcome = ChangeInterpreter::new(base, libraries, "evo:phon-author".to_owned())
        .unwrap()
        .run(&reparsed)
        .unwrap();
    let canonical = outcome.document.source();
    assert!(canonical.contains("shift:"));
    assert!(canonical.contains("Then propagate:"));
    assert!(canonical.contains("c => a"));

    let generated = conlang_language::codegen::compile_full(outcome.document.language())
        .expect("Tshiatūn accepts authored structured block");
    assert!(generated.grammar.phon_source.contains("Then:"));
    assert!(generated.grammar.phon_source.contains("propagate"));
}

#[test]
fn insert_never_silently_bootstraps_a_flat_rule() {
    let (base, libraries, source) = source(
        "evo:phon-no-bootstrap",
        r#"
    #0:
        insert into trait("Core").block[0].rule["shift"] at end:
            b => c
"#,
    );
    let error = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &libraries)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("use an explicit `.phon_block:` update"));
}

#[test]
fn leading_boundary_root_is_explicitly_rejected() {
    let (base, libraries, source) = source(
        "evo:phon-leading",
        r#"
    #0:
        update trait("Core").block[0].rule["shift"].phon_block:
            Then propagate:
                b => c
"#,
    );
    let error = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &libraries)
        .unwrap_err();
    assert!(error.to_string().contains("needs a leading statement"));
}
