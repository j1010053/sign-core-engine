use conlang_changeset::reconstruct::reconstruct;
use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, ReplayError, ResolvedStatement, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};

const BASE: &str = include_str!("../../../tutorials/en-standard-reconstruction/base.lang");
const RESTORE: &str = include_str!("../../../tutorials/en-standard-reconstruction/restore.chg");
const TARGET: &str = include_str!("../../language/lib/natural/en-standard/code/grammar.lang");

fn documents() -> (LanguageDocument, LanguageDocument) {
    let base = LanguageDocument::import_new_root(BASE, "evo:en-standard").expect("base parses");
    let target =
        LanguageDocument::import_new_root(TARGET, "evo:en-standard").expect("target parses");
    (base, target)
}

#[test]
fn reconstructs_the_english_grammar_through_a_dumped_changeset() {
    let (base, target) = documents();
    let base_identities = base.identities().clone();
    let edits = reconstruct(&base, &target).expect("English grammar reconstructs");
    assert!(!edits.is_empty());

    let libraries = LibrarySpec::default();
    let prelude =
        change_set_prelude(&base, &libraries, "evo:en-standard-restore").expect("prelude");
    let mut resolved = UnresolvedChangeSet::parse(&prelude)
        .expect("prelude parses")
        .resolve(&base, &libraries)
        .expect("prelude resolves");
    resolved.statements = vec![ResolvedStatement { ordinal: 0, edits }];

    let dumped = resolved.dump();
    assert_eq!(
        dumped, RESTORE,
        "checked-in .chg follows reconstruct output"
    );
    let reparsed = UnresolvedChangeSet::parse(RESTORE)
        .expect("dumped .chg parses")
        .resolve(&base, &libraries)
        .expect("dumped .chg resolves");
    assert_eq!(reparsed.statements[0].edits.len(), 85);
    assert_eq!(
        reparsed.statements[0]
            .edits
            .iter()
            .filter(|edit| matches!(
                edit,
                conlang_changeset::PrimitiveEdit::Insert {
                    subtree: conlang_changeset::DetachedNode::DslDeclaration(_),
                    ..
                }
            ))
            .count(),
        18,
        "17 Symbol declarations and one Class are persisted in .chg"
    );
    let replayed =
        ChangeInterpreter::new(base.clone(), libraries.clone(), "evo:en-standard-restore")
            .expect("interpreter")
            .run(&reparsed.clone())
            .expect("dumped .chg replays")
            .document;
    let replayed_again = ChangeInterpreter::new(base, libraries, "evo:en-standard-restore")
        .expect("second interpreter")
        .run(&reparsed)
        .expect("dumped .chg replays deterministically")
        .document;

    assert_eq!(replayed.source(), target.source());
    assert_eq!(
        replayed.source(),
        replayed_again.source(),
        "replay source is deterministic"
    );
    assert_eq!(
        replayed.identities(),
        replayed_again.identities(),
        "replay identity allocation is deterministic"
    );
    for original in &base_identities.nodes {
        assert!(
            replayed
                .identities()
                .nodes
                .iter()
                .any(|entry| entry.id == original.id && entry.kind == original.kind),
            "base identity {} survives reconstruction",
            original.id
        );
    }
    assert_eq!(
        replayed.identities().active_namespace.to_string(),
        "evo:en-standard-restore",
        "new grammar nodes retain ChangeSet provenance"
    );
    assert_eq!(reparsed.dump(), dumped, ".chg dump is canonical");
}

#[test]
fn restore_rejects_a_different_base_before_replay() {
    let altered =
        LanguageDocument::import_new_root(&format!("{BASE}\nSymbol z\n"), "evo:en-standard")
            .expect("altered base parses");
    let error = UnresolvedChangeSet::parse(RESTORE)
        .expect("restore parses")
        .resolve(&altered, &LibrarySpec::default())
        .expect_err("digest mismatch must reject another base");
    assert!(matches!(error, ReplayError::BaseSourceMismatch));
}
