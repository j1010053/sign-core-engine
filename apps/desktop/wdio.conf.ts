import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { TauriCapabilities } from "@wdio/tauri-service";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const binary = path.join(root, "target", "debug", process.platform === "win32" ? "langcraft-desktop.exe" : "langcraft-desktop");
const capabilities: TauriCapabilities = { browserName: "tauri", "tauri:options": { application: binary } };

export const config: WebdriverIO.Config = {
  runner: "local",
  outputDir: path.join(root, "target", "wdio-logs"),
  // 巢狀陣列 = wdio 的 spec group:群組內的檔案在**同一個 worker、同一個
  // session** 內依序跑。這裡必須如此,原因是 embedded provider 的一個硬約束:
  //
  //   driverProvider "embedded" 只會生**一個** app 行程,而內嵌的 WebDriver
  //   server 就住在那個行程裡。每個 worker 結束時會呼叫 deleteSession(),
  //   Windows 的 WebView2 會因此讓 app 退出——於是下一個 spec 連到屍體,
  //   得到 `fetch failed`,後續全部 ECONNREFUSED。
  //
  // Linux 的 WebKitGTK 剛好在 deleteSession 之後沒退出,所以這條路徑
  // **只在 Windows 現形**。
  //
  // 順序有意義:launcher 斷言「空的 project slot」,必須排在 workbench
  // 開專案之前——兩者共用同一個 app 行程,也就共用同一個 ProjectSlot。
  //
  // 別改成 maxInstances > 1 想讓它們各拿一個 app:embedded provider 不吃
  // 那條路(那個 perWorkerMode 判斷是給 tauri-driver 用的),實測結果是兩個
  // worker 併行搶同一個 ProjectSlot,workbench 的專案會漏進 launcher 的斷言。
  specs: [["./e2e/launcher.e2e.ts", "./e2e/workbench.e2e.ts"]],
  maxInstances: 1,
  capabilities: [capabilities],
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { timeout: 60_000 },
  services: [["@wdio/tauri-service", {
    appBinaryPath: binary,
    driverProvider: "embedded",
    // The embedded driver dies with the app. Forward Rust stderr so CI keeps
    // the original panic/stack-overflow message instead of only `fetch failed`.
    captureBackendLogs: true,
  }]],
  onPrepare() {
    // Windows 必須走 shell:自 CVE-2024-27980 修補起(Node ≥18.20.2),
    // spawn 一個 `.cmd`/`.bat` 而不開 shell 會直接回 EINVAL——`pnpm` 在
    // Windows 正是 `pnpm.cmd`。症狀很容易誤讀成建置失敗:status 是 `null`
    // 而不是非零,且 stdio:"inherit" 什麼都印不出來(cargo 根本沒被執行)。
    const windows = process.platform === "win32";
    const result = spawnSync(
      "pnpm",
      ["tauri", "build", "--debug", "--no-bundle", "--features", "wdio", "--config", "src-tauri/tauri.e2e.conf.json"],
      {
        cwd: here,
        env: { ...process.env, VITE_WDIO: "true" },
        stdio: "inherit",
        shell: windows,
      },
    );
    // `result.error` 一定要往外帶:spawn 失敗(EINVAL/ENOENT)與建置失敗
    // 兩者的 status 都不是 0,但只有 error 分得出是哪一種。先前這裡把它
    // 丟掉了,於是 CI 上只留下一句 "failed with no exit code"。
    if (result.error) throw new Error(`Tauri E2E build could not be started: ${result.error.message}`);
    if (result.status !== 0) throw new Error(`Tauri E2E build failed with ${result.status ?? "no exit code"}`);
  },
};
