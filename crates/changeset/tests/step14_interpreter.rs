use conlang_changeset::{
    change_set_prelude, Anchor, ChangeInterpreter, ChangeSession, DetachedNode, PrimitiveEdit,
    ReplayError, ResolvedStatement, UnresolvedChangeSet,
};
use conlang_language::{IdentityNamespace, LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol d
Symbol o
Symbol g
Symbol c
Symbol a
Symbol t
Class vowel {o, a}

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
fn authoring_selectors_resolve_to_stable_ids_and_replay_is_deterministic() {
    let base = base();
    let dog_id = base.ref_for_sign("dog").unwrap().id;
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:child").unwrap();
    source.push_str(
        r#"
    statement 0:
        update sign("dog").name = hound

    statement 1:
        insert sign under language at end:
            sign cat:
                belongs LocalNoun
                phon:
                    /cat/
"#,
    );

    let unresolved = UnresolvedChangeSet::parse(&source).unwrap();
    let resolved = unresolved.resolve(&base, &spec).unwrap();
    let canonical = resolved.dump().expect("dump");
    assert!(canonical.contains("update node(sign, @evo:root:"));
    assert!(!canonical.contains("sign(\"dog\")"));

    let canonical_resolved = UnresolvedChangeSet::parse(&canonical)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(canonical_resolved.dump().expect("dump"), canonical);

    let interpreter = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:child").unwrap();
    let first = interpreter.run(&resolved).unwrap();
    let second = interpreter.run(&resolved).unwrap();
    assert_eq!(
        first.document.dump_pair().unwrap(),
        second.document.dump_pair().unwrap()
    );
    assert_eq!(first.records, second.records);
    assert_eq!(first.document.ref_for_sign("hound").unwrap().id, dog_id);
    assert_eq!(
        first.document.ref_for_sign("cat").unwrap().id.namespace,
        IdentityNamespace::Document("evo:child".to_owned())
    );
    assert_eq!(first.document.identities().allocators.len(), 2);
}

#[test]
fn lazy_compile_reuses_cache_and_successful_commit_invalidates_it() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:lazy").unwrap();
    source.push_str(
        r#"
    statement 0:
        update sign("dog").name = hound
"#,
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let interpreter = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:lazy").unwrap();
    let replay = interpreter.run(&resolved).unwrap();

    let mut session = ChangeSession::new(base, spec);
    assert!(session.is_dirty());
    session.compiled_system().unwrap();
    assert_eq!(session.compile_count(), 1);
    session.compiled_system().unwrap();
    assert_eq!(session.compile_count(), 1);

    session.commit(replay.document);
    assert!(session.is_dirty());
    assert_eq!(session.compile_count(), 1);
    let compiled = session.compiled_system().unwrap();
    assert!(compiled.language().sign_named("hound").is_some());
    assert_eq!(session.compile_count(), 2);
}

#[test]
fn digest_mismatch_is_rejected_before_any_statement() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:bad").unwrap();
    source = source.replace("base_source = sha256:", "base_source = sha256:00");
    source.push_str(
        r#"
    statement 0:
        update sign("dog").name = hound
"#,
    );
    let unresolved = UnresolvedChangeSet::parse(&source).unwrap();
    assert!(unresolved.resolve(&base, &spec).is_err());
    assert!(base.ref_for_sign("dog").is_some());
}

/// 相容性:identity-manifest digest 不符 → replay 前拒絕(第二道 digest,補 base_source 之外)。
#[test]
fn identity_manifest_digest_mismatch_is_rejected_before_any_statement() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:badid").unwrap();
    source = source.replace("base_identities = sha256:", "base_identities = sha256:00");
    source.push_str(
        r#"
    statement 0:
        update sign("dog").name = hound
"#,
    );
    let unresolved = UnresolvedChangeSet::parse(&source).unwrap();
    assert!(matches!(
        unresolved.resolve(&base, &spec),
        Err(ReplayError::BaseIdentitiesMismatch)
    ));
    assert!(base.ref_for_sign("dog").is_some(), "base 未被污染");
}

/// 相容性:library lock 不符(注入 spec 沒有的 lock)→ replay 前拒絕(第三道 digest)。
#[test]
fn library_lock_mismatch_is_rejected_before_any_statement() {
    let base = base();
    let spec = LibrarySpec::default(); // 無 library → package_locks 為空
    let mut source = change_set_prelude(&base, &spec, "evo:badlock").unwrap();
    source.push_str("    library std:ghost@0.0 sha256:0000\n"); // 憑空 lock,實際 spec 沒有
    source.push_str(
        r#"
    statement 0:
        update sign("dog").name = hound
"#,
    );
    let unresolved = UnresolvedChangeSet::parse(&source).unwrap();
    assert!(matches!(
        unresolved.resolve(&base, &spec),
        Err(ReplayError::LibraryLockMismatch(_))
    ));
    assert!(base.ref_for_sign("dog").is_some(), "base 未被污染");
}

#[test]
fn path_selector_resolves_definition_to_a_stable_node() {
    let base = base();
    let spec = LibrarySpec::default();
    let dog_id = base.ref_for_sign("dog").unwrap().id;
    let mut source = change_set_prelude(&base, &spec, "evo:path").unwrap();
    source.push_str(
        r#"
    statement 0:
        update sign("dog").def[phon].value = /cat/
"#,
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert!(resolved
        .dump()
        .expect("dump")
        .contains("update node(definition, @evo:root:"));
    let replay = ChangeInterpreter::new(base, spec, "evo:path")
        .unwrap()
        .run(&resolved)
        .unwrap();
    assert_eq!(replay.document.ref_for_sign("dog").unwrap().id, dog_id);
    assert!(replay.document.source().contains("/cat/"));
}

#[test]
fn failed_statement_rolls_back_document_allocator_and_compile_cache() {
    let base = base();
    let spec = LibrarySpec::default();
    let interpreter = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:rollback").unwrap();
    let mut session = ChangeSession::new(base.fork("evo:rollback").unwrap(), spec);
    session.compiled_system().unwrap();
    let before = session.document().dump_pair().unwrap();
    let count = session.compile_count();
    let invalid = ResolvedStatement {
        ordinal: 7,
        edits: vec![PrimitiveEdit::Delete {
            node: session.document().root_ref(),
        }],
    };

    assert!(session.apply_statement(&interpreter, &invalid).is_err());
    assert_eq!(session.document().dump_pair().unwrap(), before);
    assert!(!session.is_dirty());
    session.compiled_system().unwrap();
    assert_eq!(session.compile_count(), count);
}

#[test]
fn statement_may_temporarily_dangle_and_only_validates_its_final_state() {
    let base = base();
    let spec = LibrarySpec::default();
    let interpreter = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:atomic").unwrap();
    let mut session = ChangeSession::new(base.fork("evo:atomic").unwrap(), spec);
    let old_trait = session.document().ref_for_trait("LocalNoun").unwrap();
    let replacement = session.document().language().traits[0].clone();
    let statement = ResolvedStatement {
        ordinal: 0,
        edits: vec![
            PrimitiveEdit::Delete {
                node: old_trait.clone(),
            },
            PrimitiveEdit::Insert {
                parent: session.document().root_ref(),
                anchor: Anchor::End,
                subtree: DetachedNode::Trait(replacement),
            },
        ],
    };

    session.apply_statement(&interpreter, &statement).unwrap();
    let new_trait = session.document().ref_for_trait("LocalNoun").unwrap();
    assert_ne!(new_trait.id, old_trait.id);
    assert!(session.compiled_system().is_ok());
}

#[test]
fn later_failed_statement_keeps_an_earlier_commit() {
    let base = base();
    let spec = LibrarySpec::default();
    let interpreter = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:partial").unwrap();
    let mut session = ChangeSession::new(base.fork("evo:partial").unwrap(), spec);
    let dog = session.document().ref_for_sign("dog").unwrap();
    let first = ResolvedStatement {
        ordinal: 0,
        edits: vec![PrimitiveEdit::Update {
            node: dog,
            change: conlang_changeset::NodeUpdate::Rename("hound".to_owned()),
        }],
    };
    session.apply_statement(&interpreter, &first).unwrap();
    let committed = session.document().dump_pair().unwrap();
    let second = ResolvedStatement {
        ordinal: 1,
        edits: vec![PrimitiveEdit::Delete {
            node: session.document().root_ref(),
        }],
    };

    assert!(session.apply_statement(&interpreter, &second).is_err());
    assert_eq!(session.document().dump_pair().unwrap(), committed);
    assert!(session.document().ref_for_sign("hound").is_some());
}
