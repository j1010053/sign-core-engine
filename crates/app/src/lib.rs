//! 資訊流 D 的 **App 層**——薄有狀態殼(鳥瞰步驟 21;流 D 框架 §2、§5)。
//!
//! # 職責邊界:協調,不是第二個 persistence
//!
//! `conlang-persistence`(P60/P64)**擁有**檔案格式、fs I/O、驗證與交易。
//! 本 crate 只做協調(orchestration/facade):
//!
//! ```text
//! conlang-persistence   格式、I/O、驗證、交易        ← 唯一擁有者
//! conlang-app           串接 persistence/query/command;工作階段歷史
//!                       **不得自行定義第二套檔案格式或 loader**
//! ```
//!
//! 工作副本因此是一份**普通的 `.chg`**(`ResolvedChangeSet::dump()` /
//! `UnresolvedChangeSet::parse()`),不是 app 自創的格式——見 [`Session::save_working_copy`]。
//!
//! # 唯一不受 §4 約束的 crate
//!
//! 其餘 crate(`language`/`changeset`/`generate`/`stats`/`query`/`command`)
//! 一律無 fs、wasm 綠。fs 集中在這裡與 `persistence`。
//!
//! # Undo/Redo 是工作階段歷史,不是演化圖邊的反向遍歷
//!
//! 流 D 框架 §5.1 的實測理由:`EvolutionGraph` 的節點**只有 parents,
//! 沒有 children 索引**,且節點 id 由 parents 的 id 算出。故:
//!
//! - **redo 不可能靠圖走**——「子節點」不是可查詢的方向,要找得掃全圖;
//! - **undo 也已多選一**——`parents` 是 `Vec`(全 parent merge)。
//!
//! 所以 [`Session`] 自己維護 `history`/`redo` 兩個堆疊,存的是**走過的節點 id**。
//! immutable node 的紅利是 undo **不需要複製 Language 快照**,但那不等於
//! 不需要歷史。
//!
//! # 三條 undo 線,不是同一條
//!
//! | | 活動 | undo |
//! |---|---|---|
//! | (A) | 專案編輯(加詞、改 gloss) | 編輯開著的 `.chg` —— [`Session::discard_last_edit`] |
//! | (B) | 演化 commit | [`Session::undo`] / [`Session::redo`] |
//! | | `views/`、`data/` | 一般文件編輯歷史,不在本型別上 |

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod cache;
pub mod compile;
pub mod view;
pub mod workspace;

pub use cache::{ContentDigest, DiffKey, LexiconKey, QueryCache};
pub use compile::{CompileKey, CompileService, CompileServiceError};
pub use view::apply_view_command;
pub use workspace::Workspace;

use conlang_changeset::evolution::{Edge, EvolutionError, EvolutionGraph, Nativization, NodeId};
use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, PrimitiveEdit, ReplayError, ResolvedChangeSet,
    ResolvedStatement, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument, StoreError};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Evolution(#[from] EvolutionError),
    #[error(transparent)]
    Replay(#[from] ReplayError),
    #[error(transparent)]
    Compile(#[from] crate::compile::CompileServiceError),
    /// `project.toml` 宣告的套件解析不開。
    #[error("APP_LIBRARY: {0}")]
    Library(conlang_language::LibraryLoadError),
    #[error("APP_IO: {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("APP_NO_ACTIVE_NODE: open a node before editing")]
    NoActiveNode,
    #[error("APP_NOTHING_TO_UNDO")]
    NothingToUndo,
    #[error("APP_NOTHING_TO_REDO")]
    NothingToRedo,
}

/// 一次工作階段。**唯一有狀態的東西。**
#[derive(Debug)]
pub struct Session {
    graph: EvolutionGraph,
    libraries: LibrarySpec,
    /// 目前正在看/編輯的節點。
    active: Option<NodeId>,
    /// 尚未提交的編輯——**一份寫到一半的 `.chg`**(§5.2)。
    pending: Option<ResolvedChangeSet>,
    /// 走過的節點。**不是**演化圖的邊,是使用者這次的足跡。
    history: Vec<NodeId>,
    /// 被 undo 掉的,供 redo 取回。
    redo: Vec<NodeId>,
}

impl Session {
    /// 開啟一個專案目錄。
    ///
    /// **這是 UI 的入口**,取代此前的「開啟 store 目錄 + 自己猜 `LibrarySpec`」。
    /// 流程:
    ///
    /// 1. 讀 `project.toml` 得出要載入的套件(R3 的 import 表);
    /// 2. **沒有那個檔案時退回 `fallback`** —— 既有的 store 目錄照樣打得開,
    ///    不製造遷移斷點;
    /// 3. 以該組套件 `load` 演化圖(replay/fsck 在 persistence 側做);
    /// 4. active 指標停在**第一個 root**(依 id 序,故決定性);圖是空的就不開。
    ///
    /// 回傳專案宣告本身,好讓 UI 拿到 `name` / `default_view`。
    pub fn open_project(
        store: &GraphStore,
        fallback: LibrarySpec,
    ) -> Result<(Session, Option<ProjectDocument>), AppError> {
        let project = store.read_project()?;
        let libraries = match &project {
            Some(declared) => declared.to_spec()?,
            None => fallback,
        };
        // **宣告必須當場解析得開。** 只有一個 root 的專案沒有任何 changeset 要
        // replay,`load` 因此完全不碰套件組合——打錯一個套件名會一路安靜到使用者
        // 去查詢時才炸在一個看不懂的地方。錯誤要報在使用者能處理的位置。
        conlang_language::library::embedded_catalog()
            .and_then(|catalog| catalog.select(&libraries))
            .map_err(AppError::Library)?;
        let graph = store.load(libraries.clone())?;
        let first_root = graph.roots().next().cloned();
        let mut session = Session::new(graph, libraries);
        if let Some(root) = first_root {
            session.open(&root)?;
        }
        Ok((session, project))
    }

    pub fn new(graph: EvolutionGraph, libraries: LibrarySpec) -> Session {
        Session {
            graph,
            libraries,
            active: None,
            pending: None,
            history: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn graph(&self) -> &EvolutionGraph {
        &self.graph
    }

    pub fn active(&self) -> Option<&NodeId> {
        self.active.as_ref()
    }

    pub fn pending(&self) -> Option<&ResolvedChangeSet> {
        self.pending.as_ref()
    }

    /// 切到某個節點。**清空 redo**——分岔之後舊的 redo 路徑已無意義,
    /// 留著會讓「redo 跳到一個我沒去過的地方」。同一般編輯器的作法。
    pub fn open(&mut self, id: &NodeId) -> Result<(), AppError> {
        if self.graph.node(id).is_none() {
            return Err(EvolutionError::UnknownNode(id.clone()).into());
        }
        if let Some(previous) = self.active.replace(id.clone()) {
            self.history.push(previous);
        }
        self.redo.clear();
        self.pending = None;
        Ok(())
    }

    /// 目前節點的語言快照。
    pub fn snapshot(&self) -> Result<&LanguageDocument, AppError> {
        let id = self.active.as_ref().ok_or(AppError::NoActiveNode)?;
        Ok(self.graph.snapshot(id)?)
    }

    /// 開一份空的 pending `.chg`(基底 = 目前節點),namespace 由呼叫端給。
    ///
    /// 零條 statement 的 `.chg` 合法,故這一步就已經是個完整的檔案。
    pub fn begin_edit(&mut self, namespace: &str) -> Result<(), AppError> {
        let document = self.snapshot()?.clone();
        let prelude = change_set_prelude(&document, &self.libraries, namespace)?;
        let resolved = UnresolvedChangeSet::parse(&prelude)?.resolve(&document, &self.libraries)?;
        self.pending = Some(resolved);
        Ok(())
    }

    /// 把一批原語堆成一個 statement(交易單位)。委派給 `conlang-command`。
    pub fn stage(&mut self, edits: Vec<PrimitiveEdit>) -> Result<u64, AppError> {
        let pending = self.pending.as_mut().ok_or(AppError::NoActiveNode)?;
        Ok(conlang_command::stage(pending, edits))
    }

    /// **(A) 專案編輯的 undo**:丟棄最後一個未提交的 statement。
    ///
    /// 這條線與 [`undo`](Self::undo) **無關**——它不動 active 指標、
    /// 不進 history 堆疊,因為它還沒產生節點。
    pub fn discard_last_edit(&mut self) -> Result<Option<ResolvedStatement>, AppError> {
        let pending = self.pending.as_mut().ok_or(AppError::NoActiveNode)?;
        Ok(conlang_command::discard_last(pending))
    }

    /// 提交:pending `.chg` → 新節點。**提交邊界是整份 changeset,不是單一 command。**
    ///
    /// 提交後 active 指標移到新節點,舊節點入 history、redo 清空。
    pub fn commit(&mut self, label: Option<String>) -> Result<NodeId, AppError> {
        let parent = self.active.clone().ok_or(AppError::NoActiveNode)?;
        let pending = self.pending.take().ok_or(AppError::NoActiveNode)?;
        let document = self.graph.snapshot(&parent)?.clone();

        // replay 一次以取得 `.chg` 原文所需的降階結果;節點的 snapshot 由
        // `EvolutionGraph::commit` 自己重放邊上的 changeset 產生(P56 因果契約)。
        let dumped = pending.dump();
        ChangeInterpreter::new(document, self.libraries.clone(), pending.namespace.clone())?
            .run(&pending)?;

        let id = self.graph.commit(
            vec![Edge::trunk(parent.clone(), dumped)],
            Nativization::None,
            label,
        )?;
        self.history.push(parent);
        self.redo.clear();
        self.active = Some(id.clone());
        Ok(id)
    }

    /// **(B) 演化 commit 的 undo**:退回上一個走過的節點。
    ///
    /// **不是** `graph.parent`——見模組文件。分岔之後 parent 有多個、
    /// child 根本查不到,只有工作階段的足跡知道「上一步在哪」。
    ///
    /// 節點本身**不刪除**。要真的移除得顯式呼叫
    /// `EvolutionGraph::remove_node` + `GraphStore::remove_node`(只准葉節點)。
    pub fn undo(&mut self) -> Result<NodeId, AppError> {
        let previous = self.history.pop().ok_or(AppError::NothingToUndo)?;
        if let Some(current) = self.active.replace(previous.clone()) {
            self.redo.push(current);
        }
        self.pending = None;
        Ok(previous)
    }

    pub fn redo(&mut self) -> Result<NodeId, AppError> {
        let next = self.redo.pop().ok_or(AppError::NothingToRedo)?;
        if let Some(current) = self.active.replace(next.clone()) {
            self.history.push(current);
        }
        self.pending = None;
        Ok(next)
    }

    // ── 落盤(唯一碰 fs 的地方,且格式一律借用既有的)──────────────────

    /// 存 pending 為一份 `.chg` 檔。
    ///
    /// **不是新格式**:內容就是 `ResolvedChangeSet::dump()`,可被
    /// `UnresolvedChangeSet::parse` 讀回。app 不擁有格式(§2.2)。
    pub fn save_working_copy(&self, path: &Path) -> Result<(), AppError> {
        let pending = self.pending.as_ref().ok_or(AppError::NoActiveNode)?;
        std::fs::write(path, pending.dump()).map_err(|source| AppError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    /// 讀回一份工作副本。基底必須是目前節點——三道 digest 會在 resolve 時把關。
    pub fn load_working_copy(&mut self, path: &Path) -> Result<(), AppError> {
        let text = std::fs::read_to_string(path).map_err(|source| AppError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let document = self.snapshot()?.clone();
        let resolved =
            UnresolvedChangeSet::parse(&text)?.resolve(&document, &self.libraries)?;
        self.pending = Some(resolved);
        Ok(())
    }

    /// 把圖寫進 store。`save` 是 append-only 且**不靜默**——store 有而圖沒有的
    /// 節點會回 `StoreError::StaleNode`,指向 `remove_node`。
    pub fn persist(&self, store: &GraphStore) -> Result<(), AppError> {
        Ok(store.save(&self.graph)?)
    }

    /// 這個工作階段實際載入的套件組合。
    ///
    /// UI 存檔時以此寫 `project.toml`,故「開啟時載了什麼」與「存檔時宣告什麼」
    /// 是同一份——不會存出一份自己開不起來的宣告。
    pub fn libraries(&self) -> &LibrarySpec {
        &self.libraries
    }
}
