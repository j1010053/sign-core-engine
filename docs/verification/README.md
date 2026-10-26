# Verification

> **類型**：功能確認、測試索引與封板證據
> **權威邊界**：本目錄證明「哪些行為已由測試觀測」；規範與 P 系列決策仍以 `docs/` 的規格及架構修補彙整為準。

本目錄把測試索引與各階段封板報告集中存放，避免將完成證據與規範、細部實作架構混在同一閱讀序列。

| 文件 | 類型 | 證明範圍 |
|---|---|---|
| `測試案例集總索引_v0.1.md` | 測試索引 | DSL、折磨測試、實例、Rust 測試與狀態的全專案映射 |
| `M1++_P38-P44_封板證據.md` | 封板證據 | 單一 ontology、四維、construction、Else、typed patch |
| `Step13_PrimitiveEdit與SourceIdentity_封板_v1.md` | 封板報告 | Primitive Edit、source identity、V2 expression identity |
| `Step14_ChangeSetInterpreter_封板_v1.md` | 封板報告 | `.chg` resolve/replay、交易、digest、lazy compile |
| `Step16_文件契約與驗收矩陣_v1.0.md` | 收官矩陣 | diff/reconstruct、identity reconcile、merge/donor、phon、persistence |

判定完成時必須同時具備規範契約、實作路徑與本目錄中的可觀測證據；只有 parser、編譯成功或表層輸出，不單獨視為功能封板。

本段討論的實作驗證（各報告保留當時工作樹範圍）：

- [2026-10-25 具名型別實參](named-type-arguments_2026-10-25.md)：位置／具名等價、巢狀傳遞、診斷、套件／private、實例身分、rename／replay、CLI 與持久化
- [2026-10-23 Sign 實例型別](instance-types_2026-10-23.md)：參數傳遞至 token、名義匹配、套件／private、語意交換、rename／replay、CLI 與持久化
- [2026-10-21 case 共用求值](case-unification_2026-10-21.md)：分支錯誤不回退、跨入口缺值／巢狀結果／trace、既有 base 政策與模板一致性
- [2026-10-19 模板一致性與共用作用域](template-consistency_2026-10-19.md)：直接 Sign／模板等價、私有存取、接收者別名、衝突、修改／replay、CLI 與持久化
- [2026-10-18 巢狀宣告與型別傳遞](nested-types_2026-10-18.md)：實參捕捉、家族隔離、私有成員、derive、CLI、身分、修改／replay 與持久化
- [2026-10-16 套件 export 與限定引用](package-exports_2026-10-16.md)：公開／私有作用域、限定引用、覆寫、replay、cache 與離線重開

- [2026-10-26 I31–I36 範圍提交驗證](discussion-scope_2026-10-26.md)：排除其他未提交工作後的實際提交測試。
