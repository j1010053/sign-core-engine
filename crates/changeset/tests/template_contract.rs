use conlang_changeset::{apply_edit, Anchor, DetachedNode, NodeUpdate, PrimitiveEdit};
use conlang_language::{Language, LanguageDocument, LibrarySpec, RefTargetV1};

const SOURCE: &str = "Symbol a\nClass vowel {a}\nmarker trait N:\ntrait Template<T>:\n    export sign leaf:\n        belongs T\n        phon:\n            /a/\nsign Receiver:\n    belongs Template<N>\n    sign local:\n        belongs N\n        syn:\n            slots:\n                value [N]\n        phon:\n            /{$slot.value}/\n    syn:\n        slots:\n            value [N]\n        map value autofill local(value: leaf())\n    phon:\n        /{$slot.value}/\n";

#[test]
fn inherited_alias_rename_replays_and_nested_sign_ids_survive() {
    let base = LanguageDocument::import_new_root(SOURCE, "template:edit").unwrap();
    let source_leaf = base.ref_for_sign("Template.leaf").unwrap();
    let local = base.ref_for_sign("Receiver.local").unwrap();
    assert!(
        base.ref_for_sign("Receiver.leaf").is_none(),
        "an alias is not a source declaration"
    );
    assert!(base
        .identities()
        .refs
        .iter()
        .any(|r| r.field == "slot_map.argument.0.callee"
            && matches!(&r.target, RefTargetV1::Local { target } if target == &source_leaf)));
    let renamed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: source_leaf.clone(),
            change: NodeUpdate::Rename("word".into()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert_eq!(renamed.ref_for_sign("Template.word").unwrap(), source_leaf);
    assert_eq!(renamed.ref_for_sign("Receiver.local").unwrap(), local);
    assert!(renamed.source().contains("Receiver.word()"));
    let renamed = apply_edit(
        &renamed,
        PrimitiveEdit::Update {
            node: renamed.ref_for_sign("Receiver").unwrap(),
            change: NodeUpdate::Rename("Phrase".into()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert_eq!(renamed.ref_for_sign("Phrase.local").unwrap(), local);
    assert!(renamed.source().contains("Phrase.word()"));
    let edits = conlang_changeset::reconstruct::reconstruct(&base, &renamed).unwrap();
    let mut replay = base.clone();
    for edit in edits {
        replay = apply_edit(&replay, edit, &LibrarySpec::default())
            .unwrap()
            .document;
    }
    assert_eq!(replay.dump_pair().unwrap(), renamed.dump_pair().unwrap());
    let (text, ids) = replay.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&text, &ids).unwrap();
    let system = conlang_language::compile_document(&reopened, &LibrarySpec::default()).unwrap();
    assert_eq!(
        system
            .derive("Phrase", &[], &Default::default())
            .unwrap()
            .surface,
        "a"
    );
    assert_eq!(reopened.language().signs_recursive().len(), 3);
}

#[test]
fn sign_scope_insertion_deletion_and_failed_delete_are_atomic() {
    let base = LanguageDocument::import_new_root(SOURCE, "template:insert").unwrap();
    let before = base.dump_pair().unwrap();
    let deleted = apply_edit(
        &base,
        PrimitiveEdit::Delete {
            node: base.ref_for_sign("Template.leaf").unwrap(),
        },
        &LibrarySpec::default(),
    );
    assert!(
        deleted.is_err(),
        "inherited aliases keep references to their source"
    );
    assert_eq!(base.dump_pair().unwrap(), before);
    let mut fragment = Language::parse("export sign Added:\n    export marker trait Tag:\n    export sign child:\n        belongs Tag\n").unwrap();
    let inserted = apply_edit(
        &base,
        PrimitiveEdit::Insert {
            parent: base.ref_for_sign("Receiver").unwrap(),
            anchor: Anchor::End,
            subtree: DetachedNode::Sign(fragment.signs.remove(0)),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let added = inserted.ref_for_sign("Receiver.Added").unwrap();
    let child = inserted.ref_for_sign("Receiver.Added.child").unwrap();
    assert_eq!(
        inserted
            .identities()
            .nodes
            .iter()
            .find(|entry| entry.id == child.id)
            .unwrap()
            .parent
            .as_ref(),
        Some(&added.id)
    );
    let deleted = apply_edit(
        &inserted,
        PrimitiveEdit::Delete { node: added },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert!(deleted.ref_for_sign("Receiver.Added.child").is_none());
    assert_eq!(deleted.source(), base.source());
    let (text, ids) = deleted.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&text, &ids).unwrap(), deleted);
}

#[test]
fn a_nested_sign_change_is_counted_once() {
    let base = LanguageDocument::import_new_root(SOURCE, "template:diff").unwrap();
    let changed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: base.ref_for_sign("Receiver.local").unwrap(),
            change: NodeUpdate::Exported(true),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let diff = conlang_changeset::diff::diff_vector(&base, &changed);
    assert_eq!(diff.structural.signs.changed, 1);
    assert_eq!(diff.born_signs(), 0);
    assert_eq!(diff.died_signs(), 0);
}

#[test]
fn metadata_alias_references_rename_and_block_dangling_source_deletion() {
    let source = format!("{SOURCE}sign Derived:\n    origin = sign(Receiver.leaf)\n    components = sign(Receiver.leaf), sign(Receiver)\n    phon:\n        /a/\n");
    let base = LanguageDocument::import_new_root(&source, "template:metadata").unwrap();
    let leaf = base.ref_for_sign("Template.leaf").unwrap();
    for field in ["origin", "components.0"] {
        assert!(base.identities().refs.iter().any(|r| r.field == field
            && matches!(&r.target, RefTargetV1::Local { target } if target == &leaf)));
    }
    let renamed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: leaf.clone(),
            change: NodeUpdate::Rename("word".into()),
        },
        &Default::default(),
    )
    .unwrap()
    .document;
    let sign = renamed.language().sign_named("Derived").unwrap();
    assert_eq!(sign.origin().unwrap().0, "Receiver.word");
    assert_eq!(sign.components().unwrap()[0].0, "Receiver.word");
    assert!(apply_edit(
        &renamed,
        PrimitiveEdit::Delete { node: leaf },
        &Default::default()
    )
    .is_err());
    let (source, ids) = renamed.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &ids).unwrap(), renamed);
}

#[test]
fn multilevel_aliases_track_leaf_and_owner_renames() {
    let source = "Symbol a\nClass vowel {a}\nmarker trait N:\ntrait Template<T>:\n    export sign Builder:\n        export sign leaf:\n            belongs T\n            phon:\n                /a/\nsign Receiver:\n    belongs Template<N>\n    syn:\n        slots:\n            value [N]\n        map value autofill Builder.leaf()\n    phon:\n        /{$slot.value}/\nsign Derived:\n    origin = sign(Receiver.Builder.leaf)\n    phon:\n        /a/\n";
    let base = LanguageDocument::import_new_root(source, "template:deep-alias").unwrap();
    let leaf = base.ref_for_sign("Template.Builder.leaf").unwrap();
    for field in ["origin", "slot_map.autofill"] {
        assert!(base.identities().refs.iter().any(|r| r.field == field
            && matches!(&r.target, RefTargetV1::Local { target } if target == &leaf)));
    }
    let renamed = apply_edit(
        &base,
        PrimitiveEdit::Update {
            node: leaf.clone(),
            change: NodeUpdate::Rename("word".into()),
        },
        &Default::default(),
    )
    .unwrap()
    .document;
    let renamed = apply_edit(
        &renamed,
        PrimitiveEdit::Update {
            node: renamed.ref_for_sign("Template.Builder").unwrap(),
            change: NodeUpdate::Rename("Factory".into()),
        },
        &Default::default(),
    )
    .unwrap()
    .document;
    assert_eq!(renamed.ref_for_sign("Template.Factory.word").unwrap(), leaf);
    assert!(renamed.source().contains("Receiver.Factory.word()"));
    assert_eq!(
        renamed
            .language()
            .sign_named("Derived")
            .unwrap()
            .origin()
            .unwrap()
            .0,
        "Receiver.Factory.word"
    );
    let system = conlang_language::compile_document(&renamed, &Default::default()).unwrap();
    assert_eq!(
        system
            .derive("Receiver", &[], &Default::default())
            .unwrap()
            .surface,
        "a"
    );
    let mut replay = base.clone();
    for edit in conlang_changeset::reconstruct::reconstruct(&base, &renamed).unwrap() {
        replay = apply_edit(&replay, edit, &Default::default())
            .unwrap()
            .document;
    }
    assert_eq!(replay.dump_pair().unwrap(), renamed.dump_pair().unwrap());
}
