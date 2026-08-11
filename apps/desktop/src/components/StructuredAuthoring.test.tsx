import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AuthoringCatalog } from "../contracts";
import "../i18n";
import { StructuredAuthoring } from "./StructuredAuthoring";

const mocks = vi.hoisted(() => ({
  stageSoundChange: vi.fn(),
  stageStructuredEdit: vi.fn(),
  authoringMoveOptions: vi.fn(),
}));

vi.mock("../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../ipc")>();
  return { ...actual, api: { ...actual.api, ...mocks } };
});

const catalog: AuthoringCatalog = {
  schema: "conlang.ui/v1",
  revision: "revision-1",
  nodes: [{
    selector: "node(sign, @root:1)",
    kind: "sign",
    path: "sign kat",
    summary: "kat",
    deletable: true,
    movable: true,
    fields: [{ name: "name", label: "Name", control: "text", choices: [] }],
  }],
  signs: [{ name: "kat", selector: "node(sign, @root:1)" }],
  traits: [
    { name: "Core", global: true, blocks: 1, source: "local", selector: "node(trait, @root:2)" },
    { name: "Noun", global: false, blocks: 1, source: "library" },
  ],
  rule_homes: [{ value: "Core", label: "Core" }],
  body_containers: [{ value: "node(sign, @root:1)", label: "sign kat" }],
};

const staged = {
  schema: "conlang.ui/v1" as const,
  source: "changeset ui:test:\n",
  statements: 1,
  diff: { aligned: 1, born: 1, died: 0, phon: 0, syn: 0, sem: 0, prag: 0, structural: 1 },
};

function renderPanel(rawDirty = false) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const onStaged = vi.fn();
  render(
    <QueryClientProvider client={queryClient}>
      <StructuredAuthoring
        catalog={catalog}
        rawDirty={rawDirty}
        statements={0}
        onStaged={onStaged}
        onDiscard={vi.fn()}
        discarding={false}
      />
    </QueryClientProvider>,
  );
  return onStaged;
}

describe("Structured authoring panel", () => {
  beforeEach(() => {
    for (const mock of Object.values(mocks)) mock.mockReset();
    mocks.stageSoundChange.mockResolvedValue(staged);
    mocks.stageStructuredEdit.mockResolvedValue(staged);
  });
  afterEach(cleanup);

  it("locks every structured submission while the raw draft is dirty", async () => {
    const user = userEvent.setup();
    renderPanel(true);
    await user.type(screen.getByLabelText("音變規則"), "a => e");
    await user.selectOptions(screen.getByLabelText("規則歸屬"), "Core");
    expect(screen.getByRole("button", { name: "加入 working copy" })).toBeDisabled();
    expect(screen.getByText(/Raw 草稿尚未 Validate/)).toBeVisible();
  });

  it("builds the complete Insert Sign payload", async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.click(screen.getByRole("tab", { name: "Insert Sign" }));
    await user.type(screen.getByLabelText("名稱"), "stone");
    await user.selectOptions(screen.getByLabelText("Belongs（可多選）"), "Noun");
    await user.type(screen.getByLabelText("Underlying phon"), "kat");
    await user.type(screen.getByLabelText("Core gloss"), "STONE");
    await user.click(screen.getByRole("button", { name: "加入 working copy" }));

    await waitFor(() => expect(mocks.stageStructuredEdit).toHaveBeenCalledWith({
      revision: "revision-1",
      action: "insert_sign",
      name: "stone",
      belongs: ["Noun"],
      phon: "kat",
      gloss: "STONE",
    }));
  });

  it("builds a fixed flat-rule Insert Body Item payload", async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.click(screen.getByRole("tab", { name: "Insert Body Item" }));
    await user.selectOptions(screen.getByLabelText("Sign／trait block"), "node(sign, @root:1)");
    await user.selectOptions(screen.getByLabelText("Body item 類型"), "rule");
    await user.selectOptions(screen.getByLabelText("Dimension"), "phon");
    await user.type(screen.getByLabelText("Rule body"), "t => k");
    await user.type(screen.getByLabelText("Rule 名稱（選填）"), "shift");
    await user.selectOptions(screen.getByLabelText("Stage"), "phrase");
    await user.click(screen.getByRole("button", { name: "加入 working copy" }));

    await waitFor(() => expect(mocks.stageStructuredEdit).toHaveBeenCalledWith({
      revision: "revision-1",
      action: "insert_body",
      container: "node(sign, @root:1)",
      body: { kind: "rule", dim: "phon", body: "t => k", name: "shift", stage: "phrase" },
    }));
  });

  it("uses catalog field metadata to build Update", async () => {
    const user = userEvent.setup();
    const onStaged = renderPanel();
    await user.click(screen.getByRole("tab", { name: "Update" }));
    await user.selectOptions(screen.getByLabelText("目標節點"), "node(sign, @root:1)");
    await user.type(await screen.findByLabelText("Name"), "renamed");
    await user.click(screen.getByRole("button", { name: "加入 working copy" }));

    await waitFor(() => expect(mocks.stageStructuredEdit).toHaveBeenCalledWith({
      revision: "revision-1",
      action: "update",
      target: "node(sign, @root:1)",
      field: "name",
      value: "renamed",
    }));
    await waitFor(() => expect(onStaged).toHaveBeenCalledWith(staged));
  });
});
