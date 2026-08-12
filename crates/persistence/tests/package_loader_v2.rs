use conlang_language::{
    LibrarySpec, PackageId, PackageLayer, PackageRequirement, PackageSource, PackageSpec,
};
use conlang_persistence::{
    GraphStore, LockedPackage, PackagesLock, ProjectDocument, StoreError, PACKAGES_LOCK_SCHEMA,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let ordinal = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-persistence-loader-v2-{name}-{}-{ordinal}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn write(path: impl AsRef<Path>, source: &str) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn write_reference_package(store: &GraphStore) {
    let package = store.root().join("packages/catalog/persist-test");
    write(
        package.join("package.toml"),
        r#"schema = 2
id = "catalog:persist-test"
version = "1.2.3"
layer = "reference"
capabilities = ["traits", "functions", "data"]
exports = "config/public.tsv"
code = ["code/main.lang", "code/extra.lang"]
functions = ["code/recipes.chg"]
data = ["data/categories.tsv"]
"#,
    );
    write(
        package.join("config/public.tsv"),
        "stable_id\tkind\talias\ncatalog:persist-test:PersistCaseUnique\ttrait\tPersistCaseUnique\n",
    );
    write(package.join("code/main.lang"), "trait PersistCaseUnique:\n");
    write(
        package.join("code/extra.lang"),
        "trait PersistExtraUnique:\n",
    );
    write(
        package.join("code/recipes.chg"),
        "function persist_test_recipe():\n",
    );
    write(
        package.join("data/categories.tsv"),
        "id\tlabel\ncase\tCase\n",
    );
    // 表型宣告(P29):消費者依穩定 ID 取表,故 loader 也得把 config/tables.tsv
    // 讀進來——否則外部套件永遠只有「宣告不到表型」一種狀態。
    write(
        package.join("config/tables.tsv"),
        "path\ttype\ndata/categories.tsv\tcatalog:persist-test:CategoryTable\n",
    );
}

fn write_data_only_package_without_exports(store: &GraphStore) {
    let package = store.root().join("packages/dataset/observations");
    write(
        package.join("package.toml"),
        r#"schema = 2
id = "dataset:observations"
version = "2026.08"
layer = "data"
capabilities = ["data"]
data = ["data/observations.tsv"]
"#,
    );
    write(
        package.join("data/observations.tsv"),
        "language\tfeature\neng\tSVO\n",
    );
}

#[test]
fn project_v2_round_trips_and_legacy_projects_still_migrate() {
    let temp = TestDirectory::new("project");
    let store = GraphStore::init(&temp.0).unwrap();
    let spec = PackageSpec {
        roots: vec![
            PackageRequirement::exact(
                "catalog:persist-test".parse::<PackageId>().unwrap(),
                "1.2.3",
            ),
            "dataset:observations".parse::<PackageId>().unwrap().into(),
        ],
        aliases: BTreeMap::from([("trad".to_owned(), "catalog:persist-test".parse().unwrap())]),
    };
    let project = ProjectDocument::from_package_spec(&spec);
    store.write_project(&project).unwrap();

    let reopened = store.read_project().unwrap().unwrap();
    assert_eq!(reopened.to_package_spec().unwrap(), spec);
    assert_eq!(store.package_spec_or(PackageSpec::default()).unwrap(), spec);
    assert!(matches!(
        reopened.to_spec(),
        Err(StoreError::ProjectFormat(_))
    ));

    let legacy = LibrarySpec {
        std: vec!["std:grambank".parse().unwrap()],
        natural: Some("natural:en-standard".parse().unwrap()),
        plugins: vec!["plugin:legacy".parse().unwrap()],
    };
    let legacy_project = ProjectDocument::from_spec(&legacy);
    store.write_project(&legacy_project).unwrap();
    let reopened = store.read_project().unwrap().unwrap();
    assert_eq!(reopened.to_spec().unwrap(), legacy);
    assert_eq!(
        reopened.to_package_spec().unwrap(),
        PackageSpec::from_legacy(&legacy)
    );
}

#[test]
fn mixed_legacy_and_v2_project_intent_is_a_read_time_error() {
    let temp = TestDirectory::new("mixed-project");
    let store = GraphStore::init(&temp.0).unwrap();
    write(
        store.root().join("project.toml"),
        "[packages]\nroots = [\"catalog:case\"]\nstd = [\"std:core\"]\n",
    );

    assert!(matches!(
        store.read_project(),
        Err(StoreError::ProjectFormat(message)) if message.contains("cannot be combined")
    ));

    write(
        store.root().join("project.toml"),
        "[packages]\nroots = []\nplugins = [\"plugin:legacy\"]\n",
    );
    assert!(matches!(
        store.read_project(),
        Err(StoreError::ProjectFormat(message)) if message.contains("cannot be combined")
    ));
}

#[test]
fn package_lock_is_typed_canonical_and_round_trips_exact_sources() {
    let temp = TestDirectory::new("lock");
    let store = GraphStore::init(&temp.0).unwrap();
    let lock = PackagesLock {
        schema: PACKAGES_LOCK_SCHEMA.to_owned(),
        packages: vec![
            LockedPackage {
                id: "theory:zeta".parse().unwrap(),
                version: "2.0.0".to_owned(),
                digest: "b".repeat(64),
                source: PackageSource::Vendored("packages/theory/zeta".to_owned()),
                layer: PackageLayer::Overlay,
            },
            LockedPackage {
                id: "catalog:alpha".parse().unwrap(),
                version: "1.0.0".to_owned(),
                digest: "a".repeat(64),
                source: PackageSource::Embedded,
                layer: PackageLayer::Reference,
            },
        ],
    };

    store.write_packages_lock(&lock).unwrap();
    let raw: Value =
        serde_json::from_slice(&fs::read(store.root().join("packages.lock.json")).unwrap())
            .unwrap();
    assert_eq!(raw["schema"], PACKAGES_LOCK_SCHEMA);
    assert_eq!(raw["packages"][0]["id"], "catalog:alpha");
    assert_eq!(raw["packages"][1]["id"], "theory:zeta");
    assert_eq!(raw["packages"][1]["source"]["kind"], "vendored");
    assert_eq!(
        raw["packages"][1]["source"]["location"],
        "packages/theory/zeta"
    );

    let reopened = store.read_packages_lock().unwrap().unwrap();
    assert_eq!(reopened.packages[0].id.to_string(), "catalog:alpha");
    assert_eq!(reopened.packages[1].source, lock.packages[0].source);

    let mut unsupported = raw;
    unsupported["schema"] = Value::String("conlang-packages-lock/v0".to_owned());
    fs::write(
        store.root().join("packages.lock.json"),
        serde_json::to_vec_pretty(&unsupported).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        store.read_packages_lock(),
        Err(StoreError::Format(message)) if message.contains("unsupported schema")
    ));
}

#[test]
fn replacing_an_existing_package_lock_exposes_only_the_complete_new_document() {
    let temp = TestDirectory::new("lock-replace");
    let store = GraphStore::init(&temp.0).unwrap();
    let old = PackagesLock {
        schema: PACKAGES_LOCK_SCHEMA.to_owned(),
        packages: vec![LockedPackage {
            id: "catalog:old".parse().unwrap(),
            version: "1.0.0".to_owned(),
            digest: "a".repeat(64),
            source: PackageSource::Embedded,
            layer: PackageLayer::Reference,
        }],
    };
    let new = PackagesLock {
        schema: PACKAGES_LOCK_SCHEMA.to_owned(),
        packages: vec![LockedPackage {
            id: "dataset:new".parse().unwrap(),
            version: "2026.08".to_owned(),
            digest: "b".repeat(64),
            source: PackageSource::Vendored("packages/dataset/new".to_owned()),
            layer: PackageLayer::Data,
        }],
    };

    store.write_packages_lock(&old).unwrap();
    store.write_packages_lock(&new).unwrap();

    let raw = fs::read(store.root().join("packages.lock.json")).unwrap();
    let parsed: Value = serde_json::from_slice(&raw).expect("replacement is complete JSON");
    assert_eq!(parsed["packages"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["packages"][0]["id"], "dataset:new");
    assert_eq!(store.read_packages_lock().unwrap().unwrap(), new);
    assert!(!store.root().join(".packages.lock.json.tmp").exists());
}

#[test]
fn vendored_reader_preserves_manifest_order_and_resolves_offline() {
    let temp = TestDirectory::new("vendored");
    let store = GraphStore::init(&temp.0).unwrap();
    write_reference_package(&store);
    write_data_only_package_without_exports(&store);

    let sources = store.read_vendored_packages().unwrap();
    assert_eq!(sources.len(), 2);
    let reference = sources
        .iter()
        .find(|source| source.config.contains("catalog:persist-test"))
        .unwrap();
    assert_eq!(
        reference.source,
        PackageSource::Vendored("packages/catalog/persist-test".to_owned())
    );
    assert_eq!(
        reference.code,
        "trait PersistCaseUnique:\n\ntrait PersistExtraUnique:\n"
    );
    assert_eq!(reference.functions[0].path, "code/recipes.chg");
    assert_eq!(reference.data_files[0].path, "data/categories.tsv");
    assert_eq!(
        reference.tables, "path\ttype\ndata/categories.tsv\tcatalog:persist-test:CategoryTable\n",
        "config/tables.tsv 應隨 vendored 套件讀入"
    );
    assert!(
        data_only_tables_are_optional(&sources),
        "缺 config/tables.tsv 合法,應得空字串"
    );
    let mut installed_older = reference.clone();
    installed_older.config = installed_older.config.replace("1.2.3", "0.9.0");
    installed_older.source = PackageSource::Installed("cache/catalog/persist-test".to_owned());
    let data_only = sources
        .iter()
        .find(|source| source.config.contains("dataset:observations"))
        .unwrap();
    assert!(
        data_only.exports.is_empty(),
        "missing optional exports stays empty"
    );

    let project = ProjectDocument::from_package_spec(&PackageSpec {
        roots: vec![
            PackageRequirement::exact("catalog:persist-test".parse().unwrap(), "1.2.3"),
            PackageRequirement::exact("dataset:observations".parse().unwrap(), "2026.08"),
        ],
        aliases: BTreeMap::from([(
            "persist".to_owned(),
            "catalog:persist-test".parse().unwrap(),
        )]),
    });
    let resolved = store
        .resolve_project_packages(&project, [installed_older])
        .expect("vendored packages resolve without network");
    assert!(resolved
        .selection
        .standard
        .trait_named("PersistCaseUnique")
        .is_some());
    assert_eq!(
        resolved
            .package(&"dataset:observations".parse().unwrap())
            .unwrap()
            .data,
        "language\tfeature\neng\tSVO\n"
    );
    assert_eq!(
        resolved
            .selection
            .resolved
            .iter()
            .find(|package| package.id.to_string() == "catalog:persist-test")
            .unwrap()
            .source,
        PackageSource::Vendored("packages/catalog/persist-test".to_owned()),
        "project vendored package overrides an installed package with the same id"
    );

    let graph = store.load_with_packages(resolved.clone()).unwrap();
    assert!(graph.is_empty());
    assert_eq!(
        graph.packages().unwrap().lock_digest(),
        resolved.lock_digest()
    );
}

#[test]
fn project_open_rejects_every_exact_lock_field_mismatch() {
    let temp = TestDirectory::new("verify-lock");
    let store = GraphStore::init(&temp.0).unwrap();
    write_reference_package(&store);
    let project = ProjectDocument::from_package_spec(&PackageSpec::default().with_root(
        PackageRequirement::exact("catalog:persist-test".parse().unwrap(), "1.2.3"),
    ));
    let resolved = store
        .resolve_project_packages(&project, Vec::new())
        .unwrap();
    let exact = PackagesLock::from_resolved(&resolved);
    exact.verify_resolved(&resolved).unwrap();

    for (field, mutate) in [
        ("version", 0_u8),
        ("digest", 1),
        ("source", 2),
        ("layer", 3),
    ] {
        let mut changed = exact.clone();
        let entry = &mut changed.packages[0];
        match mutate {
            0 => entry.version = "9.9.9".to_owned(),
            1 => entry.digest = "f".repeat(64),
            2 => entry.source = PackageSource::Installed("cache/catalog/persist-test".to_owned()),
            3 => entry.layer = PackageLayer::Overlay,
            _ => unreachable!(),
        }
        assert!(matches!(
            changed.verify_resolved(&resolved),
            Err(StoreError::PackageLockMismatch { field: actual, .. }) if actual == field
        ));
    }

    let mut stale = exact;
    stale.packages[0].digest = "e".repeat(64);
    store.write_packages_lock(&stale).unwrap();
    assert!(matches!(
        store.resolve_project_packages(&project, Vec::new()),
        Err(StoreError::PackageLockMismatch {
            field: "digest",
            ..
        })
    ));
}

#[test]
fn vendored_reader_rejects_absolute_traversal_and_unsafe_optional_exports() {
    for (name, declaration, field) in [
        ("traversal", "code = [\"../outside.lang\"]", "code"),
        ("absolute", "code = [\"C:/outside.lang\"]", "code"),
        (
            "exports-traversal",
            "exports = \"../../outside.tsv\"\ncode = [\"code/main.lang\"]",
            "exports",
        ),
    ] {
        let temp = TestDirectory::new(name);
        let store = GraphStore::init(&temp.0).unwrap();
        let package = store.root().join("packages/catalog/unsafe");
        write(
            package.join("package.toml"),
            &format!(
                "schema = 2\nid = \"catalog:unsafe-{name}\"\nversion = \"1\"\nlayer = \"reference\"\ncapabilities = [\"traits\"]\n{declaration}\n"
            ),
        );
        write(package.join("code/main.lang"), "trait UnsafePathUnique:\n");

        assert!(matches!(
            store.read_vendored_packages(),
            Err(StoreError::InvalidPackagePath { field: actual, .. }) if actual == field
        ));
    }
}

/// 缺 `config/tables.tsv` = 該套件沒有具型別的表,合法。
fn data_only_tables_are_optional(sources: &[conlang_language::PackageSources]) -> bool {
    sources
        .iter()
        .find(|source| source.config.contains("dataset:observations"))
        .expect("data-only package")
        .tables
        .is_empty()
}
