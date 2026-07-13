# corpus/ — Lexurgy 社群語料(I5:submodule,不 vendor)

- `lexurgy/` = git submodule → https://github.com/def-gthill/lexurgy(GPL-3.0)。
  **僅作測試資料與行為參照**;哨兵規則(CLAUDE.md §0):不翻譯其實作。
- 黑盒比對(.lsc → 匯入轉換器 → 本引擎 → 與期望 .wli 比對)= **M2 匯入器**;
  M0 步驟 7 交付 = harness 骨架(.wli 讀取器 + .lsc 規則分類器)+ 分歧白名單初版。
- 白名單:`whitelist.md`。
