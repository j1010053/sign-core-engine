# LangCraft desktop

LangCraft 是此 workspace 的單視窗桌面工作台。React 只保存路由、篩選器與尚未送出的
草稿；專案圖、working copy、生成、統計、群組、reconcile 與 rebase 都由
`conlang-app` 經 Tauri IPC 提供。前端不直接讀寫專案檔。

## 開發環境

- Node.js 24、pnpm 11、stable Rust。
- Windows：MSVC Build Tools 與 WebView2。
- Linux：WebKitGTK 4.1、libayatana-appindicator、librsvg、patchelf；E2E 另需 Xvfb。

從 repo 根目錄執行：

```sh
pnpm install --frozen-lockfile
pnpm --filter @langcraft/desktop dev
```

一般驗證：

```sh
cargo test --workspace --locked
cargo test --manifest-path tshiatun/Cargo.toml --workspace --locked
pnpm --filter @langcraft/desktop typecheck
pnpm --filter @langcraft/desktop lint
pnpm --filter @langcraft/desktop test
pnpm --filter @langcraft/desktop build
cargo check -p langcraft-desktop --locked
```

WebDriver 垂直測試會以 test-only Cargo feature 與 capability 建置，不會擴大正式應用權限：

```sh
pnpm --filter @langcraft/desktop typecheck:e2e
pnpm --filter @langcraft/desktop e2e
```

## 發佈

`tauri.conf.json` 產生 unsigned Windows NSIS、Linux AppImage 與 `.deb`。推送
`langcraft-v*` tag 或手動執行 `Desktop packages` workflow 會上傳安裝包 artifact；
簽章、憑證與自動更新不在 v1 範圍。

## 儲存邊界

- metadata 與 project-level manual weights 採立即、原子保存。
- `.chg` working copy、commit 到記憶體 graph、Save Project 是三個明確操作。
- package library 變更會先驗證 catalog 與完整 graph，dirty 時拒絕，成功才更新
  `project.toml` 並重開 session。
- proposal 採用只加入 pending `.chg`；phoneme projection 永遠只供報表。
