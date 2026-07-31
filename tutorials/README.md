# Tutorials

- `en-standard-reconstruction/`：以可 replay 的 `.chg` 從空 Language 重建
  Standard English 的 17 個 Symbol、1 個 Class、37 個 trait 與 30 個 sign。

本目錄保存可直接操作、由範例帶入概念的教學文件；規範性契約仍位於 `docs/`。

| 文件 | 用途 | 驗證 |
|---|---|---|
| `共時lang語法教學_v1.md` | 從最小 form–meaning pairing 到完整共時 `.lang` 推導 | `crates/language/tests/tutorial_examples.rs` 會抽取完整範例並執行 parse、compile、derive 與決定性檢查 |

教學若與 `docs/` 的規範或 P 系列決策衝突，以規範與架構決策為準；修正教學時必須同步更新可執行範例測試。
