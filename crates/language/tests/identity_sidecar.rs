use conlang_language::{
    check_document, compile_system_ref, AddressSegment, IdentityError, IdentityNamespace, Language,
    LanguageDocument, LibrarySpec, NodeKind, RefTargetV1, IDENTITY_SCHEMA_V1, IDENTITY_SCHEMA_V2,
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

const V2_EXPRESSION_SOURCE: &str = r#"schema conlang.lang/v2

trait Entity:

trait Marked:

sign atom:
    belongs Entity
    phon:
        /a/

sign Wrap:
    syn:
        slots:
            value [*]
    phon:
        /{value}/

sign Outer:
    syn:
        slots:
            value [*]
    phon:
        /<{value}>/

sign root:
    belongs Entity
    syn:
        slots:
            actor [Entity]
        feature:
            number = enum(singular, plural)
            number =>
                case:
                    else:
                        singular
    sem:
        roles:
            actor [Entity]
            actor =
                case:
                    else:
                        atom()
    phon:
        /{actor}/
        realization:
            case:
                else:
                    /{Outer(value = Wrap(value = {$self})).phon.ret}/
    case:
        else:
            Outer(value = Wrap(value = {$self}))
            belongs Marked
"#;

#[test]
fn v2_expression_nodes_have_recursive_unique_stable_addresses() {
    let document =
        LanguageDocument::import_new_root(V2_EXPRESSION_SOURCE, "evo:expressions").unwrap();
    let nodes = &document.identities().nodes;

    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Case)
            .count(),
        4
    );
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.kind == NodeKind::CaseBranch)
            .count(),
        4
    );
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Application)
            .count(),
        5
    );
    assert_eq!(
        nodes
            .iter()
            .filter(|node| {
                node.address
                    .0
                    .iter()
                    .any(|segment| matches!(segment, AddressSegment::ApplicationArguments(0)))
            })
            .count(),
        2
    );

    let mut addresses = nodes
        .iter()
        .map(|node| node.address.clone())
        .collect::<Vec<_>>();
    addresses.sort();
    assert!(addresses.windows(2).all(|pair| pair[0] != pair[1]));

    let application_refs = document
        .identities()
        .refs
        .iter()
        .filter(|binding| binding.field == "application.callee")
        .count();
    assert_eq!(application_refs, 5);
    assert!(document
        .identities()
        .refs
        .iter()
        .any(|binding| binding.field == "case.belongs[0]"));

    let (source, manifest) = document.dump_pair().unwrap();
    assert_eq!(
        LanguageDocument::open(&source, &manifest).unwrap(),
        document
    );
}

#[test]
fn anonymous_sign_context_fragment_items_keep_stable_identity_and_refs() {
    let source = r#"schema conlang.lang/v2

trait FragmentMark:

sign root:
    phon:
        /r/
    case:
        else:
            belongs FragmentMark
            sem:
                selected = yes
"#;
    let document = LanguageDocument::import_new_root(source, "evo:fragment").unwrap();
    let branch = document
        .identities()
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::CaseBranch)
        .unwrap();
    let fragment_items = document
        .identities()
        .nodes
        .iter()
        .filter(|node| {
            node.parent.as_ref() == Some(&branch.id)
                && matches!(node.kind, NodeKind::Belongs | NodeKind::Definition)
        })
        .collect::<Vec<_>>();
    assert_eq!(fragment_items.len(), 2);
    assert!(fragment_items.iter().all(|node| {
        node.address.0.windows(2).any(|pair| {
            matches!(
                pair,
                [AddressSegment::CaseBranches(_), AddressSegment::Items(_)]
            )
        })
    }));
    assert!(document.identities().refs.iter().any(|binding| {
        binding.owner == fragment_items[0].id
            && binding.field == "belongs"
            && matches!(
                &binding.target,
                RefTargetV1::Local { target } if target.expected == NodeKind::Trait
            )
    }));

    let (canonical, manifest) = document.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&canonical, &manifest).unwrap();
    assert_eq!(reopened, document);
}

#[test]
fn dimension_when_fragment_items_round_trip_with_stable_identity() {
    let source = r#"schema conlang.lang/v2

sign root:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = on
            selected = enum(no, yes)
            selected = no
        when:
            $self.syn.trigger == on:
                feature:
                    selected = yes
    phon:
        /r/
"#;
    let document = LanguageDocument::import_new_root(source, "evo:dim-fragment").unwrap();
    let branch = document
        .identities()
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::CaseBranch)
        .unwrap();
    let value = document
        .identities()
        .nodes
        .iter()
        .find(|node| {
            node.parent.as_ref() == Some(&branch.id) && node.kind == NodeKind::FeatureValue
        })
        .unwrap();
    assert!(value.address.0.windows(2).any(|pair| {
        matches!(
            pair,
            [AddressSegment::CaseBranches(_), AddressSegment::Items(_)]
        )
    }));
    let value_id = value.id.clone();
    let (canonical, manifest) = document.dump_pair().unwrap();
    let reopened = LanguageDocument::open(&canonical, &manifest).unwrap();
    assert!(reopened
        .identities()
        .nodes
        .iter()
        .any(|node| node.id == value_id && node.kind == NodeKind::FeatureValue));
}

#[test]
fn expression_refs_keep_target_ids_across_display_rename_and_reopen() {
    let document =
        LanguageDocument::import_new_root(V2_EXPRESSION_SOURCE, "evo:rename-expressions").unwrap();
    let wrap_id = document.ref_for_sign("Wrap").unwrap().id;
    let marked_id = document.ref_for_trait("Marked").unwrap().id;
    let source = document
        .source()
        .replace("trait Marked:", "trait Decorated:")
        .replace("belongs Marked", "belongs Decorated")
        .replace("sign Wrap:", "sign Wrapper:")
        .replace("Wrap(value", "Wrapper(value");
    let language = Language::parse(&source).unwrap();
    let (_, identities) = document.into_edit_parts();
    let renamed = LanguageDocument::from_edit_parts(language, identities).unwrap();

    assert_eq!(renamed.ref_for_sign("Wrapper").unwrap().id, wrap_id);
    assert_eq!(renamed.ref_for_trait("Decorated").unwrap().id, marked_id);
    assert!(renamed.identities().refs.iter().any(|binding| {
        binding.field == "application.callee"
            && matches!(
                &binding.target,
                RefTargetV1::Local { target } if target.id == wrap_id
            )
    }));
    assert!(renamed.identities().refs.iter().any(|binding| {
        binding.field == "case.belongs[0]"
            && matches!(
                &binding.target,
                RefTargetV1::Local { target } if target.id == marked_id
            )
    }));

    let (source, manifest) = renamed.dump_pair().unwrap();
    assert_eq!(LanguageDocument::open(&source, &manifest).unwrap(), renamed);
}
