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
