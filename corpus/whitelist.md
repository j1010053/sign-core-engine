# Lexurgy 分歧白名單(初版,M0 步驟 7)

黑盒比對中「預期不同」的案例必須列於此,標**原因 + 決策編號**;
紅燈才分得清 bug vs 設計差異(docs/05 §7)。

## 系統性分歧(整類)

| Lexurgy 行為 | 本引擎 | 決策 |
|---|---|---|
| 浮游變音符 = 音段旗標(`Diacritic ́ (floating)`) | 獨立旋律 tier 自體段 | 架構必異(docs/05 §3);D14 浮游存亡 |
| 音節 = 逐音段標注(`Syllables:`) | Span 韻律層 + 空莫拉 + dominate | I8;D23 lazy reparse |
| 詞界 `$` | `#` | D19 |
| 規則套用模型(逐規則全詞掃描) | snapshot-and-actions + parallel/iterative 宣告 | A1/B5/I1 |
| `Else:`/`defer`/`cleanup` 階層 | 匯入器映射待補 | docs/02 §13 開放項 |
| 多值特徵矩陣直接改寫任意特徵 | M0 子集僅欄位 set_field + Inventory 反查 | I12(無對應=error) |

## 逐檔標注(隨 M2 匯入器充實)

| 檔案 | 狀態 | 備註 |
|---|---|---|
| cli/test/test_all_errors.lsc | 排除 | 錯誤處理測試,非行為語料 |
| cli/test/circular_*.lsc | 排除 | include 循環偵測(檔案機制,殼層職責) |
| cli/test/kharulian.lsc 等完整 conlang | M2 | 需 Diacritic/多值特徵/Syllables: 匯入 |
