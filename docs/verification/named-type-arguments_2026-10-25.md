# 具名型別實參驗證（2026-10-25）

> 本報告保留當日工作樹的驗證結果；僅提交 I31–I36 的隔離版本另見[2026-10-26 提交驗證](discussion-scope_2026-10-26.md)。

契約：[I36](../implementation/named-type-arguments.md)。環境：Windows／PowerShell、Asia/Taipei；
`rustc 1.97.1 (8bab26f4f 2026-07-14)`、`cargo 1.97.1 (c980f4866 2026-06-30)`。離線使用既有依賴快取。

## 行為與證據

| 契約 | 可觀測證據 |
|---|---|
| 位置／具名／混合寫法等價 | 外部未曾寫過的具名排列可查找同一巢狀成員；完整 derive 比對 SignId、語意及表層 |
| 每個參數恰好指定一次 | 宣告和成員呼叫分別驗證未知名、大小寫、重複、位置重複、缺少、過多及具名後位置實參 |
| 參數傳遞與 bound | 各段參數獨立綁定；倒序具名仍滿足較早參數的 bound，錯誤上界拒絕；多層 owner 的裸 mount 沿用等價綁定 |
| 來源標籤與型別值分開 | `Box<T=T>` 捕捉右側參數；同名全域宣告不成為標籤依賴；`Wrap<VP=VP>` 改名後為 `Wrap<VP=VerbPhrase>` |
| slots／roles／guards／值套用 | 同一 named TypeRef 穿過各入口；單槽位置值呼叫可包含具名型別成員呼叫；role 型別等號不誤判為賦值 |
| 不變性及名義型別 | 同表層 Twin 與 VP 保持不同 producer identity；錯誤 Comp 和不同 XP 家族的 Bar 被拒絕 |
| 套件與私有範圍 | 具名標籤不作套件查找；公開型別成功；私有型別即使已在內層建立實例，外部仍不可直接使用 |
| 共用實作 | 同一 binder 用於特化、成員組合、外部 lookup、`expand_traits`、`trait_view`；後兩者僅證明 macro pass，執行語意另以完整 derive 斷言 |
| 修改與保存 | 實際 `.chg` rename、reconstruct／逐步 replay、dump/open 比對；GraphStore 重開保留完整 token 和編譯成員身分 |
| 公開操作 | 真實 CLI preview 執行教學、直接具名路徑及錯誤反例；失敗為非零退出、沒有成功輸出、保留診斷碼 |

主要測試為 [language](../../crates/language/tests/named_type_arguments.rs)、
[changeset](../../crates/changeset/tests/named_type_arguments.rs)、
[persistence](../../crates/persistence/tests/store_roundtrip.rs)、CLI（當時本機工作樹，未包含於本次範圍提交）。
型別語法是專案擴充；bound、可見性、token 名義匹配、推導順序及來源身分沿用既有契約。

## 執行結果

最終命令：

```sh
cargo test --workspace --exclude langcraft-desktop --locked --offline
```

退出碼 0；155 份測試摘要合計 **1,405 passed、0 failed、1 ignored、0 measured、0 filtered**。
紀錄：`target/named-type-workspace.log`。其中新具名型別測試為 language 13 個、changeset 2 個，
CLI `preview` 共 9 個、persistence `store_roundtrip` 共 18 個；本次在後兩者各新增 1 個案例。
既有 template contract、instance types、nested types、P76、identity 相容、其他 changeset／app／CLI 測試亦包含在同一輪。

唯一 ignored 是既有 `crates/app/src/ipc.rs` 的文件範例。3 個既有 `non_shorthand_field_patterns` warning
來自 query 的 `lexicon` 測試與 changeset 的 `std_function_roles` 測試；沒有新編譯警告。

開發過程另執行：

```sh
cargo check -p conlang-language --locked --offline
cargo test -p conlang-language --test named_type_arguments --test p76_parameterized_trait --locked --offline
cargo test -p conlang-changeset --test named_type_arguments --locked --offline
cargo test -p conlang-language --test named_type_arguments -p conlang-cli --test preview -p conlang-persistence --test store_roundtrip --locked --offline
```

全部通過；開發中曾以反例找出 role 宣告等號切分及嵌入 Sign application 的等號切分問題，修正後由最終 workspace 再驗證。
以 `rustfmt --check --edition 2021 --config skip_children=true` 檢查本次 14 個 Rust 檔案通過，
`git diff --check`、相對執行前備份的新增行空白檢查，以及新增／修改文件連結目標檢查通過。

## 範圍與限制

workspace 命令排除 `langcraft-desktop`；沒有驗證 Tauri、前端、GUI E2E、獨立 Tshiatūn workspace 或 WASM target。
本次未增加預設型別實參、一般型別推導、Sign／Paradigm 自有泛型或執行時新家族。
fixture 的 `a u b` 是診斷材料，不是自然英語語料。

工作樹原已有大量未提交變更。本次以 `target/named-type-arguments-baseline/` 對照執行前內容；未提交、重設或清除既有工作。
來源 AST／持久化 schema 維持既有格式；結構化 Rust TypeRef API 新增標籤欄位，編譯快取版本更新為 10，詳見 I36。
