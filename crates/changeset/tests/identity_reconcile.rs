use conlang_changeset::reconcile::{
    reconcile_edited_source, ReconcileError, ReconcileHint, ReconcileHints,
};
use conlang_changeset::reconstruct::reconstruct;
use conlang_changeset::{apply_edit, PrimitiveEdit};
use conlang_language::{LanguageDocument, LibrarySpec, NodeKind, NodeRef};

const SOURCE: &str = r#"
trait A:
trait B:

sign root:
    belongs A
    belongs B
    phon:
        /r/
"#;

fn replay(
    before: &LanguageDocument,
    after: &LanguageDocument,
    namespace: &str,
) -> LanguageDocument {
    let mut document = before.fork(namespace).unwrap();
    for edit in reconstruct(&document, after).unwrap() {
        document = apply_edit(&document, edit, &LibrarySpec::default())
            .unwrap()
            .document;
    }
    document
}

#[test]
fn rename_reorder_and_insert_preserve_only_proven_identities() {
    let before = LanguageDocument::import_new_root(SOURCE, "evo:reconcile-root").unwrap();
    let root_id = before.ref_for_sign("root").unwrap().id;
    let old_belongs = before
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.kind == NodeKind::Belongs)
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();

    let renamed_source = before.source().replace("sign root:", "sign stem:");
    let (renamed, report) = reconcile_edited_source(
        &before,
        &renamed_source,
        "evo:reconcile-rename",
        &ReconcileHints::default(),
    )
    .unwrap();
    assert_eq!(renamed.ref_for_sign("stem").unwrap().id, root_id);
    assert!(report.inserted.is_empty());
    assert!(report.deleted.is_empty());

    let reordered_source = r#"
trait A:
trait B:

sign root:
    belongs B
    belongs A
    phon:
        /r/
"#;
    let (reordered, _) = reconcile_edited_source(
        &before,
        reordered_source,
        "evo:reconcile-reorder",
        &ReconcileHints::default(),
    )
    .unwrap();
    let mut reordered_entries = reordered
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.kind == NodeKind::Belongs)
        .collect::<Vec<_>>();
    reordered_entries.sort_by(|left, right| left.address.cmp(&right.address));
    let reordered_ids = reordered_entries
        .into_iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        reordered_ids,
        [old_belongs[1].clone(), old_belongs[0].clone()]
    );
    let edits = reconstruct(&before, &reordered).unwrap();
    assert_eq!(
        edits
            .iter()
            .filter(|edit| matches!(edit, PrimitiveEdit::Move { .. }))
            .count(),
        1
    );

    let inserted_source = before
        .source()
        .replace("trait B:\n", "trait B:\ntrait C:\n");
    let (inserted, insert_report) = reconcile_edited_source(
        &before,
        &inserted_source,
        "evo:reconcile-insert",
        &ReconcileHints::default(),
    )
    .unwrap();
    assert_eq!(insert_report.inserted.len(), 2);
    assert!(insert_report
        .inserted
        .iter()
        .any(|node| node.expected == NodeKind::Trait));
    assert!(insert_report.inserted.iter().all(|node| {
        node.id.namespace
            == conlang_language::IdentityNamespace::Document("evo:reconcile-insert".to_owned())
    }));
    let replayed = replay(&before, &inserted, "evo:reconcile-insert");
    assert_eq!(replayed.source(), inserted.source());
    let (source, sidecar) = inserted.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &sidecar).unwrap(), inserted);
}

#[test]
fn identical_siblings_are_ambiguous_until_every_pair_is_hinted() {
    let source = r#"
trait A:

sign root:
    belongs A
    belongs A
    phon:
        /r/
"#;
    let before = LanguageDocument::import_new_root(source, "evo:ambiguous-root").unwrap();
    let error = reconcile_edited_source(
        &before,
        &before.source(),
        "evo:ambiguous-edit",
        &ReconcileHints::default(),
    )
    .unwrap_err();
    let ReconcileError::Ambiguous(ambiguities) = error else {
        panic!("expected explicit ambiguity")
    };
    assert_eq!(
        ambiguities
            .iter()
            .filter(|item| item.kind == NodeKind::Belongs)
            .count(),
        2
    );

    let belongs = before
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.kind == NodeKind::Belongs)
        .collect::<Vec<_>>();
    let hints = ReconcileHints {
        matches: belongs
            .iter()
            .map(|entry| ReconcileHint {
                previous: NodeRef::new(entry.id.clone(), entry.kind),
                edited_address: entry.address.clone(),
            })
            .collect(),
    };
    let (reconciled, report) =
        reconcile_edited_source(&before, &before.source(), "evo:ambiguous-edit", &hints).unwrap();
    assert_eq!(reconciled.source(), before.source());
    assert!(report.inserted.is_empty());
    assert!(report.deleted.is_empty());
}

#[test]
fn a_fully_changed_unnamed_node_requires_a_hint_instead_of_position_guessing() {
    let before = LanguageDocument::import_new_root(
        "sign root:\n    entrenchment = 0.1\n",
        "evo:changed-root",
    )
    .unwrap();
    let edited = "sign root:\n    components = stem\n";
    assert!(matches!(
        reconcile_edited_source(
            &before,
            edited,
            "evo:changed-edit",
            &ReconcileHints::default(),
        ),
        Err(ReconcileError::Ambiguous(_))
    ));

    let old = before
        .identities()
        .nodes
        .iter()
        .find(|entry| entry.kind == NodeKind::Definition)
        .unwrap();
    let fresh = LanguageDocument::import_new_root(edited, "evo:hint-address").unwrap();
    let new = fresh
        .identities()
        .nodes
        .iter()
        .find(|entry| entry.kind == NodeKind::Definition)
        .unwrap();
    let hints = ReconcileHints {
        matches: vec![ReconcileHint {
            previous: NodeRef::new(old.id.clone(), old.kind),
            edited_address: new.address.clone(),
        }],
    };
    let (reconciled, _) =
        reconcile_edited_source(&before, edited, "evo:changed-edit", &hints).unwrap();
    assert!(reconciled
        .identities()
        .nodes
        .iter()
        .any(|entry| entry.id == old.id && entry.kind == NodeKind::Definition));
}
