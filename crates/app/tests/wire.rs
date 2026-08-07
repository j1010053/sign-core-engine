//! 出境契約的出口(裁定 ② 丙)。
//!
//! 釘三件事:
//!
//! 1. **每個出境視圖都帶 schema 標記** —— 沒有它,形狀改了沒人會發現;
//! 2. **形狀 golden** —— JSON 一變就在 diff 裡看得見,那是 `UI_SCHEMA_V1`
//!    「該不該 bump」的唯一旁證(它是約定不是機制);
//! 3. **演化樹只給 parents**,且區分主幹/引用邊。

use conlang_app::wire::UI_SCHEMA_V1;
use conlang_app::{AppError, Workspace};
use conlang_changeset::evolution::{Edge, EvolutionGraph, Nativization};
use conlang_changeset::{change_set_prelude, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument, StoreError};
use conlang_query::{LexiconFilter, ViewConfig};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "Symbol k\nSymbol a\nSymbol t\n\nglobal trait Core:\n\n\
sign kat:\n    belongs Noun\n    phon:\n        /kat/\n    sem:\n        senses:\n            core = STONE\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Temp {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-wire-{name}-{}-{ordinal}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        Temp(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

/// 一個 root + 一個子節點(主幹)。
fn project(temp: &Temp) -> GraphStore {
    let store = GraphStore::init(&temp.0).expect("init");
    let spec = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(spec.clone());
    let root = graph
        .add_root(LanguageDocument::import_new_root(SOURCE, "wire:root").expect("root"))
        .expect("add_root");
    graph.set_label(&root, Some("Proto".to_owned())).expect("label");

    let base = graph.snapshot(&root).expect("snapshot").clone();
    let mut text = change_set_prelude(&base, &spec, "wire:child").expect("prelude");
    text.push_str("\n    #0:\n        update sign(\"kat\").def[phon].value = /kats/\n");
    UnresolvedChangeSet::parse(&text).expect("parses");
    graph
        .commit(
            vec![Edge::trunk(root, text)],
            Nativization::None,
            Some("Daughter".to_owned()),
        )
        .expect("commit");

    store.save(&graph).expect("save");
    store
        .write_project(&ProjectDocument::from_spec(&spec))
        .expect("project");
    store
}

fn workspace(store: &GraphStore) -> Workspace {
    Workspace::open(store, LibrarySpec::default()).expect("open")
}

// ── ① 每個視圖都帶 schema ────────────────────────────────────────────────

/// 🔑 **四個出境視圖全都帶 `schema`。**
///
/// 判別性:漏掉任何一個,對應的斷言就紅。沒有 schema 的視圖等於選了「甲案」
/// ——形狀改了沒人會發現。
#[test]
fn every_outbound_view_carries_a_schema_marker() {
    let temp = Temp::new("schema");
    let store = project(&temp);
    let mut workspace = workspace(&store);

    assert_eq!(workspace.tree().schema, UI_SCHEMA_V1);
    assert_eq!(
        workspace
            .lexicon_view(&LexiconFilter::all(), &ViewConfig::default())
            .expect("lexicon")
            .schema,
        UI_SCHEMA_V1
    );
    assert_eq!(workspace.node_detail(&store).expect("detail").schema, UI_SCHEMA_V1);
    assert_eq!(
        workspace
            .grouping_view(
                &conlang_query::TreeEdgeCut { threshold: 0.6 },
                &conlang_query::ExploratoryHeuristicV1::suggested(),
                &conlang_query::GroupingOverride::default(),
            )
            .schema,
        UI_SCHEMA_V1
    );
}

// ── ② 形狀 golden ────────────────────────────────────────────────────────

/// 🔑 **出境 JSON 的形狀被釘住。**
///
/// 這是 `UI_SCHEMA_V1` 該不該 bump 的**唯一旁證**——它是約定不是機制,
/// 沒有東西強制;但形狀一變這裡就 churn,審查時看得見。
///
/// 只比對**鍵**不比對值:節點 id 是內容雜湊,寫進 golden 會讓任何無關的引擎
/// 改動都讓本測試紅,那會訓練人忽略它。
#[test]
fn the_outbound_shapes_are_pinned() {
    let temp = Temp::new("golden");
    let store = project(&temp);
    let mut workspace = workspace(&store);

    let tree = serde_json::to_value(workspace.tree()).expect("json");
    assert_eq!(keys(&tree), vec!["active", "nodes", "schema"]);
    let root = tree["nodes"]
        .as_array()
        .expect("tree nodes")
        .iter()
        .find(|node| node["parents"].as_array().expect("一律有 parents").is_empty())
        .expect("tree has a root");
    assert_eq!(
        keys(root),
        vec!["id", "label", "parents"],
        "🔑 root 也要有 `parents`(空陣列)——省略它只會讓消費端多一個 undefined 分支"
    );

    let lexicon = serde_json::to_value(
        workspace
            .lexicon_view(&LexiconFilter::all(), &ViewConfig::default())
            .expect("lexicon"),
    )
    .expect("json");
    assert_eq!(keys(&lexicon), vec!["lexicon", "node", "schema"]);
    assert_eq!(
        keys(&lexicon["lexicon"]),
        vec!["entries", "total_before_filter"]
    );
    assert_eq!(
        keys(&lexicon["lexicon"]["entries"][0]),
        vec![
            "categories",
            "dimensions",
            "gloss",
            "name",
            "senses",
            "underlying_form"
        ]
    );

    let detail = serde_json::to_value(workspace.node_detail(&store).expect("detail")).expect("json");
    assert_eq!(
        keys(&detail),
        vec!["id", "label", "schema", "sign_count", "state"],
        "annotations 為空時不序列化"
    );
}

/// 維度以**小寫關鍵詞**出境,與 `.lang` 的區塊名一致。
#[test]
fn dimensions_serialize_as_lowercase_keywords() {
    let temp = Temp::new("dims");
    let store = project(&temp);
    let mut workspace = workspace(&store);
    let lexicon = serde_json::to_value(
        workspace
            .lexicon_view(&LexiconFilter::all(), &ViewConfig::default())
            .expect("lexicon"),
    )
    .expect("json");

    let dims = &lexicon["lexicon"]["entries"][0]["dimensions"];
    let names: Vec<&str> = dims
        .as_array()
        .expect("array")
        .iter()
        .map(|pair| pair[0].as_str().expect("dim name"))
        .collect();
    assert_eq!(names, vec!["phon", "syn", "sem", "prag"]);
}

/// 出境形狀**讀得回來**——`deny_unknown_fields` 之下,序列化與反序列化必須對稱。
#[test]
fn outbound_views_round_trip() {
    let temp = Temp::new("roundtrip");
    let store = project(&temp);
    let workspace = workspace(&store);

    let tree = workspace.tree();
    let text = serde_json::to_string(&tree).expect("to json");
    let back: conlang_app::EvolutionTreeV1 = serde_json::from_str(&text).expect("from json");
    assert_eq!(back, tree);

    let detail = workspace.node_detail(&store).expect("detail");
    let back: conlang_app::NodeDetailV1 =
        serde_json::from_str(&serde_json::to_string(&detail).expect("to")).expect("from");
    assert_eq!(back, detail);
}

// ── ③ 演化樹只給 parents,且分邊種 ──────────────────────────────────────

/// 🔑 **樹只給 parents;主幹與引用邊分得開。**
///
/// 不假造 children:`EvolutionGraph` 本身就沒有 children 索引,出境形狀不該
/// 宣稱一個資料層沒有的關係。而引用邊(donor / 合併)不是世系鄰接,UI 該畫得
/// 不一樣——分群也不沿它切。
#[test]
fn the_tree_exposes_parents_and_distinguishes_edge_kinds() {
    let temp = Temp::new("tree");
    let store = project(&temp);
    let workspace = workspace(&store);
    let tree = workspace.tree();

    assert_eq!(tree.nodes.len(), 2);
    let roots: Vec<&conlang_app::TreeNodeV1> =
        tree.nodes.iter().filter(|n| n.parents.is_empty()).collect();
    assert_eq!(roots.len(), 1, "一個 root");
    assert_eq!(roots[0].label.as_deref(), Some("Proto"));

    let child = tree
        .nodes
        .iter()
        .find(|n| !n.parents.is_empty())
        .expect("有子節點");
    assert_eq!(child.label.as_deref(), Some("Daughter"));
    assert_eq!(child.parents.len(), 1);
    assert_eq!(child.parents[0].kind, "trunk", "parents[0] 一律是主幹");
    assert_eq!(child.parents[0].from, roots[0].id);

    // 出境形狀沒有 children——那是前端自己反轉的
    let json = serde_json::to_value(&tree).expect("json");
    assert!(
        !json.to_string().contains("children"),
        "不得假造資料層沒有的關係"
    );
    assert_eq!(tree.active.as_deref(), Some(roots[0].id.as_str()), "open 停在 root");
}

/// 引用邊(合併/donor)標成 `reference`,不是 `trunk`。
#[test]
fn a_reference_edge_is_labelled_as_such() {
    let temp = Temp::new("reference");
    let store = project(&temp);
    let spec = LibrarySpec::default();
    let mut graph = store.load(spec.clone()).expect("load");

    let ids: Vec<_> = graph.ids().cloned().collect();
    let (a, b) = (ids[0].clone(), ids[1].clone());
    let base = graph.merged_base(&[a.clone(), b.clone()]).expect("merged");
    let text = change_set_prelude(&base, &spec, "wire:merge").expect("prelude");
    graph
        .commit(
            vec![Edge::trunk(a, text), Edge::reference(b.clone())],
            Nativization::None,
            Some("Merged".to_owned()),
        )
        .expect("commit");
    store.save(&graph).expect("save");

    let workspace = Workspace::open(&store, spec).expect("open");
    let merged = workspace
        .tree()
        .nodes
        .into_iter()
        .find(|n| n.label.as_deref() == Some("Merged"))
        .expect("有合併節點");
    assert_eq!(merged.parents.len(), 2);
    assert_eq!(merged.parents[0].kind, "trunk");
    assert_eq!(merged.parents[1].kind, "reference");
    assert_eq!(merged.parents[1].from, b.as_str());
}

// ── 節點編輯頁 ───────────────────────────────────────────────────────────

/// 🔑 **編輯頁要的東西全是雜湊外的**(P64):label / state / annotation。
#[test]
fn the_node_detail_page_shows_only_hash_external_metadata() {
    let temp = Temp::new("detail");
    let store = project(&temp);
    let workspace = workspace(&store);
    let id = workspace.session().active().expect("active").clone();

    let before = workspace.node_detail(&store).expect("detail");
    assert_eq!(before.label.as_deref(), Some("Proto"));
    assert_eq!(before.state, Default::default());
    assert!(before.annotations.is_empty());
    assert_eq!(before.sign_count, 1);

    // 改它們
    store
        .write_state(
            &id,
            &conlang_changeset::state::EvolutionState {
                region: Some("河谷".to_owned()),
                ..Default::default()
            },
        )
        .expect("state");
    store
        .write_annotation(&id, "culture.md", b"note")
        .expect("annotation");

    let after = Workspace::open(&store, LibrarySpec::default())
        .expect("reopen")
        .node_detail(&store)
        .expect("detail");
    assert_eq!(after.state.region.as_deref(), Some("河谷"));
    assert_eq!(after.annotations, vec!["culture.md"]);
    // **語言內容完全沒動**
    assert_eq!(after.sign_count, before.sign_count);
    assert_eq!(after.id, before.id, "節點 id 不變——那些欄位在雜湊外");
}

/// 旁註列舉是持久層 I/O；損毀或缺失不能被 UI 偽裝成「這個節點沒有旁註」。
#[test]
fn node_detail_propagates_annotation_listing_errors() {
    let temp = Temp::new("annotation-error");
    let store = project(&temp);
    let workspace = workspace(&store);
    let id = workspace.session().active().expect("active");
    let annotation_dir = store.root().join("nodes").join(id.as_str()).join("annotation");
    fs::remove_dir(&annotation_dir).expect("移除空的 annotation 目錄");

    assert!(matches!(
        workspace.node_detail(&store),
        Err(AppError::Store(StoreError::Io { .. }))
    ));
}

/// JSON 物件的鍵,排序後。
fn keys(value: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = value
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    names.sort();
    names
}

/// 沒有 active 節點時,需要節點的視圖回明確錯誤。
#[test]
fn views_that_need_a_node_refuse_when_there_is_none() {
    let temp = Temp::new("empty");
    let store = GraphStore::init(&temp.0).expect("init");
    let mut workspace = workspace(&store);
    assert!(workspace.session().active().is_none());
    assert!(workspace
        .lexicon_view(&LexiconFilter::all(), &ViewConfig::default())
        .is_err());
    assert!(workspace.node_detail(&store).is_err());
    // 但樹視圖照樣給得出來(空的)
    let tree = workspace.tree();
    assert!(tree.nodes.is_empty() && tree.active.is_none());
}

/// 🔑 **入境的未知欄位必須被拒**(`deny_unknown_fields`)。
///
/// 前端打錯一個欄位名時,serde 預設會**靜默忽略**——那與這個 repo 的其他
/// 邊界一致地選了「不靜默」:`.chg` 的 digest、package 的 exports、
/// `project.toml` 的套件 id,全都是對不上就硬錯。
///
/// 判別性:拿掉任一個 `deny_unknown_fields`,對應的斷言就綠掉。
#[test]
fn an_unknown_field_from_the_front_end_is_rejected() {
    // 每個出境型別各一組:合法的必須讀得回,多一個欄位必須失敗
    let cases: Vec<(&str, &str, &str)> = vec![
        (
            "TreeNodeV1",
            r#"{"id":"abc"}"#,
            r#"{"id":"abc","childrn":[]}"#,
        ),
        (
            "TreeEdgeV1",
            r#"{"from":"abc","kind":"trunk"}"#,
            r#"{"from":"abc","kind":"trunk","weight":1}"#,
        ),
    ];
    for (name, good, bad) in cases {
        match name {
            "TreeNodeV1" => {
                serde_json::from_str::<conlang_app::TreeNodeV1>(good)
                    .unwrap_or_else(|e| panic!("{name} 合法輸入該過:{e}"));
                assert!(
                    serde_json::from_str::<conlang_app::TreeNodeV1>(bad).is_err(),
                    "{name} 多一個欄位應被拒,而不是靜默忽略"
                );
            }
            _ => {
                serde_json::from_str::<conlang_app::TreeEdgeV1>(good)
                    .unwrap_or_else(|e| panic!("{name} 合法輸入該過:{e}"));
                assert!(
                    serde_json::from_str::<conlang_app::TreeEdgeV1>(bad).is_err(),
                    "{name} 多一個欄位應被拒"
                );
            }
        }
    }

    // 頂層信封同理
    let tree = r#"{"schema":"conlang.ui/v1","nodes":[]}"#;
    serde_json::from_str::<conlang_app::EvolutionTreeV1>(tree).expect("合法信封該過");
    assert!(serde_json::from_str::<conlang_app::EvolutionTreeV1>(
        r#"{"schema":"conlang.ui/v1","nodes":[],"版本":1}"#
    )
    .is_err());
}
