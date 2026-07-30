# Step 14 ChangeSet Interpreter（已封板 2026-07-24）

> **本契約已封板。** Primitive-only `.chg` 的解析／resolve／replay／lazy compile 與
> 交易語意定稿為 Step 14 completion。相容性測試涵蓋:replay 跨執行決定性、
> `ResolvedChangeSet` dump round-trip、**三道 digest**(base source／identity-manifest／
> library lock)於 replay 前拒絕、statement 交易回滾與部分保留、lazy compile cache
> 重用/失效,以及 v1→v2 identity 升級**內容無損**(移除 v1 前的閘門)。
> `cargo test --workspace` 全綠為證(本機無 PowerShell,`scripts/verify-step14.ps1`
> 未實跑)。**不含**舊 v1 `.chg` replay 相容承諾(硬移除後 v1 檔不可載入)。

Step 14 只改 caller `LanguageDocument`。target 永遠不是 effective library overlay、
CompiledSystem、DerivedToken 或 surface。權威操作仍只有 Insert／Delete／Update／Move；
`.chg` 是四原語的持久、可 replay 編排層。

## 共時封板介面

`DerivedToken` 私下保存 composition 後的 `DeepTokenState` 與原始
`DerivationContext`。外層 `slot_features` 先讀 frozen probe，再原子驗證 constraints；stored
sign 從 effective base、derived token 從 deep baseline 重跑 Syn→Sem→Prag，檢查 constraint
未被規則覆寫後重選 realization。`OccurrenceRecord` 分開保存 probe/committed rules、來源、
constraints、重跑狀態與 realization。公開入口包含 `evaluate_sign_with_context`、
`recontextualize_token` 與 `compile_document`。

## Identity v2

canonical sidecar schema 是 `conlang.language-identities/v2`：
`root_namespace`、`active_namespace`、sorted `allocators[]`、source digest、nodes、refs。
`LanguageDocument::fork` 保留祖先 ID，只讓 Insert 從 active ChangeSet allocator 配號。
**identity v1 已硬移除(2026-07-24)**:`LanguageDocument::open` **只接受 v2 sidecar**,
v1/未知 schema → `UnknownSchema` 拒絕(不再讀取或升級);重複／未知 allocator、落後
counter、library editable node 都拒絕。（移除前的 v1→v2 升級無損證據見 git history。）

## `.chg` 與交易

`UnresolvedChangeSet::parse` 接受 name selector；`resolve` 在鎖定 base 上 dry-run，將 selector
固化為 `node(kind,@namespace:ordinal)`；`ResolvedChangeSet::dump` 只輸出 stable selector。
replay 前驗證 base source digest、identity-manifest digest、namespace 與 package version/content
lock（**三道 digest 均有拒絕測試**:`step14_interpreter.rs` 的 `digest_mismatch_*`、
`identity_manifest_digest_mismatch_*`、`library_lock_mismatch_*`)。每個 statement 在 clone 上依序
套 primitive，最後驗證一次；失敗時該句與 allocator 回滾,已透過 `ChangeSession::apply_statement`
committed 的前句不回滾。

`ChangeSession::compiled_system` 只在 dirty revision 首次要求時呼叫 `compile_document`；同一
revision 重複取得 cache，成功 commit 使 cache 失效，失敗 statement 不修改 session。

## 證據與界線

- occurrence：`crates/language/tests/slot_feature_bindings.rs`
- identity v2：`crates/language/tests/identity_sidecar.rs`
- parser／replay／lazy compile：`crates/changeset/tests/step14_interpreter.rs`
- 文件可執行 fixture：`crates/language/tests/tutorial_examples.rs`
- 本機 gate：`scripts/verify-step14.ps1`

本步驟不包含 Atomic Rewrite、Recipe、Goal、Weight DB、Evolution graph、State、contact/adopt、
服務、History record–replay 或暫停恢復。
