import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../i18n";
import type { ProjectSummary } from "../contracts";
import { Shell } from "./Shell";

const { saveProject } = vi.hoisted(() => ({ saveProject: vi.fn() }));

vi.mock("../ipc", () => ({
  api: { saveProject },
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTitle: vi.fn().mockResolvedValue(undefined) }),
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

function renderShell() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={<Shell project={project} />}>
            <Route index element={<p>overview route</p>} />
            <Route path="analysis" element={<p>analysis route</p>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Shell desktop commands", () => {
  afterEach(cleanup);

  beforeEach(() => {
    saveProject.mockReset();
    saveProject.mockResolvedValue({ ...project, graph_dirty: false });
  });

  it("saves dirty project state with Ctrl+S", async () => {
    renderShell();

    fireEvent.keyDown(window, { key: "s", ctrlKey: true });

    await waitFor(() => expect(saveProject).toHaveBeenCalledOnce());
  });

  it("uses Alt+number for workbench navigation", async () => {
    renderShell();

    fireEvent.keyDown(window, { key: "4", altKey: true });

    expect(await screen.findByText("analysis route")).toBeInTheDocument();
  });
});
