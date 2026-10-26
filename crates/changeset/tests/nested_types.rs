use conlang_changeset::{
    apply_edit, change_set_prelude, Anchor, ChangeInterpreter, DetachedNode, NodeUpdate,
    PrimitiveEdit, UnresolvedChangeSet,
};
use conlang_language::{Language, LanguageDocument, LibrarySpec};

const SOURCE: &str = "marker trait N:\ntrait Owner<T>:\n    export marker trait Bar:\n    export sign leaf:\n        belongs Bar\nsign seed:\n    belongs Owner<N>\n";

#[test]
fn nested_edits_reconstruct_replay_and_reopen_with_stable_ids() {
    let base = LanguageDocument::import_new_root(SOURCE, "nested:edit").unwrap();
    let leaf = base.ref_for_sign("Owner.leaf").unwrap();
    let mut text = change_set_prelude(&base, &LibrarySpec::default(), "nested:child").unwrap();
    text.push_str("\n    #0:\n        update sign(\"Owner.leaf\").name = renamed\n    #1:\n        update trait(\"N\").name = Nominal\n");
    let resolved = UnresolvedChangeSet::parse(&text)
        .unwrap()
        .resolve(&base, &LibrarySpec::default())
        .unwrap();
    let after = ChangeInterpreter::new(base.clone(), LibrarySpec::default(), "nested:child")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert_eq!(after.ref_for_sign("Owner.renamed").unwrap(), leaf);
    assert!(after.source().contains("Owner<Nominal>"));
    let edits = conlang_changeset::reconstruct::reconstruct(&base, &after).unwrap();
    let mut rebuilt = base.fork("nested:child").unwrap();
    for edit in edits {
        rebuilt = apply_edit(&rebuilt, edit, &LibrarySpec::default())
            .unwrap()
            .document;
    }
    assert_eq!(rebuilt.dump_pair().unwrap(), after.dump_pair().unwrap());
    let (source, ids) = after.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &ids).unwrap(), after);
}

#[test]
fn nested_insert_delete_and_owner_rename_keep_ownership() {
    let base = LanguageDocument::import_new_root(SOURCE, "nested:insert").unwrap();
    let owner = base.ref_for_trait("Owner").unwrap();
    let child = Language::parse("export marker trait Added:\n")
        .unwrap()
        .traits
        .remove(0);
    let inserted = apply_edit(
        &base,
        PrimitiveEdit::Insert {
            parent: owner.clone(),
            anchor: Anchor::End,
            subtree: DetachedNode::Trait(child),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let child = inserted.ref_for_trait("Owner.Added").unwrap();
    let renamed = apply_edit(
        &inserted,
        PrimitiveEdit::Update {
            node: owner.clone(),
            change: NodeUpdate::Rename("Phrase".into()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert_eq!(renamed.ref_for_trait("Phrase.Added").unwrap(), child);
    assert!(renamed.source().contains("Phrase<T>.Bar"));
    let deleted = apply_edit(
        &renamed,
        PrimitiveEdit::Delete { node: child },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let (source, ids) = deleted.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &ids).unwrap(), deleted);
    assert!(deleted.ref_for_trait("Phrase.Added").is_none());
}

#[test]
fn global_type_rename_does_not_rename_a_captured_parameter() {
    let base = LanguageDocument::import_new_root("marker trait T:\ntrait Owner<T>:\n    export sign leaf:\n        belongs T\nsign seed:\n    belongs Owner<T>\n", "nested:shadow").unwrap();
    let edited = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: base.ref_for_trait("T").unwrap(),
            change: NodeUpdate::Rename("N".into()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert!(edited.source().contains("trait Owner<T>"));
    assert!(edited.source().contains("belongs T\n"));
    assert!(edited.source().contains("Owner<N>"));
}

#[test]
fn nested_changes_are_counted_once_in_source_diff() {
    let base = LanguageDocument::import_new_root(SOURCE, "nested:diff").unwrap();
    let changed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: base.ref_for_sign("Owner.leaf").unwrap(),
            change: NodeUpdate::Exported(false),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let vector = conlang_changeset::diff::diff_vector(&base, &changed);
    assert_eq!(vector.structural.signs.changed, 1);
    assert_eq!(vector.born_signs(), 0);
    assert_eq!(vector.died_signs(), 0);
    let changed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: base.ref_for_trait("Owner.Bar").unwrap(),
            change: NodeUpdate::Exported(false),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let vector = conlang_changeset::diff::diff_vector(&base, &changed);
    assert_eq!(vector.structural.trait_content.counts.changed, 1);
    assert_eq!(
        vector.structural.trait_content.event_reaches.changed[0].before,
        1
    );
}

#[test]
fn adding_a_parameter_rebinds_same_spelled_global_references() {
    let base = LanguageDocument::import_new_root(
        "marker trait T:\ntrait Owner:\n    sign leaf:\n        belongs T\n",
        "nested:binding",
    )
    .unwrap();
    let changed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: base.ref_for_trait("Owner").unwrap(),
            change: NodeUpdate::TraitTypeParams(vec![conlang_language::TraitTypeParam {
                name: "T".into(),
                bound: None,
            }]),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let deleted = apply_edit(
        &changed,
        PrimitiveEdit::Delete {
            node: changed.ref_for_trait("T").unwrap(),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert!(deleted.source().contains("trait Owner<T>"));
}
