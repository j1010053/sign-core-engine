use conlang_language::{
    check_language_with_packages, compile_with_packages_ref, Language, LibraryCatalog, LibraryKind,
    LibraryLoadError, LibrarySpec, PackageFile, PackageId, PackageLayer, PackageRequirement,
    PackageResolver, PackageSource, PackageSources, PackageSpec, Severity,
};

fn reference_package(id: &str, version: &str, alias: &str) -> PackageSources {
    PackageSources {
        config: format!(
            "schema = 2\n\
             id = \"{id}\"\n\
             version = \"{version}\"\n\
             layer = \"reference\"\n\
             capabilities = [\"traits\"]\n\
             code = [\"code/main.lang\"]\n"
        ),
        exports: format!("stable_id\tkind\talias\n{id}:{alias}\ttrait\t{alias}\n"),
        code: format!("trait {alias}:\n"),
        ..PackageSources::default()
    }
}

fn code_package_with_manifest(config: String, id: &str, alias: &str) -> PackageSources {
    PackageSources {
        config,
        exports: format!("stable_id\tkind\talias\n{id}:{alias}\ttrait\t{alias}\n"),
        code: format!("trait {alias}:\n"),
        ..PackageSources::default()
    }
}

#[test]
fn arbitrary_namespace_ids_and_exact_requirements_round_trip() {
    let text = "heritage-catalog:traditional_case";
    let id: PackageId = text.parse().expect("open package namespace parses");

    assert_eq!(id.namespace, "heritage-catalog");
    assert_eq!(id.name, "traditional_case");
    assert_eq!(id.to_string(), text);

    let requirement_text = format!("{text}@2026.08");
    let requirement: PackageRequirement = requirement_text
        .parse()
        .expect("exact package requirement parses");
    assert_eq!(requirement.id, id);
    assert_eq!(requirement.version.as_deref(), Some("2026.08"));
    assert_eq!(requirement.to_string(), requirement_text);
}

#[test]
fn exact_versions_reject_empty_or_whitespace_values() {
    assert!("catalog:case@".parse::<PackageRequirement>().is_err());
    assert!("catalog:case@1.0 beta"
        .parse::<PackageRequirement>()
        .is_err());

    let sources = reference_package("catalog:blank-version", " ", "BlankVersionTrait");
    let error = LibraryCatalog::with_packages([sources])
        .expect_err("schema-2 manifests require a usable exact version");
    assert!(matches!(
        error,
        LibraryLoadError::Config { message, .. }
            if message.contains("non-empty exact version")
    ));
}

#[test]
fn package_spec_default_has_no_implicit_roots() {
    let spec = PackageSpec::default();

    assert!(spec.roots.is_empty());
    assert!(spec.aliases.is_empty());
}

#[test]
fn v2_reference_package_selects_at_an_exact_version() {
    let id: PackageId = "catalog:traditional-categories".parse().unwrap();
    let catalog = LibraryCatalog::with_packages([reference_package(
        "catalog:traditional-categories",
        "1.4.2",
        "TraditionalNominal",
    )])
    .expect("v2 reference package loads");
    let spec = PackageSpec::default().with_root(PackageRequirement::exact(id.clone(), "1.4.2"));

    let resolved = catalog.resolve(&spec).expect("exact version resolves");
    let package = resolved.package(&id).expect("selected package is retained");

    assert_eq!(package.manifest_schema, 2);
    assert_eq!(package.layer, PackageLayer::Reference);
    assert_eq!(package.version, "1.4.2");
    assert!(resolved
        .selection
        .standard
        .trait_named("TraditionalNominal")
        .is_some());
    assert!(resolved.selection.overlay.traits.is_empty());
    assert_eq!(resolved.selection.resolved.len(), 1);
    assert_eq!(resolved.selection.resolved[0].version, "1.4.2");
}

#[test]
fn v2_data_only_package_accepts_zero_exports() {
    let id: PackageId = "dataset:grammatical-observations".parse().unwrap();
    let sources = PackageSources {
        config: r#"schema = 2
id = "dataset:grammatical-observations"
version = "1.0.0"
layer = "data"
capabilities = ["data"]
data = ["data/observations.tsv"]
"#
        .to_owned(),
        exports: String::new(),
        data: "language\ttrait\neng\tSVO\n".to_owned(),
        data_files: vec![PackageFile {
            path: "data/observations.tsv".to_owned(),
            source: "language\ttrait\neng\tSVO\n".to_owned(),
        }],
        ..PackageSources::default()
    };
    let catalog = LibraryCatalog::with_packages([sources]).expect("data-only package loads");

    let resolved = catalog
        .resolve(&PackageSpec::default().with_root(id.clone()))
        .expect("data-only package resolves");
    let package = resolved.package(&id).unwrap();

    assert_eq!(package.layer, PackageLayer::Data);
    assert!(package.capabilities.data);
    assert!(package.exports.is_empty());
    assert_eq!(package.data_paths, ["data/observations.tsv"]);
    assert!(resolved.selection.standard.traits.is_empty());
    assert!(resolved.selection.overlay.traits.is_empty());
}

#[test]
fn v2_manifest_rejects_content_outside_its_capabilities() {
    let id = "catalog:wrong-capability";
    let sources = code_package_with_manifest(
        format!(
            "schema = 2\n\
             id = \"{id}\"\n\
             version = \"1.0.0\"\n\
             layer = \"reference\"\n\
             capabilities = [\"data\"]\n\
             code = [\"code/main.lang\"]\n"
        ),
        id,
        "CapabilityLeak",
    );
    let error = LibraryCatalog::with_packages([sources])
        .expect_err("trait code without the traits capability must fail");

    assert!(matches!(
        &error,
        LibraryLoadError::UnsupportedContent { package, message }
            if package == id && message.contains("traits capability")
    ));
    assert_eq!(error.code(), "LIBRARY_CONTENT_UNSUPPORTED");
}

#[test]
fn resolver_rejects_an_exact_version_mismatch() {
    let id: PackageId = "catalog:versioned-categories".parse().unwrap();
    let catalog = LibraryCatalog::with_packages([reference_package(
        "catalog:versioned-categories",
        "2.0.0",
        "VersionedCategory",
    )])
    .unwrap();
    let spec = PackageSpec::default().with_root(PackageRequirement::exact(id.clone(), "1.0.0"));

    let error = catalog
        .resolve(&spec)
        .expect_err("the resolver must honor the exact requested version");

    assert!(matches!(
        &error,
        LibraryLoadError::VersionMismatch {
            package,
            expected,
            actual,
        } if package == &id && expected == "1.0.0" && actual == "2.0.0"
    ));
    assert_eq!(error.code(), "PACKAGE_VERSION_MISMATCH");
}

#[test]
fn v2_manifest_rejects_unsafe_and_normalized_duplicate_paths() {
    let unsafe_id = "catalog:unsafe-path";
    let unsafe_sources = code_package_with_manifest(
        format!(
            "schema = 2\n\
             id = \"{unsafe_id}\"\n\
             version = \"1.0.0\"\n\
             layer = \"reference\"\n\
             capabilities = [\"traits\"]\n\
             code = [\"../escape.lang\"]\n"
        ),
        unsafe_id,
        "UnsafePathTrait",
    );
    let unsafe_error = LibraryCatalog::with_packages([unsafe_sources])
        .expect_err("parent traversal must be rejected");
    assert!(matches!(
        &unsafe_error,
        LibraryLoadError::Config { message, .. }
            if message.contains("unsafe relative path") && message.contains("../escape.lang")
    ));

    let duplicate_id = "catalog:duplicate-path";
    let duplicate_sources = code_package_with_manifest(
        format!(
            r#"schema = 2
id = "{duplicate_id}"
version = "1.0.0"
layer = "reference"
capabilities = ["traits"]
code = ["code/main.lang", 'code\main.lang']
"#
        ),
        duplicate_id,
        "DuplicatePathTrait",
    );
    let duplicate_error = LibraryCatalog::with_packages([duplicate_sources])
        .expect_err("separator-normalized duplicate paths must be rejected");
    assert!(matches!(
        &duplicate_error,
        LibraryLoadError::Config { message, .. }
            if message.contains("duplicate path") && message.contains("code/main.lang")
    ));
}

#[test]
fn public_compile_and_check_consume_the_same_injected_resolved_packages() {
    let id: PackageId = "catalog:injected-reference".parse().unwrap();
    let catalog = LibraryCatalog::with_packages([reference_package(
        "catalog:injected-reference",
        "1.0.0",
        "InjectedReferenceTrait",
    )])
    .unwrap();
    let selected = catalog
        .resolve(&PackageSpec::default().with_root(PackageRequirement::exact(id.clone(), "1.0.0")))
        .unwrap();
    let unselected = catalog.resolve(&PackageSpec::default()).unwrap();
    let language = Language::parse("sign specimen:\n    belongs InjectedReferenceTrait\n").unwrap();

    let without_package = check_language_with_packages(&language, &unselected);
    assert!(without_package.diagnostics().iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message.contains("InjectedReferenceTrait")
    }));

    let report = check_language_with_packages(&language, &selected);
    assert!(
        !report.has_errors(),
        "injected trait must be visible to public check: {:?}",
        report.diagnostics()
    );

    let system = compile_with_packages_ref(&language, &selected)
        .expect("the same resolved package snapshot must compile");
    assert_eq!(system.libraries(), [id]);
    assert!(system.effective_language().sign_named("specimen").is_some());
}

#[test]
fn vendored_sources_override_installed_candidates_deterministically() {
    let id: PackageId = "catalog:precedence".parse().unwrap();
    let mut installed = reference_package("catalog:precedence", "1.0.0", "InstalledTrait");
    installed.source = PackageSource::Installed("catalog-precedence-v1".to_owned());
    let mut vendored = reference_package("catalog:precedence", "2.0.0", "VendoredTrait");
    vendored.source = PackageSource::Vendored("packages/catalog-precedence".to_owned());

    let catalog = LibraryCatalog::with_source_precedence([vendored], [installed])
        .expect("higher source tier replaces the same package ID");
    let resolved = catalog
        .resolve(&PackageSpec::default().with_root(PackageRequirement::exact(id.clone(), "2.0.0")))
        .expect("vendored exact version resolves");
    let selected = resolved.package(&id).unwrap();

    assert_eq!(selected.version, "2.0.0");
    assert!(matches!(
        &selected.source,
        PackageSource::Vendored(path) if path == "packages/catalog-precedence"
    ));
    assert!(resolved
        .selection
        .standard
        .trait_named("VendoredTrait")
        .is_some());
    assert!(resolved
        .selection
        .standard
        .trait_named("InstalledTrait")
        .is_none());
}

#[test]
fn legacy_resolve_keeps_library_spec_kind_validation() {
    let catalog = LibraryCatalog::embedded().expect("embedded catalog");
    let spec = LibrarySpec {
        std: vec![PackageId::new("catalog", "case")],
        natural: None,
        plugins: Vec::new(),
    };

    let error = catalog
        .resolve_legacy(&spec)
        .expect_err("legacy std roots must still use the std namespace");
    assert!(matches!(
        error,
        LibraryLoadError::WrongKind {
            expected: LibraryKind::Std,
            actual,
            ..
        } if actual == "catalog"
    ));
}
