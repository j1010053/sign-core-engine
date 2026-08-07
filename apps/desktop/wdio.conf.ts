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
  specs: ["./e2e/**/*.e2e.ts"],
  maxInstances: 1,
  capabilities: [capabilities],
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { timeout: 60_000 },
  services: [["@wdio/tauri-service", { appBinaryPath: binary, driverProvider: "embedded" }]],
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
