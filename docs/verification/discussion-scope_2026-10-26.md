# I31–I36 範圍提交驗證（2026-10-26）

本次只提交套件 export／限定名稱、巢狀宣告與參數傳遞、Sign／trait 模板一致性、
case 共用求值與取消命中後回退、Sign 實例型別及具名型別實參。
契約與歷史驗證分別由 [I 系列索引](../implementation/05_M0實作參照_v1.0.md)
及[驗證入口](README.md)連結。

## 提交範圍

以既有 V3 基礎提交 `3739935` 為基底，對照本段實作開始前的本機檔案快照，
在隔離副本組合提交內容。包含相關解析、編譯、完整推導、來源身分、修改重播、
套件與快取、持久化、查詢、測試、範例及規格。

其他工作中的 observability、桌面除錯、CLI／app preview、一般模組拆分、
語意／語用擴充、編輯器及課程重整未包含於此提交。
重疊檔案的功能變更保留在既有原始碼位置；共用 case 核心及型別元件仍有各自的模組。
本次必要的 lexical occurrence 與 case 條件共用輔助邏輯，隨實例型別及 case 契約一併驗證。

範例文件使用本提交已有的引擎測試入口，不依賴另項工作的 preview 指令。
10-16 至 10-25 的驗證報告保留當時工作樹結果，不能當成此隔離版本的測試數。

## 檢查結果

Windows／PowerShell，離線依賴快取，使用鎖定的 Cargo.lock 與 Tshiatūn submodule。
以下命令均針對隔離副本：

| 指令 | 結果 |
|---|---|
| `cargo check --workspace --exclude langcraft-desktop --locked --offline` | 通過 |
| `cargo test --workspace --exclude langcraft-desktop --locked --offline --no-fail-fast` | 146 個結果區段；1,353 passed、0 failed、1 ignored |
| `cargo test -p conlang-language --test nested_types --test template_contract --locked --offline` | 補入兩份教學檔實際載入測試後，23 passed、0 failed；其中 21 項是重跑，2 項為新增 |

測試包括套件私有與公開邊界、遞迴型別／具名綁定、完整 derive、名義身分、
同名衝突、命中分支錯誤、case trace、rename／reconstruct／replay、GraphStore 重開，
以及既有 app、CLI、query、generation、statistics 回歸。

唯一 ignored 是既有 app IPC doctest。既有三個非簡寫欄位 pattern 警告保留。
未執行桌面 Rust、前端／GUI E2E、獨立 Tshiatūn suite 或 WASM target。
原始日誌位於忽略追蹤的 `target/push-discussion/`。
