use conlang_app::{CompileKey, CompileService};
use conlang_language::{
    LanguageDocument, LibraryCatalog, PackageFile, PackageId, PackageResolver, PackageSources,
    PackageSpec, ResolvedPackages,
};

const DOCUMENT: &str = "sign probe:\n    belongs AppInjectedMarker\n";

fn injected_package(code: &str) -> PackageSources {
    PackageSources {
        config: "kind = plugin\nname = app-probe\nversion = 0.1.0\n\
                 rule_namespace = plugin:app-probe\nenabled = true\npriority = 0\n\
                 requires =\ncode = code/main.lang\ndata = data/notes.tsv\n"
            .to_owned(),
        exports: "stable_id\tkind\talias\nplugin:app-probe:Marker\ttrait\tAppInjectedMarker\n"
            .to_owned(),
        code: code.to_owned(),
        data: "key\tvalue\n".to_owned(),
        data_files: vec![PackageFile {
            path: "data/notes.tsv".to_owned(),
            source: "key\tvalue\n".to_owned(),
        }],
        ..PackageSources::default()
    }
}

fn resolved(code: &str) -> ResolvedPackages {
    let catalog =
        LibraryCatalog::with_packages([injected_package(code)]).expect("valid injected package");
    catalog
        .resolve(&PackageSpec::default().with_root(PackageId::new("plugin", "app-probe")))
        .expect("resolve injected package")
}

fn resolved_with_unused_package(code: &str) -> ResolvedPackages {
    let unused = PackageSources {
        config: "schema = 2\nid = \"catalog:unused\"\nversion = \"1.0.0\"\n\
                 layer = \"reference\"\ncapabilities = [\"traits\"]\n\
                 code = [\"code/main.lang\"]\n"
            .to_owned(),
        exports:
            "stable_id\tkind\talias\ncatalog:unused:UnusedCatalogMarker\ttrait\tUnusedCatalogMarker\n"
                .to_owned(),
        code: "trait UnusedCatalogMarker:\n".to_owned(),
        ..PackageSources::default()
    };
    let catalog = LibraryCatalog::with_packages([injected_package(code), unused])
        .expect("catalog with unselected package");
    catalog
        .resolve(&PackageSpec::default().with_root(PackageId::new("plugin", "app-probe")))
        .expect("resolve the same selected package")
}

fn document() -> LanguageDocument {
    LanguageDocument::import_new_root(DOCUMENT, "app:package-test").expect("valid document")
}

#[test]
fn compile_key_uses_the_resolved_package_lock_directly() {
    let document = document();
    let packages = resolved("trait AppInjectedMarker:\n");

    let key = CompileKey::of_with_packages(&document, &packages).expect("cache key");

    assert_eq!(key.library_lock, packages.lock_digest());
}

#[test]
fn compile_service_consumes_injected_packages_without_rediscovery() {
    let document = document();
    let packages = resolved("trait AppInjectedMarker:\n");
    let mut service = CompileService::new();

    assert!(service
        .peek_with_packages(&document, &packages)
        .expect("peek")
        .is_none());

    let first = service
        .get_with_packages(&document, &packages)
        .expect("the injected trait must be available during compile");
    let second = service
        .get_with_packages(&document, &packages)
        .expect("cache hit");

    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(service.stats(), (1, 1));
    assert_eq!(service.len(), 1);
    assert!(service
        .peek_with_packages(&document, &packages)
        .expect("peek")
        .is_some());
    assert!(first
        .effective_language()
        .trait_named("AppInjectedMarker")
        .is_some());
}

#[test]
fn changed_resolved_package_bytes_do_not_reuse_a_stale_entry() {
    let document = document();
    let first_packages = resolved("trait AppInjectedMarker:\n");
    let changed_packages = resolved("trait AppInjectedMarker:\ntrait AdditionalMarker:\n");
    let mut service = CompileService::new();

    let first_key =
        CompileKey::of_with_packages(&document, &first_packages).expect("first cache key");
    let changed_key =
        CompileKey::of_with_packages(&document, &changed_packages).expect("changed cache key");
    assert_ne!(first_key.library_lock, changed_key.library_lock);

    service
        .get_with_packages(&document, &first_packages)
        .expect("first compile");
    service
        .get_with_packages(&document, &changed_packages)
        .expect("changed compile");

    assert_eq!(service.stats(), (0, 2));
    assert_eq!(service.len(), 2);
}

#[test]
fn different_available_export_indexes_do_not_share_a_cache_entry() {
    let document = document();
    let narrow = resolved("trait AppInjectedMarker:\n");
    let wider = resolved_with_unused_package("trait AppInjectedMarker:\n");
    let narrow_key = CompileKey::of_with_packages(&document, &narrow).expect("narrow key");
    let wider_key = CompileKey::of_with_packages(&document, &wider).expect("wider key");

    assert_eq!(narrow_key.library_lock, wider_key.library_lock);
    assert_ne!(narrow_key.available_exports, wider_key.available_exports);

    let mut service = CompileService::new();
    service
        .get_with_packages(&document, &narrow)
        .expect("narrow compile");
    service
        .get_with_packages(&document, &wider)
        .expect("wider compile");
    assert_eq!(service.stats(), (0, 2));
    assert_eq!(service.len(), 2);
}
