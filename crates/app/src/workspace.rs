//! 一個開著的專案:工作階段 + 兩層快取(步驟 21 收尾)。
//!
//! # 為什麼 `Session` 不自己持有快取
//!
//! `Session` 的職責是**工作階段歷史**(active 指標、undo/redo、pending 編輯)。
//! 把 `CompileService` 與 `QueryCache` 塞進去,它就同時管兩件不相干的事:
//! 「使用者走到哪」與「算過的東西留著」。前者是語意,後者純粹是效能。
//!
//! 故並列在本型別之下。三者的生命週期一致(都隨專案開關),但職責分開:
//!
//! ```text
//! Workspace
//!   ├─ Session         走到哪、改了什麼、還沒提交什麼
//!   ├─ CompileService  CompiledSystem 的記憶體快取(lazy / 可丟棄)
//!   └─ QueryCache      派生視圖的記憶體快取(鍵完整涵蓋輸入)
//! ```
//!
//! # 快取一律可丟
//!
//! [`drop_caches`](Workspace::drop_caches) 任何時候都能呼叫,不影響任何答案
//! ——正確性由鍵保證,不由生命週期保證(§6.2)。

use conlang_language::{CompiledSystem, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument};
use std::sync::Arc;

use crate::cache::{LexiconKey, QueryCache};
use crate::compile::CompileService;
use crate::{AppError, Session};

/// 一個開著的專案。
#[derive(Debug)]
pub struct Workspace {
    session: Session,
    compiler: CompileService,
    lexicons: QueryCache<LexiconKey, conlang_query::Lexicon>,
    project: Option<ProjectDocument>,
}

impl Workspace {
    /// 開啟專案目錄。`fallback` 用於沒有 `project.toml` 的舊 store。
    pub fn open(store: &GraphStore, fallback: LibrarySpec) -> Result<Workspace, AppError> {
        let (session, project) = Session::open_project(store, fallback)?;
        Ok(Workspace {
            session,
            compiler: CompileService::new(),
            lexicons: QueryCache::new(),
            project,
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    pub fn project(&self) -> Option<&ProjectDocument> {
        self.project.as_ref()
    }

    pub fn replace_project(&mut self, project: ProjectDocument) {
        self.project = Some(project);
    }

    /// 目前節點的編譯產物。**lazy**——沒人問就不編譯。
    pub fn compiled(&mut self) -> Result<Arc<CompiledSystem>, AppError> {
        let document = self.session.snapshot()?.clone();
        let libraries = self.session.libraries().clone();
        Ok(self.compiler.get(&document, &libraries)?)
    }

    /// 詞典視圖,經快取。
    pub fn lexicon(
        &mut self,
        filter: &conlang_query::LexiconFilter,
        view: &conlang_query::ViewConfig,
    ) -> Result<conlang_query::Lexicon, AppError> {
        let document = self.session.snapshot()?.clone();
        let compiled = self.compiled()?;
        let key = LexiconKey {
            document: crate::cache::ContentDigest::of(&document),
            filter: crate::cache::LexiconFilterKey::from(filter),
            view: crate::cache::ViewKey::from(view),
        };
        Ok(self
            .lexicons
            .get_or_insert_with(key, || conlang_query::lexicon(&compiled, filter, view)))
    }

    /// 命中/未命中:(編譯, 詞典)。
    pub fn cache_stats(&self) -> ((u64, u64), (u64, u64)) {
        (self.compiler.stats(), self.lexicons.stats())
    }

    /// **任何時候都能丟**——正確性由鍵保證,不由生命週期保證。
    pub fn drop_caches(&mut self) {
        self.compiler.clear();
        self.lexicons.clear();
    }
}

// ── 出境視圖(裁定 ② 丙:帶 schema 標記)────────────────────────────────

use crate::wire::{
    EvolutionTreeV1, GroupingViewV1, LexiconViewV1, NodeDetailV1, TreeEdgeV1, TreeNodeV1,
    UI_SCHEMA_V1,
};

impl Workspace {
    /// 演化樹。依 id 排序,故決定性。
    pub fn tree(&self) -> EvolutionTreeV1 {
        let graph = self.session.graph();
        let nodes = graph
            .ids()
            .map(|id| {
                let node = graph.node(id).expect("ids() 只給存在的節點");
                TreeNodeV1 {
                    id: id.as_str().to_owned(),
                    label: node.label().map(str::to_owned),
                    parents: node
                        .parents()
                        .iter()
                        .enumerate()
                        .map(|(index, edge)| TreeEdgeV1 {
                            from: edge.from.as_str().to_owned(),
                            // parents[0] 是主幹(帶 changeset);其餘是引用邊
                            kind: if index == 0 { "trunk" } else { "reference" }.to_owned(),
                        })
                        .collect(),
                }
            })
            .collect();
        EvolutionTreeV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            nodes,
            active: self.session.active().map(|id| id.as_str().to_owned()),
        }
    }

    /// 目前節點的詞典視圖。
    pub fn lexicon_view(
        &mut self,
        filter: &conlang_query::LexiconFilter,
        view: &conlang_query::ViewConfig,
    ) -> Result<LexiconViewV1, AppError> {
        let node = self
            .session
            .active()
            .ok_or(AppError::NoActiveNode)?
            .as_str()
            .to_owned();
        Ok(LexiconViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node,
            lexicon: self.lexicon(filter, view)?,
        })
    }

    /// 節點編輯頁:**全部雜湊外**,改它們不動任何 replay 產物。
    ///
    /// 需要 store 是因為 state 與 annotation 住在檔案系統那側(P64),
    /// 不在圖裡。
    pub fn node_detail(&self, store: &GraphStore) -> Result<NodeDetailV1, AppError> {
        let id = self.session.active().ok_or(AppError::NoActiveNode)?;
        let node =
            self.session.graph().node(id).ok_or_else(|| {
                conlang_changeset::evolution::EvolutionError::UnknownNode(id.clone())
            })?;
        // A commit deliberately creates only an in-memory graph node.  Until the
        // explicit Save Project boundary, that node has no hash-external files
        // to read yet.  An existing node directory is different: failures below
        // it must still propagate so damaged metadata is never hidden.
        let node_dir = store.root().join("nodes").join(id.as_str());
        let (state, annotations) = if node_dir.exists() {
            (
                store.read_state(id)?,
                store
                    .list_annotations(id)?
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect(),
            )
        } else {
            (
                conlang_changeset::state::EvolutionState::default(),
                Vec::new(),
            )
        };
        Ok(NodeDetailV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            id: id.as_str().to_owned(),
            label: node.label().map(str::to_owned),
            state,
            annotations,
            sign_count: node.snapshot().language().signs.len(),
        })
    }

    /// 方言分群視圖。
    pub fn grouping_view(
        &self,
        strategy: &dyn conlang_query::DialectGroupingStrategy,
        measure: &dyn conlang_query::IntelligibilityMeasure,
        override_: &conlang_query::GroupingOverride,
    ) -> GroupingViewV1 {
        GroupingViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            grouping: conlang_query::dialect_groups(
                self.session.graph(),
                strategy,
                measure,
                override_,
            ),
        }
    }
}
