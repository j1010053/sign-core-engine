import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../i18n";
import { EvolutionPage } from "./EvolutionPage";

const mocks = vi.hoisted(() => ({
  summary: vi.fn(),
  pendingChange: vi.fn(),
  beginEdit: vi.fn(),
  replacePendingSource: vi.fn(),
  authoringCatalog: vi.fn(),
  authoringMoveOptions: vi.fn(),
  stageStructuredEdit: vi.fn(),
  stageSoundChange: vi.fn(),
  discardLastEdit: vi.fn(),
  commit: vi.fn(),
  saveProject: vi.fn(),
  undo: vi.fn(),
  redo: vi.fn(),
  removeActiveLeaf: vi.fn(),
  saveWorkingCopy: vi.fn(),
  loadWorkingCopy: vi.fn(),
}));

vi.mock("../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../ipc")>();
  return { ...actual, api: mocks };
});

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={queryClient}>
      <EvolutionPage />
    </QueryClientProvider>,
  );
}

describe("Evolution working copy", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  beforeEach(() => {
    for (const mock of Object.values(mocks)) mock.mockReset();
    mocks.summary.mockResolvedValue({
      schema: "conlang.ui/v1",
      path: "C:\\Languages\\proto",
      name: "Proto Test",
      legacy: false,
      graph_dirty: false,
      has_pending: true,
      node_count: 1,
      active: "root",
      packages: [],
    });
    mocks.pendingChange.mockResolvedValue({
      schema: "conlang.ui/v1",
      source: "changeset ui:test:\n    schema = conlang.changeset/v1\n",
      statements: 0,
      diff: { aligned: 0, born: 0, died: 0, phon: 0, syn: 0, sem: 0, prag: 0, structural: 0 },
    });
    mocks.authoringCatalog.mockResolvedValue({
      schema: "conlang.ui/v1",
      revision: "rev-1",
      nodes: [
        {
          selector: "node(sign, @root:1)",
          kind: "sign",
          path: "sign kat",
          summary: "kat",
          deletable: true,
          movable: true,
          fields: [{ name: "name", label: "Name", control: "text", choices: [] }],
        },
      ],
      signs: [{ name: "kat", selector: "node(sign, @root:1)" }],
      traits: [
        { name: "Core", global: true, blocks: 1, source: "local", selector: "node(trait, @root:2)" },
        { name: "Noun", global: false, blocks: 1, source: "library" },
      ],
      rule_homes: [{ value: "Core", label: "Core" }],
      body_containers: [{ value: "node(sign, @root:1)", label: "sign kat" }],
    });
    mocks.stageSoundChange.mockResolvedValue({
      schema: "conlang.ui/v1",
      source: "changeset ui:test:\n    schema = conlang.changeset/v1\n\n    #0:\n        insert ...\n",
      statements: 1,
      diff: { aligned: 1, born: 0, died: 0, phon: 1, syn: 0, sem: 0, prag: 0, structural: 0 },
    });
    mocks.stageStructuredEdit.mockResolvedValue({
      schema: "conlang.ui/v1",
      source: "changeset ui:test:\n    schema = conlang.changeset/v1\n\n    #0:\n        insert ...\n",
      statements: 1,
      diff: { aligned: 1, born: 1, died: 0, phon: 0, syn: 0, sem: 0, prag: 0, structural: 1 },
    });
  });

  it("does not submit the sound-change examples as real working-copy input", async () => {
    renderPage();

    const stage = await screen.findByRole("button", { name: "加入 working copy" });
    expect(stage).toBeDisabled();
    expect(screen.getByLabelText("音變規則")).toHaveAttribute("placeholder", "t => k");
    expect(screen.getByLabelText("規則歸屬")).toHaveValue("");
    fireEvent.click(stage);
    expect(mocks.stageSoundChange).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("音變規則"), { target: { value: "a => e" } });
    expect(stage).toBeDisabled();
    fireEvent.change(screen.getByLabelText("規則歸屬"), { target: { value: "Core" } });
    await waitFor(() => expect(stage).toBeEnabled());
    expect(mocks.stageSoundChange).not.toHaveBeenCalled();
  });

  it("keeps every guided tool available before a working copy exists", async () => {
    mocks.summary
      .mockResolvedValueOnce({
        schema: "conlang.ui/v1",
        path: "C:\\Languages\\proto",
        name: "Proto Test",
        legacy: false,
        graph_dirty: false,
        has_pending: false,
        node_count: 1,
        active: "root",
        packages: [],
      })
      .mockResolvedValue({
        schema: "conlang.ui/v1",
        path: "C:\\Languages\\proto",
        name: "Proto Test",
        legacy: false,
        graph_dirty: false,
        has_pending: true,
        node_count: 1,
        active: "root",
        packages: [],
      });
    renderPage();

    expect(await screen.findByRole("button", { name: "開始空白 raw working copy" })).toBeVisible();
    fireEvent.click(screen.getByRole("tab", { name: "Insert Trait" }));
    fireEvent.change(await screen.findByLabelText("名稱"), { target: { value: "Aspect" } });
    fireEvent.click(screen.getByLabelText("Global trait"));
    fireEvent.click(screen.getByRole("button", { name: "加入 working copy" }));

    await waitFor(() => expect(mocks.stageStructuredEdit).toHaveBeenCalledWith({
      revision: "rev-1",
      action: "insert_trait",
      name: "Aspect",
      global: true,
    }));
    expect(await screen.findByText(/操作已 Stage/)).toBeVisible();
  });

  it("confirms delete and does not submit when the user cancels", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    renderPage();
    fireEvent.click(await screen.findByRole("tab", { name: "Delete" }));
    fireEvent.change(await screen.findByLabelText("目標節點"), { target: { value: "node(sign, @root:1)" } });
    fireEvent.click(screen.getByRole("button", { name: "刪除" }));
    expect(confirm).toHaveBeenCalledOnce();
    expect(mocks.stageStructuredEdit).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it("retains structured input when staging fails", async () => {
    mocks.stageStructuredEdit.mockRejectedValueOnce(new Error("duplicate trait"));
    renderPage();
    fireEvent.click(await screen.findByRole("tab", { name: "Insert Trait" }));
    const name = await screen.findByLabelText("名稱");
    fireEvent.change(name, { target: { value: "Noun" } });
    fireEvent.click(screen.getByRole("button", { name: "加入 working copy" }));

    expect(await screen.findByText("duplicate trait")).toBeVisible();
    expect(name).toHaveValue("Noun");
  });
});
