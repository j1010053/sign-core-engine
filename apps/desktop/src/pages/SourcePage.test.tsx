import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../i18n";
import { DirtyDraftProvider, type SaveDirtyDraft } from "../dirtyGuard";
import { SourcePage } from "./SourcePage";

const mocks = vi.hoisted(() => ({
  summary: vi.fn(),
  source: vi.fn(),
  tree: vi.fn(),
  reconcileSource: vi.fn(),
  previewRebase: vi.fn(),
  applyRebase: vi.fn(),
  registerDraft: vi.fn(),
}));

vi.mock("../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../ipc")>();
  return { ...actual, api: mocks };
});

vi.mock("@uiw/react-codemirror", () => ({
  default: ({ value, onChange }: { value: string; onChange(value: string): void }) => (
    <textarea
      aria-label="Canonical .lang source"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={queryClient}>
      <DirtyDraftProvider value={mocks.registerDraft}>
        <MemoryRouter>
          <SourcePage />
        </MemoryRouter>
      </DirtyDraftProvider>
    </QueryClientProvider>,
  );
  return queryClient;
}

describe("expert .lang draft", () => {
  beforeEach(() => {
    for (const mock of Object.values(mocks)) mock.mockReset();
    mocks.summary.mockResolvedValue({
      schema: "conlang.ui/v1",
      path: "C:\\Languages\\proto",
      name: "Proto Test",
      legacy: false,
      graph_dirty: false,
      has_pending: false,
      node_count: 1,
      active: "root",
      packages: [],
    });
    mocks.source.mockResolvedValue({
      schema: "conlang.ui/v1",
      node: "root",
      source: "sign old:\n",
    });
    mocks.tree.mockResolvedValue({
      schema: "conlang.ui/v1",
      active: "root",
      nodes: [{ id: "root", label: "Root", parents: [] }],
    });
    mocks.reconcileSource.mockResolvedValue({
      schema: "conlang.ui/v1",
      matched: 1,
      inserted: 0,
      deleted: 0,
      primitive_edits: 1,
      pending: {
        schema: "conlang.ui/v1",
        source: "changeset ui:source:\n    schema = conlang.changeset/v1\n",
        statements: 1,
        diff: { aligned: 1, born: 0, died: 0, phon: 0, syn: 0, sem: 0, prag: 0, structural: 1 },
      },
    });
  });

  afterEach(cleanup);

  it("registers the visible draft and preserves it across source refetches", async () => {
    const queryClient = renderPage();
    const editor = await screen.findByLabelText("Canonical .lang source");
    await waitFor(() => expect(editor).toHaveValue("sign old:\n"));

    fireEvent.change(editor, { target: { value: "sign local:\n" } });
    await waitFor(() => expect(editor).toHaveValue("sign local:\n"));
    await waitFor(() => expect(mocks.registerDraft).toHaveBeenCalledWith(
      "expert-source",
      expect.any(Function),
    ));

    queryClient.setQueryData(["source"], {
      schema: "conlang.ui/v1",
      node: "root",
      source: "sign refreshed:\n",
    });
    expect(editor).toHaveValue("sign local:\n");

    const saveDraft = [...mocks.registerDraft.mock.calls]
      .reverse()
      .find(([, save]) => typeof save === "function")?.[1] as SaveDirtyDraft;
    await saveDraft();
    expect(mocks.reconcileSource).toHaveBeenCalledWith("sign local:\n");
  });
});
