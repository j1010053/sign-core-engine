use conlang_changeset::change_set_prelude;
use conlang_changeset::evolution::{Edge, EvolutionGraph, Nativization, NodeId};
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_persistence::{GraphStore, NodeConfig, StoreError};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const ROOT: &str = r#"
global trait Core:
    syn:
        feature:
            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)
            category = lexical

trait Affix:
    syn:
        feature:
            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)
            category = bound

sign x:
    syn:
        feature:
            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)
            category = noun

sign y:
    syn:
        feature:
            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)
            category = noun
"#;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> TestDirectory {
        let ordinal = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-persistence-{name}-{}-{ordinal}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        TestDirectory(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn fixture() -> (EvolutionGraph, NodeId, NodeId) {
    let libraries = LibrarySpec::default();
    let root = LanguageDocument::import_new_root(ROOT, "store:root").unwrap();
    let mut graph = EvolutionGraph::new(libraries.clone());
    let root_id = graph.add_root(root).unwrap();
    graph.set_label(&root_id, Some("Proto".to_owned())).unwrap();
    let base = graph.snapshot(&root_id).unwrap().clone();
    let mut changeset = change_set_prelude(&base, &libraries, "store:child").unwrap();
    changeset.push_str("\n    #0:\n        update sign(\"x\").feature[syn.category].value = verb\n");
    let child_id = graph
        .commit(
            vec![Edge::trunk(root_id.clone(), changeset)],
            Nativization::None,
            Some("Daughter".to_owned()),
        )
        .unwrap();
    (graph, root_id, child_id)
}

fn manifest(path: &Path, id: &NodeId) -> Value {
    serde_json::from_slice(
        &fs::read(path.join("nodes").join(id.as_str()).join("manifest")).unwrap(),
    )
    .unwrap()
}

fn named_object(manifest: &Value, collection: &str, name: &str) -> String {
    manifest[collection]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == name)
        .unwrap()["object"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn shared_objects_round_trip_graph_and_hash_external_metadata() {
    let temp = TestDirectory::new("roundtrip");
    let store = GraphStore::init(&temp.0).unwrap();
    let (graph, root_id, child_id) = fixture();
    store.save(&graph).unwrap();

    let root_manifest = manifest(&temp.0, &root_id);
    let child_manifest = manifest(&temp.0, &child_id);
    assert_eq!(
        named_object(&root_manifest, "signs", "y"),
        named_object(&child_manifest, "signs", "y"),
        "unchanged sign y must reuse one P60 object"
    );
    assert_ne!(
        named_object(&root_manifest, "signs", "x"),
        named_object(&child_manifest, "signs", "x"),
        "changed sign x must receive a new P60 object"
    );
    assert_eq!(
        root_manifest["traits"], child_manifest["traits"],
        "unchanged traits share objects too"
    );

    let mut config = store.read_config(&child_id).unwrap();
    config
        .preferences
        .insert("editor_zoom".to_owned(), json!(1.25));
    store.write_config(&child_id, &config).unwrap();
    store
        .write_annotation(&child_id, "culture/metaphors.md", b"river = lineage\n")
        .unwrap();
    assert_eq!(
        store.list_annotations(&child_id).unwrap(),
        vec![PathBuf::from("culture").join("metaphors.md")]
    );
    assert_eq!(
        store
            .read_annotation(&child_id, "culture/metaphors.md")
            .unwrap(),
        b"river = lineage\n"
    );

    // Re-saving synchronizes the label but preserves unrelated hash-external
    // config and annotations.
    store.save(&graph).unwrap();
    assert_eq!(
        store.read_config(&child_id).unwrap().preferences,
        config.preferences
    );

    // A stale interrupted node directory is ignored; only canonical ids load.
    fs::create_dir_all(temp.0.join("nodes").join(".interrupted.tmp")).unwrap();
    let loaded = GraphStore::open(&temp.0)
        .unwrap()
        .load(LibrarySpec::default())
        .unwrap();
    assert_eq!(loaded.len(), graph.len());
    assert_eq!(
        loaded.snapshot(&root_id).unwrap().source(),
        graph.snapshot(&root_id).unwrap().source()
    );
    assert_eq!(
        loaded.snapshot(&child_id).unwrap().manifest_json().unwrap(),
        graph.snapshot(&child_id).unwrap().manifest_json().unwrap()
    );
    assert_eq!(
        loaded.node(&root_id).unwrap().label(),
        graph.node(&root_id).unwrap().label()
    );
    assert_eq!(
        loaded.node(&child_id).unwrap().label(),
        graph.node(&child_id).unwrap().label()
    );
    loaded.verify_all().unwrap();

    assert!(matches!(
        store.write_annotation(&child_id, "../escape", b"no"),
        Err(StoreError::InvalidAnnotationPath(_))
    ));
}

#[test]
fn corrupt_shared_object_is_rejected_before_graph_restore() {
    let temp = TestDirectory::new("corrupt-object");
    let store = GraphStore::init(&temp.0).unwrap();
    let (graph, _, child_id) = fixture();
    store.save(&graph).unwrap();
    let child_manifest = manifest(&temp.0, &child_id);
    let identity_object = child_manifest["identities"].as_str().unwrap();
    fs::write(
        temp.0.join("objects").join(identity_object),
        b"corrupt identity object",
    )
    .unwrap();

    assert!(matches!(
        store.load(LibrarySpec::default()),
        Err(StoreError::ObjectCorrupt { .. })
    ));
}

#[test]
fn immutable_node_manifest_cannot_be_replaced_by_save() {
    let temp = TestDirectory::new("immutable");
    let store = GraphStore::init(&temp.0).unwrap();
    let (graph, root_id, _) = fixture();
    store.save(&graph).unwrap();
    let path = temp.0.join("nodes").join(root_id.as_str()).join("manifest");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["source_sha256"] = json!("0".repeat(64));
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    assert!(matches!(
        store.save(&graph),
        Err(StoreError::ImmutableNode {
            field: "manifest",
            ..
        })
    ));
}

#[test]
fn config_is_hash_external_and_never_changes_language_state() {
    let temp = TestDirectory::new("config");
    let store = GraphStore::init(&temp.0).unwrap();
    let (graph, _, child_id) = fixture();
    store.save(&graph).unwrap();
    let source = graph.snapshot(&child_id).unwrap().source();

    let config = NodeConfig {
        label: Some("Display-only rename".to_owned()),
        preferences: BTreeMap::from([
            ("contact_injection".to_owned(), json!("ignored")),
            ("theme".to_owned(), json!("dark")),
        ]),
    };
    store.write_config(&child_id, &config).unwrap();
    let loaded = store.load(LibrarySpec::default()).unwrap();
    assert_eq!(loaded.snapshot(&child_id).unwrap().source(), source);
    assert_eq!(
        loaded.node(&child_id).unwrap().label(),
        Some("Display-only rename")
    );
    assert_eq!(loaded.ids().find(|id| *id == &child_id), Some(&child_id));
}

#[test]
fn multi_root_donor_merge_and_nativization_survive_persistence() {
    let temp = TestDirectory::new("multi-parent");
    let store = GraphStore::init(&temp.0).unwrap();
    let libraries = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(libraries.clone());
    let root_a = graph
        .add_root(
            LanguageDocument::import_new_root(
                "sign a:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
                "store:a",
            )
            .unwrap(),
        )
        .unwrap();
    let root_b = graph
        .add_root(
            LanguageDocument::import_new_root(
                "sign b:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
                "store:b",
            )
            .unwrap(),
        )
        .unwrap();

    let base_a = graph.snapshot(&root_a).unwrap().clone();
    let mut donor_change = change_set_prelude(&base_a, &libraries, "store:borrow").unwrap();
    donor_change.push_str(&format!("    donor other {}\n", root_b.as_str()));
    donor_change.push_str("\n    #0:\n        update sign(\"a\").feature[syn.category].value = verb\n");
    let borrowed = graph
        .commit(
            vec![Edge::trunk(root_a, donor_change)],
            Nativization::Pidgin,
            None,
        )
        .unwrap();

    let parents = vec![borrowed.clone(), root_b.clone()];
    let merged_base = graph.merged_base(&parents).unwrap();
    let merge_change = change_set_prelude(&merged_base, &libraries, "store:creole").unwrap();
    let creole = graph
        .commit(
            vec![Edge::trunk(borrowed, merge_change), Edge::reference(root_b)],
            Nativization::Creole { generation: 2 },
            Some("Contact language".to_owned()),
        )
        .unwrap();
    assert!(graph
        .snapshot(&creole)
        .unwrap()
        .source()
        .contains("sign a:"));
    assert!(graph
        .snapshot(&creole)
        .unwrap()
        .source()
        .contains("sign b:"));

    store.save(&graph).unwrap();
    let loaded = store.load(LibrarySpec::default()).unwrap();
    assert_eq!(
        loaded.node(&creole).unwrap().nativization(),
        Nativization::Creole { generation: 2 }
    );
    assert_eq!(loaded.node(&creole).unwrap().parents().len(), 2);
    assert!(loaded
        .snapshot(&creole)
        .unwrap()
        .source()
        .contains("sign a:"));
    assert!(loaded
        .snapshot(&creole)
        .unwrap()
        .source()
        .contains("sign b:"));
    loaded.verify_all().unwrap();
}

#[test]
fn a_node_folder_cannot_outlive_its_persisted_parent() {
    let temp = TestDirectory::new("orphan");
    let store = GraphStore::init(&temp.0).unwrap();
    let (graph, root_id, _) = fixture();
    store.save(&graph).unwrap();
    fs::remove_dir_all(temp.0.join("nodes").join(root_id.as_str())).unwrap();

    assert!(matches!(
        store.load(LibrarySpec::default()),
        Err(StoreError::Evolution(
            conlang_changeset::evolution::EvolutionError::PersistedParentMissing { .. }
        ))
    ));
}

/// 步驟 20:State 往返,且**雜湊外**。
///
/// 裁定 (A):State 只在撰寫時被讀,replay 不看它——故它必須與
/// `manifest`/`edges` 分檔,寫入不改變 node-v2 的 immutable 內容。
#[test]
fn state_round_trips_and_stays_outside_the_hash() {
    use conlang_changeset::state::{Contact, ContactIntensity, EvolutionState};

    let temp = TestDirectory::new("state");
    let store = GraphStore::init(&temp.0).unwrap();
    let (graph, root_id, _child) = fixture();
    store.save(&graph).unwrap();

    // 前提:沒寫過時是空的,不是錯誤
    assert!(store.read_state(&root_id).unwrap().is_empty());
    let before = manifest(&temp.0, &root_id);

    let state = EvolutionState {
        time: Some("約 800".to_owned()),
        region: Some("河谷".to_owned()),
        society: vec!["農耕".to_owned()],
        contacts: vec![Contact {
            counterpart: "neighbour".to_owned(),
            period: Some("800–1100".to_owned()),
            intensity: ContactIntensity::Bilingual,
        }],
    };
    store.write_state(&root_id, &state).unwrap();

    // 往返
    assert_eq!(store.read_state(&root_id).unwrap(), state);

    // 🔑 雜湊外:寫 State 不得改動 immutable node 內容
    assert_eq!(
        manifest(&temp.0, &root_id),
        before,
        "State 不進 node-v2 雜湊——它不是語言內容"
    );

    // 且 save 可重跑(immutable 檔逐位元比對不得因 State 而失敗)
    store.save(&graph).unwrap();
    assert_eq!(store.read_state(&root_id).unwrap(), state, "save 不覆寫 State");
}
