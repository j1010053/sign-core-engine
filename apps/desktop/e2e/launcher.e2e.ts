describe("LangCraft launcher", () => {
  it("boots with the Tauri IPC bridge and exposes the empty project slot", async () => {
    await expect($("h1")).toHaveText("LangCraft");
    expect(await $("body").getText()).toContain("開啟專案資料夾");
    const summary = await browser.tauri.execute(async ({ core }) => core.invoke("project_summary"));
    expect(summary).toBeNull();
  });
});
