import { invoke } from "@tauri-apps/api/core";
import { load } from "@tauri-apps/plugin-store";
import type { ZodType } from "zod";
import {
  authoringCatalogSchema,
  authoringMoveOptionsSchema,
  derivationViewSchema,
  evolutionTreeSchema,
  groupingViewSchema,
  intelligibilityViewSchema,
  lexiconViewSchema,
  nodeDetailSchema,
  packageCatalogSchema,
  pendingChangeSchema,
  projectSummarySchema,
  proposalsViewSchema,
  rebasePreviewSchema,
  sourceReconcileSchema,
  sourceViewSchema,
  statsViewSchema,
  uiErrorSchema,
  weightConfigSchema,
  type EvolutionState,
  type GroupingQuery,
  type LexiconQuery,
  type PackageSelection,
  type ProjectSummary,
  type ProposalQuery,
  type SegmentWeight,
  type StructuredEditInput,
  type UiError,
} from "./contracts";

export class LangCraftError extends Error implements UiError {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "LangCraftError";
    this.code = code;
  }
}

function normalizeError(error: unknown): LangCraftError {
  const parsed = uiErrorSchema.safeParse(error);
  if (parsed.success) return new LangCraftError(parsed.data.code, parsed.data.message);
  if (error instanceof Error) return new LangCraftError("APP_CLIENT", error.message);
  return new LangCraftError("APP_CLIENT", String(error));
}

async function call<T>(command: string, args: Record<string, unknown>, schema: ZodType<T>): Promise<T> {
  try {
    const raw = await invoke<unknown>(command, args);
    const parsed = schema.safeParse(raw);
    if (!parsed.success) {
      throw new LangCraftError(
        "UI_SCHEMA_MISMATCH",
        `LangCraft 與核心契約不相容：${parsed.error.issues[0]?.message ?? "unknown shape"}`,
      );
    }
    return parsed.data;
  } catch (error) {
    if (error instanceof LangCraftError) throw error;
    throw normalizeError(error);
  }
}

async function action(command: string, args: Record<string, unknown> = {}): Promise<void> {
  try {
    await invoke(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

const optionalSummarySchema = projectSummarySchema.nullable();

export const api = {
  summary: () => call("project_summary", {}, optionalSummarySchema),
  openProject: (path: string, discardDirty = false) =>
    call("open_project", { path, discardDirty }, projectSummarySchema),
  createProject: (input: {
    path: string;
    /** 省略 ⇒ 空白專案(引擎的 canonical empty root,P28)。 */
    sourcePath?: string;
    name?: string;
    namespace: string;
    discardDirty?: boolean;
  }) => call("create_project", { ...input, discardDirty: input.discardDirty ?? false }, projectSummarySchema),
  closeProject: (discardDirty = false) => action("close_project", { discardDirty }),
  packageCatalog: () => call("package_catalog", {}, packageCatalogSchema),
  configurePackages: (input: PackageSelection) =>
    call("configure_packages", { input }, projectSummarySchema),
  weightConfig: () => call("weight_config", {}, weightConfigSchema),
  setWeights: (entries: SegmentWeight[]) => call("set_weights", { entries }, weightConfigSchema),
  tree: () => call("tree", {}, evolutionTreeSchema),
  selectNode: (id: string) => call("select_node", { id }, nodeDetailSchema),
  lexicon: (query: LexiconQuery) => call("lexicon", { query }, lexiconViewSchema),
  nodeDetail: () => call("node_detail", {}, nodeDetailSchema),
  setLabel: (label?: string) => call("set_label", { label: label || null }, nodeDetailSchema),
  setState: (value: EvolutionState) => call("set_state", { value }, nodeDetailSchema),
  readAnnotation: (path: string) => invoke<string>("read_annotation", { path }),
  writeAnnotation: (path: string, content: string) =>
    call("write_annotation", { path, content }, nodeDetailSchema),
  beginEdit: (namespace: string) => call("begin_edit", { namespace }, pendingChangeSchema),
  pendingChange: () => call("pending_change", {}, pendingChangeSchema),
  replacePendingSource: (source: string) =>
    call("replace_pending_source", { source }, pendingChangeSchema),
  authoringCatalog: () => call("authoring_catalog", {}, authoringCatalogSchema),
  authoringMoveOptions: (target: string, revision: string) =>
    call(
      "authoring_move_options",
      { target, revision },
      authoringMoveOptionsSchema,
    ),
  stageStructuredEdit: (input: StructuredEditInput) =>
    call("stage_structured_edit", { input }, pendingChangeSchema),
  stageSoundChange: (rule: string, home = "Core", revision?: string) =>
    call(
      "stage_sound_change",
      { input: { rule, home, ...(revision ? { revision } : {}) } },
      pendingChangeSchema,
    ),
  discardLastEdit: () => call("discard_last_edit", {}, pendingChangeSchema),
  saveWorkingCopy: (path: string) => action("save_working_copy", { path }),
  saveWorkingCopySource: (path: string, source: string) =>
    call("save_working_copy_source", { path, source }, pendingChangeSchema),
  loadWorkingCopy: (path: string) => call("load_working_copy", { path }, pendingChangeSchema),
  commit: (label?: string) => call("commit_change", { label: label || null }, nodeDetailSchema),
  saveProject: () => call("save_project", {}, projectSummarySchema),
  undo: () => call("undo_navigation", {}, nodeDetailSchema),
  redo: () => call("redo_navigation", {}, nodeDetailSchema),
  removeActiveLeaf: () => call("remove_active_leaf", {}, evolutionTreeSchema),
  propose: (query: ProposalQuery) => call("propose", { query }, proposalsViewSchema),
  adoptProposal: (query: ProposalQuery, index: number) =>
    call("adopt_proposal", { query, index }, pendingChangeSchema),
  stats: (inventory: string[]) => call("stats", { inventory }, statsViewSchema),
  grouping: (query: GroupingQuery) => call("grouping", { query }, groupingViewSchema),
  assignGroup: (query: GroupingQuery, node: string, group: string) =>
    call("assign_group", { query, node, group }, groupingViewSchema),
  labelGroup: (query: GroupingQuery, group: string, label: string) =>
    call("label_group", { query, group, label }, groupingViewSchema),
  intelligibility: (source: string, target: string) =>
    call("intelligibility", { source, target }, intelligibilityViewSchema),
  derivation: (sign: string) => call("derivation", { sign }, derivationViewSchema),
  source: () => call("source", {}, sourceViewSchema),
  reconcileSource: (source: string) =>
    call("reconcile_source", { source }, sourceReconcileSchema),
  previewRebase: (node: string, onto: string) =>
    call("preview_rebase", { node, onto }, rebasePreviewSchema),
  applyRebase: (node: string, onto: string) =>
    call("apply_rebase", { node, onto }, rebasePreviewSchema),
};

export type RecentProject = Pick<ProjectSummary, "path" | "name"> & { openedAt: string };

export async function readRecents(): Promise<RecentProject[]> {
  try {
    const store = await load("recents.json", { autoSave: false, defaults: {} });
    return (await store.get<RecentProject[]>("projects")) ?? [];
  } catch {
    return [];
  }
}

export async function rememberProject(project: ProjectSummary): Promise<void> {
  const store = await load("recents.json", { autoSave: false, defaults: {} });
  const current = (await store.get<RecentProject[]>("projects")) ?? [];
  const next = [
    { path: project.path, name: project.name, openedAt: new Date().toISOString() },
    ...current.filter((item) => item.path !== project.path),
  ].slice(0, 12);
  await store.set("projects", next);
  await store.save();
}

export async function forgetRecent(path: string): Promise<void> {
  const store = await load("recents.json", { autoSave: false, defaults: {} });
  const current = (await store.get<RecentProject[]>("projects")) ?? [];
  await store.set("projects", current.filter((item) => item.path !== path));
  await store.save();
}
