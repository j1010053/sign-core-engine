//! 步驟 22 前置四裁定的出口(擁有者 2026-08-04)。
//!
//! 1. `NodeMetadataCommand` —— per-node 雜湊外中繼資料自成一類;
//! 2. `views/` 由 persistence 提供**資料層 API**,它不認得 `ViewCommand`;
//! 3. (Tauri 是選型,無測試);
//! 4. `CompileService` —— lazy / memory-only / 可丟棄,鍵涵蓋全部編譯輸入。

use conlang_app::compile::{CompileKey, CompileService};
use conlang_app::view::apply_view_command;
use conlang_changeset::state::{Contact, ContactIntensity, EvolutionState};
use conlang_command::{NodeMetadataCommand, ViewCommand};
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_persistence::{GraphStore, StoreError, ViewDocument};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const ONE: &str = "sign dog:\n    belongs Noun\n    phon:\n        /dog/\n";
const TWO: &str = "sign dog:\n    belongs Noun\n    phon:\n        /dok/\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-pre22-{name}-{}-{ordinal}",
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

fn document(source: &str, ns: &str) -> LanguageDocument {
    LanguageDocument::import_new_root(source, ns).expect("parses")
}

// ── ① NodeMetadataCommand ────────────────────────────────────────────────

/// 四個成員都是 per-node 雜湊外的,**沒有降階路徑**(型別上就沒有 `lower()`)。
///
/// 判別性靠對照:同一份意圖集合裡,只有 `LanguageCommand` 進得了 `.chg`。
#[test]
fn node_metadata_commands_carry_hash_external_intent_only() {
    let state = EvolutionState {
        time: Some("約 800".to_owned()),
        contacts: vec![Contact {
            counterpart: "neighbour".to_owned(),
            period: None,
            intensity: ContactIntensity::Trade,
        }],
        ..EvolutionState::default()
    };
    let commands = [
        NodeMetadataCommand::SetState(state.clone()),
        NodeMetadataCommand::SetLabel(Some("Proto".to_owned())),
        NodeMetadataCommand::SetPreference {
            key: "colour".to_owned(),
            value: "#883322".to_owned(),
        },
        NodeMetadataCommand::WriteAnnotation {
            path: "culture.md".to_owned(),
            content: "牧民用語".to_owned(),
        },
    ];
    // 可建構、可比較、彼此不同——四個槽位是四件事
    for (index, left) in commands.iter().enumerate() {
        assert_eq!(left, &left.clone());
        for right in &commands[index + 1..] {
            assert_ne!(left, right);
        }
    }
    assert!(matches!(&commands[0], NodeMetadataCommand::SetState(s) if s == &state));
}

/// State 往返 store,且**雜湊外**——改它不動 `manifest`。
#[test]
fn a_state_command_round_trips_through_the_hash_external_slot() {
    let temp = TempDir::new("state");
    let store = GraphStore::init(&temp.0).expect("init");
    let libraries = LibrarySpec::default();
    let mut graph = conlang_changeset::evolution::EvolutionGraph::new(libraries.clone());
    let root = graph.add_root(document(ONE, "pre:root")).expect("root");
    store.save(&graph).expect("save");

    let manifest_before = fs::read(temp.0.join("nodes").join(root.as_str()).join("manifest"))
        .expect("manifest");

    let NodeMetadataCommand::SetState(state) = NodeMetadataCommand::SetState(EvolutionState {
        region: Some("河谷".to_owned()),
        ..EvolutionState::default()
    }) else {
        unreachable!()
    };
    store.write_state(&root, &state).expect("write");

    assert_eq!(store.read_state(&root).expect("read"), state);
    assert_eq!(
        fs::read(temp.0.join("nodes").join(root.as_str()).join("manifest")).expect("manifest"),
        manifest_before,
        "State 是雜湊外的——manifest 不得改變"
    );
}

// ── ② views/ 是資料層 API,persistence 不認得 ViewCommand ────────────────

/// 🔑 **翻譯住 app,不住 persistence。**
///
/// `apply_view_command` 把意圖變成 `ViewDocument` 的修改;persistence 只負責
/// 讀寫那份資料。判別性:若哪天 persistence 直接吃 command,這個翻譯函數就會
/// 變成無人呼叫的死碼。
#[test]
fn view_commands_are_translated_by_the_app_and_stored_as_plain_data() {
    let temp = TempDir::new("views");
    let store = GraphStore::init(&temp.0).expect("init");

    let mut view = ViewDocument::default();
    let commands = [
        ViewCommand::SetViewConfig {
            view: "political".to_owned(),
            sort: "name".to_owned(),
        },
        ViewCommand::AssignGroup {
            view: "political".to_owned(),
            node: "abc".to_owned(),
            group: "bulgarian".to_owned(),
        },
        ViewCommand::LabelGroup {
            view: "political".to_owned(),
            group: "bulgarian".to_owned(),
            label: "保加利亞語群".to_owned(),
        },
    ];
    for command in &commands {
        assert_eq!(apply_view_command(&mut view, command), "political");
    }

    assert_eq!(view.sort.as_deref(), Some("name"));
    assert_eq!(view.assignments.get("abc").map(String::as_str), Some("bulgarian"));
    assert_eq!(view.labels.len(), 1);

    store.write_view("political", &view).expect("write");
    assert_eq!(store.read_view("political").expect("read"), view);
    assert_eq!(store.list_views().expect("list"), vec!["political"]);
}

/// R4 一套一檔:兩個視角互不污染。
#[test]
fn two_views_are_independent_files() {
    let temp = TempDir::new("two-views");
    let store = GraphStore::init(&temp.0).expect("init");

    let mut political = ViewDocument::default();
    apply_view_command(
        &mut political,
        &ViewCommand::AssignGroup {
            view: "political".to_owned(),
            node: "abc".to_owned(),
            group: "one".to_owned(),
        },
    );
    store.write_view("political", &political).expect("write");
    store
        .write_view("linguistic", &ViewDocument::default())
        .expect("write");

    assert!(store.read_view("linguistic").expect("read").assignments.is_empty());
    assert_eq!(store.read_view("political").expect("read"), political);
    assert_eq!(store.list_views().expect("list"), vec!["linguistic", "political"]);
}

/// 沒有的視角回**預設空值**,不是錯誤——UI 新建視角就是從這裡開始。
#[test]
fn an_unknown_view_reads_as_empty() {
    let temp = TempDir::new("missing-view");
    let store = GraphStore::init(&temp.0).expect("init");
    assert_eq!(store.read_view("nope").expect("read"), ViewDocument::default());
    assert!(store.list_views().expect("list").is_empty());
}

/// 🔑 視角名含路徑分隔或 `..` 必須被擋——否則寫得出專案根之外。
#[test]
fn a_view_name_that_escapes_the_project_root_is_refused() {
    let temp = TempDir::new("escape");
    let store = GraphStore::init(&temp.0).expect("init");
    for bad in ["../evil", "a/b", "", "."] {
        assert!(
            matches!(store.read_view(bad), Err(StoreError::InvalidViewName(_))),
            "{bad:?} 應被拒"
        );
        assert!(store.write_view(bad, &ViewDocument::default()).is_err());
    }
    // 正向控制組
    store
        .write_view("ok", &ViewDocument::default())
        .expect("正常名字要過");
}

// ── ④ CompileService ─────────────────────────────────────────────────────

/// lazy + 命中不重編。
#[test]
fn compiling_twice_reuses_the_cached_system() {
    let document = document(ONE, "pre:a");
    let libraries = LibrarySpec::default();
    let mut service = CompileService::new();
    assert!(service.is_empty(), "lazy:沒人問就不編譯");

    let first = service.get(&document, &libraries).expect("compile");
    let second = service.get(&document, &libraries).expect("compile");
    assert_eq!(service.stats(), (1, 1));
    assert!(std::sync::Arc::ptr_eq(&first, &second), "同一份,不是重編的");
}

/// 🔑 **鍵涵蓋文件內容**——換文件必須重編。
#[test]
fn a_different_document_is_compiled_separately() {
    let libraries = LibrarySpec::default();
    let mut service = CompileService::new();
    service.get(&document(ONE, "pre:a"), &libraries).expect("a");
    service.get(&document(TWO, "pre:a"), &libraries).expect("b");
    assert_eq!(service.len(), 2);
    assert_eq!(service.stats(), (0, 2));
}

/// 🔑 **鍵涵蓋 identity manifest**——同一份源文字、不同 identity 是不同輸入。
///
/// 判別性:只用 source digest 當鍵的實作在這裡會命中,而那是錯的
/// ——identity 決定 sign id,編譯產物因此不同。
#[test]
fn the_same_source_under_a_different_identity_is_a_different_key() {
    let libraries = LibrarySpec::default();
    let (a, b) = (document(ONE, "pre:a"), document(ONE, "pre:b"));
    assert_eq!(a.source(), b.source(), "前提:源文字逐字相同");

    let ka = CompileKey::of(&a, &libraries).expect("key");
    let kb = CompileKey::of(&b, &libraries).expect("key");
    assert_eq!(ka.document, kb.document, "內容 digest 相同");
    assert_ne!(ka.identities, kb.identities, "但 identity 不同");
    assert_ne!(ka, kb, "故整個鍵不同");

    let mut service = CompileService::new();
    service.get(&a, &libraries).expect("a");
    service.get(&b, &libraries).expect("b");
    assert_eq!(service.len(), 2, "必須各編一次");
}

/// 🔑 **鍵涵蓋 library lock**——換套件組合必須重編。
///
/// `std:core` 換了 ontology,整個閉包與投影都會變。
#[test]
fn a_different_library_selection_is_a_different_key() {
    let document = document(ONE, "pre:a");
    let full = LibrarySpec::default();
    let bare = LibrarySpec {
        std: Vec::new(),
        ..LibrarySpec::default()
    };

    let kf = CompileKey::of(&document, &full).expect("key");
    let kb = CompileKey::of(&document, &bare).expect("key");
    assert_ne!(kf.library_lock, kb.library_lock, "載入的套件不同");
    assert_ne!(kf, kb);
}

/// 🔑 **鍵涵蓋 compiler semantics version。**
#[test]
fn the_key_pins_the_compiler_semantics_version() {
    let key = CompileKey::of(&document(ONE, "pre:a"), &LibrarySpec::default()).expect("key");
    assert_eq!(key.semantics, conlang_language::COMPILER_SEMANTICS_VERSION);
    assert!(!key.semantics.is_empty(), "空字串等於沒放進鍵");
}

/// **可丟棄**:清空不影響正確性,只是下次重編(P8)。
#[test]
fn clearing_the_service_costs_a_recompile_but_never_correctness() {
    let document = document(ONE, "pre:a");
    let libraries = LibrarySpec::default();
    let mut service = CompileService::new();

    let before = service.get(&document, &libraries).expect("compile");
    service.clear();
    assert!(service.is_empty());
    assert!(service.peek(&document, &libraries).expect("peek").is_none());

    let after = service.get(&document, &libraries).expect("compile");
    assert!(!std::sync::Arc::ptr_eq(&before, &after), "確實重編了一份");
    // 而編譯結果等價:同樣的 sign、同樣的診斷數
    assert_eq!(
        before.effective_language().signs.len(),
        after.effective_language().signs.len()
    );
    assert_eq!(
        before.validation.diagnostics().len(),
        after.validation.diagnostics().len()
    );
}

/// 編譯失敗**不進快取**——否則使用者修好了仍看到舊錯誤。
#[test]
fn a_failed_compile_is_not_cached() {
    let broken = document(
        "sign x:\n    belongs NoSuchTraitAnywhere\n",
        "pre:broken",
    );
    let libraries = LibrarySpec::default();
    let mut service = CompileService::new();

    assert!(service.get(&broken, &libraries).is_err(), "前提:確實編不過");
    assert!(service.is_empty(), "失敗不得留在快取裡");
    assert_eq!(service.stats(), (0, 1));
}
