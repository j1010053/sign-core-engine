//! **前端邊界**:Tauri command 會 1:1 呼叫的那組函數。
//!
//! # 為什麼邏輯在這裡而不是在 Tauri crate
//!
//! 與 CLI 同一個作法(`conlang-cli` 的邏輯全在 lib、`main.rs` 只有五行):
//! **能測的部分不要跟不能測的部分綁在一起**。
//!
//! Tauri crate 需要 `webkit2gtk` 之類的系統庫與 Node 建置鏈;把邏輯放進去,
//! 整組就只能在有那套環境的機器上驗證。放在這裡,它跟其餘 860 條測試一起跑。
//!
//! Tauri 那層因此只剩:
//!
//! ```ignore
//! #[tauri::command]
//! fn tree(state: State<Mutex<UiSession>>) -> Result<EvolutionTreeV1, UiError> {
//!     state.lock().unwrap().tree()
//! }
//! ```
//!
//! # 單一 session(裁定 ③)
//!
//! [`UiSession`] 是整個應用**唯一**的狀態,Tauri 以 `State<Mutex<UiSession>>`
//! 持有。它同時握 store(檔案側)與 workspace(記憶體側)——編輯頁要的
//! State/annotation 住檔案系統,詞典住記憶體,兩者都要。
//!
//! # 錯誤形狀沿用既有慣例
//!
//! `UiError { code, message }` 對映 repo 既有的 `Diagnostic { code, message }`
//! ——不發明第二種錯誤表示。`code` 是可比對的字串(`APP_*`/`PERSISTENCE_*`…),
//! 前端據此分辨,而不是解析人類可讀訊息。

use conlang_changeset::diff::diff_vector;
use conlang_changeset::evolution::{EvolutionGraph, NodeId, RebaseOutcome};
use conlang_changeset::rewrite::{AtomicRewrite, DonorScope, RuleHome, ServiceContext};
use conlang_changeset::state::EvolutionState;
use conlang_command::{LanguageCommand, ViewCommand};
use conlang_generate::{DistributionGenerator, Generator, Need, NeedOrigin, Proposal, Strategies};
use conlang_language::{library::embedded_catalog, LanguageDocument, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument, ProjectPackages};
use conlang_query::{
    derivation_family, intelligibility, project_phoneme_freq, ExploratoryHeuristicV1,
    GroupingOverride, LexiconFilter, SortKey, TreeEdgeCut, ViewConfig,
};
use conlang_stats::{load_prior_from_packages, EffectiveDistribution, Layer, WeightTable};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::wire::{
    CatalogPackageV1, DerivationViewV1, EvolutionTreeV1, GroupingViewV1, IntelligibilityViewV1,
    LexiconViewV1, NodeDetailV1, PackageCatalogV1, PendingChangeV1, ProjectSummaryV1, ProposalV1,
    ProposalsViewV1, RebasePreviewV1, SegmentStatV1, SourceReconcileV1, SourceViewV1, StatsViewV1,
    WeightConfigV1, WeightEntryV1, UI_SCHEMA_V1,
};
use crate::{apply_view_command, AppError, Workspace};

/// 過 IPC 的錯誤。**帶可比對的 `code`**,前端不必解析訊息文字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiError {
    pub code: String,
    pub message: String,
}

impl UiError {
    pub fn of(error: impl std::fmt::Display) -> UiError {
        let text = error.to_string();
        // 既有錯誤一律是 `CODE: 詳細` 的形狀(APP_*/PERSISTENCE_*/CHANGESET_*…)
        let (code, message) = match text.split_once(": ") {
            Some((code, rest))
                if code
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') =>
            {
                (code.to_owned(), rest.to_owned())
            }
            _ => ("APP_ERROR".to_owned(), text.clone()),
        };
        UiError { code, message }
    }
}

fn ui_error(code: &str, message: impl Into<String>) -> UiError {
    UiError {
        code: code.to_owned(),
        message: message.into(),
    }
}

impl From<AppError> for UiError {
    fn from(error: AppError) -> UiError {
        UiError::of(error)
    }
}

impl From<conlang_persistence::StoreError> for UiError {
    fn from(error: conlang_persistence::StoreError) -> UiError {
        UiError::of(error)
    }
}

/// 前端送來的詞典查詢條件。
///
/// 刻意**不**直接收 `LexiconFilter`——那是 query 的內部型別,讓它出現在入境
/// 契約上等於把內部形狀綁死。這裡只收前端真的會給的三個東西。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexiconQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss_contains: Option<String>,
    /// `"name"` | `"form"` | `"gloss"`。其餘值視為 `"name"`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentWeight {
    pub segment: String,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default = "default_template")]
    pub template: String,
    #[serde(default = "default_proposal_count")]
    pub count: usize,
    #[serde(default)]
    pub seed: u64,
    pub weights: Vec<SegmentWeight>,
}

fn default_template() -> String {
    "CVC".to_owned()
}

const fn default_proposal_count() -> usize {
    8
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundChangeInput {
    pub rule: String,
    #[serde(default = "default_rule_home")]
    pub home: String,
}

fn default_rule_home() -> String {
    "Core".to_owned()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupingQuery {
    #[serde(default = "default_view_name")]
    pub view: String,
    #[serde(default = "default_grouping_threshold")]
    pub threshold: f64,
}

/// Root package declarations written to `project.toml` after validation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSelectionInput {
    #[serde(default)]
    pub std: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natural: Option<String>,
    #[serde(default)]
    pub plugins: Vec<String>,
}

fn default_view_name() -> String {
    "default".to_owned()
}

const fn default_grouping_threshold() -> f64 {
    0.6
}

#[derive(Debug)]
struct CachedProposals {
    query: ProposalQuery,
    need: Need,
    proposals: Vec<Proposal>,
}

impl LexiconQuery {
    fn split(&self) -> (LexiconFilter, ViewConfig) {
        let mut filter = LexiconFilter::all();
        if let Some(category) = &self.category {
            filter = filter.with_category(category.clone());
        }
        if let Some(needle) = &self.gloss_contains {
            filter = filter.with_gloss_containing(needle.clone());
        }
        let view = ViewConfig {
            sort: match self.sort.as_deref() {
                Some("form") => SortKey::UnderlyingForm,
                Some("gloss") => SortKey::Gloss,
                _ => SortKey::Name,
            },
        };
        (filter, view)
    }
}

/// 整個應用的**唯一**狀態(裁定 ③:單一 session)。
#[derive(Debug)]
pub struct UiSession {
    store: GraphStore,
    workspace: Workspace,
    graph_dirty: bool,
    last_proposals: Option<CachedProposals>,
}

impl UiSession {
    /// 開啟專案。`fallback` 供沒有 `project.toml` 的舊 store。
    pub fn open(
        path: impl AsRef<Path>,
        fallback: conlang_language::LibrarySpec,
    ) -> Result<UiSession, UiError> {
        let store = GraphStore::open(path.as_ref())?;
        let workspace = Workspace::open(&store, fallback)?;
        Ok(UiSession {
            store,
            workspace,
            graph_dirty: false,
            last_proposals: None,
        })
    }

    /// 從 `.lang` 建立新專案。先完整解析 source，成功後才初始化目錄。
    /// 建一個新專案。
    ///
    /// `source_path` 為 `None` ⇒ **空白專案**。空的 Language 是一等公民
    /// (P28:canonical empty root 永遠存在,四原語有處掛靠),故不必先手寫一份
    /// 佔位 `.lang`——那是把實作細節外洩給使用者。
    pub fn create(
        path: impl AsRef<Path>,
        source_path: Option<impl AsRef<Path>>,
        name: Option<String>,
        namespace: &str,
    ) -> Result<UiSession, UiError> {
        let source = match &source_path {
            Some(from) => std::fs::read_to_string(from.as_ref()).map_err(UiError::of)?,
            None => String::new(),
        };
        let document =
            LanguageDocument::import_new_root(&source, namespace).map_err(UiError::of)?;
        let project_path = path.as_ref();
        if project_path.exists() {
            let mut entries = std::fs::read_dir(project_path).map_err(UiError::of)?;
            if entries.next().transpose().map_err(UiError::of)?.is_some() {
                return Err(ui_error(
                    "APP_PROJECT_NOT_EMPTY",
                    project_path.display().to_string(),
                ));
            }
        }

        let store = GraphStore::init(project_path)?;
        let libraries = LibrarySpec::default();
        let mut graph = EvolutionGraph::new(libraries.clone());
        graph.add_root(document).map_err(AppError::Evolution)?;
        store.save(&graph)?;
        let mut project = ProjectDocument::from_spec(&libraries);
        project.name = name;
        store.write_project(&project)?;
        let workspace = Workspace::open(&store, libraries)?;
        Ok(UiSession {
            store,
            workspace,
            graph_dirty: false,
            last_proposals: None,
        })
    }

    pub fn summary(&self) -> ProjectSummaryV1 {
        let spec = self.workspace.session().libraries();
        let mut packages: Vec<String> = spec.std.iter().map(ToString::to_string).collect();
        if let Some(natural) = &spec.natural {
            packages.push(natural.to_string());
        }
        packages.extend(spec.plugins.iter().map(ToString::to_string));
        ProjectSummaryV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            path: self.store.root().display().to_string(),
            name: self
                .workspace
                .project()
                .and_then(|project| project.name.clone()),
            legacy: self.workspace.project().is_none(),
            graph_dirty: self.graph_dirty,
            has_pending: self.workspace.session().pending().is_some(),
            node_count: self.workspace.session().graph().len(),
            active: self
                .workspace
                .session()
                .active()
                .map(|id| id.as_str().to_owned()),
            packages,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.graph_dirty || self.workspace.session().pending().is_some()
    }

    /// List the immutable catalog bundled with this LangCraft build.
    pub fn package_catalog(&self) -> Result<PackageCatalogV1, UiError> {
        let catalog = embedded_catalog().map_err(|error| UiError::of(AppError::Library(error)))?;
        let spec = self.workspace.session().libraries();
        let selected = catalog
            .select(spec)
            .map_err(|error| UiError::of(AppError::Library(error)))?
            .packages
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let declared = spec
            .std
            .iter()
            .chain(spec.natural.iter())
            .chain(spec.plugins.iter())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let packages = catalog
            .packages()
            .iter()
            .map(|package| CatalogPackageV1 {
                id: package.id.to_string(),
                kind: package.id.kind.keyword().to_owned(),
                version: package.version.clone(),
                source: "embedded".to_owned(),
                enabled: package.enabled,
                declared: declared.contains(&package.id),
                selected: selected.contains(&package.id),
                requires: package.requires.iter().map(ToString::to_string).collect(),
            })
            .collect();
        Ok(PackageCatalogV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            packages,
        })
    }

    /// Validate a new declaration against both the catalog and the complete
    /// graph before atomically updating `project.toml`, then reopen the session.
    pub fn configure_packages(
        &mut self,
        input: PackageSelectionInput,
    ) -> Result<ProjectSummaryV1, UiError> {
        if self.is_dirty() {
            return Err(ui_error(
                "APP_DIRTY_PROJECT",
                "save or discard project changes before changing package libraries",
            ));
        }

        let mut project = self.workspace.project().cloned().unwrap_or_default();
        project.packages = ProjectPackages {
            std: input.std,
            natural: input.natural,
            plugins: input.plugins,
        };
        let spec = project.to_spec()?;
        let catalog = embedded_catalog().map_err(|error| UiError::of(AppError::Library(error)))?;
        catalog
            .select(&spec)
            .map_err(|error| UiError::of(AppError::Library(error)))?;
        // This is a read-only validation pass. It catches package-lock and
        // replay incompatibilities before the declaration on disk changes.
        self.store.load(spec.clone())?;

        let path = self.store.root().to_path_buf();
        self.store.write_project(&project)?;
        let next = UiSession::open(&path, spec)?;
        let summary = next.summary();
        *self = next;
        Ok(summary)
    }

    pub fn weight_config(&self) -> Result<WeightConfigV1, UiError> {
        let manual = self
            .workspace
            .project()
            .map(|project| &project.weights)
            .into_iter()
            .flatten()
            .map(|(segment, weight)| WeightEntryV1 {
                segment: segment.clone(),
                weight: *weight,
                source: "manual".to_owned(),
            })
            .collect();
        let distribution = self.effective_distribution(&[])?;
        let effective = distribution
            .resolve()
            .iter()
            .map(|(segment, weight)| WeightEntryV1 {
                segment: segment.to_owned(),
                weight,
                source: match distribution.provenance(segment) {
                    Some(Layer::Manual) => "manual",
                    Some(Layer::Imported) => "imported",
                    Some(Layer::Prior) | None => "prior",
                }
                .to_owned(),
            })
            .collect();
        Ok(WeightConfigV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            declaration_source: "project.toml:weights".to_owned(),
            manual,
            effective,
        })
    }

    /// Manual weights are project authoring data and save immediately, like
    /// node metadata; they do not mutate an immutable language snapshot.
    pub fn set_weights(&mut self, entries: Vec<SegmentWeight>) -> Result<WeightConfigV1, UiError> {
        let mut weights = std::collections::BTreeMap::new();
        for entry in entries {
            let segment = entry.segment.trim();
            if segment.is_empty() || !entry.weight.is_finite() || entry.weight < 0.0 {
                return Err(ui_error(
                    "APP_INVALID_WEIGHT",
                    "segments must be non-empty and weights finite and non-negative",
                ));
            }
            if weights.insert(segment.to_owned(), entry.weight).is_some() {
                return Err(ui_error(
                    "APP_DUPLICATE_WEIGHT",
                    format!("duplicate segment {segment:?}"),
                ));
            }
        }

        let mut project =
            self.workspace.project().cloned().unwrap_or_else(|| {
                ProjectDocument::from_spec(self.workspace.session().libraries())
            });
        project.weights = weights;
        self.store.write_project(&project)?;
        self.workspace.replace_project(project);
        self.last_proposals = None;
        self.weight_config()
    }

    // ── 演化樹面板 ──────────────────────────────────────────────────────

    pub fn tree(&self) -> EvolutionTreeV1 {
        self.workspace.tree()
    }

    /// 點一個節點 → 切過去並回傳編輯頁要的東西。
    pub fn select_node(&mut self, id: &str) -> Result<NodeDetailV1, UiError> {
        if self.workspace.session().pending().is_some() {
            return Err(ui_error(
                "APP_DIRTY_WORKING_COPY",
                "save or discard the pending .chg before switching nodes",
            ));
        }
        let node = NodeId::parse(id).map_err(|_| UiError {
            code: "APP_UNKNOWN_NODE".to_owned(),
            message: id.to_owned(),
        })?;
        self.workspace.session_mut().open(&node)?;
        self.last_proposals = None;
        self.node_detail()
    }

    // ── 辭典面板 ────────────────────────────────────────────────────────

    pub fn lexicon(&mut self, query: &LexiconQuery) -> Result<LexiconViewV1, UiError> {
        let (filter, view) = query.split();
        Ok(self.workspace.lexicon_view(&filter, &view)?)
    }

    // ── 節點編輯頁(**全部雜湊外**,改它們不動任何 replay 產物)──────────

    pub fn node_detail(&self) -> Result<NodeDetailV1, UiError> {
        Ok(self.workspace.node_detail(&self.store)?)
    }

    /// 改標籤。標籤不進 node-v2 雜湊,故**改名不改身分**。
    pub fn set_label(&mut self, label: Option<String>) -> Result<NodeDetailV1, UiError> {
        let id = self.active()?;
        self.workspace
            .session_mut()
            .graph_mut()
            .set_label(&id, label)
            .map_err(|error| UiError::of(AppError::Evolution(error)))?;
        self.workspace.session().persist(&self.store)?;
        self.node_detail()
    }

    /// 改環境。裁定 (A):State 只在撰寫時被讀,**replay 永不看它**。
    pub fn set_state(&mut self, state: &EvolutionState) -> Result<NodeDetailV1, UiError> {
        let id = self.active()?;
        self.store.write_state(&id, state)?;
        // State 只影響撰寫時提議，不影響 replay/lexicon；只清 proposal cache。
        self.last_proposals = None;
        self.node_detail()
    }

    /// 寫旁註。旁註層**正交於本體**(07 §5c):不參與 replay、不被 diff、
    /// 不約束生成。
    pub fn write_annotation(&mut self, path: &str, content: &str) -> Result<NodeDetailV1, UiError> {
        let id = self.active()?;
        self.store.write_annotation(&id, path, content.as_bytes())?;
        self.node_detail()
    }

    pub fn read_annotation(&self, path: &str) -> Result<String, UiError> {
        let id = self.active()?;
        let bytes = self.store.read_annotation(&id, path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    // ── working copy / 演化編寫 ────────────────────────────────────────

    pub fn begin_edit(&mut self, namespace: &str) -> Result<PendingChangeV1, UiError> {
        self.workspace.session_mut().begin_edit(namespace)?;
        self.pending_change()
    }

    pub fn pending_change(&self) -> Result<PendingChangeV1, UiError> {
        let session = self.workspace.session();
        let source = session
            .pending_source()
            .ok_or_else(|| ui_error("APP_NO_PENDING_CHANGE", "no pending .chg"))?;
        let statements = session
            .pending()
            .map(|pending| pending.statements.len())
            .unwrap_or(0);
        let diff = session.preview_pending()?;
        Ok(PendingChangeV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            source,
            statements,
            diff: diff.into(),
        })
    }

    pub fn replace_pending_source(&mut self, source: &str) -> Result<PendingChangeV1, UiError> {
        self.workspace
            .session_mut()
            .replace_pending_source(source)?;
        self.pending_change()
    }

    pub fn stage_sound_change(
        &mut self,
        input: &SoundChangeInput,
    ) -> Result<PendingChangeV1, UiError> {
        if input.rule.trim().is_empty() {
            return Err(ui_error("APP_RULE_EMPTY", "sound-change rule is empty"));
        }
        if self.workspace.session().pending().is_none() {
            let namespace = format!("ui:evolve:{}", self.workspace.session().graph().len());
            self.workspace.session_mut().begin_edit(&namespace)?;
        }
        let document = self.workspace.session().snapshot()?.clone();
        let rewrite = AtomicRewrite::SoundChange {
            home: RuleHome::Global(input.home.clone()),
            body: input.rule.clone(),
        };
        let edits = conlang_command::lower(
            &LanguageCommand::ApplyRewrite(&rewrite),
            &document,
            &Strategies::default(),
            &ServiceContext::offline(),
            &DonorScope::new(),
        )
        .map_err(UiError::of)?;
        self.workspace.session_mut().stage(edits)?;
        self.pending_change()
    }

    pub fn discard_last_edit(&mut self) -> Result<PendingChangeV1, UiError> {
        self.workspace.session_mut().discard_last_edit()?;
        self.pending_change()
    }

    pub fn save_working_copy(&self, path: impl AsRef<Path>) -> Result<(), UiError> {
        self.workspace.session().save_working_copy(path.as_ref())?;
        Ok(())
    }

    pub fn load_working_copy(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<PendingChangeV1, UiError> {
        self.workspace
            .session_mut()
            .load_working_copy(path.as_ref())?;
        self.pending_change()
    }

    pub fn commit(&mut self, label: Option<String>) -> Result<NodeDetailV1, UiError> {
        self.workspace.session_mut().commit(label)?;
        self.workspace.drop_caches();
        self.graph_dirty = true;
        self.last_proposals = None;
        self.node_detail()
    }

    /// 明確的 graph persist 邊界；commit 本身只建立記憶體節點。
    pub fn save_project(&mut self) -> Result<ProjectSummaryV1, UiError> {
        self.workspace.session().persist(&self.store)?;
        self.graph_dirty = false;
        Ok(self.summary())
    }

    pub fn undo_navigation(&mut self) -> Result<NodeDetailV1, UiError> {
        self.ensure_no_pending()?;
        self.workspace.session_mut().undo()?;
        self.last_proposals = None;
        self.node_detail()
    }

    pub fn redo_navigation(&mut self) -> Result<NodeDetailV1, UiError> {
        self.ensure_no_pending()?;
        self.workspace.session_mut().redo()?;
        self.last_proposals = None;
        self.node_detail()
    }

    pub fn remove_active_leaf(&mut self) -> Result<EvolutionTreeV1, UiError> {
        self.ensure_no_pending()?;
        let id = self.active()?;
        let has_dependents = self.workspace.session().graph().ids().any(|candidate| {
            self.workspace
                .session()
                .graph()
                .node(candidate)
                .map(|node| node.parents().iter().any(|edge| edge.from == id))
                .unwrap_or(false)
        });
        if has_dependents {
            return Err(ui_error(
                "APP_NODE_HAS_DEPENDENTS",
                "only leaf nodes can be deleted",
            ));
        }
        self.store.remove_node(&id)?;
        self.workspace.session_mut().remove_node(&id)?;
        self.workspace.drop_caches();
        self.last_proposals = None;
        Ok(self.tree())
    }

    // ── generation / statistics ────────────────────────────────────────

    pub fn propose(&mut self, query: &ProposalQuery) -> Result<ProposalsViewV1, UiError> {
        if query.count == 0 || query.count > 512 {
            return Err(ui_error(
                "APP_PROPOSAL_COUNT",
                "proposal count must be between 1 and 512",
            ));
        }
        let need = Need {
            name: query.name.clone(),
            categories: query.categories.clone(),
            gloss: query.gloss.clone(),
            origin: NeedOrigin::Coined,
        };
        let distribution = self.effective_distribution(&query.weights)?.resolve();
        let compiled = self.workspace.compiled()?;
        let generated = DistributionGenerator {
            distribution: &distribution,
            template: &query.template,
            count: query.count,
            seed: query.seed,
        }
        .propose(&need, &compiled)
        .map_err(UiError::of)?;
        let proposals: Vec<Proposal> = conlang_generate::ranked(&generated)
            .into_iter()
            .cloned()
            .collect();
        let view = ProposalsViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node: self.active()?.as_str().to_owned(),
            proposals: proposals.iter().map(proposal_view).collect(),
        };
        self.last_proposals = Some(CachedProposals {
            query: query.clone(),
            need,
            proposals,
        });
        Ok(view)
    }

    /// 採用只 stage 到 pending `.chg`，永不偷偷 commit。
    pub fn adopt_proposal(
        &mut self,
        query: &ProposalQuery,
        index: usize,
    ) -> Result<PendingChangeV1, UiError> {
        let cache = self.last_proposals.as_ref().ok_or_else(|| {
            ui_error(
                "APP_PROPOSALS_STALE",
                "generate proposals again before adopting",
            )
        })?;
        if &cache.query != query {
            return Err(ui_error(
                "APP_PROPOSALS_STALE",
                "proposal inputs changed; generate again",
            ));
        }
        let chosen =
            cache.proposals.get(index).cloned().ok_or_else(|| {
                ui_error("APP_UNKNOWN_PROPOSAL", format!("proposal index {index}"))
            })?;
        let need = cache.need.clone();
        let document = self.workspace.session().snapshot()?.clone();
        let edits = conlang_command::lower(
            &LanguageCommand::AdoptProposal {
                need: &need,
                chosen: &chosen,
            },
            &document,
            &Strategies::default(),
            &ServiceContext::offline(),
            &DonorScope::new(),
        )
        .map_err(UiError::of)?;
        if self.workspace.session().pending().is_none() {
            let namespace = format!("ui:coin:{}", self.workspace.session().graph().len());
            self.workspace.session_mut().begin_edit(&namespace)?;
        }
        self.workspace.session_mut().stage(edits)?;
        self.last_proposals = None;
        self.pending_change()
    }

    pub fn stats(&self, inventory: &[String]) -> Result<StatsViewV1, UiError> {
        let document = self.workspace.session().snapshot()?;
        let refs: Vec<&str> = inventory.iter().map(String::as_str).collect();
        let report = project_phoneme_freq(document.language(), &refs);
        Ok(StatsViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node: self.active()?.as_str().to_owned(),
            segmentation: if inventory.is_empty() {
                "per-character".to_owned()
            } else {
                "longest-match".to_owned()
            },
            sampling_source: false,
            segments: report
                .iter()
                .map(|(segment, count)| SegmentStatV1 {
                    segment: segment.to_owned(),
                    count,
                })
                .collect(),
        })
    }

    // ── analysis / views ───────────────────────────────────────────────

    pub fn grouping(&self, query: &GroupingQuery) -> Result<GroupingViewV1, UiError> {
        if !query.threshold.is_finite() || query.threshold < 0.0 {
            return Err(ui_error(
                "APP_GROUP_THRESHOLD",
                "threshold must be finite and non-negative",
            ));
        }
        let view = self.store.read_view(&query.view)?;
        let override_ = GroupingOverride {
            assignments: view.assignments,
            labels: view.labels,
        };
        Ok(self.workspace.grouping_view(
            &TreeEdgeCut {
                threshold: query.threshold,
            },
            &ExploratoryHeuristicV1::suggested(),
            &override_,
        ))
    }

    pub fn assign_group(
        &self,
        query: &GroupingQuery,
        node: String,
        group: String,
    ) -> Result<GroupingViewV1, UiError> {
        let mut document = self.store.read_view(&query.view)?;
        apply_view_command(
            &mut document,
            &ViewCommand::AssignGroup {
                view: query.view.clone(),
                node,
                group,
            },
        );
        self.store.write_view(&query.view, &document)?;
        self.grouping(query)
    }

    pub fn label_group(
        &self,
        query: &GroupingQuery,
        group: String,
        label: String,
    ) -> Result<GroupingViewV1, UiError> {
        let mut document = self.store.read_view(&query.view)?;
        apply_view_command(
            &mut document,
            &ViewCommand::LabelGroup {
                view: query.view.clone(),
                group,
                label,
            },
        );
        self.store.write_view(&query.view, &document)?;
        self.grouping(query)
    }

    pub fn intelligibility(
        &self,
        source: &str,
        target: &str,
    ) -> Result<IntelligibilityViewV1, UiError> {
        let source_id = parse_node(source)?;
        let target_id = parse_node(target)?;
        let source_document = self
            .workspace
            .session()
            .graph()
            .snapshot(&source_id)
            .map_err(AppError::Evolution)?;
        let target_document = self
            .workspace
            .session()
            .graph()
            .snapshot(&target_id)
            .map_err(AppError::Evolution)?;
        let vector = diff_vector(source_document, target_document);
        Ok(IntelligibilityViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            source: source.to_owned(),
            target: target.to_owned(),
            score: intelligibility(
                source_document,
                target_document,
                &ExploratoryHeuristicV1::suggested(),
            ),
            diff: vector.into(),
        })
    }

    pub fn derivation(&mut self, sign: &str) -> Result<DerivationViewV1, UiError> {
        let node = self.active()?.as_str().to_owned();
        let compiled = self.workspace.compiled()?;
        Ok(DerivationViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node,
            family: derivation_family(&compiled, sign),
        })
    }

    pub fn source(&self) -> Result<SourceViewV1, UiError> {
        let id = self.active()?;
        Ok(SourceViewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node: id.as_str().to_owned(),
            source: self.workspace.session().snapshot()?.source(),
        })
    }

    /// 將編輯過的 canonical `.lang` 原子轉成 pending `.chg`。
    ///
    /// reconcile 與 reconstruct 都先在區域值上完成。任何 ambiguity、parse 或
    /// unsupported error 都不會碰 session；只有兩步都成功才建立 working copy。
    pub fn reconcile_source(&mut self, source: &str) -> Result<SourceReconcileV1, UiError> {
        self.ensure_no_pending()?;
        let before = self.workspace.session().snapshot()?.clone();
        let namespace = format!("ui:source:{}", self.workspace.session().graph().len());
        let (after, report) = conlang_changeset::reconcile::reconcile_edited_source(
            &before,
            source,
            &namespace,
            &conlang_changeset::reconcile::ReconcileHints::default(),
        )
        .map_err(UiError::of)?;
        let edits =
            conlang_changeset::reconstruct::reconstruct(&before, &after).map_err(UiError::of)?;
        if edits.is_empty() {
            return Err(ui_error(
                "APP_SOURCE_UNCHANGED",
                "the edited .lang has no semantic changes",
            ));
        }
        let primitive_edits = edits.len();

        self.workspace.session_mut().begin_edit(&namespace)?;
        self.workspace.session_mut().stage(edits)?;
        let pending = self.pending_change()?;
        Ok(SourceReconcileV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            matched: report.matched.len(),
            inserted: report.inserted.len(),
            deleted: report.deleted.len(),
            primitive_edits,
            pending,
        })
    }

    /// 在圖副本上執行 rebase。即使結果是 clean，也不會改 active 或新增正式節點。
    pub fn preview_rebase(&self, node: &str, onto: &str) -> Result<RebasePreviewV1, UiError> {
        self.ensure_no_pending()?;
        let node = parse_node(node)?;
        let onto = parse_node(onto)?;
        let mut preview = self.workspace.session().graph().clone();
        let outcome = preview
            .rebase(&node, &onto)
            .map_err(|error| UiError::of(AppError::Evolution(error)))?;
        Ok(rebase_view(&node, &onto, outcome).0)
    }

    /// 使用者確認 preview 後才在正式圖上重跑同一 rebase。
    /// 衝突結果不會建立節點；clean 結果建立並存的新節點，舊鏈保持不動。
    pub fn apply_rebase(&mut self, node: &str, onto: &str) -> Result<RebasePreviewV1, UiError> {
        self.ensure_no_pending()?;
        let node = parse_node(node)?;
        let onto = parse_node(onto)?;
        let outcome = self
            .workspace
            .session_mut()
            .graph_mut()
            .rebase(&node, &onto)
            .map_err(|error| UiError::of(AppError::Evolution(error)))?;
        let (view, result) = rebase_view(&node, &onto, outcome);
        if let Some(result) = result {
            self.workspace.session_mut().open(&result)?;
            self.workspace.drop_caches();
            self.graph_dirty = true;
            self.last_proposals = None;
        }
        Ok(view)
    }

    fn ensure_no_pending(&self) -> Result<(), UiError> {
        if self.workspace.session().pending().is_some() {
            return Err(ui_error(
                "APP_DIRTY_WORKING_COPY",
                "save or discard the pending .chg first",
            ));
        }
        Ok(())
    }

    fn active(&self) -> Result<NodeId, UiError> {
        self.workspace
            .session()
            .active()
            .cloned()
            .ok_or_else(|| UiError::of(AppError::NoActiveNode))
    }

    fn effective_distribution(
        &self,
        overrides: &[SegmentWeight],
    ) -> Result<EffectiveDistribution, UiError> {
        let catalog = embedded_catalog().map_err(|error| UiError::of(AppError::Library(error)))?;
        let selected = catalog
            .select(self.workspace.session().libraries())
            .map_err(|error| UiError::of(AppError::Library(error)))?;
        let packages = selected
            .packages
            .iter()
            .filter_map(|id| catalog.packages().iter().find(|package| &package.id == id))
            .collect::<Vec<_>>();
        let prior = load_prior_from_packages(&packages)
            .map_err(|error| ui_error("APP_WEIGHT_PRIOR", error.to_string()))?;
        let mut manual = WeightTable::new();
        if let Some(project) = self.workspace.project() {
            for (segment, weight) in &project.weights {
                manual.set(segment.clone(), *weight);
            }
        }
        for entry in overrides {
            if entry.segment.trim().is_empty() || !entry.weight.is_finite() || entry.weight < 0.0 {
                return Err(ui_error(
                    "APP_INVALID_WEIGHT",
                    "segments must be non-empty and weights finite and non-negative",
                ));
            }
            manual.set(entry.segment.trim(), entry.weight);
        }
        Ok(EffectiveDistribution::from_prior(prior).with_manual(manual))
    }
}

fn proposal_view(proposal: &Proposal) -> ProposalV1 {
    ProposalV1 {
        phon: proposal.phon.clone(),
        score: proposal.score,
        rationale: proposal.rationale.clone(),
    }
}

fn parse_node(id: &str) -> Result<NodeId, UiError> {
    NodeId::parse(id).map_err(|_| ui_error("APP_UNKNOWN_NODE", id))
}

fn rebase_view(
    node: &NodeId,
    onto: &NodeId,
    outcome: RebaseOutcome,
) -> (RebasePreviewV1, Option<NodeId>) {
    let (status, statement, message, result) = match outcome {
        RebaseOutcome::Clean(id) => ("clean", None, None, Some(id)),
        RebaseOutcome::Conflict { statement, error } => {
            ("conflict", statement, Some(error.to_string()), None)
        }
        RebaseOutcome::Environment(error) => ("environment", None, Some(error.to_string()), None),
        RebaseOutcome::Broken(error) => ("broken", None, Some(error.to_string()), None),
    };
    let result_text = result.as_ref().map(|id| id.as_str().to_owned());
    (
        RebasePreviewV1 {
            schema: UI_SCHEMA_V1.to_owned(),
            node: node.as_str().to_owned(),
            onto: onto.as_str().to_owned(),
            status: status.to_owned(),
            statement,
            message,
            result: result_text,
        },
        result,
    )
}

/// Launcher 可以在沒有專案時存在；Tauri 只管理這一個 slot。
#[derive(Debug, Default)]
pub struct ProjectSlot {
    session: Option<UiSession>,
}

impl ProjectSlot {
    pub fn summary(&self) -> Option<ProjectSummaryV1> {
        self.session.as_ref().map(UiSession::summary)
    }

    pub fn open(
        &mut self,
        path: impl AsRef<Path>,
        discard_dirty: bool,
    ) -> Result<ProjectSummaryV1, UiError> {
        self.ensure_replace_allowed(discard_dirty)?;
        let next = UiSession::open(path, LibrarySpec::default())?;
        let summary = next.summary();
        self.session = Some(next);
        Ok(summary)
    }

    pub fn create(
        &mut self,
        path: impl AsRef<Path>,
        source_path: Option<impl AsRef<Path>>,
        name: Option<String>,
        namespace: &str,
        discard_dirty: bool,
    ) -> Result<ProjectSummaryV1, UiError> {
        self.ensure_replace_allowed(discard_dirty)?;
        let next = UiSession::create(path, source_path, name, namespace)?;
        let summary = next.summary();
        self.session = Some(next);
        Ok(summary)
    }

    pub fn close(&mut self, discard_dirty: bool) -> Result<(), UiError> {
        self.ensure_replace_allowed(discard_dirty)?;
        self.session = None;
        Ok(())
    }

    pub fn session(&self) -> Result<&UiSession, UiError> {
        self.session
            .as_ref()
            .ok_or_else(|| ui_error("APP_NO_PROJECT", "no project is open"))
    }

    pub fn session_mut(&mut self) -> Result<&mut UiSession, UiError> {
        self.session
            .as_mut()
            .ok_or_else(|| ui_error("APP_NO_PROJECT", "no project is open"))
    }

    fn ensure_replace_allowed(&self, discard_dirty: bool) -> Result<(), UiError> {
        if !discard_dirty && self.session.as_ref().is_some_and(UiSession::is_dirty) {
            return Err(ui_error(
                "APP_DIRTY_PROJECT",
                "save or discard the current project before replacing it",
            ));
        }
        Ok(())
    }
}
