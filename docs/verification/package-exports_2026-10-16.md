# 套件 export 與限定引用驗證

> 本報告保留當日工作樹的驗證結果；僅提交 I31–I36 的隔離版本另見[2026-10-26 提交驗證](discussion-scope_2026-10-26.md)。

日期：2026-10-16。範圍：`sign-core-engine` 目前工作目錄，Windows／PowerShell，離線依賴快取。
契約：[I31](../implementation/package-exports-and-qualified-names.md)。操作入口：
[English 套件範例](../../tutorials/package-exports/README.md)。

## 結果與執行範圍

| 指令 | 結果 |
|---|---|
| `cargo check --workspace --locked --offline` | 通過 |
| `cargo test -p conlang-language -p conlang-changeset --test package_exports --locked --offline --no-fail-fast` | 20 項通過：language 15、changeset 5 |
| `cargo test --workspace --locked --offline --no-fail-fast` | 149 個測試結果區段，共 1,324 項通過、0 項失敗、1 項 ignored |
| 在 `editors/vscode` 執行 `npm test` | 11 項通過，包含語法高亮與語料 tokenization |
| `git diff --check` | 通過；Git 有既存換行正規化提示 |

唯一 ignored 項目是 `crates/app/src/ipc.rs` 的既有 doctest，不算通過。
完整 workspace 指令包含桌面 Rust crate；沒有執行桌面前端、GUI E2E 或獨立的
Tshiatūn workspace／WASM target。這些未執行項目不在本次結果涵蓋範圍。
本機原始日誌保存在忽略追蹤的 `target/package-phase-b/`：
`check.log`、`exports.log`、`workspace-final.log`、`editor.log`。

## 語義路徑與反例

| 要求 | 可觀測證據 | 關鍵反例／不變量 |
|---|---|---|
| 三種宣告保留 export | [language 測試](../../crates/language/tests/package_exports.rs)：`export_and_qualified_references_survive_source_and_identity_round_trips` | printer 後重新讀取，source／identity 對保持一致；錯誤 export header 拒絕 |
| source 決定可見性 | 同檔 `export_table_cannot_promote_a_private_declaration` | TSV 不得把私有定義提升為公開 |
| 私有實作仍可執行 | 同檔 `exported_sign_can_use_private_trait_and_sign_without_exposing_them`、`public_paradigm_preserves_private_member_identity` | 公開入口可使用私有 helper；外部直接呼叫及保留內部名稱均失敗，私有 member ID 不變 |
| 限定引用與歧義 | 同檔 `same_export_names_coexist_and_ambiguous_bare_names_fail`、`qualified_slot_constraints_and_runtime_fillers_resolve` | 兩個套件可匯出同名定義；裸名拒絕、限定名有正確推導結果 |
| 完整執行而非只解析 | 同檔 `qualified_bounds_arguments_mounts_and_category_guards_execute`、`tutorial_package_is_executable` | 型別參數／界限、trait mount、category guard 進入 runtime；範例輸出 `kat`／`kats` |
| 作用域固定 | 同檔 `package_aliases_and_dependency_bindings_ignore_project_alias_overrides`、`packages_cannot_capture_undeclared_dependencies_or_private_exports` | project alias 或無關 root 不改包內 binding；未宣告依賴與私有跨包引用拒絕 |
| 私有 global 不外溢 | 同檔 `private_globals_stay_inside_their_package` | 包內 Sign 仍取得該 trait，caller 不自動繼承 |
| 所有外部執行入口守可見性 | 同檔 `private_names_cannot_enter_through_runtime_arguments_and_maps` | 巢狀 application、Sign filler、autofill map 無法繞過；同名本地 helper 不捕獲私有 helper |
| 公開定義可本地覆寫 | 同檔 `only_exported_declarations_can_be_overridden`、`public_trait_and_paradigm_can_be_overridden_locally` | trait／sign／paradigm 均可替換；私有名稱拒絕；套件 bytes 不變 |
| 歷史修改可還原 | [changeset 測試](../../crates/changeset/tests/package_exports.rs) | export insert/update、reconstruct、dump/reparse、replay、source／identity reopen；公開覆寫可推導，私有覆寫失敗 |
| 別名是語義輸入 | 同檔 `alias_changes_are_rejected_before_resolution_and_replay`；[cache 測試](../../crates/app/tests/compile_packages.rs) `changing_only_a_project_alias_cannot_reuse_a_compiled_system` | package bytes 相同但 alias target 不同，舊 replay 與 cache 均不可沿用 |
| 真實檔案可重開 | [persistence 測試](../../crates/persistence/tests/package_loader_v2.rs) `source_exports_and_project_qualifiers_survive_offline_reopen` | project／vendored manifest／來源／lock 從磁碟重開，限定名仍有效，修改 export 使 digest 失效 |
| 公開查詢名稱可使用 | [query 測試](../../crates/query/tests/paradigms.rs) `package_catalogs_expose_usable_qualified_names_and_filter_private_declarations` | 私有 sign／paradigm 不列入 catalog；同名公開項目使用限定名，篩選與求值仍有效 |

## 相容性與審核界線

內建公開定義補上 `export` 後，package digests 必然改變。首次回歸中的舊 fixture
已改為顯式公開；兩個 std 匯出數量斷言依來源中的公開宣告更新。
AST golden 只新增三個宣告的 `exported: false` 欄位及測試行號。

English reconstruction 的 `restore.chg` 透過既有
`cargo run -p conlang-changeset --example bless_en_standard_restore --locked --offline`
重新產生。對照任務開始時的 fixture，變動僅為 71 個 export 前綴與 3 個實際計算的
package digest；完整重建測試已通過。沒有改寫使用者專案的歷史鎖。

本次修改 package 名稱綁定與公開介面，未新增語言演化事件、時間軸或抽樣語義。
原有 evolution、reconstruction、derivation 與 seeded sampling 回歸均由 workspace 測試覆蓋。
泛型案例驗證限定型別實參、bound 與 mount 的執行，不擴充既有泛型 slot 合併規則。
`effective_language()` 等低階檢視仍包含實作所需的私有節點；可見性限制作用於 DSL
與公開命名呼叫／修改入口，不把原始 AST 當作機密資料。

範例只示範套件機制，尚非完整標準英語文法。獨立 `.lang` 的 CLI preview 不會自動
讀取旁邊的 `project.toml`；本次使用 project resolver 與實際整合測試驗證套件例子。
