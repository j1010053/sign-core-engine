# Tutorials

- `en-standard-reconstruction/`：以可 replay 的 `.chg` 從空 Language 重建
  Standard English 的 17 個 Symbol、1 個 Class、37 個 trait 與 30 個 sign。

本目錄保存可直接操作、由範例帶入概念的教學文件；規範性契約仍位於 `docs/`。

| 文件 | 用途 | 驗證 |
|---|---|---|
| `共時lang語法教學_v1.md` | 從最小 form–meaning pairing 到完整共時 `.lang` 推導 | `crates/language/tests/tutorial_examples.rs` 會抽取完整範例並執行 parse、compile、derive 與決定性檢查 |
| `歷時chg授權教學_v1.md` | 從四原語、trait 標頭更新到 identity-preserving replay | `crates/changeset/tests/tutorial_chg.rs` 會逐一抽取標記過的 `.chg`，實際執行 parse、resolve、dump、replay 與錯誤案例 |
| `CLI操作教學_v1.md` | 一條完整工作流:建專案 → 詞典 → 演化 → 造詞 → 統計 → 分群 → 旁註 → 環境 | `crates/cli/tests/tutorial.rs` 抽出教學裡的起始 `.lang`,照節次實際執行每一條命令,並斷言教學宣稱的關鍵事實(閉包過濾、候選等權、旁註不改語言、State 不擾動既有節點) |

教學涵蓋的是可操作的 `.lang`、`.chg` 與 CLI 表面。Rust 專用的逐事件
`TraitDiff.event_reaches` 契約由 CLI 教學解釋其可觀察效果，完整欄位與公式則以
`docs/architecture/分層差異向量_v0.2_裁定.md` 為準；尚未落地的 Diff UI v2 不冒充
成現有功能。

## 功能涵蓋對照

| 功能 | 教學位置 | 可執行證據 |
|---|---|---|
| 泛型 trait 的 slot／role 作用域、具體展開與 bound 傳遞 | 共時教學 §5 | `tutorial_examples.rs` 與 `p76_parameterized_trait.rs` |
| trait `global`／`marker`／`type_params` 更新、清除、衝突與 replay | 歷時教學 §6 | `tutorial_chg.rs`、`update_fields.rs`、`reconstruct_roundtrip.rs` |
| Move 保身分；規則同維搬家、跨維搬家與前後 reach | 歷時教學 §7 | `diff_vector.rs`、`reconstruct_roundtrip.rs` |
| 逐事件間接傷害與 8／9 條規則分群邊界 | CLI 教學 §7 | `tutorial.rs` 與 `query/tests/grouping.rs` |
| Wire/UI V1 相容界線與延後的 Diff UI v2 | 本頁上方與差異向量架構文件 | 現有 Wire V1 未新增欄位、未 bump schema |

教學若與 `docs/` 的規範或 P 系列決策衝突，以規範與架構決策為準；修正教學時必須同步更新可執行範例測試。
