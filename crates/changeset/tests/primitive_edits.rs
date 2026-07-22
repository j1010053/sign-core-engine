use conlang_changeset::{
    apply_edit, Anchor, DetachedNode, EditError, LanguageDiffEntry, NodeUpdate, PrimitiveEdit,
    PrimitiveKind,
};
use conlang_language::{
    codegen, word, IdentityNamespace, Language, LanguageDocument, LibrarySpec, NodeId, NodeKind,
    NodeRef, SignItem,
};

const SOURCE: &str = r#"
trait Canine:
trait Mammal:
trait Pet:

global trait SoundHome:
    phon:
        a => b

trait OtherHome:

sign dog:
    belongs Mammal
    belongs Pet
    phon:
        /dog/
    sem:
        kind = animal

sign puppy:
    belongs Canine
    origin = sign(dog)
    phon:
        /papi/
"#;

fn document() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:test").unwrap()
}

fn child(document: &LanguageDocument, parent: &NodeRef, kind: NodeKind, ordinal: usize) -> NodeRef {
    let node = document
        .identities()
        .nodes
        .iter()
        .filter(|node| node.parent.as_ref() == Some(&parent.id) && node.kind == kind)
        .nth(ordinal)
        .unwrap();
    NodeRef::new(node.id.clone(), kind)
}

fn trait_block(document: &LanguageDocument, name: &str) -> NodeRef {
    let parent = document.ref_for_trait(name).unwrap();
    child(document, &parent, NodeKind::Block, 0)
}

#[test]
fn rename_preserves_identity_and_rewrites_stable_origin_display() {
    let before = document();
    let dog = before.ref_for_sign("dog").unwrap();
    let edit = PrimitiveEdit::Update {
        node: dog.clone(),
        change: NodeUpdate::Rename("hound".to_owned()),
    };
    let outcome = apply_edit(&before, edit.clone(), &LibrarySpec::default()).unwrap();
    let repeated = apply_edit(&before, edit, &LibrarySpec::default()).unwrap();

    assert_eq!(outcome.record.operation, PrimitiveKind::Update);
    assert_eq!(outcome, repeated);
    assert_eq!(outcome.document.ref_for_sign("hound").unwrap().id, dog.id);
    assert!(outcome.document.source().contains("origin = sign(hound)"));
    assert!(before.ref_for_sign("dog").is_some());
    assert!(before.source().contains("origin = sign(dog)"));

    let (source, manifest) = outcome.document.dump_pair().unwrap();
    assert_eq!(
        LanguageDocument::open(&source, &manifest).unwrap(),
        outcome.document
    );
}

#[test]
fn insert_delete_insert_is_birth_death_not_update() {
    let before = document();
    let cat = Language::parse(
        r#"sign cat:
    belongs Canine
    phon:
        /cat/
"#,
    )
    .unwrap()
    .signs
    .remove(0);
    let inserted = apply_edit(
        &before,
        PrimitiveEdit::Insert {
            parent: before.root_ref(),
            anchor: Anchor::End,
            subtree: DetachedNode::Sign(cat.clone()),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    let first_id = inserted.document.ref_for_sign("cat").unwrap().id;
    assert!(inserted
        .record
        .diff
        .entries
        .iter()
        .any(|entry| matches!(entry, LanguageDiffEntry::Inserted(node) if node.id == first_id)));

    let deleted = apply_edit(
        &inserted.document,
        PrimitiveEdit::Delete {
            node: inserted.document.ref_for_sign("cat").unwrap(),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    assert!(deleted.document.ref_for_sign("cat").is_none());
    assert!(deleted.record.deleted_ids.contains(&first_id));

    let reinserted = apply_edit(
        &deleted.document,
        PrimitiveEdit::Insert {
            parent: deleted.document.root_ref(),
            anchor: Anchor::End,
            subtree: DetachedNode::Sign(cat),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    assert_ne!(
        reinserted.document.ref_for_sign("cat").unwrap().id,
        first_id
    );
}

#[test]
fn update_form_keeps_sign_identity_but_changes_surface() {
    let before = LanguageDocument::import_new_root(
        r#"Symbol d
Symbol o
Symbol g
Symbol a
Class vowel {o, a}

sign dog:
    phon:
        /dog/
"#,
        "evo:surface",
    )
    .unwrap();
    let dog = before.ref_for_sign("dog").unwrap();
    let phon = child(&before, &dog, NodeKind::Definition, 0);
    let before_artifacts = codegen::compile_full(before.language()).unwrap();
    let before_surface = word::derive(
        &before_artifacts,
        &word::PhraseSpec(vec![word::Component::sign("dog")]),
    )
    .unwrap()
    .surface;

    let outcome = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: phon,
            change: NodeUpdate::DefinitionValue("/dag/".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    let after_artifacts = codegen::compile_full(outcome.document.language()).unwrap();
    let after_surface = word::derive(
        &after_artifacts,
        &word::PhraseSpec(vec![word::Component::sign("dog")]),
    )
    .unwrap()
    .surface;

    assert_eq!(outcome.document.ref_for_sign("dog").unwrap().id, dog.id);
    assert_eq!(before_surface, "dog");
    assert_eq!(after_surface, "dag");
}

#[test]
fn semantic_change_is_observable_when_surface_is_identical() {
    let before = document();
    let dog = before.ref_for_sign("dog").unwrap();
    let semantic_def = child(&before, &dog, NodeKind::Definition, 1);
    let outcome = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: semantic_def,
            change: NodeUpdate::DefinitionValue("companion".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    assert_eq!(
        word::phrase_text(
            &codegen::compile_full(before.language()).unwrap(),
            &word::PhraseSpec(vec![word::Component::sign("dog")])
        )
        .unwrap(),
        word::phrase_text(
            &codegen::compile_full(outcome.document.language()).unwrap(),
            &word::PhraseSpec(vec![word::Component::sign("dog")])
        )
        .unwrap()
    );
    assert!(outcome.document.source().contains("kind = companion"));
    assert!(outcome.record.diff.entries.iter().any(|entry| {
        matches!(entry, LanguageDiffEntry::Updated { after, .. } if after.value.contains("companion"))
    }));
}

#[test]
fn updating_one_parent_preserves_other_inheritance_links() {
    let before = document();
    let dog = before.ref_for_sign("dog").unwrap();
    let first_parent = child(&before, &dog, NodeKind::Belongs, 0);
    let outcome = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: first_parent,
            change: NodeUpdate::Belongs("Canine".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    let dog = outcome.document.language().sign_named("dog").unwrap();
    let parents: Vec<_> = dog
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::Belongs(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(parents, ["Canine", "Pet"]);
}

#[test]
fn move_rule_between_homes_preserves_rule_identity() {
    let before = document();
    let source_block = trait_block(&before, "SoundHome");
    let target_block = trait_block(&before, "OtherHome");
    let rule = child(&before, &source_block, NodeKind::Rule, 0);
    let before_rule_id = match before.language().trait_named("SoundHome").unwrap().blocks[0]
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::Rule(rule) => Some(rule.id.clone()),
            _ => None,
        }) {
        Some(id) => id,
        None => panic!("missing source rule"),
    };
    let outcome = apply_edit(
        &before,
        PrimitiveEdit::Move {
            node: rule.clone(),
            new_parent: target_block,
            anchor: Anchor::End,
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    assert!(outcome.record.moved_ids.contains(&rule.id));
    let moved_rule_id = outcome
        .document
        .language()
        .trait_named("OtherHome")
        .unwrap()
        .blocks[0]
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::Rule(rule) => Some(rule.id.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(moved_rule_id, before_rule_id);
}

#[test]
fn invalid_anchor_and_dangling_delete_fail_without_mutating_source() {
    let before = document();
    let dog = before.ref_for_sign("dog").unwrap();
    let puppy = before.ref_for_sign("puppy").unwrap();
    let cat = Language::parse("sign cat:\n    phon:\n        /cat/\n")
        .unwrap()
        .signs
        .remove(0);
    let anchor_error = apply_edit(
        &before,
        PrimitiveEdit::Insert {
            parent: before.root_ref(),
            anchor: Anchor::Before(puppy),
            subtree: DetachedNode::Sign(cat),
        },
        &LibrarySpec::default(),
    )
    .unwrap_err();
    assert!(matches!(anchor_error, EditError::AnchorInvalid(_)));
    assert_eq!(before, document());

    let delete_error = apply_edit(
        &before,
        PrimitiveEdit::Delete { node: dog },
        &LibrarySpec::default(),
    )
    .unwrap_err();
    assert!(matches!(delete_error, EditError::Validation(_)));
    assert_eq!(before, document());
}

#[test]
fn stable_anchor_survives_prior_insert_and_delete() {
    let before = document();
    let dog = before.ref_for_sign("dog").unwrap();
    let mammal = child(&before, &dog, NodeKind::Belongs, 0);
    let pet = child(&before, &dog, NodeKind::Belongs, 1);

    let inserted = apply_edit(
        &before,
        PrimitiveEdit::Insert {
            parent: dog,
            anchor: Anchor::Before(pet.clone()),
            subtree: DetachedNode::Item(SignItem::Belongs("Canine".to_owned())),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    let deleted = apply_edit(
        &inserted.document,
        PrimitiveEdit::Delete { node: mammal },
        &LibrarySpec::default(),
    )
    .unwrap();
    let reinserted = apply_edit(
        &deleted.document,
        PrimitiveEdit::Insert {
            parent: deleted.document.ref_for_sign("dog").unwrap(),
            anchor: Anchor::Before(pet.clone()),
            subtree: DetachedNode::Item(SignItem::Belongs("Mammal".to_owned())),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    let parents: Vec<_> = reinserted
        .document
        .language()
        .sign_named("dog")
        .unwrap()
        .items
        .iter()
        .filter_map(|item| match item {
            SignItem::Belongs(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(parents, ["Canine", "Mammal", "Pet"]);
    assert_eq!(
        reinserted.document.resolve_node(&pet).unwrap().node.id,
        pet.id
    );
}

#[test]
fn library_owned_target_is_never_editable() {
    let before = document();
    let external = NodeRef::new(
        NodeId::new(IdentityNamespace::Library("std:core".to_owned()), 1),
        NodeKind::Trait,
    );
    let error = apply_edit(
        &before,
        PrimitiveEdit::Delete { node: external },
        &LibrarySpec::default(),
    )
    .unwrap_err();

    assert!(matches!(error, EditError::ExternalTarget(_)));
    assert_eq!(before, document());
}
