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

## 疑難排解

### 視窗開了但**全白**(NVIDIA + Linux)

症狀:`pnpm dev` 跑完、進程活著、`xwininfo` 也看得到視窗,但內容永遠是白底。
log 裡有:

```
KMS: DRM_IOCTL_MODE_CREATE_DUMB failed: Permission denied
Failed to create GBM buffer of size 1360x860: Permission denied
GBM-DRV error (nv_gbm_create_device_native)
```

WebKitGTK 預設走 **DMA-BUF** 做硬體合成;NVIDIA 驅動在部分 session 下拿不到
DRM 權限,而它失敗之後**不會退回軟體算圖,是整個不畫**——所以那片白是
WebKit 的預設底色,不是應用程式的背景(本專案是深色主題,`--bg: #0c1110`)。

看起來像應用程式壞了,其實是驅動路徑的問題。關掉那條路徑即可:

```sh
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
pnpm --filter @langcraft/desktop dev
```

實測(RTX,Ubuntu 24.04,X11):GPU 錯誤由一整串降為 0,視窗的相異顏色數由
**3**(等於全白)變成 **180**,主色 `#0c1110`——與 `styles.css` 的 `--bg` 相符。

這兩個變數是**機器相關**的,故不寫進程式碼或 `tauri.conf.json`。

### `node` / `pnpm` 找不到(nvm)

nvm 靠 `.bashrc` 注入 `PATH`,**只對互動 shell 生效**。腳本、CI 步驟或工具開的
非互動 shell 會找不到它們,即使機器上明明裝了:

```sh
export PATH="$HOME/.nvm/versions/node/<版本>/bin:$PATH"
corepack enable          # pnpm 版本由根 package.json 的 packageManager 決定
```

### `cargo test --workspace` 在缺 GUI dev 套件的機器上整組失敗

見 CLAUDE.md §4.1。

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
