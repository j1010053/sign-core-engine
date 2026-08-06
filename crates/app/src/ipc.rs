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

use conlang_changeset::evolution::NodeId;
use conlang_changeset::state::EvolutionState;
use conlang_persistence::GraphStore;
use conlang_query::{LexiconFilter, SortKey, ViewConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::wire::{EvolutionTreeV1, LexiconViewV1, NodeDetailV1};
use crate::{AppError, Workspace};

/// 過 IPC 的錯誤。**帶可比對的 `code`**,前端不必解析訊息文字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiError {
    pub code: String,
    pub message: String,
}

impl UiError {
    fn of(error: impl std::fmt::Display) -> UiError {
        let text = error.to_string();
        // 既有錯誤一律是 `CODE: 詳細` 的形狀(APP_*/PERSISTENCE_*/CHANGESET_*…)
        let (code, message) = match text.split_once(": ") {
            Some((code, rest)) if code.chars().all(|c| c.is_ascii_uppercase() || c == '_') => {
                (code.to_owned(), rest.to_owned())
            }
            _ => ("APP_ERROR".to_owned(), text.clone()),
        };
        UiError { code, message }
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
}

impl UiSession {
    /// 開啟專案。`fallback` 供沒有 `project.toml` 的舊 store。
    pub fn open(
        path: impl AsRef<Path>,
        fallback: conlang_language::LibrarySpec,
    ) -> Result<UiSession, UiError> {
        let store = GraphStore::open(path.as_ref())?;
        let workspace = Workspace::open(&store, fallback)?;
        Ok(UiSession { store, workspace })
    }

    // ── 演化樹面板 ──────────────────────────────────────────────────────

    pub fn tree(&self) -> EvolutionTreeV1 {
        self.workspace.tree()
    }

    /// 點一個節點 → 切過去並回傳編輯頁要的東西。
    pub fn select_node(&mut self, id: &str) -> Result<NodeDetailV1, UiError> {
        let node = NodeId::parse(id).map_err(|_| UiError {
            code: "APP_UNKNOWN_NODE".to_owned(),
            message: id.to_owned(),
        })?;
        self.workspace.session_mut().open(&node)?;
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
        self.node_detail()
    }

    /// 寫旁註。旁註層**正交於本體**(07 §5c):不參與 replay、不被 diff、
    /// 不約束生成。
    pub fn write_annotation(
        &mut self,
        path: &str,
        content: &str,
    ) -> Result<NodeDetailV1, UiError> {
        let id = self.active()?;
        self.store.write_annotation(&id, path, content.as_bytes())?;
        self.node_detail()
    }

    pub fn read_annotation(&self, path: &str) -> Result<String, UiError> {
        let id = self.active()?;
        let bytes = self.store.read_annotation(&id, path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn active(&self) -> Result<NodeId, UiError> {
        self.workspace
            .session()
            .active()
            .cloned()
            .ok_or_else(|| UiError::of(AppError::NoActiveNode))
    }
}
