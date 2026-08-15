import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

const SOURCE = `Symbol k
Symbol a
Symbol t

Class consonant {k, t}
Class vowel {a}

global trait Core:

sign kat:
    belongs Noun
    phon:
        /kat/
    sem:
        senses:
            core = STONE
`;

async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return await browser.tauri.execute(
    async ({ core }, name, payload) => core.invoke(name, payload),
    command,
    args,
  ) as T;
}

describe("LangCraft F1-F5 vertical workbench", () => {
  let fixtureRoot = "";
  let projectPath = "";
  let sourcePath = "";

  before(async () => {
    fixtureRoot = await mkdtemp(path.join(tmpdir(), "langcraft-e2e-"));
    projectPath = path.join(fixtureRoot, "project");
    sourcePath = path.join(fixtureRoot, "root.lang");
    await writeFile(sourcePath, SOURCE, "utf8");
  });

  after(async () => {
    try {
      await invoke("close_project", { discardDirty: true });
    } catch {
      // The fixture may already be closed after an assertion failure.
    }
    await rm(fixtureRoot, { recursive: true, force: true });
  });

  it("creates, edits metadata, saves, closes and reopens", async () => {
    const created = await invoke<{ node_count: number }>("create_project", {
      path: projectPath,
      sourcePath,
      name: "E2E language",
      namespace: "e2e:root",
      discardDirty: false,
    });
    expect(created.node_count).toBe(1);
    await invoke("set_label", { label: "Proto E2E" });
    await invoke("save_project");
    await invoke("close_project", { discardDirty: false });
    await invoke("open_project", { path: projectPath, discardDirty: false });
    const detail = await invoke<{ label?: string }>("node_detail");
    expect(detail.label).toBe("Proto E2E");
  });

  it("keeps an invalid draft out of Rust pending, then commits a valid change", async () => {
    await invoke("begin_edit", { namespace: "e2e:evolve" });
    await expect(invoke("replace_pending_source", { source: "not a .chg" })).rejects.toBeTruthy();
    const unchanged = await invoke<{ statements: number }>("pending_change");
    expect(unchanged.statements).toBe(0);

    // Windows regression: this must execute through the async Tauri command.
    // A synchronous command runs lower/replay/diff on the 1 MiB UI-thread stack
    // and terminates the app before DirectEval can return a response.
    const pending = await invoke<{ statements: number; diff: { phon: number } }>(
      "stage_sound_change",
      { input: { rule: "t => k", home: "Core" } },
    );
    expect(pending.statements).toBeGreaterThan(0);
    // ⚠ **規則變動目前不進差異向量**,故 diff 全為零。
    //
    // `diff_vector` 只走 `signs`(以 SignId 對齊);音變加的是 `global trait Core`
    // 底下的一條規則,不是 sign。連帶後果:一次音變之後兩節點的互通度是 **1.0**,
    // 方言分群也看不見它。
    //
    // 這與《演化圖本體論》§6.1 不符——該節明列「規則性音變」為差異來源,
    // 且「差異 = ChangeSet 距離的分層投影」。屬**實作缺口**,不是設計選擇;
    // 待裁決點見 docs(規則歸哪一維、分母是什麼、範圍是否含 trait 本身)。
    //
    // 此處斷言**現況**而非期望,好讓缺口有指標:補上之後這條會紅,提醒同步更新。
    expect(pending.diff.phon).toBe(0);
    await invoke("commit_change", { label: "Evolved" });
    const dirty = await invoke<{ graph_dirty: boolean }>("project_summary");
    expect(dirty.graph_dirty).toBe(true);
    await invoke("save_project");
  });

  it("invalidates proposals after state changes and adoption only creates pending", async () => {
    await invoke("set_weights", {
      entries: [
        { segment: "a", weight: 1 },
        { segment: "k", weight: 1 },
        { segment: "t", weight: 0.5 },
      ],
    });
    const query = {
      name: "e2e_word",
      gloss: "TEST",
      categories: ["Noun"],
      template: "CVC",
      count: 3,
      seed: 17,
      weights: [],
    };
    const first = await invoke<{ proposals: unknown[] }>("propose", { query });
    expect(first.proposals).toHaveLength(3);
    await invoke("set_state", {
      value: { time: "100", region: "test", society: [], contacts: [] },
    });
    await expect(invoke("adopt_proposal", { query, index: 0 })).rejects.toBeTruthy();

    await invoke("propose", { query });
    await invoke("adopt_proposal", { query, index: 0 });
    const pending = await invoke<{ graph_dirty: boolean; has_pending: boolean }>("project_summary");
    expect(pending.has_pending).toBe(true);
    expect(pending.graph_dirty).toBe(false);
    await invoke("commit_change", { label: "Coined" });
    await invoke("save_project");
  });

  it("persists grouping overrides and previews rebase without mutating the graph", async () => {
    const tree = await invoke<{ nodes: Array<{ id: string; parents: unknown[] }>; active?: string }>("tree");
    const root = tree.nodes.find((node) => node.parents.length === 0);
    const child = tree.nodes.find((node) => node.parents.length > 0);
    expect(root).toBeDefined();
    expect(child).toBeDefined();

    const query = { view: "e2e", threshold: 0.6 };
    const grouped = await invoke<{ grouping: { members: Record<string, string> } }>("assign_group", {
      query,
      node: tree.active,
      group: "manual-e2e",
    });
    expect(grouped.grouping.members[tree.active ?? ""]).toBe("manual-e2e");

    const before = tree.nodes.length;
    await invoke("preview_rebase", { node: child?.id, onto: root?.id });
    const after = await invoke<{ nodes: unknown[] }>("tree");
    expect(after.nodes).toHaveLength(before);
    await invoke("close_project", { discardDirty: false });
    const reopened = await invoke<{ node_count: number }>("open_project", {
      path: projectPath,
      discardDirty: false,
    });
    expect(reopened.node_count).toBe(before);
  });
});
