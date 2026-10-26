# `.lang` case 共用求值（I34）

日期：2026-10-21。使用者授權範圍是「先不更改 base 行為，只統一實作和取消回退」。
本次不採納先前提案中的 Paradigm 分類 selector、新增預設語法或放寬最後 `base` 的要求。
語言契約仍以 [case／when 規格](../specifications/case_when與context_fragment_v2.md)與
[Paradigm 規格](../specifications/Paradigm與SignApplication_V3.md)為準。

## 共用邊界

`crates/language/src/case_eval.rs` 是執行期唯一的分支控制核心，負責 first-match、巢狀 case、
`when` 的 frozen matching 與分支事件。所有入口都使用它：Sign／維度 context、feature、
semantic role、phon realization、Paradigm，以及 stored／applied filler。

`synchronic/case.rs` 繼續把 guard、Equals、bundle 降成相同的 Guard 判定。
各 host 只提供讀取環境、既有基底、結果操作與紀錄轉換，不重新遍歷分支。
feature 的 enum 結果檢查共用 `FeatureCaseHost`；直接 Stored Sign 與 stored filler
更共用整個 `run_sign_feature_expressions`。phon 的模板、預設形及分支規則共用 `phon_leaf`。

| 操作 | 保留的專用責任 |
|---|---|
| Sign／context | Sign application、fragment merge、branch membership；失敗不提交暫存 trace／rules |
| feature | 宣告與 enum domain 檢查、維度寫入；既有候選值集合保留原表示 |
| role | slot 綁定、分類約束、optional／未飽和／必填 role 政策 |
| phon | Stored／token 模板展開與可用的投影服務、純 phon 邊界 |
| Paradigm | 成員介面、剩餘參數轉送、成員 SignId 與選擇來源 |

## 分支錯誤與基底

只有條件 `Unmatched` 可以繼續下一支。任何命中分支的求值或約束錯誤立即返回，
不再由 guard／Equals 特別捕捉硬約束錯誤後嘗試下一支或 `base`。
role 命中未填 slot 後也停止選擇：可選或未飽和 role 仍可暫缺，飽和後缺必填 role 仍報錯。
這不改變候選列舉對不相容候選的既有過濾政策。

| 位置 | 本次保留的零命中／空白 `base:` 政策 |
|---|---|
| Sign／維度 context | 保留進入 case 時的 Sign |
| feature | 保留既有值；無值則 `CASE_DEFAULT_MISSING` |
| role | 保留既有 binding，依 optional／飽和狀態檢查缺值 |
| phon | 保留 deep/default phon |
| Paradigm | 仍必須以明確、非空的 Sign application `base` 結尾 |
| `when` | 零命中保留現值，最後 `else` 的現有規則不變 |

基底是已有值，不是新選出的 enum 字面值。空白 base 不將未定案的 `a | b` 當成一個
enum 成員，也不把原 `FeatureValue.values` 改寫成單一拼接字串；新 enum leaf 才檢查 domain。
這是統一既有「保留基底」契約所需的表示修正，沒有新增 base 政策。

## 可觀測結果與相容性

stored filler 的 bundle／Equals 不再吞掉 P75 必填缺值；stored feature、stored phon 與
occurrence phon 接受相同的巢狀 case。filler trace 包含 feature／phon 的 Matched、Unmatched、
巢狀來源與規則無效診斷；不能因為表層相同就省略內層事件。

`MoreSpecificBlocked` enum 值保留給舊 API／trace 消費者，但新 case 求值不再產生它。
AST、parser、printer、NodeId／SignId、identity schema 與 reference semantics 格式未修改。
快取語意版本升為 `conlang-compile/8`，防止沿用舊的回退求值結果。

既有能力邊界仍需區分：低階 construction filler realization 沒有完整 Sign application
服務，因此 phon projection application 仍回報不支援；主系統的 phon projection 沿用原服務。
這不是分支選擇的另一套實作。本次未宣稱補齊全部 filler projection 能力，也不修改 `.chg`
function 的獨立交易與選擇語義。

驗證入口與已知限制集中於 [2026-10-21 驗證紀錄](../verification/case-unification_2026-10-21.md)。
後續 [統一 `base:` 控制方案](../architecture/base-branch-control-proposal.md)僅是提案。
