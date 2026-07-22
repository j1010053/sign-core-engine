use conlang_language::{
    check_document, compile_system_ref, IdentityError, IdentityNamespace, LanguageDocument,
    LibrarySpec, NodeKind, IDENTITY_SCHEMA_V1, IDENTITY_SCHEMA_V2,
};

const SOURCE: &str = r#"
trait Canine:

sign dog:
    belongs Canine
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

#[test]
fn source_sidecar_round_trip_is_deterministic_and_binds_runtime_ids() {
    let first = LanguageDocument::import_new_root(SOURCE, "evo:proto").unwrap();
    let (source, manifest) = first.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&source, &manifest).unwrap();

    assert_eq!(first, reopened);
    assert_eq!(manifest, reopened.manifest_json().unwrap());
    assert_eq!(
        reopened.ref_for_sign("dog").unwrap().id.namespace,
        IdentityNamespace::Document("evo:proto".to_owned())
    );
    assert!(!reopened
        .identities()
        .nodes
        .iter()
        .any(|node| node.kind == NodeKind::RealizationBranch));

    let report = check_document(&reopened, &LibrarySpec::default());
    assert!(!report.has_errors(), "{:?}", report.diagnostics());

    let compiled = compile_system_ref(reopened.language()).unwrap();
    assert_eq!(
        compiled.language().sign_named("dog").unwrap().id,
        compiled.effective_language().sign_named("dog").unwrap().id
    );
}

#[test]
fn typed_resolver_rejects_fields_that_do_not_belong_to_node_kind() {
    let document = LanguageDocument::import_new_root(SOURCE, "evo:resolver").unwrap();
    let sign = document.ref_for_sign("dog").unwrap();
    let name = conlang_language::path::parse_path("name").unwrap();
    let guard = conlang_language::path::parse_path("guard").unwrap();

    assert_eq!(
        document.resolve_path(&sign, &name).unwrap().field,
        Some(conlang_language::EditableField::Name)
    );
    assert!(matches!(
        document.resolve_path(&sign, &guard),
        Err(IdentityError::Resolve(_))
    ));
}

#[test]
fn changed_source_never_recovers_identity_by_guessing() {
    let document = LanguageDocument::import_new_root(SOURCE, "evo:proto").unwrap();
    let (source, manifest) = document.dump_pair().unwrap();
    let changed = source.replace("sign dog:", "sign hound:");

    assert_eq!(
        LanguageDocument::open(&changed, &manifest).unwrap_err(),
        IdentityError::SourceMismatch
    );
}

#[test]
fn invalid_namespace_is_rejected_instead_of_using_random_identity() {
    assert_eq!(
        LanguageDocument::import_new_root(SOURCE, "bad namespace").unwrap_err(),
        IdentityError::InvalidNamespace
    );
}

#[test]
fn v1_is_read_and_canonically_upgraded_to_v2() {
    let document = LanguageDocument::import_new_root(SOURCE, "evo:legacy").unwrap();
    let source = document.source();
    let current = document.identities();
    let legacy = serde_json::json!({
        "schema": IDENTITY_SCHEMA_V1,
        "namespace": current.root_namespace,
        "next_ordinal": current.allocators[0].next_ordinal,
        "source_sha256": current.source_sha256,
        "nodes": current.nodes,
        "refs": current.refs,
    });
    let reopened =
        LanguageDocument::open(&source, &serde_json::to_string(&legacy).unwrap()).unwrap();
    assert_eq!(reopened.identities().schema, IDENTITY_SCHEMA_V2);
    assert_eq!(reopened.identities().root_namespace, "evo:legacy");
    assert_eq!(reopened.identities().active_namespace, "evo:legacy");
    assert_eq!(reopened.identities().allocators.len(), 1);
}

#[test]
fn fork_preserves_ancestor_ids_and_adds_an_active_allocator() {
    let root = LanguageDocument::import_new_root(SOURCE, "evo:root").unwrap();
    let dog = root.ref_for_sign("dog").unwrap();
    let child = root.fork("evo:child").unwrap();

    assert_eq!(child.ref_for_sign("dog").unwrap(), dog);
    assert_eq!(child.identities().root_namespace, "evo:root");
    assert_eq!(child.identities().active_namespace, "evo:child");
    assert!(child.owns(&dog.id));
    assert_eq!(child.identities().allocators.len(), 2);
    assert!(child.fork("evo:child").is_err());
}
