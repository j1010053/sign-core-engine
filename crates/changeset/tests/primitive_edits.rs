use conlang_changeset::{
    apply_edit, Anchor, DetachedNode, EditError, LanguageDiff, LanguageDiffEntry, NodeUpdate,
    PrimitiveEdit, PrimitiveKind,
};
use conlang_language::{
    codegen, word, AddressSegment, CaseBranch, CaseCondition, CaseSelection, ConstraintPredicate,
    Def, Expression, IdentityNamespace, Language, LanguageDocument, LibrarySpec, NodeAddress,
    NodeId, NodeKind, NodeRef, RefTargetV1, SignApplication, SignExpression, SignItem,
    SignProjection, SourceLocation,
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
        feature:
            kind = enum(animal, companion)
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

const V2_SOURCE: &str = r#"trait Atom:

trait Other:

trait Selected:

trait Alternate:

sign wrap:
    syn:
        slots:
            value [*]
    phon:
        /{value}+w/

sign alt:
    syn:
        slots:
            value [*]
    phon:
        /{value}+a/

sign outer:
    syn:
        slots:
            value [*]
    phon:
        /<{value}>/

sign atom:
    belongs Atom
    phon:
        /x/
    case:
        $self == [Atom]:
            wrap({$self})
            belongs Selected
        $self == [Other]:
            outer(wrap({$self}))
            belongs Alternate
        else:
            alt({$self})

sign sequence:
    syn:
        slots:
            left [*]
            right [*]
    phon:
        /{left} {right}/
    constraints:
        before(left, right)
"#;

fn v2_document() -> LanguageDocument {
    LanguageDocument::import_new_root(V2_SOURCE, "evo:v2-edits").unwrap()
}

fn sign_case(document: &LanguageDocument, name: &str) -> NodeRef {
    let sign = document.ref_for_sign(name).unwrap();
    child(document, &sign, NodeKind::Case, 0)
}

fn source_case<'a>(document: &'a LanguageDocument, name: &str) -> &'a conlang_language::TypedCase {
    document
        .language()
        .sign_named(name)
        .unwrap()
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::SignExpression(expression) => match &expression.expression {
                Expression::Case(case) => Some(case.as_ref()),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}

fn direct_application(branch: &CaseBranch) -> SignApplication {
    match &branch.result {
        Expression::SignApplication(application) => application.clone(),
        other => panic!("expected direct application, got {other:?}"),
    }
}

#[test]
fn sign_context_fragment_items_support_all_four_primitive_edits() {
    let source = r#"trait PrimitiveFragmentMark:

sign root:
    phon:
        /r/
    case:
        else:
            belongs PrimitiveFragmentMark
            sem:
                time.past = one
                time.present = two
"#;
    let before = LanguageDocument::import_new_root(source, "evo:fragment-edits").unwrap();
    let case = sign_case(&before, "root");
    let branch = child(&before, &case, NodeKind::CaseBranch, 0);
    let first = child(&before, &branch, NodeKind::Definition, 0);

    let updated = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: first.clone(),
            change: NodeUpdate::DefinitionValue("updated".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert!(updated.source().contains("time.past = updated"));
    assert_eq!(
        child(&updated, &branch, NodeKind::Definition, 0).id,
        first.id
    );

    let inserted = apply_edit(
        &updated,
        PrimitiveEdit::Insert {
            parent: branch.clone(),
            anchor: Anchor::End,
            subtree: DetachedNode::Item(SignItem::Def(Def {
                path: "sem.time.future".to_owned(),
                value: "three".to_owned(),
            })),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let third = child(&inserted, &branch, NodeKind::Definition, 2);

    let moved = apply_edit(
        &inserted,
        PrimitiveEdit::Move {
            node: third.clone(),
            new_parent: branch.clone(),
            anchor: Anchor::Before(first),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let moved_third = moved
        .identities()
        .nodes
        .iter()
        .find(|node| node.id == third.id)
        .unwrap();
    assert!(
        matches!(moved_third.address.0.last(), Some(AddressSegment::Items(1))),
        "Move preserves identity and updates the logical fragment position"
    );

    let deleted = apply_edit(
        &moved,
        PrimitiveEdit::Delete {
            node: third.clone(),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    assert!(!deleted
        .identities()
        .nodes
        .iter()
        .any(|node| node.id == third.id));
    assert!(!deleted.source().contains("third = three"));
    let (canonical, manifest) = deleted.dump_pair().unwrap();
    assert_eq!(
        LanguageDocument::open(&canonical, &manifest).unwrap(),
        deleted
    );
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
    // P71 §4.3:dog 的 sem 內容已是宣告過的 feature,故走 FeatureAssignment。
    let semantic_value = child(&before, &dog, NodeKind::FeatureValue, 0);
    let outcome = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: semantic_value,
            change: NodeUpdate::FeatureAssignment("companion".to_owned()),
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
            SignItem::TraitMount { name: name, kind: conlang_language::TraitMountKind::Declaration } => Some(name.as_str()),
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
            subtree: DetachedNode::Item(SignItem::TraitMount { name: "Canine".to_owned(), kind: conlang_language::TraitMountKind::Declaration }),
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
            subtree: DetachedNode::Item(SignItem::TraitMount { name: "Mammal".to_owned(), kind: conlang_language::TraitMountKind::Declaration }),
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
            SignItem::TraitMount { name: name, kind: conlang_language::TraitMountKind::Declaration } => Some(name.as_str()),
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

#[test]
fn v2_typed_updates_preserve_case_application_and_constraint_identity() {
    let before = v2_document();
    let case = sign_case(&before, "atom");
    let branch = child(&before, &case, NodeKind::CaseBranch, 0);
    let application = child(&before, &branch, NodeKind::Application, 0);
    let sequence = before.ref_for_sign("sequence").unwrap();
    let constraint = child(&before, &sequence, NodeKind::Constraint, 0);

    let mut replacement_application = direct_application(&source_case(&before, "atom").branches[0]);
    replacement_application.callee = "alt".to_owned();
    let application_update = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: application.clone(),
            change: NodeUpdate::SignApplication(replacement_application),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    assert_eq!(
        application_update
            .document
            .resolve_node(&application)
            .unwrap()
            .node
            .id,
        application.id
    );
    assert!(application_update
        .document
        .source()
        .contains("alt({$self})"));
    assert!(application_update.record.diff.entries.iter().any(
        |entry| matches!(entry, LanguageDiffEntry::Updated { after, .. } if after.id == application.id)
    ));

    let mut replacement_branch = source_case(&before, "atom").branches[0].clone();
    replacement_branch.belongs = vec!["Alternate".to_owned()];
    let branch_update = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: branch.clone(),
            change: NodeUpdate::CaseBranch(replacement_branch),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    assert_eq!(
        branch_update
            .document
            .resolve_node(&branch)
            .unwrap()
            .node
            .id,
        branch.id
    );
    assert_eq!(
        child(&branch_update.document, &branch, NodeKind::Application, 0).id,
        application.id,
        "unchanged result application retains its identity"
    );
    assert!(branch_update
        .document
        .source()
        .contains("belongs Alternate"));

    let mut replacement_constraint = before
        .language()
        .sign_named("sequence")
        .unwrap()
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::Constraint(value) => Some(value.clone()),
            _ => None,
        })
        .unwrap();
    replacement_constraint.predicate = ConstraintPredicate::Adjacent;
    let constraint_update = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: constraint.clone(),
            change: NodeUpdate::Constraint(replacement_constraint),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    assert_eq!(
        constraint_update
            .document
            .resolve_node(&constraint)
            .unwrap()
            .node
            .id,
        constraint.id
    );
    assert!(constraint_update
        .document
        .source()
        .contains("adjacent(left, right)"));
}

#[test]
fn case_selection_is_a_typed_identity_preserving_update() {
    let before = LanguageDocument::import_new_root(
        r#"sign root:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = on
            result = enum(base, selected)
            result = base
    phon:
        /x/
    case:
        $self.syn.trigger == on:
            syn:
                feature:
                    result = selected
"#,
        "evo:case-selection",
    )
    .unwrap();
    let sign = before.ref_for_sign("root").unwrap();
    let case = child(&before, &sign, NodeKind::Case, 0);
    let updated = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: case.clone(),
            change: NodeUpdate::CaseSelection(CaseSelection::Accumulate),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    assert_eq!(
        updated.document.resolve_node(&case).unwrap().node.id,
        case.id
    );
    assert!(updated.document.source().contains("    when:"));
    assert!(updated.record.diff.entries.iter().any(
        |entry| matches!(entry, LanguageDiffEntry::Updated { after, .. } if after.id == case.id)
    ));
}

#[test]
fn case_branch_insert_allocates_nested_expression_subtree_and_delete_removes_it() {
    let before = v2_document();
    let case = sign_case(&before, "atom");
    let fallback = child(&before, &case, NodeKind::CaseBranch, 2);
    let mut nested = source_case(&before, "atom").branches[1].clone();
    nested.condition = CaseCondition::Guard("$self == [Selected]".to_owned());

    let inserted = apply_edit(
        &before,
        PrimitiveEdit::Insert {
            parent: case.clone(),
            anchor: Anchor::Before(fallback.clone()),
            subtree: DetachedNode::CaseBranch(nested),
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    let allocated = inserted
        .record
        .allocated_ids
        .iter()
        .filter_map(|id| {
            inserted
                .document
                .identities()
                .nodes
                .iter()
                .find(|entry| &entry.id == id)
                .map(|entry| (entry.id.clone(), entry.kind))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        allocated
            .iter()
            .filter(|(_, kind)| *kind == NodeKind::CaseBranch)
            .count(),
        1
    );
    assert_eq!(
        allocated
            .iter()
            .filter(|(_, kind)| *kind == NodeKind::Application)
            .count(),
        2,
        "outer(wrap(...)) allocates both application identities"
    );
    let inserted_branch_id = allocated
        .iter()
        .find(|(_, kind)| *kind == NodeKind::CaseBranch)
        .unwrap()
        .0
        .clone();
    let inserted_branch = NodeRef::new(inserted_branch_id.clone(), NodeKind::CaseBranch);
    let outer = child(
        &inserted.document,
        &inserted_branch,
        NodeKind::Application,
        0,
    );
    let inner = child(&inserted.document, &outer, NodeKind::Application, 0);
    assert_ne!(outer.id, inner.id);
    assert_eq!(
        inserted.document.resolve_node(&fallback).unwrap().node.id,
        fallback.id,
        "stable fallback anchor survives the insertion"
    );

    let deleted = apply_edit(
        &inserted.document,
        PrimitiveEdit::Delete {
            node: inserted_branch,
        },
        &LibrarySpec::default(),
    )
    .unwrap();
    assert!(deleted.record.deleted_ids.contains(&inserted_branch_id));
    assert!(deleted.record.deleted_ids.contains(&outer.id));
    assert!(deleted.record.deleted_ids.contains(&inner.id));
    assert_eq!(
        deleted.document.resolve_node(&fallback).unwrap().node.id,
        fallback.id
    );
}

#[test]
fn moving_case_branch_preserves_branch_and_nested_application_identities() {
    let before = v2_document();
    let case = sign_case(&before, "atom");
    let first = child(&before, &case, NodeKind::CaseBranch, 0);
    let second = child(&before, &case, NodeKind::CaseBranch, 1);
    let outer = child(&before, &second, NodeKind::Application, 0);
    let inner = child(&before, &outer, NodeKind::Application, 0);

    let moved = apply_edit(
        &before,
        PrimitiveEdit::Move {
            node: second.clone(),
            new_parent: case,
            anchor: Anchor::Before(first),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    assert!(moved.record.moved_ids.contains(&second.id));
    assert_eq!(
        child(&moved.document, &second, NodeKind::Application, 0).id,
        outer.id
    );
    assert_eq!(
        child(&moved.document, &outer, NodeKind::Application, 0).id,
        inner.id
    );
    assert!(moved.record.diff.entries.iter().all(|entry| !matches!(
        entry,
        LanguageDiffEntry::Updated { after, .. }
            if [second.id.clone(), outer.id.clone(), inner.id.clone()].contains(&after.id)
    )));
    let (source, manifest) = moved.document.dump_pair().unwrap();
    assert_eq!(
        LanguageDocument::open(&source, &manifest).unwrap(),
        moved.document
    );
}

#[test]
fn invalid_case_update_rolls_back_ast_ids_and_allocator() {
    let before = v2_document();
    let case = sign_case(&before, "atom");
    let branch = child(&before, &case, NodeKind::CaseBranch, 0);
    let mut invalid = source_case(&before, "atom").branches[0].clone();
    let Expression::SignApplication(application) = &mut invalid.result else {
        panic!("fixture branch must return an application")
    };
    application.callee = "MissingSign".to_owned();

    let error = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: branch,
            change: NodeUpdate::CaseBranch(invalid),
        },
        &LibrarySpec::default(),
    )
    .unwrap_err();
    assert!(matches!(error, EditError::Validation(_)));
    assert_eq!(before, v2_document());
}

#[test]
fn renaming_a_sign_rewrites_direct_and_nested_application_spellings() {
    let before = v2_document();
    let wrap = before.ref_for_sign("wrap").unwrap();
    let renamed = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: wrap.clone(),
            change: NodeUpdate::Rename("wrapper".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    assert_eq!(
        renamed.document.ref_for_sign("wrapper").unwrap().id,
        wrap.id
    );
    assert!(!renamed.document.source().contains("wrap({$self})"));
    assert!(renamed.document.source().contains("wrapper({$self})"));
    let (source, manifest) = renamed.document.dump_pair().unwrap();
    assert_eq!(
        LanguageDocument::open(&source, &manifest).unwrap(),
        renamed.document
    );
}

#[test]
fn renaming_a_trait_rewrites_and_rebinds_typed_case_guard_categories() {
    let before = v2_document();
    let atom = before.ref_for_trait("Atom").unwrap();
    let case = sign_case(&before, "atom");
    let guarded_branch = child(&before, &case, NodeKind::CaseBranch, 0);

    let renamed = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: atom.clone(),
            change: NodeUpdate::Rename("Element".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    assert_eq!(
        renamed.document.ref_for_trait("Element").unwrap().id,
        atom.id
    );
    assert!(renamed.document.source().contains("$self == [Element]:"));
    assert!(!renamed.document.source().contains("$self == [Atom]:"));
    assert!(renamed.document.identities().refs.iter().any(|binding| {
        binding.owner == guarded_branch.id
            && binding.field == "case.guard[0].category"
            && matches!(
                &binding.target,
                RefTargetV1::Local { target } if target.id == atom.id
            )
    }));
    let (source, manifest) = renamed.document.dump_pair().unwrap();
    assert_eq!(
        LanguageDocument::open(&source, &manifest).unwrap(),
        renamed.document
    );
}

#[test]
fn malformed_typed_case_guard_is_rejected_atomically() {
    let before = v2_document();
    let case = sign_case(&before, "atom");
    let branch = child(&before, &case, NodeKind::CaseBranch, 0);
    let mut invalid = source_case(&before, "atom").branches[0].clone();
    invalid.condition = CaseCondition::Guard("not a guard".to_owned());

    let error = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: branch,
            change: NodeUpdate::CaseBranch(invalid),
        },
        &LibrarySpec::default(),
    )
    .unwrap_err();

    assert!(matches!(error, EditError::Validation(_)));
    assert_eq!(before, v2_document());
}

#[test]
fn distribution_update_keeps_identity_through_canonical_reordering() {
    let before = LanguageDocument::import_new_root(
        "distribution:\n    alpha = first\n    beta = second\n",
        "evo:distribution",
    )
    .unwrap();
    let alpha_entry = before
        .identities()
        .nodes
        .iter()
        .find(|entry| {
            entry.address == NodeAddress(vec![AddressSegment::Distribution(0)])
                && entry.kind == NodeKind::Distribution
        })
        .unwrap();
    let beta_entry = before
        .identities()
        .nodes
        .iter()
        .find(|entry| {
            entry.address == NodeAddress(vec![AddressSegment::Distribution(1)])
                && entry.kind == NodeKind::Distribution
        })
        .unwrap();
    let alpha = NodeRef::new(alpha_entry.id.clone(), NodeKind::Distribution);
    let beta = NodeRef::new(beta_entry.id.clone(), NodeKind::Distribution);

    let outcome = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: alpha.clone(),
            change: NodeUpdate::Distribution {
                key: "zeta".to_owned(),
                value: "last".to_owned(),
            },
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    let alpha_address = outcome.document.resolve_node(&alpha).unwrap().address;
    let beta_address = outcome.document.resolve_node(&beta).unwrap().address;
    let [AddressSegment::Distribution(alpha_index)] = alpha_address.0.as_slice() else {
        panic!("updated distribution identity must retain a distribution address")
    };
    let [AddressSegment::Distribution(beta_index)] = beta_address.0.as_slice() else {
        panic!("unchanged distribution identity must retain a distribution address")
    };
    assert_eq!(
        outcome.document.language().distribution[*alpha_index],
        ("zeta".to_owned(), "last".to_owned())
    );
    assert_eq!(
        outcome.document.language().distribution[*beta_index],
        ("beta".to_owned(), "second".to_owned())
    );
    assert!(outcome.record.diff.entries.iter().any(
        |entry| matches!(entry, LanguageDiffEntry::Updated { after, .. } if after.id == alpha.id)
    ));
    assert!(outcome
        .record
        .diff
        .entries
        .iter()
        .all(|entry| !matches!(entry, LanguageDiffEntry::Moved { .. })));
}

#[test]
fn diff_observes_projection_and_interpolation_wrapper_changes() {
    let before = LanguageDocument::import_new_root(
        r#"sign wrap:
    syn:
        slots:
            value [*]
    phon:
        /{value}/

sign root:
    phon:
        /r/
        realization:
            case:
                else:
                    /{wrap({$self}).phon.ret}/
"#,
        "evo:projection-diff",
    )
    .unwrap();
    let root = before.ref_for_sign("root").unwrap();
    // 取徑 A 之後 realization item 自己就是 `Case` 節點,不再多一層 wrapper。
    let case = child(&before, &root, NodeKind::Case, 0);
    let branch = child(&before, &case, NodeKind::CaseBranch, 0);
    let (mut language, identities) = before.clone().into_edit_parts();
    let realization = language
        .signs
        .iter_mut()
        .find(|sign| sign.name == "root")
        .unwrap()
        .items
        .iter_mut()
        .find_map(|item| match item {
            SignItem::Realization(value) => Some(value),
            _ => None,
        })
        .unwrap();
    let result = &mut realization.expression.branches[0].result;
    let Expression::PhonInterpolation(application) = result else {
        panic!("fixture must contain a phon interpolation")
    };
    *result = Expression::Projection {
        value: Box::new(Expression::SignApplication(application.clone())),
        dimension: SignProjection::Phon,
    };
    let after = LanguageDocument::from_edit_parts(language, identities).unwrap();

    let diff = LanguageDiff::between(&before, &after);
    assert!(diff.entries.iter().any(
        |entry| matches!(entry, LanguageDiffEntry::Updated { after, .. } if after.id == branch.id)
    ));
}

#[test]
fn slot_rename_rewrites_typed_consumers_and_named_applications() {
    let before = LanguageDocument::import_new_root(
        r#"trait LocalEntity:

sign wrap:
    syn:
        slots:
            value [*]
    phon:
        /{value}/

sign target:
    belongs LocalEntity
    syn:
        slots:
            subject [LocalEntity]
            object [LocalEntity]
    phon:
        /{subject} {object}/
    sem:
        roles:
            agent [LocalEntity]
            agent = {subject}
    constraints:
        before(subject, object)
    case:
        $slot.subject == [LocalEntity]:
            wrap({subject})
        else:
            $self

sign caller:
    belongs LocalEntity
    phon:
        /caller/
    case:
        else:
            target(subject: {$self})
"#,
        "evo:slot-rename",
    )
    .unwrap();
    let target = before.ref_for_sign("target").unwrap();
    let subject = child(&before, &target, NodeKind::Slot, 0);

    let renamed = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: subject.clone(),
            change: NodeUpdate::SlotName("actor".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    assert_eq!(
        renamed.document.resolve_node(&subject).unwrap().node.id,
        subject.id
    );
    let source = renamed.document.source();
    for expected in [
        "actor [LocalEntity]",
        "/{actor} {object}/",
        "agent = {actor}",
        "before(actor, object)",
        "$slot.actor == [LocalEntity]:",
        "wrap({actor})",
        "target(actor: {$self})",
    ] {
        assert!(source.contains(expected), "missing rewritten {expected:?}");
    }
    assert!(!source.contains("$slot.subject"));
    assert!(!source.contains("target(subject ="));
    let (source, manifest) = renamed.document.dump_pair().unwrap();
    assert_eq!(
        LanguageDocument::open(&source, &manifest).unwrap(),
        renamed.document
    );
}

#[test]
fn trait_slot_rename_stops_at_an_intermediate_shadow() {
    let before = LanguageDocument::import_new_root(
        r#"trait Base:
    syn:
        slots:
            subject [*]

trait Pass:
    belongs Base
    phon:
        /p{subject}/

trait Shadow:
    belongs Base
    syn:
        slots:
            subject [*]
    phon:
        /s{subject}/

trait Leaf:
    belongs Shadow
    phon:
        /l{subject}/
"#,
        "evo:trait-slot-shadow",
    )
    .unwrap();
    let base = before.ref_for_trait("Base").unwrap();
    let block = child(&before, &base, NodeKind::Block, 0);
    let subject = child(&before, &block, NodeKind::Slot, 0);

    let renamed = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: subject.clone(),
            change: NodeUpdate::SlotName("actor".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    assert_eq!(
        renamed.document.resolve_node(&subject).unwrap().node.id,
        subject.id
    );
    let source = renamed.document.source();
    assert!(source.contains("actor [*]"));
    assert!(source.contains("/p{actor}/"));
    assert!(source.contains("subject [*]"));
    assert!(source.contains("/s{subject}/"));
    assert!(source.contains("/l{subject}/"));
}

#[test]
fn nested_case_identity_survives_branch_move_and_canonical_reopen() {
    let before = LanguageDocument::import_new_root(
        r#"trait Atom:
trait Other:

sign wrap:
    syn:
        slots:
            value [*]
    phon:
        /{value}/

sign atom:
    belongs Atom
    phon:
        /x/
    case:
        $self == [Atom]:
            case:
                $self == [Other]:
                    wrap({$self})
                $self == [Atom]:
                    wrap({$self})
        else:
            $self
"#,
        "evo:nested-case",
    )
    .unwrap();
    let atom = before.ref_for_sign("atom").unwrap();
    let outer_case = child(&before, &atom, NodeKind::Case, 0);
    let outer_branch = child(&before, &outer_case, NodeKind::CaseBranch, 0);
    let inner_case = child(&before, &outer_branch, NodeKind::Case, 0);
    let first = child(&before, &inner_case, NodeKind::CaseBranch, 0);
    let second = child(&before, &inner_case, NodeKind::CaseBranch, 1);
    let first_application = child(&before, &first, NodeKind::Application, 0);

    let moved = apply_edit(
        &before,
        PrimitiveEdit::Move {
            node: first.clone(),
            new_parent: inner_case.clone(),
            anchor: Anchor::After(second),
        },
        &LibrarySpec::default(),
    )
    .unwrap();

    assert_eq!(
        moved.document.resolve_node(&inner_case).unwrap().node.id,
        inner_case.id
    );
    assert_eq!(
        moved.document.resolve_node(&first).unwrap().node.id,
        first.id
    );
    assert_eq!(
        moved
            .document
            .resolve_node(&first_application)
            .unwrap()
            .node
            .id,
        first_application.id
    );
    assert!(moved.document.source().matches("case:").count() >= 2);
    let (source, manifest) = moved.document.dump_pair().unwrap();
    assert_eq!(
        LanguageDocument::open(&source, &manifest).unwrap(),
        moved.document
    );
}

#[test]
fn detached_expression_item_kind_matches_identity_root_kind() {
    let application = SignApplication {
        callee: "wrap".to_owned(),
        arguments: Vec::new(),
        source: SourceLocation::line(1),
    };
    let item = DetachedNode::Item(SignItem::SignExpression(SignExpression {
        expression: Expression::SignApplication(application),
        source: SourceLocation::line(1),
    }));

    assert_eq!(item.kind(), NodeKind::Application);
}
