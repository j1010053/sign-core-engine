# case 共用求值驗證（2026-10-21）

> 本報告保留當日工作樹的驗證結果；僅提交 I31–I36 的隔離版本另見[2026-10-26 提交驗證](discussion-scope_2026-10-26.md)。

對象：Windows 上 `sign-core-engine` 的現有工作目錄，包含原有未提交內容。
契約見 [I34](../implementation/case-evaluation-unification.md)；實作前反例保存在
原檢查報告（當時本機工作樹，未包含於本次範圍提交），未覆寫其歷史結果。

## 執行模型與範圍

這次修改確定性的單次共時推導：Stored Sign／Applied token 讀取同一份條件環境，
case 選第一個命中結果，when 先以固定快照匹配再合併。
輸出比較包含 feature 值、role 節點、選中成員、分支來源、規則診斷、必要錯誤與 surface；
未引入新的歷史時間、抽樣或人口模型。

parser、來源 AST 與 identity schema 未改動。透過 workspace 回歸涵蓋既有
修改／replay、source identity、cache、GraphStore 與套件生命週期；
既有來源檔未因 case 重構而遷移。

## 執行結果

| 指令／檢查 | 結果與範圍 |
|---|---|
| `cargo check -p conlang-language --locked --offline` | 通過；後續 workspace build 也通過 |
| `cargo test --workspace --exclude langcraft-desktop --locked --offline` | **1369 passed、0 failed、1 ignored**；包含當時新增的 8 項 case 矩陣測試 |
| `cargo test -p conlang-language --locked --offline --test case_evaluation_unification` | **9 passed、0 failed**；在上述 workspace 之後補加 Syn／Sem／Prag 維度矩陣及原始候選值表示斷言，再次通過 |
| `cargo test -p conlang-language --locked --offline --test runtime_sealing` | 7 項通過；命中後 constraint failure 改為必須報錯 |
| `cargo build -p conlang-cli --locked --offline` | 通過，使用重建的 CLI 重跑原反例 |
| CLI `preview ... --trace` | 13 組檢查符合預期；逐例證據見 [JSON](case-unification_2026-10-21_cli.json) |

workspace 的唯一 ignored 是既有 `crates/app/src/ipc.rs` 文件測試。
原有 changeset／query 測試中的三處 redundant field pattern warning 尚在；本次沒有新增該類警告。
workspace 原始日誌位於本地 `target/case-unification-workspace-tests.log`，不作持久證據檔提交。
上述 9 項矩陣測試與 workspace 內的前 8 項重疊，不能直接相加當作不重複測試數。

## 關鍵反例

| 不變量／修正 | 實際驗證 |
|---|---|
| 同一必填缺值不可因入口改成 base | guard／Equals／bundle × direct／stored filler／token pipeline；皆 P75 錯誤 |
| `_` 不讀欄位，AND 保留短路 | 缺必要值但不被讀到時仍成功 |
| feature 可以巢狀選擇 | Syn／Sem／Prag、各入口的結果一致，filler trace 包含兩層來源 |
| phon 可以巢狀選擇 | template、有效規則、無效規則均跨三種入口驗證；無效診斷只屬執行規則的內層 |
| 命中結果失敗不得重選 | guard／Equals／bundle 的相同 category failure；原 SlotFeatureConflict 回退反例直接報錯 |
| role 命中空 slot 不選 base 的另一個 slot | optional role 保持合法缺席；飽和必填 role 報錯；未飽和可保留 residual |
| base 政策保留 | 隱式與空白 base 保留原 feature 值／候選集合，缺基底仍報錯；既有 Sign、role、phon 與 Paradigm 測試通過 |
| when 固定快照與原子合併 | `fp_v2`、`bundle_match` 的 frozen／else／guard error 反例通過 |
| Sign 與 trait 是同一內容能力 | 新增直接 Sign／trait 的 feature 值、錯誤及 occurrence trace 對照；既有 `template_contract`、`nested_types` 通過 |
| XP 範例仍可用 | `tutorials/nested-types/phrases.lang --sign demo` 得到 `ba`，bar-comp／bar-adjunct autofill 管線正常 |
| Paradigm 身分與介面未改 | `paradigm_v3`、identity、partial application、deferred autofill 與 final base 負例通過 |

新增測試使用完整 `derive` 取得產品結果。token filler 先經 `evaluate_token`，
feature fixtures 再以 occurrence slot constraint 觸發從 deep baseline 重新求值；
不把低階 `apply_construction` 當成已完成的推導。
中間表示檢查另明確斷言 `FeatureValue.values` 保持兩個候選，避免只比較拼接後的字串。

CLI 原反例修正後：missing 三種寫法的 direct／filler 全部 exit 1；nested feature
兩入口皆 `outcome = yes`；nested phon 兩入口皆 `e` 並有兩層 trace；
blocked guard／bundle 都 exit 1，不進 base。

## 未涵蓋與保留邊界

沒有執行 Tauri／前端／GUI E2E，也沒有重跑獨立 Tshiatūn workspace 或 WASM target。
本次沒有修改音變核心。

低階 filler realization 的 phon projection application 仍缺少完整 Sign 呼叫服務；
主系統投影測試通過不代表此限制已解決。Stored filler 的其他 Sign/context 執行能力
仍以既有待辦和教學邊界為準。I34 統一的是已進入求值的 case 控制與共用結果處理，
不宣稱所有來源項目的 pipeline 能力均已補齊。

`base: error ...`、缺省 base 一律報錯、when 改用 base、Paradigm 分類 selector
都尚未實作，見[獨立提案](../architecture/base-branch-control-proposal.md)。
