use conlang_app::{
    AppError, PackageSelectionInput, Session, StructuredEdit, StructuredEditInput, UiSession,
    Workspace,
};
use conlang_changeset::evolution::{EvolutionError, EvolutionGraph};
use conlang_changeset::{NodeUpdate, PrimitiveEdit, ReplayError};
use conlang_language::{LanguageDocument, LibrarySpec, PackageId, PackageRequirement, PackageSpec};
use conlang_persistence::{GraphStore, ProjectDocument, StoreError};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const ROOT: &str = r#"sign probe:
    belongs AppSessionExternalMarker
    entrenchment = 0.5
"#;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-app-package-v2-{name}-{}-{ordinal}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn write(path: impl AsRef<Path>, source: &str) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().expect("fixture file has a parent")).unwrap();
    fs::write(path, source).unwrap();
}

fn write_package(store: &GraphStore, extra_code: &str) {
    let package = store.root().join("packages/catalog/app-session");
    write(
        package.join("package.toml"),
        r#"schema = 2
id = "catalog:app-session"
version = "1.0.0"
layer = "reference"
capabilities = ["traits"]
exports = "config/exports.tsv"
code = ["code/main.lang"]
"#,
    );
    write(
        package.join("config/exports.tsv"),
        "stable_id\tkind\talias\ncatalog:app-session:Marker\ttrait\tAppSessionExternalMarker\n",
    );
    write(
        package.join("code/main.lang"),
        &format!("trait AppSessionExternalMarker:\n{extra_code}"),
    );
}

fn package_spec() -> PackageSpec {
    PackageSpec::default().with_root(PackageRequirement::exact(
        PackageId::new("catalog", "app-session"),
        "1.0.0",
    ))
}

#[test]
fn vendored_v2_packages_drive_compile_changesets_persist_and_reopen() {
    let temp = TempDir::new("roundtrip");
    let store = GraphStore::init(&temp.0).expect("store");
    write_package(&store, "");
    let project = ProjectDocument::from_package_spec(&package_spec());
    store.write_project(&project).expect("project");

    let packages = store
        .resolve_project_packages(&project, std::iter::empty())
        .expect("initial offline resolution");
    let original_lock = packages.lock_digest();
    let mut graph = EvolutionGraph::new_with_packages(packages);
    let root = graph
        .add_root(LanguageDocument::import_new_root(ROOT, "app:v2-root").expect("root"))
        .expect("add root");
    store.save(&graph).expect("seed root");

    let mut workspace =
        Workspace::open(&store, LibrarySpec::default()).expect("canonical project open");
    assert_eq!(
        workspace
            .session()
            .packages()
            .map(|packages| packages.lock_digest()),
        Some(original_lock.clone())
    );
    assert!(workspace
        .compiled()
        .expect("compile must see vendored marker")
        .language()
        .sign_named("probe")
        .is_some());

    let probe = workspace
        .session()
        .snapshot()
        .expect("active root")
        .ref_for_sign("probe")
        .expect("probe identity");
    workspace
        .session_mut()
        .stage_checked(
            "app:v2-next",
            vec![PrimitiveEdit::Update {
                node: probe,
                change: NodeUpdate::Rename("probe-renamed".to_owned()),
            }],
        )
        .expect("resolved-package structured stage");
    assert_eq!(
        workspace
            .session()
            .preview_pending()
            .expect("resolved-package preview")
            .aligned_signs(),
        1
    );
    assert!(workspace
        .session()
        .preview_document()
        .expect("preview document")
        .language()
        .sign_named("probe-renamed")
        .is_some());

    let draft = temp.0.join("draft.chg");
    workspace
        .session()
        .save_working_copy(&draft)
        .expect("save ordinary ChangeSet");
    workspace
        .session_mut()
        .begin_edit("app:v2-next")
        .expect("replace with empty draft");
    workspace
        .session_mut()
        .load_working_copy(&draft)
        .expect("load through resolved package context");
    let child = workspace
        .session_mut()
        .commit(Some("v2 child".to_owned()))
        .expect("commit through resolved graph context");
    assert!(workspace
        .session()
        .graph()
        .snapshot(&child)
        .expect("child snapshot")
        .language()
        .sign_named("probe-renamed")
        .is_some());

    workspace
        .session()
        .persist(&store)
        .expect("persist graph and exact package lock");
    let persisted_lock = store
        .read_packages_lock()
        .expect("read lock")
        .expect("resolved session writes a lock");
    persisted_lock
        .verify_resolved(workspace.session().packages().expect("resolved session"))
        .expect("lock matches the exact compile/replay snapshot");

    let mut reopened = Workspace::open(&store, LibrarySpec::default()).expect("same bytes reopen");
    assert_eq!(reopened.session().graph().len(), 2);
    reopened.session_mut().open(&child).expect("open child");
    assert!(reopened
        .compiled()
        .expect("reopened child compiles")
        .language()
        .sign_named("probe-renamed")
        .is_some());
    assert!(reopened.session().graph().node(&root).is_some());

    write_package(&store, "trait PackageBytesChanged:\n");
    let error = Workspace::open(&store, LibrarySpec::default())
        .expect_err("changed package bytes must fail the exact project lock");
    assert!(
        matches!(
            error,
            AppError::Store(StoreError::PackageLockMismatch {
                field: "digest",
                ..
            })
        ),
        "{error:?}"
    );

    let changed = store
        .resolve_packages(&package_spec(), std::iter::empty())
        .expect("resolve changed bytes without consulting the old lock");
    store
        .write_resolved_packages_lock(&changed)
        .expect("explicitly update project lock");
    let error = Workspace::open(&store, LibrarySpec::default())
        .expect_err("historical ChangeSet lock must still reject changed package semantics");
    assert!(
        matches!(
            error,
            AppError::Store(StoreError::Evolution(EvolutionError::Replay(
                ReplayError::LibraryLockMismatch(_)
            )))
        ),
        "{error:?}"
    );
}

#[test]
fn fallback_open_verifies_an_existing_packages_lock() {
    let temp = TempDir::new("fallback-lock");
    let store = GraphStore::init(&temp.0).expect("store");
    write_package(&store, "");
    let fallback = LibrarySpec {
        std: Vec::new(),
        natural: None,
        plugins: vec![PackageId::new("catalog", "app-session")],
    };
    let resolved = store
        .resolve_packages(&PackageSpec::from_legacy(&fallback), std::iter::empty())
        .expect("resolve fallback");
    store
        .write_resolved_packages_lock(&resolved)
        .expect("seed exact lock");

    write_package(&store, "trait ChangedAfterFallbackLock:\n");
    let error = Session::open_project(&store, fallback)
        .expect_err("no-project fallback must not bypass an existing lock");
    assert!(
        matches!(
            error,
            AppError::Store(StoreError::PackageLockMismatch {
                field: "digest",
                ..
            })
        ),
        "{error:?}"
    );
}

#[test]
fn ui_catalog_and_structured_authoring_use_the_vendored_snapshot() {
    let temp = TempDir::new("ui-authoring");
    let store = GraphStore::init(&temp.0).expect("store");
    write_package(&store, "");
    let project = ProjectDocument::from_package_spec(&package_spec());
    store.write_project(&project).expect("project");
    let packages = store
        .resolve_project_packages(&project, std::iter::empty())
        .expect("resolve");
    let mut graph = EvolutionGraph::new_with_packages(packages);
    graph
        .add_root(LanguageDocument::import_new_root(ROOT, "app:ui-root").expect("root"))
        .expect("add root");
    store.save(&graph).expect("seed graph");

    let mut session = UiSession::open(&temp.0, LibrarySpec::default()).expect("ui open");
    assert_eq!(session.summary().packages, ["catalog:app-session"]);
    let catalog = session.package_catalog().expect("offline catalog");
    assert!(catalog.packages.iter().any(|package| {
        package.id == "catalog:app-session"
            && package.kind == "catalog"
            && package.source == "vendored"
            && package.declared
            && package.selected
    }));

    let authoring = session.authoring_catalog().expect("v2 authoring catalog");
    assert!(authoring
        .traits
        .iter()
        .any(|item| item.name == "AppSessionExternalMarker" && item.source == "library"));
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: authoring.revision,
            edit: StructuredEdit::InsertSign {
                name: "second".to_owned(),
                belongs: vec!["AppSessionExternalMarker".to_owned()],
                phon: None,
                gloss: None,
            },
        })
        .expect("structured operation resolves through vendored package context");
    assert!(session
        .pending_change()
        .expect("pending")
        .source
        .contains("AppSessionExternalMarker"));
}

#[test]
fn project_creation_pins_packages_before_returning() {
    let temp = TempDir::new("create-lock");
    let mut session = UiSession::create(
        &temp.0,
        None::<&Path>,
        Some("pinned".to_owned()),
        "app:create-root",
    )
    .expect("create");

    let store = GraphStore::open(&temp.0).expect("reopen store");
    let lock = store
        .read_packages_lock()
        .expect("read lock")
        .expect("create writes an exact package lock");
    assert!(!lock.packages.is_empty());

    write_package(&store, "");
    let summary = session
        .configure_packages(PackageSelectionInput {
            roots: Some(vec!["catalog:app-session@1.0.0".to_owned()]),
            aliases: Some([("traditional".to_owned(), "catalog:app-session".to_owned())].into()),
            ..PackageSelectionInput::default()
        })
        .expect("IPC accepts open-namespace v2 roots");
    assert_eq!(summary.packages, ["catalog:app-session"]);
    let project = store
        .read_project()
        .expect("read project")
        .expect("project exists");
    assert_eq!(project.packages.roots, ["catalog:app-session@1.0.0"]);
    assert_eq!(
        project
            .packages
            .aliases
            .get("traditional")
            .map(String::as_str),
        Some("catalog:app-session")
    );
    let resolved = store
        .resolve_project_packages(&project, std::iter::empty())
        .expect("reconfiguration reopens through the exact v2 context");
    store
        .read_packages_lock()
        .expect("read v2 lock")
        .expect("v2 reconfiguration writes a lock")
        .verify_resolved(&resolved)
        .expect("lock matches the reopened session");

    session
        .configure_packages(PackageSelectionInput {
            roots: Some(vec!["catalog:app-session@1.0.0".to_owned()]),
            // Settings does not edit aliases; omission must preserve them.
            aliases: None,
            ..PackageSelectionInput::default()
        })
        .expect("omitted aliases preserve current v2 intent");
    let project = store.read_project().unwrap().unwrap();
    assert_eq!(
        project
            .packages
            .aliases
            .get("traditional")
            .map(String::as_str),
        Some("catalog:app-session")
    );

    let before = fs::read_to_string(temp.0.join("project.toml")).expect("project text");
    let lock_before = fs::read_to_string(temp.0.join("packages.lock.json")).expect("lock text");
    session
        .configure_packages(PackageSelectionInput {
            roots: Some(Vec::new()),
            aliases: None,
            ..PackageSelectionInput::default()
        })
        .expect_err("a preserved alias may not dangle after removing its target");
    assert_eq!(
        fs::read_to_string(temp.0.join("project.toml")).expect("unchanged project"),
        before
    );
    assert_eq!(
        fs::read_to_string(temp.0.join("packages.lock.json")).expect("unchanged lock"),
        lock_before
    );

    session
        .configure_packages(PackageSelectionInput {
            roots: Some(vec!["catalog:app-session@1.0.0".to_owned()]),
            aliases: Some(Default::default()),
            ..PackageSelectionInput::default()
        })
        .expect("an explicit empty aliases object clears aliases");
    assert!(store
        .read_project()
        .unwrap()
        .unwrap()
        .packages
        .aliases
        .is_empty());

    let before = fs::read_to_string(temp.0.join("project.toml")).expect("project text");
    let error = session
        .configure_packages(PackageSelectionInput {
            std: vec!["std:core".to_owned()],
            ..PackageSelectionInput::default()
        })
        .expect_err("a legacy payload cannot downgrade an existing v2 project");
    assert_eq!(error.code, "APP_PACKAGE_SELECTION_MIGRATION_REQUIRED");
    assert_eq!(
        fs::read_to_string(temp.0.join("project.toml")).expect("unchanged project"),
        before
    );

    let error = session
        .configure_packages(PackageSelectionInput {
            roots: Some(Vec::new()),
            std: vec!["std:core".to_owned()],
            ..PackageSelectionInput::default()
        })
        .expect_err("mixed v1/v2 IPC intent is rejected");
    assert_eq!(error.code, "APP_PACKAGE_SELECTION_MIXED");
    assert_eq!(
        fs::read_to_string(temp.0.join("project.toml")).expect("unchanged project"),
        before
    );

    for payload in [r#"{"roots":[],"std":[]}"#, r#"{"aliases":{},"plugins":[]}"#] {
        assert!(
            serde_json::from_str::<PackageSelectionInput>(payload).is_err(),
            "field-presence mixing must fail for {payload}"
        );
    }
    let decoded: PackageSelectionInput = serde_json::from_str(
        r#"{"roots":["catalog:app-session@1.0.0"],"aliases":{"traditional":"catalog:app-session"}}"#,
    )
    .expect("v2 IPC shape deserializes");
    assert!(!decoded.legacy_shape);
    let encoded = serde_json::to_value(decoded).expect("v2 IPC shape serializes");
    assert!(encoded.get("std").is_none() && encoded.get("plugins").is_none());

    drop(session);
    let reopened = UiSession::open(&temp.0, LibrarySpec::default().without_std())
        .expect("saved v2 intent overrides a different no-project fallback");
    assert_eq!(reopened.summary().packages, ["catalog:app-session"]);
}

#[test]
fn no_project_fallback_can_migrate_to_v2_and_reopen_independently() {
    let temp = TempDir::new("fallback-to-v2");
    let store = GraphStore::init(&temp.0).expect("store");
    write_package(&store, "");
    let legacy = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(legacy.clone());
    graph
        .add_root(LanguageDocument::import_new_root("", "app:fallback-root").expect("root"))
        .expect("add root");
    store
        .save(&graph)
        .expect("legacy store without project.toml");

    let mut session = UiSession::open(&temp.0, legacy).expect("fallback open");
    assert!(session.summary().legacy);
    session
        .configure_packages(PackageSelectionInput {
            roots: Some(vec!["catalog:app-session@1.0.0".to_owned()]),
            aliases: Some([("catalog".to_owned(), "catalog:app-session".to_owned())].into()),
            ..PackageSelectionInput::default()
        })
        .expect("explicit v2 payload migrates fallback project");
    drop(session);

    let reopened = UiSession::open(&temp.0, LibrarySpec::default().without_std())
        .expect("persisted v2 project ignores a different fallback");
    assert!(!reopened.summary().legacy);
    assert_eq!(reopened.summary().packages, ["catalog:app-session"]);
    let project = store.read_project().unwrap().unwrap();
    assert_eq!(
        project.packages.aliases.get("catalog").map(String::as_str),
        Some("catalog:app-session")
    );
    store
        .read_packages_lock()
        .unwrap()
        .expect("migration writes exact lock")
        .verify_resolved(
            &store
                .resolve_project_packages(&project, std::iter::empty())
                .expect("re-resolve v2 project"),
        )
        .expect("persisted lock matches v2 intent");
}

#[test]
fn resolved_session_rejects_legacy_or_divergent_graph_contexts() {
    let temp = TempDir::new("session-context");
    let store = GraphStore::init(&temp.0).expect("store");
    write_package(&store, "");
    let packages = store
        .resolve_packages(&package_spec(), std::iter::empty())
        .expect("resolve original package context");

    let legacy_graph = EvolutionGraph::new(LibrarySpec::default());
    assert!(matches!(
        Session::new_with_packages(legacy_graph, packages.clone()),
        Err(AppError::PackageContextMissing)
    ));

    let graph = EvolutionGraph::new_with_packages(packages.clone());
    let mut different_intent = packages.clone();
    different_intent.intent.aliases.insert(
        "external".to_owned(),
        PackageId::new("catalog", "app-session"),
    );
    assert!(matches!(
        Session::new_with_packages(graph, different_intent),
        Err(AppError::PackageContextMismatch)
    ));

    let graph = EvolutionGraph::new_with_packages(packages);
    write_package(&store, "trait DifferentResolvedBytes:\n");
    let different_resolution = store
        .resolve_packages(&package_spec(), std::iter::empty())
        .expect("resolve changed bytes");
    assert!(matches!(
        Session::new_with_packages(graph, different_resolution),
        Err(AppError::PackageContextMismatch)
    ));
}
