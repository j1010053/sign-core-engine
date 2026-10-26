# Sign／trait 模板一致性驗證（2026-10-19）

> 本報告保留當日工作樹的驗證結果；僅提交 I31–I36 的隔離版本另見[2026-10-26 提交驗證](discussion-scope_2026-10-26.md)。

範圍：使用者授權的共用內容／作用域實作，以及「同名成員報錯、暫不覆寫」裁定。契約見 [I33](../implementation/trait-template-consistency.md)，操作入口見[成員範例](../../tutorials/template-scopes/README.md)。本報告針對當日既有未提交工作樹；不以 10-18 的測試數代替本次證據。

## 引擎與變更範圍

| 面向 | 本次契約 |
|---|---|
| 表徵 | `LanguageDocument` 是來源及 sidecar；Sign／trait 共用 `ScopedBody`；ontology、具體實例及 effective Sign 是編譯產物；`DerivedToken` 是推導結果 |
| 變更單位 | 巢狀宣告、型別引用及 PrimitiveEdit；來源節點與 runtime 成員別名分開 |
| 作用域 | Sign／trait owner、捕捉參數、套件 export、接收者私有權限及普通呼叫者環境 |
| 執行及衝突 | 同名成員報錯；同一模板實例的多個 block 不重複引入成員。phon 規則有序、case 保留既有選擇規則；已展開內容避免再次繼承 |
| 時間 | 共時 derive 與歷時 edit／replay 是不同操作；本次沒有新人口、使用事件或歷史排程模型 |
| 確定性 | 本次功能確定性執行；不引入 RNG，也不使用統計通過門檻 |
| 身分及 trace | 巢狀來源保留 parent／NodeId；具體實例指回來源；比較 feature／role、分類、case trace 及 surface |
| 輸入／持久化 | `.lang`、PackageSources、identity v2；新引用語義標為 3，凍結 binder 讀取無標記及版本 2 的文件；GraphStore 存原來源 |
| 公開入口 | `check_language`、compile／derive、`compile_document`、PrimitiveEdit、CLI `preview`、GraphStore |
| 保留邊界 | Sign metadata 的宣告位置、分類和值合併契約保留；不新增 builder、覆寫、一般型別推導、runtime 新實參家族或細粒度 member merge |

## 要求與可觀測證據

| 要求／分類 | 實作路徑 | 斷言及最小反例 |
|---|---|---|
| 共用 body／作用域：專案擴充 | [body](../../crates/language/src/body.rs)、parser／printer、member_scopes 地址遍歷 | 直接 Sign 與模板的私有 helper 完整 derive 相同；Sign → trait → Sign → Sign 有正確 parent；裸宣告不自動成為語法成分 |
| 套用一致性：既有行為保留並修正重複 | compile 展開 provenance → ontology effective Sign → derive | `belongs`／完整 mount／索引 mount；槽、optional、feature、role、residual 一致。`b => c` 後 `a => b` 應只得到 b，重跑會錯成 c；缺 block 拒絕 |
| case／realization：既有行為保留 | 共用引用 visitor → token rules → realization → phon | plural／singular／plural 連續推導得到 e／a／e，sem.count 與 case 選擇 trace 對照直接 Sign；缺 context 拒絕 |
| 接收者成員：專案擴充 | [MemberScopes](../../crates/language/src/specialization/member_scope.rs) → package binding → specialization／InstanceIndex | 內部可用 private，外部不可；公開 receiver alias 與模板成員同 SignId；普通呼叫引數不取得 private 權限 |
| 型別家族：既有不變性保留 | structural TypeRef → 具體化 → slot checking | N／V 接收者來源相同但實例不同，跨家族 filler 失敗；套用 N 不允許偷用 V 家族的 private helper |
| 衝突與終止：使用者裁定 | catalog／MemberScopes composition | 兩模板同名、本地與模板同名、同一 owner 重複宣告均報 `TEMPLATE_MEMBER_CONFLICT`；循環回傳錯誤 |
| 套件封裝：既有 export 保留 | PackageResolver → PackageScope → derive | exported 接收者可公開成員，內部私有依賴可用；私有模板本身不經接收者路徑外洩 |
| 修改與引用：專案擴充 | identity alias binding → changeset source access／rename → reconstruct／replay | `local(value: leaf())`、多層 `Receiver.Builder.leaf`、origin／components 都指回原來源。leaf 與 owner 依序更名後仍可 derive；被引用來源不可刪，失敗不改原文件；新／刪子樹保留既有 ID，單一成員 diff 只計一次 |
| 儲存相容：有意版本化 | reference_semantics 3、凍結舊 binder、GraphStore | 舊版本 2 manifest 開啟重存逐位元相同；修改升級且不重配 ID；損壞 refs 拒絕。實際 GraphStore 重開後來源、IDs、公開 alias 與 private 拒絕仍相同 |
| 正式操作：端到端 | CLI preview（當時本機工作樹，未包含於本次範圍提交） → app → derive | 範例 NP、NP.head、Container 得 a、a、ba；錯誤 filler／private 名稱失敗且 stdout 空；改 noun 音形後結果隨之改變 |

主要語義對照在 [language template_contract](../../crates/language/tests/template_contract.rs)，來源操作在 [changeset template_contract](../../crates/changeset/tests/template_contract.rs)。持久化測試在 [store_roundtrip](../../crates/persistence/tests/store_roundtrip.rs)。預期結果使用手寫直接 Sign、身分不變量及完整 round trip，沒有呼叫展開器產生 expected。

## 執行結果

環境：Windows／PowerShell，Rust 1.97.1、Cargo 1.97.1、Node 22.23.1、npm 10.9.8。依賴使用本機快取，Cargo 加 `--locked --offline`。

| 命令／檢查 | 結果 |
|---|---|
| `cargo test --workspace --locked --offline --no-fail-fast` | 153 個測試 target／doc-test 結果，**1361 passed、0 failed、1 ignored、0 filtered**；包含桌面 Rust target |
| `cargo test -p conlang-language -p conlang-changeset --locked --offline --test template_contract --no-fail-fast` | language 11、changeset 5，合計 **16 passed**；全部亦納入上方 workspace |
| `npm test`，目錄 `editors/vscode` | **12 passed、0 failed、0 skipped**；corpus smoke check 25 個檔案、4064 行 |
| 本次 Rust 檔案 `rustfmt --check`；變動 Markdown 相對連結檢查 | 32 個 Rust 檔通過格式檢查；13 個 Markdown 檔的相對檔案連結均存在 |

唯一 ignored 是既有 `crates/app/src/ipc.rs` 文件範例。CLI 新範例、GraphStore 重開及舊 sidecar 相容測試均有實際執行，沒有以略過處理。

只對本次變動的 32 個 Rust 檔執行 `rustfmt --edition 2021 --config skip_children=true`。`git diff --check` 沒有空白錯誤；Git 的換行轉換提示與既有三個測試 shorthand warning 不影響測試結果。

AST debug 快照經人工比對後更新：既有 blocks／items 移入 `ScopedBody.content`，新增 Sign member scope 與空的編譯 provenance。規則、內容與來源位置不改；canonical 來源的 roundtrip 仍另有斷言。不是以接受新輸出掩蓋語義回歸。

## 未驗範圍與工作樹

- 完整命令包含桌面 Rust target，沒有排除 `langcraft-desktop`；本次未做前端 GUI、WebDriver E2E、Linux 或 WASM 驗收。
- 獨立 `tshiatun` workspace 未修改，本次未另跑其全套測試；使用它的 derive／phon 整合路徑已納入主 workspace。
- 編輯器測試是 tokenization／語法高亮檢查，不能替代引擎的語義驗證。
- 工作樹原有大量未提交修改。本次以開工前的檔案副本比對，保留既有變更；沒有提交、推送或修改相鄰 `lang-preview`。暫存日誌及比較檔在忽略的 `target/template-consistency/`，不是產品來源。
