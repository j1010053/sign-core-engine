use conlang_changeset::function::load_functions_from_resolved;
use conlang_changeset::{
    apply_edit, apply_edit_with_packages, change_set_prelude_with_packages, ChangeInterpreter,
    NodeUpdate, PrimitiveEdit, ReplayError, UnresolvedChangeSet,
};
use conlang_language::{
    LanguageDocument, LibraryCatalog, LibrarySpec, PackageFile, PackageId, PackageRequirement,
    PackageResolver, PackageSource, PackageSources, PackageSpec,
};

fn package(function_delta: &str) -> PackageSources {
    let function = format!(
        "package dataset:demo:\n    schema = conlang.functions/v1\n\n\
         function Bump(x):\n    entrench(x, delta: {function_delta})\n"
    );
    PackageSources {
        config: r#"schema = 2
id = "dataset:demo"
version = "1.0.0"
layer = "data"
capabilities = ["functions", "data"]
exports = "config/exports.tsv"
requires = []
functions = ["code/functions.chg"]
data = ["data/weights.tsv"]
"#
        .to_owned(),
        exports: "stable_id\tkind\talias\ndataset:demo:bump\tfunction\tBump\n".to_owned(),
        functions: vec![PackageFile {
            path: "code/functions.chg".to_owned(),
            source: function,
        }],
        data: "goal\trecipe\tweight\n".to_owned(),
        data_files: vec![PackageFile {
            path: "data/weights.tsv".to_owned(),
            source: "goal\trecipe\tweight\n".to_owned(),
        }],
        source: PackageSource::Injected("test".to_owned()),
        ..PackageSources::default()
    }
}

#[test]
fn primitive_edit_validation_uses_the_resolved_reference_package() {
    let marker = PackageSources {
        config: r#"schema = 2
id = "catalog:edit-marker"
version = "1.0.0"
layer = "reference"
capabilities = ["traits"]
code = ["code/marker.lang"]
"#
        .to_owned(),
        exports: "stable_id\tkind\talias\ncatalog:edit-marker:Marker\ttrait\tExternalEditMarker\n"
            .to_owned(),
        code: "trait ExternalEditMarker:\n".to_owned(),
        source: PackageSource::Injected("edit-test".to_owned()),
        ..PackageSources::default()
    };
    let catalog = LibraryCatalog::with_packages([marker]).expect("catalog");
    let packages = catalog
        .resolve(&PackageSpec::default().with_root(PackageRequirement::exact(
            PackageId::new("catalog", "edit-marker"),
            "1.0.0",
        )))
        .expect("resolve");
    let document = LanguageDocument::import_new_root(
        "sign probe:\n    belongs ExternalEditMarker\n",
        "edit:root",
    )
    .expect("document");
    let node = document.ref_for_sign("probe").expect("probe identity");
    let edit = PrimitiveEdit::Update {
        node,
        change: NodeUpdate::Rename("renamed".to_owned()),
    };

    assert!(apply_edit(&document, edit.clone(), &LibrarySpec::default()).is_err());
    let outcome = apply_edit_with_packages(&document, edit, &packages)
        .expect("resolved package trait participates in edit validation");
    assert!(outcome.document.language().sign_named("renamed").is_some());
}

fn spec() -> PackageSpec {
    PackageSpec::default().with_root(PackageRequirement::exact(
        PackageId::new("dataset", "demo"),
        "1.0.0",
    ))
}

#[test]
fn external_functions_locks_and_replay_share_one_resolved_snapshot() {
    let catalog = LibraryCatalog::with_packages([package("0.1")]).expect("catalog");
    let packages = catalog.resolve(&spec()).expect("resolve");
    let table = load_functions_from_resolved(&packages).expect("function table");
    assert!(table.get("Bump").is_ok(), "injected function is selected");

    let base = LanguageDocument::import_new_root("", "evo:root").expect("base");
    let prelude = change_set_prelude_with_packages(&base, &packages, "evo:next").expect("prelude");
    assert!(
        prelude.contains("library dataset:demo@1.0.0 sha256:"),
        "{prelude}"
    );
    let unresolved = UnresolvedChangeSet::parse(&prelude).expect("parse");
    let resolved = unresolved
        .resolve_packages(&base, &packages)
        .expect("same package snapshot resolves");
    let replay = ChangeInterpreter::with_packages(base.clone(), packages.clone(), "evo:next")
        .expect("interpreter")
        .run(&resolved)
        .expect("replay");
    assert_eq!(replay.document.language().dump(), base.language().dump());

    let changed_catalog = LibraryCatalog::with_packages([package("0.2")]).expect("changed catalog");
    let changed = changed_catalog.resolve(&spec()).expect("changed resolve");
    assert_ne!(packages.lock_digest(), changed.lock_digest());
    assert!(matches!(
        unresolved.resolve_packages(&base, &changed),
        Err(ReplayError::LibraryLockMismatch(_))
    ));
}

#[test]
fn v1_library_lock_order_remains_std_natural_plugin() {
    let source = r#"changeset evo:next:
    schema = conlang.changeset/v1
    base_source = sha256:source
    base_identities = sha256:identities
    library plugin:demo@1 sha256:plugin
    library natural:demo@1 sha256:natural
    library std:demo@1 sha256:std
"#;

    let parsed = UnresolvedChangeSet::parse(source).expect("legacy lock prelude parses");
    let ids = parsed
        .libraries
        .iter()
        .map(|lock| lock.package.to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["std:demo", "natural:demo", "plugin:demo"]);
}
