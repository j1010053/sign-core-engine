//! `project.toml`(R3 import 表)出口:**開啟專案不再等於開啟 store 目錄**。
//!
//! 核心那條是 [`a_project_that_declares_a_natural_package_opens_where_the_default_fails`]
//! ——它直接對照「有宣告」與「沒宣告」兩種結果,而後者正是修這件事之前的行為。

use conlang_app::{AppError, Session};
use conlang_changeset::evolution::EvolutionGraph;
use conlang_language::{compile_with_libraries_ref, LanguageDocument, LibraryId, LibraryKind, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument, ProjectPackages, StoreError};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// 用了 `natural:en-standard` 才有的 trait。
const NEEDS_ENGLISH: &str = "sign s:\n    belongs EnglishCaseBearer\n";
const PLAIN: &str = "sign dog:\n    belongs Noun\n    phon:\n        /dog/\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-project-{name}-{}-{ordinal}",
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

fn english_spec() -> LibrarySpec {
    LibrarySpec {
        natural: Some(LibraryId::new(LibraryKind::Natural, "en-standard")),
        ..LibrarySpec::default()
    }
}

// ── 核心:宣告改變得了結果 ───────────────────────────────────────────────

/// 🔑 **宣告了英語套件的專案編得過;沒宣告的編不過,而且訊息叫你去改 import 表。**
///
/// 後者正是這次修改之前的**唯一**行為——UI 只能傳 `LibrarySpec::default()`。
#[test]
fn a_project_that_declares_a_natural_package_opens_where_the_default_fails() {
    let language = conlang_language::Language::parse(NEEDS_ENGLISH).expect("parses");

    // 沒宣告(= 修改前的行為)
    let error = compile_with_libraries_ref(&language, &LibrarySpec::default())
        .expect_err("預設組合下應失敗");
    let text = format!("{error:?}");
    assert!(text.contains("EnglishCaseBearer"), "{text}");
    assert!(
        text.contains("add it to your import table"),
        "訊息會叫使用者去改 import 表——那個表此前並不存在:{text}"
    );

    // 宣告了就過
    compile_with_libraries_ref(&language, &english_spec()).expect("宣告英語套件後應編得過");
}

/// `project.toml` 往返:寫出去、讀回來、翻成 `LibrarySpec` 都一致。
#[test]
fn a_project_declaration_round_trips_through_toml() {
    let temp = TempDir::new("roundtrip");
    let store = GraphStore::init(&temp.0).expect("init");

    let project = ProjectDocument {
        name: Some("tshiatun".to_owned()),
        default_view: Some("linguistic".to_owned()),
        packages: ProjectPackages {
            std: vec!["std:core".to_owned(), "std:cxg".to_owned()],
            natural: Some("natural:en-standard".to_owned()),
            plugins: Vec::new(),
        },
        weights: [("k".to_owned(), 0.8)].into_iter().collect(),
    };
    store.write_project(&project).expect("write");

    // 確實是人看得懂的 TOML
    let text = fs::read_to_string(temp.0.join("project.toml")).expect("read");
    assert!(text.contains("name = \"tshiatun\""), "{text}");
    assert!(text.contains("[packages]"), "{text}");
    assert!(text.contains("[weights]"), "{text}");

    assert_eq!(store.read_project().expect("read"), Some(project.clone()));
    let spec = project.to_spec().expect("翻譯");
    assert_eq!(spec.natural, english_spec().natural);
    assert_eq!(spec.std.len(), 2);
}

/// 🔑 **沒有 `project.toml` 時退回 fallback,而不是失敗或空集合。**
///
/// 既有的 store 目錄必須照樣打得開——升級不製造遷移斷點。
#[test]
fn a_store_without_a_project_file_falls_back_instead_of_failing() {
    let temp = TempDir::new("no-project");
    let store = GraphStore::init(&temp.0).expect("init");
    assert_eq!(store.read_project().expect("read"), None, "沒有就是 None");

    let spec = store
        .library_spec_or(english_spec())
        .expect("退回 fallback");
    assert_eq!(spec.natural, english_spec().natural);
    assert!(!spec.std.is_empty());
}

/// 🔑 **「空宣告」與「沒有宣告」必須分得開。**
///
/// 空的 `std = []` 表示**明確不載入任何 std**(R12 剛做出來的能力);
/// 沒有檔案才表示「用呼叫端的預設」。判別性:兩者若混為一談,升級後既有的
/// store 目錄會突然變成「什麼都不載入」。
#[test]
fn an_empty_declaration_means_load_nothing_not_use_defaults() {
    let temp = TempDir::new("empty-decl");
    let store = GraphStore::init(&temp.0).expect("init");

    store
        .write_project(&ProjectDocument::default())
        .expect("write");
    let spec = store.library_spec_or(LibrarySpec::default()).expect("spec");
    assert!(spec.std.is_empty(), "明確宣告不載入任何 std");
    assert_eq!(spec.natural, None);

    // 對照:把檔案刪掉就回到 fallback
    fs::remove_file(temp.0.join("project.toml")).expect("remove");
    let fallback = store.library_spec_or(LibrarySpec::default()).expect("spec");
    assert!(!fallback.std.is_empty(), "沒有檔案 ⇒ 用預設");
}

/// 壞掉的套件 id 硬錯,不靜默略過。
#[test]
fn a_malformed_package_id_is_rejected() {
    let project = ProjectDocument {
        packages: ProjectPackages {
            std: vec!["no-colon-here".to_owned()],
            ..ProjectPackages::default()
        },
        ..ProjectDocument::default()
    };
    assert!(matches!(
        project.to_spec(),
        Err(StoreError::InvalidPackageId(id)) if id == "no-colon-here"
    ));

    // 正向控制組
    ProjectDocument {
        packages: ProjectPackages {
            std: vec!["std:core".to_owned()],
            ..ProjectPackages::default()
        },
        ..ProjectDocument::default()
    }
    .to_spec()
    .expect("正常 id 要過");
}

/// 畸形的 TOML 硬錯,而非當成空宣告。
#[test]
fn a_malformed_project_file_is_an_error_not_an_empty_declaration() {
    let temp = TempDir::new("bad-toml");
    let store = GraphStore::init(&temp.0).expect("init");
    fs::write(temp.0.join("project.toml"), "this is not = = toml").expect("write");
    assert!(matches!(
        store.read_project(),
        Err(StoreError::ProjectFormat(_))
    ));
}

// ── Session::open_project ────────────────────────────────────────────────

/// 🔑 **開專案 = 讀宣告 → 依宣告載入 → 停在第一個 root。**
#[test]
fn opening_a_project_uses_the_declared_packages_and_lands_on_a_root() {
    let temp = TempDir::new("open");
    let store = GraphStore::init(&temp.0).expect("init");

    let libraries = english_spec();
    let mut graph = EvolutionGraph::new(libraries.clone());
    let root = graph
        .add_root(LanguageDocument::import_new_root(PLAIN, "proj:root").expect("root"))
        .expect("add_root");
    store.save(&graph).expect("save");
    store
        .write_project(&ProjectDocument::from_spec(&libraries))
        .expect("write project");

    let (session, project) =
        Session::open_project(&store, LibrarySpec::default()).expect("open");

    assert_eq!(session.active(), Some(&root), "停在 root");
    assert_eq!(
        session.libraries().natural,
        english_spec().natural,
        "用的是宣告裡的套件,不是 fallback"
    );
    assert!(project.is_some());
    // 存檔時寫回去的宣告與開啟時用的一致——不會存出一份自己開不起來的
    assert_eq!(
        ProjectDocument::from_spec(session.libraries()).packages,
        ProjectDocument::from_spec(&libraries).packages
    );
}

/// 沒有 `project.toml` 的舊 store 照樣開得起來。
#[test]
fn an_old_store_without_a_project_file_still_opens() {
    let temp = TempDir::new("legacy");
    let store = GraphStore::init(&temp.0).expect("init");
    let libraries = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(libraries.clone());
    let root = graph
        .add_root(LanguageDocument::import_new_root(PLAIN, "proj:root").expect("root"))
        .expect("add_root");
    store.save(&graph).expect("save");

    let (session, project) = Session::open_project(&store, libraries).expect("open");
    assert!(project.is_none(), "沒有宣告檔");
    assert_eq!(session.active(), Some(&root));
}

/// 空圖不開任何節點,但也不是錯誤——新建的專案就是這樣。
#[test]
fn an_empty_project_opens_with_no_active_node() {
    let temp = TempDir::new("empty");
    let store = GraphStore::init(&temp.0).expect("init");
    let (mut session, _) = Session::open_project(&store, LibrarySpec::default()).expect("open");
    assert_eq!(session.active(), None);
    assert!(matches!(session.begin_edit("x"), Err(AppError::NoActiveNode)));
}
