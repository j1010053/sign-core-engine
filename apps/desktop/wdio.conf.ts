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
    const result = spawnSync(
      process.platform === "win32" ? "pnpm.cmd" : "pnpm",
      ["tauri", "build", "--debug", "--no-bundle", "--features", "wdio", "--config", "src-tauri/tauri.e2e.conf.json"],
      {
        cwd: here,
        env: { ...process.env, VITE_WDIO: "true" },
        stdio: "inherit",
        shell: false,
      },
    );
    if (result.status !== 0) throw new Error(`Tauri E2E build failed with ${result.status ?? "no exit code"}`);
  },
};
