# Sign 實例型別驗證（2026-10-23–24）

> 本報告保留當日工作樹的驗證結果；僅提交 I31–I36 的隔離版本另見[2026-10-26 提交驗證](discussion-scope_2026-10-26.md)。

契約：[I35](../implementation/sign-instance-types.md)。執行環境：Windows／PowerShell、Asia/Taipei，`rustc 1.97.1 (8bab26f4f 2026-07-14)`、`cargo 1.97.1 (c980f4866 2026-06-30)`，離線使用既有依賴快取。

## 範圍與判準

本次為共時型別擴充。來源維持 Language／共用 ScopedBody；具體化產生編譯 SignDef，完整 derive 產生 DerivedToken 與 filler／語意角色快照。共用型別判定須通過 slot、role、guard、autofill、candidate 與 feature schema 邊界。

歷時入口仍為 PrimitiveEdit／ChangeSet，不新增原語或歷史來源節點。編譯階段與 token 推導分開；來源 NodeId、具體 SignId、角色快照各自保有明確身分用途。執行為決定性的既有推導與 first-match case，base 政策不變；未新增隨機機制，無新 seed 或分佈假設。測試使用少量可辨認音形，不宣稱已驗證英語語料或生成語法分析的完整性。

## 行為證據

| 需求 | 實作及可觀測證據 |
|---|---|
| Sign 成為型別實參 | `TypeRef`／共用引用 visitor → package／specialization → 名義索引；`XP<TenseHead, DP, VP>` 的完整 `demo` 推導為 `a u b` |
| token 與型別相接 | `VP(head: word())` 以具體 SignId 填入 `[VP]`，slots／roles／guards 共用判定；相同表層與 trait 分類的 Twin 被拒絕，外層 Wrapper 不冒充 VP |
| partial／多次套用 | 未飽和 VP 保留型別；外層保留轉送參數，補填後走完整 derive；不同 head 值不改變 VP 型別 |
| 家族與別名 | 同來源的 `Family<N>.leaf` 與 `Family<V>.leaf` 具體身分不同；接收者別名作為實參時與完整引用正規化到同一實例 |
| 模板哲學 | Sign 不可作 `belongs` 或 block mount；Sign 實參可經分類滿足 trait 上界，錯誤上界拒絕；Paradigm 不是型別 |
| 分類收窄與 feature schema | `[Verbal]` 可收窄到屬於它的 `[VP]`；slot feature 依 VP 的 enum 驗證，非法值與不符合分類的 Sign 收窄均拒絕 |
| 公開／私有邊界 | 套件限定 Sign 型別、同名 Sign、跨套件同名 trait／Sign、本地名稱優先、private 引用拒絕；合法捕捉 private 實參後可在模板內傳遞，外部無法藉已產生的實例繞過權限 |
| 求值入口 | SignApplication／autofill／Paradigm 成員結果、feature／role expression、realization guard、token rule、candidate search；斷言語意角色身分及命中分支索引 |
| rename／reconstruct／replay | Sign 改名連動 slot、role、guard、一般規則、遞迴實參與呼叫；重建先套用更名，重播結果與來源／sidecar 完全相同；引用中刪除被拒絕，型別參數不誤綁同名全域 Sign |
| 來源與語意持久化 | v3 sidecar 原 bytes 保存，編輯升級 v4 且 NodeId 不變；語意 JSON 往返保留 construction_id，缺失名義身分與未知身分被拒絕；GraphStore 重開後完整 token 相同 |
| 真實 CLI | 執行 `conlang preview` 得到 `a u b`；用同表層的另一個 Sign 代替 DP 時，非零退出且不產生成功輸出 |

主要測試檔為 [language instance_types](../../crates/language/tests/instance_types.rs)、[changeset instance_types](../../crates/changeset/tests/instance_types.rs)、[GraphStore](../../crates/persistence/tests/store_roundtrip.rs)、CLI preview（當時本機工作樹，未包含於本次範圍提交），另含 identity 相容測試及既有語意交換測試。直接 Sign 與套入 trait 的型別位置共用既有內容模型；原有 `template_contract` 與 case 測試一併回歸。

## 指令與結果

```sh
cargo test -p conlang-language --test instance_types --locked --offline
cargo test -p conlang-changeset --test instance_types --test nested_types --test reconstruct_roundtrip --locked --offline
cargo test -p conlang-language --lib previous_type_reference_semantics --locked --offline
cargo test --workspace --exclude langcraft-desktop --locked --offline
```

- 最終 language 專項：13 通過、0 失敗、0 忽略、0 filtered。
- 改名與重建回歸：38 通過（2 + 5 + 31），0 失敗／忽略／filtered；後續規則引用擴充亦由最終 workspace 測試覆蓋。
- v3 sidecar 相容專項：1 通過、45 filtered，0 失敗／忽略。
- 最終 workspace：153 份 test-result 摘要，**1,388 通過、0 失敗、1 忽略、0 measured、0 filtered**。包含最終 13 項型別專項、2 項來源操作專項、17 項 GraphStore 及 8 項 CLI preview；這些數字已包含在總數中，不重複相加。
- 唯一 ignored 為既有 `crates/app/src/ipc.rs` 文件範例。3 個既有 `non_shorthand_field_patterns` 警告位於 changeset／query 測試，未視為新功能失敗。
- 對本次修改的 21 個 Rust 檔執行 `rustfmt --check --edition 2021 --config skip_children=true`，通過。`git diff --check` 通過；新增內容另與本次開工前備份比較，無尾端空白。

完整本機輸出保存在 `target/instance-types-workspace-final.log`，專項輸出在 `target/instance-types-final-targeted.log`；兩者為忽略的驗證產物，不作來源提交。

## 未驗證及保留範圍

此次 workspace 命令排除 `langcraft-desktop`，未跑 Tauri／前端／GUI E2E、獨立 Tshiatūn workspace 或 WASM target。具名型別實參、一般型別推導、Sign／Paradigm 自有泛型、執行時新實參家族仍未實作。來源只存 `.lang` 而重新 import，或在編輯後沿用舊編譯 token，都不屬於本次持久化保證。

工作目錄原已有大量未提交與未追蹤內容。本次以開工前的檔案備份核對差異，保留原有工作，未建立 commit。
