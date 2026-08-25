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
pub mod ipc;
pub mod view;
pub mod wire;
pub mod workspace;

pub use cache::{ContentDigest, DiffKey, LexiconKey, QueryCache};
pub use compile::{CompileKey, CompileService, CompileServiceError};
pub use ipc::{
    BodyItemInput, GroupingQuery, LexiconQuery, MovePlacementInput, PackageSelectionInput,
    ProjectSlot, ProposalQuery, SegmentWeight, SoundChangeInput, StructuredEdit,
    StructuredEditInput, UiError, UiSession,
};
pub use view::apply_view_command;
pub use wire::{
    AuthoringCatalogV1, AuthoringChoiceV1, AuthoringFieldV1, AuthoringMoveOptionV1,
    AuthoringMoveOptionsV1, AuthoringNodeV1, AuthoringSignV1, AuthoringTraitV1, CatalogPackageV1,
    DerivationViewV1, DiffSummaryV1, EvolutionTreeV1, GroupingOverrideV1, GroupingViewV1,
    IntelligibilityViewV1, LexiconViewV1, NodeDetailV1, PackageCatalogV1, PendingChangeV1,
    ProjectSummaryV1, ProposalV1, ProposalsViewV1, RebasePreviewV1, SegmentStatV1,
    SourceReconcileV1, SourceViewV1, StatsViewV1, TreeEdgeV1, TreeNodeV1, WeightConfigV1,
    WeightEntryV1, UI_SCHEMA_V1,
};
pub use workspace::Workspace;

use conlang_changeset::diff::{diff_vector, DiffVector};
use conlang_changeset::evolution::{Edge, EvolutionError, EvolutionGraph, Nativization, NodeId};
use conlang_changeset::{
    change_set_prelude, change_set_prelude_with_packages, ChangeInterpreter, PrimitiveEdit,
    ReplayError, ResolvedChangeSet, ResolvedStatement, UnresolvedChangeSet,
};
use conlang_language::{
    LanguageDocument, LibrarySpec, PackageSources, PackageSpec, ResolvedPackages,
};
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
    /// pending 裡有一種 `.chg` 排不出來的編輯。**寧可存不了,也不能存出一份
    /// 少了那條編輯的檔案**——後者看起來完全正常,replay 回來卻不是同一份文件。
    #[error(transparent)]
    Dump(#[from] conlang_changeset::DumpError),
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
    #[error(
        "APP_PACKAGE_CONTEXT_MISSING: a resolved Session requires an EvolutionGraph built with ResolvedPackages"
    )]
    PackageContextMissing,
    #[error(
        "APP_PACKAGE_CONTEXT_MISMATCH: Session and EvolutionGraph package intent or exact resolution differ"
    )]
    PackageContextMismatch,
}

/// 一次工作階段。**唯一有狀態的東西。**
#[derive(Debug)]
pub struct Session {
    graph: EvolutionGraph,
    /// Legacy compatibility projection. Canonical package-loader v2 consumers
    /// use `packages`; old API callers retain their exact `LibrarySpec`.
    libraries: LibrarySpec,
    packages: Option<ResolvedPackages>,
    /// 目前正在看/編輯的節點。
    active: Option<NodeId>,
    /// 尚未提交的編輯——**一份寫到一半的 `.chg`**(§5.2)。
    pending: Option<ResolvedChangeSet>,
    /// 走過的節點。**不是**演化圖的邊,是使用者這次的足跡。
    history: Vec<NodeId>,
    /// 被 undo 掉的,供 redo 取回。
    redo: Vec<NodeId>,
}

/// Best-effort projection for callers that still consume the legacy accessor.
/// Open namespaces have no v1 field, so they occupy `plugins`; canonical v2
/// behavior and persistence always use `ResolvedPackages::intent` instead.
fn compatibility_library_spec(intent: &PackageSpec) -> LibrarySpec {
    let mut libraries = LibrarySpec {
        std: Vec::new(),
        natural: None,
        plugins: Vec::new(),
    };
    for requirement in &intent.roots {
        match requirement.id.namespace.as_str() {
            "std" => libraries.std.push(requirement.id.clone()),
            "natural" if libraries.natural.is_none() => {
                libraries.natural = Some(requirement.id.clone());
            }
            _ => libraries.plugins.push(requirement.id.clone()),
        }
    }
    libraries
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
        Self::open_project_with_installed(store, fallback, std::iter::empty::<PackageSources>())
    }

    /// Open a project through the offline v2 resolver chain, with optional
    /// caller-provided installed packages between project-vendored and shipped
    /// embedded sources.
    pub fn open_project_with_installed(
        store: &GraphStore,
        fallback: LibrarySpec,
        installed: impl IntoIterator<Item = PackageSources>,
    ) -> Result<(Session, Option<ProjectDocument>), AppError> {
        let project = store.read_project()?;
        let (packages, libraries) = match &project {
            Some(declared) => {
                let intent = declared.to_package_spec()?;
                let libraries = declared
                    .to_spec()
                    .unwrap_or_else(|_| compatibility_library_spec(&intent));
                (
                    store.resolve_project_packages(declared, installed)?,
                    libraries,
                )
            }
            None => {
                let intent = PackageSpec::from_legacy(&fallback);
                let packages = store.resolve_packages(&intent, installed)?;
                if let Some(lock) = store.read_packages_lock()? {
                    lock.verify_resolved(&packages)?;
                }
                (packages, fallback)
            }
        };
        let graph = store.load_with_packages(packages.clone())?;
        let first_root = graph.roots().next().cloned();
        let mut session = Session::new_resolved_with_libraries(graph, libraries, packages)?;
        if let Some(root) = first_root {
            session.open(&root)?;
        }
        Ok((session, project))
    }

    pub fn new(graph: EvolutionGraph, libraries: LibrarySpec) -> Session {
        Session {
            graph,
            libraries,
            packages: None,
            active: None,
            pending: None,
            history: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Create a session bound to one immutable package-loader v2 result.
    pub fn new_with_packages(
        graph: EvolutionGraph,
        packages: ResolvedPackages,
    ) -> Result<Session, AppError> {
        let libraries = compatibility_library_spec(&packages.intent);
        Self::new_resolved_with_libraries(graph, libraries, packages)
    }

    fn new_resolved_with_libraries(
        graph: EvolutionGraph,
        libraries: LibrarySpec,
        packages: ResolvedPackages,
    ) -> Result<Session, AppError> {
        let graph_packages = graph.packages().ok_or(AppError::PackageContextMissing)?;
        if graph_packages.intent != packages.intent
            || graph_packages.selection.resolved != packages.selection.resolved
        {
            return Err(AppError::PackageContextMismatch);
        }
        Ok(Session {
            graph,
            libraries,
            packages: Some(packages),
            active: None,
            pending: None,
            history: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub fn graph(&self) -> &EvolutionGraph {
        &self.graph
    }

    /// 可變的圖。**只給雜湊外的改動用**(目前唯一消費者是改標籤)——
    /// 語言內容一律走 `LanguageCommand` → 四原語 → `commit`(C1)。
    pub fn graph_mut(&mut self) -> &mut EvolutionGraph {
        &mut self.graph
    }

    pub fn active(&self) -> Option<&NodeId> {
        self.active.as_ref()
    }

    pub fn pending(&self) -> Option<&ResolvedChangeSet> {
        self.pending.as_ref()
    }

    /// Exact package-loader v2 snapshot used by this session. Sessions built
    /// through the legacy constructor return `None`.
    pub fn packages(&self) -> Option<&ResolvedPackages> {
        self.packages.as_ref()
    }

    fn prelude_for(
        &self,
        document: &LanguageDocument,
        namespace: &str,
    ) -> Result<String, ReplayError> {
        match &self.packages {
            Some(packages) => change_set_prelude_with_packages(document, packages, namespace),
            None => change_set_prelude(document, &self.libraries, namespace),
        }
    }

    fn resolve_unresolved(
        &self,
        document: &LanguageDocument,
        unresolved: &UnresolvedChangeSet,
    ) -> Result<ResolvedChangeSet, ReplayError> {
        match &self.packages {
            Some(packages) => unresolved.resolve_packages(document, packages),
            None => unresolved.resolve(document, &self.libraries),
        }
    }

    fn replay_change_set(
        &self,
        document: LanguageDocument,
        changeset: &ResolvedChangeSet,
    ) -> Result<LanguageDocument, ReplayError> {
        let interpreter = match &self.packages {
            Some(packages) => ChangeInterpreter::with_packages(
                document,
                packages.clone(),
                changeset.namespace.clone(),
            )?,
            None => ChangeInterpreter::new(
                document,
                self.libraries.clone(),
                changeset.namespace.clone(),
            )?,
        };
        Ok(interpreter.run(changeset)?.document)
    }

    /// 目前 pending `.chg` 的 canonical source。
    ///
    /// `None` = 沒有 pending;`Err` = 有 pending 但其中一條編輯排不成 `.chg`
    /// (見 [`conlang_changeset::DumpError`])。兩者刻意分開:前者不是錯誤。
    pub fn pending_source(&self) -> Result<Option<String>, AppError> {
        self.pending
            .as_ref()
            .map(|pending| pending.dump())
            .transpose()
            .map_err(AppError::from)
    }

    /// Materialize the document seen by the next structured authoring action.
    ///
    /// A pending ChangeSet is replayed in full, so a later action can address
    /// nodes inserted or renamed by an earlier statement in the same working
    /// copy. The active graph snapshot remains immutable.
    pub fn preview_document(&self) -> Result<LanguageDocument, AppError> {
        let document = self.snapshot()?.clone();
        let Some(pending) = self.pending.as_ref() else {
            return Ok(document);
        };
        Ok(self.replay_change_set(document, pending)?)
    }

    /// 以文字編輯器送來的完整 `.chg` 原子替換 pending。
    ///
    /// parse／resolve 全部成功後才改欄位；失敗時舊 working copy 保持可重試。
    pub fn replace_pending_source(&mut self, source: &str) -> Result<(), AppError> {
        let resolved = self.resolve_pending_source(source)?;
        self.pending = Some(resolved);
        Ok(())
    }

    fn resolve_pending_source(&self, source: &str) -> Result<ResolvedChangeSet, AppError> {
        let document = self.snapshot()?.clone();
        let unresolved = UnresolvedChangeSet::parse(source)?;
        let resolved = self.resolve_unresolved(&document, &unresolved)?;
        self.replay_change_set(document, &resolved)?;
        Ok(resolved)
    }

    /// 在不提交、不改圖的前提下 replay pending，供 UI 預覽。
    pub fn preview_pending(&self) -> Result<DiffVector, AppError> {
        let document = self.snapshot()?.clone();
        let pending = self.pending.as_ref().ok_or(AppError::NoActiveNode)?;
        let preview = self.replay_change_set(document.clone(), pending)?;
        Ok(diff_vector(&document, &preview))
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
        let prelude = self.prelude_for(&document, namespace)?;
        let unresolved = UnresolvedChangeSet::parse(&prelude)?;
        let resolved = self.resolve_unresolved(&document, &unresolved)?;
        self.pending = Some(resolved);
        Ok(())
    }

    /// 把一批原語堆成一個 statement(交易單位)。委派給 `conlang-command`。
    pub fn stage(&mut self, edits: Vec<PrimitiveEdit>) -> Result<u64, AppError> {
        let pending = self.pending.as_mut().ok_or(AppError::NoActiveNode)?;
        Ok(conlang_command::stage(pending, edits))
    }

    /// Append one statement transactionally.
    ///
    /// The candidate ChangeSet is replayed against the immutable active
    /// snapshot before it replaces `self.pending`. Invalid edits therefore do
    /// not create a phantom working copy and cannot corrupt an existing one.
    pub fn stage_checked(
        &mut self,
        namespace: &str,
        edits: Vec<PrimitiveEdit>,
    ) -> Result<u64, AppError> {
        let document = self.snapshot()?.clone();
        let mut candidate = match self.pending.as_ref() {
            Some(pending) => pending.clone(),
            None => {
                let prelude = self.prelude_for(&document, namespace)?;
                let unresolved = UnresolvedChangeSet::parse(&prelude)?;
                self.resolve_unresolved(&document, &unresolved)?
            }
        };
        let ordinal = conlang_command::stage(&mut candidate, edits);
        self.replay_change_set(document, &candidate)?;
        self.pending = Some(candidate);
        Ok(ordinal)
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
        let document = self.graph.snapshot(&parent)?.clone();

        // replay 一次以取得 `.chg` 原文所需的降階結果;節點的 snapshot 由
        // `EvolutionGraph::commit` 自己重放邊上的 changeset 產生(P56 因果契約)。
        // Keep the pending changeset available until validation and graph commit
        // have both succeeded. A failed commit must be retryable by the caller.
        let dumped = {
            let pending = self.pending.as_ref().ok_or(AppError::NoActiveNode)?;
            self.replay_change_set(document, pending)?;
            pending.dump()?
        };

        let id = self.graph.commit(
            vec![Edge::trunk(parent.clone(), dumped)],
            Nativization::None,
            label,
        )?;
        self.history.push(parent);
        self.redo.clear();
        self.active = Some(id.clone());
        self.pending = None;
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

    /// 從記憶體圖移除葉節點。
    ///
    /// 這不是 undo；呼叫端必須以顯式刪除操作觸發。已落盤的節點應先在
    /// persistence 側驗證並刪除；剛 commit、尚未 Save Project 的節點沒有對應
    /// store entry，可直接走圖側刪除。若刪的是 active，導覽會回到最近仍存在
    /// 的節點，沒有就取排序後第一個 root。
    pub fn remove_node(&mut self, id: &NodeId) -> Result<(), AppError> {
        self.graph.remove_node(id)?;
        self.history.retain(|candidate| candidate != id);
        self.redo.retain(|candidate| candidate != id);
        if self.active.as_ref() == Some(id) {
            self.active = self
                .history
                .pop()
                .filter(|candidate| self.graph.node(candidate).is_some())
                .or_else(|| self.graph.roots().next().cloned());
        }
        self.pending = None;
        Ok(())
    }

    // ── 落盤(唯一碰 fs 的地方,且格式一律借用既有的)──────────────────

    /// 存 pending 為一份 `.chg` 檔。
    ///
    /// **不是新格式**:內容就是 `ResolvedChangeSet::dump()`,可被
    /// `UnresolvedChangeSet::parse` 讀回。app 不擁有格式(§2.2)。
    pub fn save_working_copy(&self, path: &Path) -> Result<(), AppError> {
        let pending = self.pending.as_ref().ok_or(AppError::NoActiveNode)?;
        std::fs::write(path, pending.dump()?).map_err(|source| AppError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    /// Validate and save the editor-visible source as one working-copy action.
    ///
    /// The candidate is parsed, resolved, and replayed before any state changes.
    /// It replaces `pending` only after the exact canonical source has reached
    /// disk, so a validation or I/O failure leaves the previous pending copy
    /// available for retry.
    pub fn save_working_copy_source(&mut self, path: &Path, source: &str) -> Result<(), AppError> {
        let resolved = self.resolve_pending_source(source)?;
        std::fs::write(path, resolved.dump()?).map_err(|source| AppError::Io {
            path: path.display().to_string(),
            source,
        })?;
        self.pending = Some(resolved);
        Ok(())
    }

    /// 讀回一份工作副本。基底必須是目前節點——三道 digest 會在 resolve 時把關。
    pub fn load_working_copy(&mut self, path: &Path) -> Result<(), AppError> {
        let text = std::fs::read_to_string(path).map_err(|source| AppError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let document = self.snapshot()?.clone();
        let unresolved = UnresolvedChangeSet::parse(&text)?;
        let resolved = self.resolve_unresolved(&document, &unresolved)?;
        self.pending = Some(resolved);
        Ok(())
    }

    /// 把圖寫進 store。`save` 是 append-only 且**不靜默**——store 有而圖沒有的
    /// 節點會回 `StoreError::StaleNode`,指向 `remove_node`。
    pub fn persist(&self, store: &GraphStore) -> Result<(), AppError> {
        store.save(&self.graph)?;
        if let Some(packages) = &self.packages {
            store.write_resolved_packages_lock(packages)?;
        }
        Ok(())
    }

    /// Legacy package selection retained for source compatibility.
    ///
    /// For package-loader v2 sessions this is only an open-namespace
    /// compatibility projection; use [`Self::packages`] for canonical intent,
    /// exact versions, source provenance, and content digests.
    pub fn libraries(&self) -> &LibrarySpec {
        &self.libraries
    }
}
