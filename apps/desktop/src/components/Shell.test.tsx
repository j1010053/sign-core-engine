import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../i18n";
import type { ProjectSummary } from "../contracts";
import { useDirtyDraft } from "../dirtyGuard";
import { Shell } from "./Shell";

const mocks = vi.hoisted(() => ({
  saveProject: vi.fn(),
  summary: vi.fn(),
  closeProject: vi.fn(),
  saveWorkingCopy: vi.fn(),
  saveDialog: vi.fn(),
  setTitle: vi.fn(),
  onCloseRequested: vi.fn(),
  closeWindow: vi.fn(),
  unlistenClose: vi.fn(),
  saveDraft: vi.fn(),
  closeHandler: undefined as undefined | ((event: { preventDefault(): void }) => void | Promise<void>),
}));

vi.mock("../ipc", () => ({
  api: {
    saveProject: mocks.saveProject,
    summary: mocks.summary,
    closeProject: mocks.closeProject,
    saveWorkingCopy: mocks.saveWorkingCopy,
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setTitle: mocks.setTitle,
    onCloseRequested: mocks.onCloseRequested,
    close: mocks.closeWindow,
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: mocks.saveDialog,
}));

const project: ProjectSummary = {
  schema: "conlang.ui/v1",
  path: "C:\\Languages\\proto",
  name: "Proto Test",
  legacy: false,
  graph_dirty: true,
  has_pending: true,
  node_count: 2,
  active: "root",
  packages: [],
};

function DirtyRoute() {
  useDirtyDraft("test-draft", true, async () => {
    await mocks.saveDraft();
  });
  return <p>dirty route</p>;
}

function renderShell(
  dirtyRoute = false,
  projectValue = project,
  initialEntries: string[] = ["/"],
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const router = createMemoryRouter(
    [
      {
        path: "/",
        element: <Shell project={projectValue} />,
        children: [
          { index: true, element: dirtyRoute ? <DirtyRoute /> : <p>overview route</p> },
          { path: "analysis", element: <p>analysis route</p> },
        ],
      },
    ],
    { initialEntries },
  );
  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}

describe("Shell desktop commands", () => {
  afterEach(() => {
    cleanup();
    delete (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  beforeEach(() => {
    for (const mock of [
      mocks.saveProject,
      mocks.summary,
      mocks.closeProject,
      mocks.saveWorkingCopy,
      mocks.saveDialog,
      mocks.setTitle,
      mocks.onCloseRequested,
      mocks.closeWindow,
      mocks.unlistenClose,
      mocks.saveDraft,
    ]) mock.mockReset();
    mocks.closeHandler = undefined;
    mocks.saveProject.mockResolvedValue({ ...project, graph_dirty: false });
    mocks.summary.mockResolvedValue(project);
    mocks.closeProject.mockResolvedValue(undefined);
    mocks.saveWorkingCopy.mockResolvedValue(undefined);
    mocks.setTitle.mockResolvedValue(undefined);
    mocks.closeWindow.mockResolvedValue(undefined);
    mocks.saveDraft.mockResolvedValue(undefined);
    mocks.onCloseRequested.mockImplementation(async (handler) => {
      mocks.closeHandler = handler;
      return mocks.unlistenClose;
    });
  });

  it("saves dirty project state with Ctrl+S", async () => {
    renderShell();

    fireEvent.keyDown(window, { key: "s", ctrlKey: true });

    await waitFor(() => expect(mocks.saveProject).toHaveBeenCalledOnce());
  });

  it("uses Alt+number for workbench navigation", async () => {
    renderShell();

    fireEvent.keyDown(window, { key: "4", altKey: true });

    expect(await screen.findByText("analysis route")).toBeInTheDocument();
  });

  it("blocks route changes until a local draft is saved, discarded, or cancelled", async () => {
    renderShell(true);

    fireEvent.keyDown(window, { key: "4", altKey: true });
    expect(await screen.findByRole("heading", { name: "草稿尚未同步" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.getByText("dirty route")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "4", altKey: true });
    fireEvent.click(await screen.findByRole("button", { name: "儲存並離開" }));

    await waitFor(() => expect(mocks.saveDraft).toHaveBeenCalledOnce());
    expect(await screen.findByText("analysis route")).toBeInTheDocument();
  });

  it("can explicitly discard a local draft and continue navigation", async () => {
    renderShell(true);

    fireEvent.click(screen.getByRole("link", { name: /分析與群組/ }));
    fireEvent.click(await screen.findByRole("button", { name: "捨棄並離開" }));

    expect(await screen.findByText("analysis route")).toBeInTheDocument();
    expect(mocks.saveDraft).not.toHaveBeenCalled();
  });

  it("blocks browser history navigation while a local draft is dirty", async () => {
    const router = renderShell(true, project, ["/analysis", "/"]);

    await act(async () => {
      await router.navigate(-1);
    });
    expect(await screen.findByRole("heading", { name: "草稿尚未同步" })).toBeInTheDocument();
    expect(screen.getByText("dirty route")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "捨棄並離開" }));
    expect(await screen.findByText("analysis route")).toBeInTheDocument();
  });

  it("keeps the current route and draft when saving before navigation fails", async () => {
    mocks.saveDraft.mockRejectedValueOnce(new Error("invalid local draft"));
    renderShell(true);

    fireEvent.click(screen.getByRole("link", { name: /分析與群組/ }));
    fireEvent.click(await screen.findByRole("button", { name: "儲存並離開" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("invalid local draft");
    expect(screen.getByText("dirty route")).toBeInTheDocument();
    expect(screen.queryByText("analysis route")).not.toBeInTheDocument();
  });

  it("intercepts native close and discards only after explicit confirmation", async () => {
    (window as typeof window & { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    renderShell();
    await waitFor(() => expect(mocks.onCloseRequested).toHaveBeenCalledOnce());
    const preventDefault = vi.fn();

    await mocks.closeHandler?.({ preventDefault });
    expect(preventDefault).toHaveBeenCalledOnce();
    expect(await screen.findByRole("heading", { name: "是否要儲存變更？" })).toBeInTheDocument();
    expect(screen.getByText("要儲存對「Proto Test」所做的變更嗎？")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "不儲存" }));
    await waitFor(() => expect(mocks.closeWindow).toHaveBeenCalledOnce());
    expect(mocks.closeProject).not.toHaveBeenCalled();
  });

  it("cancels a native close without saving or closing", async () => {
    (window as typeof window & { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    renderShell();
    await waitFor(() => expect(mocks.onCloseRequested).toHaveBeenCalledOnce());

    await mocks.closeHandler?.({ preventDefault: vi.fn() });
    fireEvent.click(await screen.findByRole("button", { name: "取消" }));

    expect(screen.queryByRole("heading", { name: "是否要儲存變更？" })).not.toBeInTheDocument();
    expect(mocks.saveProject).not.toHaveBeenCalled();
    expect(mocks.saveWorkingCopy).not.toHaveBeenCalled();
    expect(mocks.closeWindow).not.toHaveBeenCalled();
  });

  it("allows native close without a prompt when project and editor are clean", async () => {
    (window as typeof window & { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    renderShell(false, { ...project, graph_dirty: false, has_pending: false });
    await waitFor(() => expect(mocks.onCloseRequested).toHaveBeenCalledOnce());
    const preventDefault = vi.fn();

    await mocks.closeHandler?.({ preventDefault });

    expect(preventDefault).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { name: "是否要儲存變更？" })).not.toBeInTheDocument();
  });

  it("saves pending before graph and closes only after both succeed", async () => {
    (window as typeof window & { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    mocks.saveDialog.mockResolvedValue("C:\\draft.chg");
    renderShell();
    await waitFor(() => expect(mocks.onCloseRequested).toHaveBeenCalledOnce());

    await mocks.closeHandler?.({ preventDefault: vi.fn() });
    fireEvent.click(await screen.findByRole("button", { name: "儲存" }));

    await waitFor(() => expect(mocks.closeWindow).toHaveBeenCalledOnce());
    expect(mocks.saveWorkingCopy).toHaveBeenCalledWith("C:\\draft.chg");
    expect(mocks.saveWorkingCopy.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.saveProject.mock.invocationCallOrder[0],
    );
    expect(mocks.saveProject.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.closeWindow.mock.invocationCallOrder[0],
    );
    expect(mocks.closeProject).not.toHaveBeenCalled();
  });

  it("saves a graph-only change without asking for a working-copy path", async () => {
    (window as typeof window & { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    const graphOnly = { ...project, has_pending: false };
    mocks.summary.mockResolvedValue(graphOnly);
    renderShell(false, graphOnly);
    await waitFor(() => expect(mocks.onCloseRequested).toHaveBeenCalledOnce());

    await mocks.closeHandler?.({ preventDefault: vi.fn() });
    fireEvent.click(await screen.findByRole("button", { name: "儲存" }));

    await waitFor(() => expect(mocks.closeWindow).toHaveBeenCalledOnce());
    expect(mocks.saveDialog).not.toHaveBeenCalled();
    expect(mocks.saveWorkingCopy).not.toHaveBeenCalled();
    expect(mocks.saveProject).toHaveBeenCalledOnce();
  });

  it("does not mutate the session when the close Save As dialog is cancelled", async () => {
    (window as typeof window & { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    mocks.saveDialog.mockResolvedValue(null);
    renderShell();
    await waitFor(() => expect(mocks.onCloseRequested).toHaveBeenCalledOnce());

    await mocks.closeHandler?.({ preventDefault: vi.fn() });
    fireEvent.click(await screen.findByRole("button", { name: "儲存" }));

    await waitFor(() => expect(mocks.saveDialog).toHaveBeenCalledOnce());
    expect(mocks.summary).not.toHaveBeenCalled();
    expect(mocks.saveWorkingCopy).not.toHaveBeenCalled();
    expect(mocks.saveProject).not.toHaveBeenCalled();
    expect(mocks.closeProject).not.toHaveBeenCalled();
    expect(mocks.closeWindow).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: "是否要儲存變更？" })).toBeInTheDocument();
  });

  it("keeps the dirty window open when saving the working copy fails", async () => {
    (window as typeof window & { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    mocks.saveDialog.mockResolvedValue("C:\\draft.chg");
    mocks.saveWorkingCopy.mockRejectedValue(new Error("disk full"));
    renderShell();
    await waitFor(() => expect(mocks.onCloseRequested).toHaveBeenCalledOnce());

    await mocks.closeHandler?.({ preventDefault: vi.fn() });
    fireEvent.click(await screen.findByRole("button", { name: "儲存" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
    expect(mocks.saveProject).not.toHaveBeenCalled();
    expect(mocks.closeWindow).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: "是否要儲存變更？" })).toBeInTheDocument();
  });
});
