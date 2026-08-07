describe("LangCraft launcher", () => {
  it("boots with the Tauri IPC bridge and exposes the empty project slot", async () => {
    // 品牌字樣在 eyebrow(硬編於 JSX,不隨語系變),`h1` 是**已翻譯**的標題。
    // 先前這裡斷言 `h1` = "LangCraft" —— 畫面上從來沒有那個 h1,
    // "LangCraft" 只存在於 index.html 的 <title>。這條測試在本次修好編譯錯誤
    // 之前從未被執行過(CI 停在 cargo check),所以錯誤的假設一直沒被發現。
    await expect($(".eyebrow")).toHaveText("LANGCRAFT / DESKTOP");
    await expect(browser).toHaveTitle("LangCraft");
    expect(await $("h1").getText()).toBe("從一份語言開始");
    expect(await $("body").getText()).toContain("開啟專案資料夾");
    const summary = await browser.tauri.execute(async ({ core }) => core.invoke("project_summary"));
    expect(summary).toBeNull();
  });
});
