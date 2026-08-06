//! 前端邊界的出口。
//!
//! **這組測試就是前端的測試出口**——Tauri 那層是 1:1 的膠水
//! (`state.lock().unwrap().tree()`),沒有自己的邏輯可測。
//!
//! 釘的是 MVP 兩個面板加編輯頁真的走得通:
//! 演化樹 → 點節點 → 編輯頁 → 改 label/State/旁註 → **語言內容不動**。

use conlang_app::ipc::{LexiconQuery, UiSession};
use conlang_changeset::evolution::{Edge, EvolutionGraph, Nativization};
use conlang_changeset::state::{Contact, ContactIntensity, EvolutionState};
use conlang_changeset::{change_set_prelude, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "Symbol k\nSymbol a\nSymbol t\n\nglobal trait Core:\n\n\
sign kat:\n    belongs Noun\n    phon:\n        /kat/\n    sem:\n        senses:\n            core = STONE\n\
sign tak:\n    belongs Verb\n    phon:\n        /tak/\n    sem:\n        senses:\n            core = GO\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Temp {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-ipc-{name}-{}-{ordinal}",
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

/// root(Proto)→ 子節點(Daughter)。
fn project(temp: &Temp) -> GraphStore {
    let store = GraphStore::init(&temp.0).expect("init");
    let spec = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(spec.clone());
    let root = graph
        .add_root(LanguageDocument::import_new_root(SOURCE, "ipc:root").expect("root"))
        .expect("add_root");
    graph
        .set_label(&root, Some("Proto".to_owned()))
        .expect("label");

    let base = graph.snapshot(&root).expect("snapshot").clone();
    let mut text = change_set_prelude(&base, &spec, "ipc:child").expect("prelude");
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

fn session(temp: &Temp) -> UiSession {
    project(temp);
    UiSession::open(&temp.0, LibrarySpec::default()).expect("open")
}

// ── 演化樹面板 ───────────────────────────────────────────────────────────

/// 🔑 **開啟 → 樹 → 點節點 → 編輯頁**,MVP 的主流程。
#[test]
fn the_tree_panel_leads_into_each_node_detail_page() {
    let temp = Temp::new("flow");
    let mut session = session(&temp);

    let tree = session.tree();
    assert_eq!(tree.nodes.len(), 2);
    let daughter = tree
        .nodes
        .iter()
        .find(|n| n.label.as_deref() == Some("Daughter"))
        .expect("有子節點")
        .clone();

    let detail = session.select_node(&daughter.id).expect("select");
    assert_eq!(detail.id, daughter.id);
    assert_eq!(detail.label.as_deref(), Some("Daughter"));
    assert_eq!(detail.sign_count, 2);

    // 樹的 active 跟著移動——UI 才知道現在停在哪
    assert_eq!(session.tree().active.as_deref(), Some(daughter.id.as_str()));
}

/// 點不存在的節點 ⇒ 明確的 `code`,不是靜默切換。
#[test]
fn selecting_an_unknown_node_reports_a_code() {
    let temp = Temp::new("unknown");
    let mut session = session(&temp);
    let error = session.select_node("not-a-digest").expect_err("應拒絕");
    assert_eq!(error.code, "APP_UNKNOWN_NODE");
    assert!(!error.code.contains(' '), "code 供比對,不是句子");
}

// ── 辭典面板 ─────────────────────────────────────────────────────────────

/// 🔑 **切換節點會換到那個節點的詞典。**
///
/// 判別性:root 的 `kat` 是 `/kat/`,子節點被改成 `/kats/`——若詞典沒跟著
/// active 走,兩邊會拿到同一份。
#[test]
fn the_lexicon_follows_the_selected_node() {
    let temp = Temp::new("lexicon");
    let mut session = session(&temp);
    let tree = session.tree();
    let root = tree
        .nodes
        .iter()
        .find(|n| n.parents.is_empty())
        .expect("root")
        .clone();
    let daughter = tree
        .nodes
        .iter()
        .find(|n| !n.parents.is_empty())
        .expect("child")
        .clone();

    session.select_node(&root.id).expect("root");
    let at_root = session.lexicon(&LexiconQuery::default()).expect("lexicon");
    assert_eq!(at_root.node, root.id);
    let kat = at_root
        .lexicon
        .entries
        .iter()
        .find(|e| e.name == "kat")
        .expect("kat");
    assert_eq!(kat.underlying_form.as_deref(), Some("kat"));

    session.select_node(&daughter.id).expect("daughter");
    let at_child = session.lexicon(&LexiconQuery::default()).expect("lexicon");
    assert_eq!(at_child.node, daughter.id);
    let kat = at_child
        .lexicon
        .entries
        .iter()
        .find(|e| e.name == "kat")
        .expect("kat");
    assert_eq!(
        kat.underlying_form.as_deref(),
        Some("kats"),
        "子節點的形已變"
    );
}

/// 前端送的查詢條件真的生效(範疇走 ontology 閉包、gloss 子字串、排序)。
#[test]
fn the_lexicon_query_from_the_front_end_is_applied() {
    let temp = Temp::new("query");
    let mut session = session(&temp);

    let all = session.lexicon(&LexiconQuery::default()).expect("all");
    assert_eq!(all.lexicon.entries.len(), 2);

    let nominal = session
        .lexicon(&LexiconQuery {
            category: Some("Nominal".to_owned()),
            ..LexiconQuery::default()
        })
        .expect("filtered");
    assert_eq!(
        nominal.lexicon.entries.len(),
        1,
        "belongs Noun 被 Nominal 選中"
    );
    assert_eq!(nominal.lexicon.total_before_filter, 2, "分母是過濾前");

    let by_gloss = session
        .lexicon(&LexiconQuery {
            gloss_contains: Some("STONE".to_owned()),
            ..LexiconQuery::default()
        })
        .expect("gloss");
    assert_eq!(by_gloss.lexicon.entries.len(), 1);

    let names = |query: &LexiconQuery| -> Vec<String> {
        let mut session = UiSession::open(&temp.0, LibrarySpec::default()).expect("open");
        session
            .lexicon(query)
            .expect("lexicon")
            .lexicon
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    };
    assert_eq!(names(&LexiconQuery::default()), vec!["kat", "tak"]);
    assert_eq!(
        names(&LexiconQuery {
            sort: Some("gloss".to_owned()),
            ..LexiconQuery::default()
        }),
        vec!["tak", "kat"],
        "GO < STONE"
    );
}

/// 未知的 `sort` 值視為 `name`,不是錯誤——前端傳了新值不該讓畫面炸掉。
#[test]
fn an_unrecognised_sort_falls_back_to_name_order() {
    let temp = Temp::new("sort-fallback");
    let mut session = session(&temp);
    let odd = session
        .lexicon(&LexiconQuery {
            sort: Some("由我發明的排序".to_owned()),
            ..LexiconQuery::default()
        })
        .expect("不該是錯誤");
    assert_eq!(
        odd.lexicon
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        vec!["kat", "tak"]
    );
}

// ── 節點編輯頁:改的全是雜湊外的 ─────────────────────────────────────────

/// 🔑 **改 label / State / 旁註,節點身分與語言內容全都不動。**
///
/// 這是編輯頁存在的前提:它改的東西在 P64 的雜湊外槽位,所以**不會弄壞演化史**。
/// 判別性:若哪個欄位其實進了雜湊,`id` 就會變。
#[test]
fn editing_a_node_never_disturbs_its_identity_or_language() {
    let temp = Temp::new("edit");
    let mut session = session(&temp);
    let before = session.node_detail().expect("detail");
    let lexicon_before = session.lexicon(&LexiconQuery::default()).expect("lexicon");

    let renamed = session.set_label(Some("原始語".to_owned())).expect("label");
    assert_eq!(renamed.label.as_deref(), Some("原始語"));

    let state = EvolutionState {
        time: Some("約 800".to_owned()),
        region: Some("河谷北岸".to_owned()),
        contacts: vec![Contact {
            counterpart: "鄰語".to_owned(),
            period: None,
            intensity: ContactIntensity::Trade,
        }],
        ..EvolutionState::default()
    };
    let with_state = session.set_state(&state).expect("state");
    assert_eq!(with_state.state, state);

    let annotated = session
        .write_annotation("culture.md", "石頭象徵盟約")
        .expect("annotation");
    assert_eq!(annotated.annotations, vec!["culture.md"]);
    assert_eq!(
        session.read_annotation("culture.md").expect("read"),
        "石頭象徵盟約"
    );

    // 🔑 三樣都改完了,而**身分與語言內容逐項不變**
    assert_eq!(annotated.id, before.id, "節點 id 不變——那些欄位在雜湊外");
    assert_eq!(annotated.sign_count, before.sign_count);
    assert_eq!(
        session
            .lexicon(&LexiconQuery::default())
            .expect("lexicon")
            .lexicon,
        lexicon_before.lexicon,
        "詞典逐欄位不變"
    );
}

/// label 落盤了——重開仍在。
#[test]
fn a_renamed_node_keeps_its_name_after_reopening() {
    let temp = Temp::new("rename");
    let mut session = session(&temp);
    let id = session.node_detail().expect("detail").id;
    session.set_label(Some("原始語".to_owned())).expect("label");
    drop(session);

    let reopened = UiSession::open(&temp.0, LibrarySpec::default()).expect("reopen");
    let node = reopened
        .tree()
        .nodes
        .into_iter()
        .find(|n| n.id == id)
        .expect("節點還在");
    assert_eq!(node.label.as_deref(), Some("原始語"));
}

/// 沒開節點時,編輯頁的操作回明確的 code。
#[test]
fn editing_without_a_node_reports_a_code() {
    let temp = Temp::new("no-node");
    GraphStore::init(&temp.0).expect("init");
    let mut session = UiSession::open(&temp.0, LibrarySpec::default()).expect("open");

    for error in [
        session.node_detail().expect_err("detail"),
        session.set_label(None).expect_err("label"),
        session
            .set_state(&EvolutionState::default())
            .expect_err("state"),
        session.write_annotation("a.md", "x").expect_err("annotation"),
    ] {
        assert_eq!(error.code, "APP_NO_ACTIVE_NODE", "{error:?}");
    }
    // 但樹照樣給得出來(空的)——UI 開一個新專案時就是這個狀態
    assert!(session.tree().nodes.is_empty());
}

/// 錯誤的 `code` 取自既有錯誤字串的前綴,不另發明一套。
#[test]
fn error_codes_come_from_the_existing_diagnostic_convention() {
    let temp = Temp::new("codes");
    let error =
        UiSession::open(temp.0.join("nope"), LibrarySpec::default()).expect_err("不存在的專案");
    assert!(
        error.code.starts_with("PERSISTENCE_"),
        "沿用既有前綴而非新造:{error:?}"
    );
    assert!(!error.message.is_empty());
}
