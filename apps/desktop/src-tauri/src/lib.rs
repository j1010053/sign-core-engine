#![forbid(unsafe_code)]

use conlang_app::{
    DerivationViewV1, EvolutionTreeV1, GroupingQuery, GroupingViewV1, IntelligibilityViewV1,
    LexiconQuery, LexiconViewV1, NodeDetailV1, PackageCatalogV1, PackageSelectionInput,
    PendingChangeV1, ProjectSlot, ProjectSummaryV1, ProposalQuery, ProposalsViewV1,
    RebasePreviewV1, SegmentWeight, SoundChangeInput, SourceReconcileV1, SourceViewV1, StatsViewV1,
    UiError, WeightConfigV1,
};
use conlang_changeset::state::EvolutionState;
use std::sync::{Mutex, MutexGuard};
use tauri::State;

/// `State<'r, T>` 內含 `&'r T`,故它有**兩個**生命週期:包裝本身的借用,
/// 與裡面那個引用的 `'r`。省略規則在多個輸入生命週期下無從推斷回傳值該綁哪一個
/// (E0106),必須指名。
///
/// 綁 `'r`(經 `inner()`)而非 `&self` 的借用:guard 真正能活的是 `'r`,
/// 綁短的那個會讓呼叫端在「把 guard 存成變數再跨語句使用」時撞到
/// 「temporary dropped while borrowed」。
fn locked<'r>(
    state: &State<'r, Mutex<ProjectSlot>>,
) -> Result<MutexGuard<'r, ProjectSlot>, UiError> {
    state.inner().lock().map_err(|_| UiError {
        code: "APP_STATE_POISONED".to_owned(),
        message: "the project session lock is unavailable".to_owned(),
    })
}

#[tauri::command]
fn project_summary(
    state: State<'_, Mutex<ProjectSlot>>,
) -> Result<Option<ProjectSummaryV1>, UiError> {
    Ok(locked(&state)?.summary())
}

#[tauri::command]
fn open_project(
    state: State<'_, Mutex<ProjectSlot>>,
    path: String,
    discard_dirty: bool,
) -> Result<ProjectSummaryV1, UiError> {
    locked(&state)?.open(path, discard_dirty)
}

#[tauri::command]
fn create_project(
    state: State<'_, Mutex<ProjectSlot>>,
    path: String,
    source_path: String,
    name: Option<String>,
    namespace: String,
    discard_dirty: bool,
) -> Result<ProjectSummaryV1, UiError> {
    locked(&state)?.create(path, source_path, name, &namespace, discard_dirty)
}

#[tauri::command]
fn close_project(state: State<'_, Mutex<ProjectSlot>>, discard_dirty: bool) -> Result<(), UiError> {
    locked(&state)?.close(discard_dirty)
}

#[tauri::command]
fn package_catalog(state: State<'_, Mutex<ProjectSlot>>) -> Result<PackageCatalogV1, UiError> {
    locked(&state)?.session()?.package_catalog()
}

#[tauri::command]
fn configure_packages(
    state: State<'_, Mutex<ProjectSlot>>,
    input: PackageSelectionInput,
) -> Result<ProjectSummaryV1, UiError> {
    locked(&state)?.session_mut()?.configure_packages(input)
}

#[tauri::command]
fn weight_config(state: State<'_, Mutex<ProjectSlot>>) -> Result<WeightConfigV1, UiError> {
    locked(&state)?.session()?.weight_config()
}

#[tauri::command]
fn set_weights(
    state: State<'_, Mutex<ProjectSlot>>,
    entries: Vec<SegmentWeight>,
) -> Result<WeightConfigV1, UiError> {
    locked(&state)?.session_mut()?.set_weights(entries)
}

#[tauri::command]
fn tree(state: State<'_, Mutex<ProjectSlot>>) -> Result<EvolutionTreeV1, UiError> {
    Ok(locked(&state)?.session()?.tree())
}

#[tauri::command]
fn select_node(state: State<'_, Mutex<ProjectSlot>>, id: String) -> Result<NodeDetailV1, UiError> {
    locked(&state)?.session_mut()?.select_node(&id)
}

#[tauri::command]
fn lexicon(
    state: State<'_, Mutex<ProjectSlot>>,
    query: LexiconQuery,
) -> Result<LexiconViewV1, UiError> {
    locked(&state)?.session_mut()?.lexicon(&query)
}

#[tauri::command]
fn node_detail(state: State<'_, Mutex<ProjectSlot>>) -> Result<NodeDetailV1, UiError> {
    locked(&state)?.session()?.node_detail()
}

#[tauri::command]
fn set_label(
    state: State<'_, Mutex<ProjectSlot>>,
    label: Option<String>,
) -> Result<NodeDetailV1, UiError> {
    locked(&state)?.session_mut()?.set_label(label)
}

#[tauri::command]
fn set_state(
    state: State<'_, Mutex<ProjectSlot>>,
    value: EvolutionState,
) -> Result<NodeDetailV1, UiError> {
    locked(&state)?.session_mut()?.set_state(&value)
}

#[tauri::command]
fn write_annotation(
    state: State<'_, Mutex<ProjectSlot>>,
    path: String,
    content: String,
) -> Result<NodeDetailV1, UiError> {
    locked(&state)?
        .session_mut()?
        .write_annotation(&path, &content)
}

#[tauri::command]
fn read_annotation(state: State<'_, Mutex<ProjectSlot>>, path: String) -> Result<String, UiError> {
    locked(&state)?.session()?.read_annotation(&path)
}

#[tauri::command]
fn begin_edit(
    state: State<'_, Mutex<ProjectSlot>>,
    namespace: String,
) -> Result<PendingChangeV1, UiError> {
    locked(&state)?.session_mut()?.begin_edit(&namespace)
}

#[tauri::command]
fn pending_change(state: State<'_, Mutex<ProjectSlot>>) -> Result<PendingChangeV1, UiError> {
    locked(&state)?.session()?.pending_change()
}

#[tauri::command]
fn replace_pending_source(
    state: State<'_, Mutex<ProjectSlot>>,
    source: String,
) -> Result<PendingChangeV1, UiError> {
    locked(&state)?
        .session_mut()?
        .replace_pending_source(&source)
}

#[tauri::command]
fn stage_sound_change(
    state: State<'_, Mutex<ProjectSlot>>,
    input: SoundChangeInput,
) -> Result<PendingChangeV1, UiError> {
    locked(&state)?.session_mut()?.stage_sound_change(&input)
}

#[tauri::command]
fn discard_last_edit(state: State<'_, Mutex<ProjectSlot>>) -> Result<PendingChangeV1, UiError> {
    locked(&state)?.session_mut()?.discard_last_edit()
}

#[tauri::command]
fn save_working_copy(state: State<'_, Mutex<ProjectSlot>>, path: String) -> Result<(), UiError> {
    locked(&state)?.session()?.save_working_copy(path)
}

#[tauri::command]
fn load_working_copy(
    state: State<'_, Mutex<ProjectSlot>>,
    path: String,
) -> Result<PendingChangeV1, UiError> {
    locked(&state)?.session_mut()?.load_working_copy(path)
}

#[tauri::command]
fn commit_change(
    state: State<'_, Mutex<ProjectSlot>>,
    label: Option<String>,
) -> Result<NodeDetailV1, UiError> {
    locked(&state)?.session_mut()?.commit(label)
}

#[tauri::command]
fn save_project(state: State<'_, Mutex<ProjectSlot>>) -> Result<ProjectSummaryV1, UiError> {
    locked(&state)?.session_mut()?.save_project()
}

#[tauri::command]
fn undo_navigation(state: State<'_, Mutex<ProjectSlot>>) -> Result<NodeDetailV1, UiError> {
    locked(&state)?.session_mut()?.undo_navigation()
}

#[tauri::command]
fn redo_navigation(state: State<'_, Mutex<ProjectSlot>>) -> Result<NodeDetailV1, UiError> {
    locked(&state)?.session_mut()?.redo_navigation()
}

#[tauri::command]
fn remove_active_leaf(state: State<'_, Mutex<ProjectSlot>>) -> Result<EvolutionTreeV1, UiError> {
    locked(&state)?.session_mut()?.remove_active_leaf()
}

#[tauri::command]
fn propose(
    state: State<'_, Mutex<ProjectSlot>>,
    query: ProposalQuery,
) -> Result<ProposalsViewV1, UiError> {
    locked(&state)?.session_mut()?.propose(&query)
}

#[tauri::command]
fn adopt_proposal(
    state: State<'_, Mutex<ProjectSlot>>,
    query: ProposalQuery,
    index: usize,
) -> Result<PendingChangeV1, UiError> {
    locked(&state)?.session_mut()?.adopt_proposal(&query, index)
}

#[tauri::command]
fn stats(
    state: State<'_, Mutex<ProjectSlot>>,
    inventory: Vec<String>,
) -> Result<StatsViewV1, UiError> {
    locked(&state)?.session()?.stats(&inventory)
}

#[tauri::command]
fn grouping(
    state: State<'_, Mutex<ProjectSlot>>,
    query: GroupingQuery,
) -> Result<GroupingViewV1, UiError> {
    locked(&state)?.session()?.grouping(&query)
}

#[tauri::command]
fn assign_group(
    state: State<'_, Mutex<ProjectSlot>>,
    query: GroupingQuery,
    node: String,
    group: String,
) -> Result<GroupingViewV1, UiError> {
    locked(&state)?.session()?.assign_group(&query, node, group)
}

#[tauri::command]
fn label_group(
    state: State<'_, Mutex<ProjectSlot>>,
    query: GroupingQuery,
    group: String,
    label: String,
) -> Result<GroupingViewV1, UiError> {
    locked(&state)?.session()?.label_group(&query, group, label)
}

#[tauri::command]
fn intelligibility(
    state: State<'_, Mutex<ProjectSlot>>,
    source: String,
    target: String,
) -> Result<IntelligibilityViewV1, UiError> {
    locked(&state)?.session()?.intelligibility(&source, &target)
}

#[tauri::command]
fn derivation(
    state: State<'_, Mutex<ProjectSlot>>,
    sign: String,
) -> Result<DerivationViewV1, UiError> {
    locked(&state)?.session_mut()?.derivation(&sign)
}

#[tauri::command]
fn source(state: State<'_, Mutex<ProjectSlot>>) -> Result<SourceViewV1, UiError> {
    locked(&state)?.session()?.source()
}

#[tauri::command]
fn reconcile_source(
    state: State<'_, Mutex<ProjectSlot>>,
    source: String,
) -> Result<SourceReconcileV1, UiError> {
    locked(&state)?.session_mut()?.reconcile_source(&source)
}

#[tauri::command]
fn preview_rebase(
    state: State<'_, Mutex<ProjectSlot>>,
    node: String,
    onto: String,
) -> Result<RebasePreviewV1, UiError> {
    locked(&state)?.session()?.preview_rebase(&node, &onto)
}

#[tauri::command]
fn apply_rebase(
    state: State<'_, Mutex<ProjectSlot>>,
    node: String,
    onto: String,
) -> Result<RebasePreviewV1, UiError> {
    locked(&state)?.session_mut()?.apply_rebase(&node, &onto)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build());
    #[cfg(feature = "wdio")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .manage(Mutex::new(ProjectSlot::default()))
        .invoke_handler(tauri::generate_handler![
            project_summary,
            open_project,
            create_project,
            close_project,
            package_catalog,
            configure_packages,
            weight_config,
            set_weights,
            tree,
            select_node,
            lexicon,
            node_detail,
            set_label,
            set_state,
            write_annotation,
            read_annotation,
            begin_edit,
            pending_change,
            replace_pending_source,
            stage_sound_change,
            discard_last_edit,
            save_working_copy,
            load_working_copy,
            commit_change,
            save_project,
            undo_navigation,
            redo_navigation,
            remove_active_leaf,
            propose,
            adopt_proposal,
            stats,
            grouping,
            assign_group,
            label_group,
            intelligibility,
            derivation,
            source,
            reconcile_source,
            preview_rebase,
            apply_rebase,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LangCraft");
}
