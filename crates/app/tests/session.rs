//! 步驟 21-3 出口:工作階段歷史、三條 undo 線、工作副本。
//!
//! 最要緊的兩條:
//!
//! - [`undo_retraces_navigation_not_the_graph_parent`] —— 唯一能區分
//!   「足跡」與「圖上父節點」的情境(姊妹分支間瀏覽);
//! - [`redo_after_a_branch_returns_to_where_the_user_actually_was`] ——
//!   分岔下 redo 根本無法靠圖走(`EvolutionGraph` 沒有 children 索引)。

use conlang_app::{AppError, Session};
use conlang_changeset::evolution::{EvolutionGraph, NodeId};
use conlang_changeset::rewrite::{AtomicRewrite, DonorScope, RuleHome, ServiceContext};
use conlang_changeset::{NodeUpdate, PrimitiveEdit};
use conlang_command::{lower, LanguageCommand};
use conlang_generate::Strategies;
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_persistence::GraphStore;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const BASE: &str = "Symbol k\nSymbol a\nSymbol t\nSymbol u\n\nClass vowel {a, u}\n\n\
global trait Core:\n\nsign old:\n    belongs Noun\n    phon:\n        /kat/\n";

const DUPLICATE_NAME_BASE: &str = "Symbol a\n\nClass vowel {a}\n\n\
sign a:\n    belongs Noun\n    phon:\n        /a/\n\n\
sign b:\n    belongs Noun\n    phon:\n        /a/\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-app-{name}-{}-{ordinal}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
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

fn session() -> (Session, NodeId) {
    let libraries = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(libraries.clone());
    let root = graph
        .add_root(LanguageDocument::import_new_root(BASE, "app:root").expect("root parses"))
        .expect("add_root");
    let mut session = Session::new(graph, libraries);
    session.open(&root).expect("open");
    (session, root)
}

fn duplicate_name_session() -> (Session, NodeId) {
    let libraries = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(libraries.clone());
    let root = graph
        .add_root(
            LanguageDocument::import_new_root(DUPLICATE_NAME_BASE, "app:duplicate")
                .expect("root parses"),
        )
        .expect("add_root");
    let mut session = Session::new(graph, libraries);
    session.open(&root).expect("open");
    (session, root)
}

/// 堆一條音變並提交,回傳新節點。
fn evolve(session: &mut Session, namespace: &str, body: &str) -> NodeId {
    session.begin_edit(namespace).expect("begin");
    let document = session.snapshot().expect("snapshot").clone();
    let rewrite = AtomicRewrite::SoundChange {
        home: RuleHome::Global("Core".to_owned()),
        body: body.to_owned(),
    };
    let edits = lower(
        &LanguageCommand::ApplyRewrite(&rewrite),
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("降階");
    session.stage(edits).expect("stage");
    session.commit(Some(namespace.to_owned())).expect("commit")
}

// ── (B) 演化 commit 的 undo/redo ─────────────────────────────────────────

/// 提交後 active 指標移到新節點,undo 退回父節點,redo 再回來。
#[test]
fn commit_moves_the_active_pointer_and_undo_walks_it_back() {
    let (mut session, root) = session();
    let child = evolve(&mut session, "app:a", "t => k");

    assert_eq!(session.active(), Some(&child));
    assert_eq!(session.undo().expect("undo"), root);
    assert_eq!(session.active(), Some(&root));
    assert_eq!(session.redo().expect("redo"), child);
    assert_eq!(session.active(), Some(&child));

    // 節點**不因 undo 消失**——撤銷的是「我在哪」,不是「它存不存在」
    assert!(session.graph().node(&child).is_some());
    assert_eq!(session.graph().len(), 2);
}

/// 🔑 驗證失敗不可以消耗工作副本；使用者必須能看見並修改原本那份 pending `.chg`。
#[test]
fn a_failed_commit_preserves_the_pending_changeset() {
    let (mut session, root) = duplicate_name_session();
    session.begin_edit("app:duplicate-name").expect("begin");
    let b = session
        .snapshot()
        .expect("snapshot")
        .ref_for_sign("b")
        .expect("b ref");
    session
        .stage(vec![PrimitiveEdit::Update {
            node: b,
            change: NodeUpdate::Rename("a".to_owned()),
        }])
        .expect("stage");
    let before = session.pending().expect("pending").dump();

    // `b → a` 會造成重名，因此 commit 必須失敗；這正是從前會把 pending take 掉的路徑。
    assert!(session.commit(None).is_err(), "重名不可提交");

    assert_eq!(
        session.pending().map(|pending| pending.dump()),
        Some(before),
        "失敗後必須保留可修正、可重試的原草稿"
    );
    assert_eq!(session.active(), Some(&root), "失敗不得移動 active");
    assert_eq!(session.graph().len(), 1, "失敗不得新增節點");
}

/// 🔑 **分岔之後 redo 回到使用者真正去過的那一支。**
///
/// 這是「undo = 移到 parent、redo = 移到 child」死掉的地方:root 有**兩個**
/// 子節點,圖上無從得知該回哪一個——而且 `EvolutionGraph` 根本沒有 children
/// 索引(`Node` 只有 `parents: Vec<Edge>`,節點 id 由 parents 的 id 算出)。
///
/// 只有工作階段的足跡知道答案。
#[test]
fn redo_after_a_branch_returns_to_where_the_user_actually_was() {
    let (mut session, root) = session();
    let first = evolve(&mut session, "app:a", "t => k");

    // 退回 root,往另一個方向再演化一次 ⇒ root 現在有兩個子節點
    session.undo().expect("undo");
    let second = evolve(&mut session, "app:b", "a => u");
    assert_ne!(first, second);

    // 前提:兩者都掛在 root 底下
    for id in [&first, &second] {
        let parents = session.graph().node(id).expect("在").parents();
        assert_eq!(parents.len(), 1);
        assert_eq!(&parents[0].from, &root, "都以 root 為父");
    }

    // undo 回 root、redo 回**第二支**——因為那才是使用者剛剛走的
    assert_eq!(session.undo().expect("undo"), root);
    assert_eq!(
        session.redo().expect("redo"),
        second,
        "redo 必須回到足跡上的那一支,不是任一個子節點"
    );
}

/// 走新的一步會**清空 redo**——否則 redo 會跳到一個沒去過的地方。
#[test]
fn committing_after_an_undo_clears_the_redo_stack() {
    let (mut session, _root) = session();
    evolve(&mut session, "app:a", "t => k");
    session.undo().expect("undo");
    evolve(&mut session, "app:b", "a => u");

    assert!(matches!(session.redo(), Err(AppError::NothingToRedo)));
}

/// 沒得 undo/redo 時是明確的錯誤,不是靜默的 no-op。
#[test]
fn undo_and_redo_report_when_there_is_nothing_to_do() {
    let (mut session, _root) = session();
    assert!(matches!(session.undo(), Err(AppError::NothingToUndo)));
    assert!(matches!(session.redo(), Err(AppError::NothingToRedo)));
}

// ── (A) 專案編輯的 undo 是另一條線 ───────────────────────────────────────

/// 🔑 **丟棄未提交的編輯不動 active 指標,也不進 history。**
///
/// 判別性:若把兩條線混成一條,`discard_last_edit` 之後 `undo` 就會少一格
/// (或 active 指標會亂跳)。這裡同時檢查兩者。
#[test]
fn discarding_an_uncommitted_edit_is_a_different_line_from_undo() {
    let (mut session, root) = session();
    let child = evolve(&mut session, "app:a", "t => k");

    session.begin_edit("app:draft").expect("begin");
    let document = session.snapshot().expect("snapshot").clone();
    let rewrite = AtomicRewrite::SoundChange {
        home: RuleHome::Global("Core".to_owned()),
        body: "k => a".to_owned(),
    };
    let edits = lower(
        &LanguageCommand::ApplyRewrite(&rewrite),
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("降階");
    session.stage(edits).expect("stage");
    assert_eq!(session.pending().expect("有 pending").statements.len(), 1);

    let dropped = session.discard_last_edit().expect("丟得掉");
    assert!(dropped.is_some());
    assert!(session.pending().expect("仍在").statements.is_empty());

    // active 沒動、圖沒長大
    assert_eq!(session.active(), Some(&child), "丟未提交的編輯不動指標");
    assert_eq!(session.graph().len(), 2, "也沒有多出節點");

    // 而 (B) 那條線仍完整:undo 照樣退得回 root
    assert_eq!(session.undo().expect("undo"), root);
}

/// 空的 pending 丟不出東西,但**不是錯誤**。
#[test]
fn discarding_from_an_empty_buffer_yields_none() {
    let (mut session, _root) = session();
    session.begin_edit("app:draft").expect("begin");
    assert!(session.discard_last_edit().expect("不是錯誤").is_none());
}

/// 沒開節點就編輯 → 明確錯誤。
#[test]
fn editing_without_an_active_node_is_refused() {
    let libraries = LibrarySpec::default();
    let mut session = Session::new(EvolutionGraph::new(libraries.clone()), libraries);
    assert!(matches!(
        session.begin_edit("x"),
        Err(AppError::NoActiveNode)
    ));
    assert!(matches!(
        session.stage(Vec::<PrimitiveEdit>::new()),
        Err(AppError::NoActiveNode)
    ));
}

// ── 工作副本:是一份普通的 `.chg`,不是 app 自創格式 ────────────────────

/// 🔑 **存出來的工作副本可被 `UnresolvedChangeSet::parse` 讀回。**
///
/// 這是 §2.2「app 不得自行定義第二套檔案格式」的出口。判別性:若 app 改用
/// 自己的序列化(JSON 之類),下面的 parse 會失敗。
#[test]
fn the_working_copy_is_an_ordinary_chg_file() {
    let temp = TempDir::new("working");
    let (mut session, _root) = session();
    session.begin_edit("app:draft").expect("begin");

    let document = session.snapshot().expect("snapshot").clone();
    let rewrite = AtomicRewrite::SoundChange {
        home: RuleHome::Global("Core".to_owned()),
        body: "t => k".to_owned(),
    };
    let edits = lower(
        &LanguageCommand::ApplyRewrite(&rewrite),
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("降階");
    session.stage(edits).expect("stage");

    let path = temp.0.join("draft.chg");
    session.save_working_copy(&path).expect("save");

    // 由**既有**的 parser 讀得回來 —— 沒有第二套格式
    let text = fs::read_to_string(&path).expect("read");
    assert!(
        text.starts_with("changeset "),
        "就是 .chg 的表面語法:{text}"
    );
    conlang_changeset::UnresolvedChangeSet::parse(&text).expect("既有 parser 讀得回");

    // 而 Session 自己也讀得回,內容一致
    session.begin_edit("app:draft").expect("重開一份空的");
    assert!(session.pending().expect("空的").statements.is_empty());
    session.load_working_copy(&path).expect("load");
    assert_eq!(
        session.pending().expect("讀回來").statements.len(),
        1,
        "存進去的那條 statement 要回得來"
    );
}

/// 讀回的工作副本仍能提交,產物與沒存過檔一致。
#[test]
fn a_reloaded_working_copy_still_commits() {
    let temp = TempDir::new("reload");
    let (mut session, _root) = session();
    session.begin_edit("app:draft").expect("begin");
    let document = session.snapshot().expect("snapshot").clone();
    let rewrite = AtomicRewrite::SoundChange {
        home: RuleHome::Global("Core".to_owned()),
        body: "t => k".to_owned(),
    };
    let edits = lower(
        &LanguageCommand::ApplyRewrite(&rewrite),
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("降階");
    session.stage(edits).expect("stage");
    let path = temp.0.join("draft.chg");
    session.save_working_copy(&path).expect("save");

    session.begin_edit("app:draft").expect("清掉");
    session.load_working_copy(&path).expect("load");
    let id = session.commit(Some("reloaded".to_owned())).expect("commit");

    let source = session.graph().snapshot(&id).expect("snapshot").source();
    assert!(source.contains("t => k"), "存檔往返不改變產物:{source}");
}

#[test]
fn save_as_writes_the_supplied_editor_version_and_replaces_pending() {
    let temp = TempDir::new("save-visible");
    let (mut session, _root) = session();
    session.begin_edit("app:visible").expect("begin visible");
    let document = session.snapshot().expect("snapshot").clone();
    let edits = lower(
        &LanguageCommand::ApplyRewrite(&AtomicRewrite::SoundChange {
            home: RuleHome::Global("Core".to_owned()),
            body: "t => k".to_owned(),
        }),
        &document,
        &Strategies::default(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("lower");
    session.stage(edits).expect("stage visible");
    let visible = session.pending_source().expect("visible source");

    session.begin_edit("app:stale").expect("replace with stale");
    let stale = session.pending_source().expect("stale source");
    assert_ne!(
        visible, stale,
        "counterexample requires two distinct versions"
    );

    let path = temp.0.join("visible.chg");
    session
        .save_working_copy_source(&path, &visible)
        .expect("save visible source");

    let saved = fs::read_to_string(path).expect("read saved source");
    assert_eq!(saved, session.pending_source().expect("new pending"));
    assert_ne!(saved, stale, "Save As must not serialize stale pending");
    assert!(saved.contains("t => k"));
}

#[test]
fn failed_editor_save_as_keeps_both_file_and_pending_unchanged() {
    let temp = TempDir::new("save-visible-failure");
    let (mut session, _root) = session();
    session.begin_edit("app:stale").expect("begin stale");
    let stale = session.pending_source().expect("stale source");
    let path = temp.0.join("draft.chg");
    fs::write(&path, "existing file").expect("seed file");

    assert!(session
        .save_working_copy_source(&path, "not a .chg")
        .is_err());
    assert_eq!(
        fs::read_to_string(&path).expect("read file"),
        "existing file"
    );
    assert_eq!(session.pending_source().expect("pending"), stale);

    // A valid source paired with an unwritable target must also leave pending
    // unchanged. A directory is not a writable file on both supported hosts.
    assert!(session.save_working_copy_source(&temp.0, &stale).is_err());
    assert_eq!(
        session.pending_source().expect("pending after I/O error"),
        stale
    );
}

// ── 落盤 ─────────────────────────────────────────────────────────────────

/// 整個工作階段落盤後讀得回來,節點數一致。
#[test]
fn the_session_graph_round_trips_through_the_store() {
    let temp = TempDir::new("persist");
    let store = GraphStore::init(temp.0.join("store")).expect("init");
    let (mut session, root) = session();
    let child = evolve(&mut session, "app:a", "t => k");

    session.persist(&store).expect("persist");
    let reloaded = store.load(LibrarySpec::default()).expect("load");
    assert_eq!(reloaded.len(), 2);
    assert!(reloaded.node(&root).is_some());
    assert!(reloaded.node(&child).is_some());
}

/// 🔑 **undo 退回「上一個看的節點」,不是「圖上的父節點」。**
///
/// 前一條分岔測試只證得了 **redo**——那些節點的 history 頂端剛好等於 parent,
/// 所以「undo = 走 `parents[0]`」在那裡給出相同答案,**活過了突變**。
///
/// 真正分得開的是**瀏覽**:在兩支姊妹分支之間切換後,
///
/// ```text
/// root ── A        足跡:… → A → B
///      └─ B        B 的圖上父節點是 root,但上一個看的是 A
/// ```
///
/// undo 必須回 **A**。走 `parents[0]` 會回 root ——那是使用者沒去過的地方。
#[test]
fn undo_retraces_navigation_not_the_graph_parent() {
    let (mut session, root) = session();
    let first = evolve(&mut session, "app:a", "t => k");
    session.undo().expect("回 root");
    let second = evolve(&mut session, "app:b", "a => u");

    // 前提:兩支的圖上父節點都是 root,彼此不是對方的父
    for id in [&first, &second] {
        let parents = session.graph().node(id).expect("在").parents();
        assert_eq!(&parents[0].from, &root);
    }

    // 瀏覽:A → B(不是演化,只是切換視角)
    session.open(&first).expect("open A");
    session.open(&second).expect("open B");
    assert_eq!(session.active(), Some(&second));

    assert_eq!(
        session.undo().expect("undo"),
        first,
        "必須回上一個**看過**的節點 A;走 parents[0] 會錯回 root"
    );
}

/// `open` 一個不存在的節點是錯誤,不是靜默切換。
#[test]
fn opening_an_unknown_node_is_refused() {
    let (mut session, root) = session();
    let bogus = NodeId::parse("0".repeat(64)).expect("形式合法");
    assert!(session.open(&bogus).is_err());
    assert_eq!(session.active(), Some(&root), "失敗不得動到 active");
}

/// **瀏覽也會清空 redo**——`open` 等同於瀏覽器裡「點了新連結」。
///
/// undo 到 root 之後若改去看別的節點,原本那條 forward 路徑已不是使用者
/// 想回去的地方;留著會讓 redo 跳到一個與當前脈絡無關的節點。
///
/// 判別性:`open` 少了 `redo.clear()` 時,下面的 `redo()` 會成功回到
/// 那個已被放棄的分支。
#[test]
fn navigating_elsewhere_also_clears_the_redo_path() {
    let (mut session, root) = session();
    let first = evolve(&mut session, "app:a", "t => k");
    session.undo().expect("回 root");
    assert_eq!(session.active(), Some(&root));

    // 此時 redo 指著 first;改去看 first 以外的地方(這裡就用 root 自己的另一支)
    let second = evolve(&mut session, "app:b", "a => u");
    session.undo().expect("回 root");
    session.open(&first).expect("改去看另一支");

    assert!(
        matches!(session.redo(), Err(AppError::NothingToRedo)),
        "瀏覽之後不得還能 redo 回 {second:?}"
    );
}
