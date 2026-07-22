# Step 13 Primitive Edit 與 Source Identity（v1）

> Step 14 後續狀態：sidecar canonical schema 已升為
> `conlang.language-identities/v2`；v1 仍可讀並決定性升級。v2 以
> `root_namespace + active_namespace + allocators[]` 保存祖先與 ChangeSet namespace 的
> mixed NodeId。Step 13 四原語不變；statement transaction、`.chg`、replay 與 lazy compile
> 已由 docs/22 接手。

## 1. 邊界與資料流

Step 13 的唯一持久化 target 是 caller `Language`：

```text
(.lang, .lang.ids.json)
  → LanguageDocument
  → Insert | Delete | Update | Move
  → check_document
  → (Language', identity sidecar', PrimitiveRecord)
```

effective language、std/natural/plugin overlay、Compiled Grammar、DerivedToken、surface、
State 與 Evolution graph 均不可作 primitive target。`conlang-changeset` 只依賴
`conlang-language`，維持 `changeset → language → dsl`。

Step 14 才加入 statement transaction、`.chg` parser、ChangeSet replay 與 lazy
recompile；Step 13 只有單一 primitive 的 immutable checked entry point。

## 2. Identity document

`.lang` 不增加 `@id`。歷時專案以同名 `<name>.lang.ids.json` 保存：

- canonical schema 為 `conlang.language-identities/v2`；v1 僅作可讀的遷移輸入，驗證成功後決定性升級；
- 保存 `root_namespace`、`active_namespace` 與按 namespace 分離的 `allocators[]`；祖先節點保留原 ID，分支 Insert 只使用 active ChangeSet allocator；
- canonical UTF-8/LF `.lang` 的 SHA-256；
- 每個 editable node 的 `NodeId`、`NodeKind`、parent 與 typed address；
- reference occurrence 的 owner、field 與 local/external target。

`LanguageDocument::open` 只接受 digest、node shape、kind 與 Ref binding 完全一致的
pair。外部手改 source 後回報 `IDENTITY_SOURCE_MISMATCH`，不得以名稱、相似度或
vector index 猜回身分；要把手改內容納入歷史，必須明示 `import_new_root` 並提供新
namespace。

bare `Language::parse` 仍可用於共時 compile，使用 ephemeral ID；Primitive Edit
只接受 `LanguageDocument`。document 開啟後 Sign/Rule runtime ID 綁到 document
namespace，library merge 不再重新配發 caller/package sign ID。

## 3. Primitive 契約

```rust
apply_edit(
    source: &LanguageDocument,
    edit: PrimitiveEdit,
    libraries: &LibrarySpec,
) -> Result<EditOutcome, EditError>
```

- `Insert`：DetachedNode 不攜 persistent ID；整棵新子樹由 document counter 配發。
- `Delete`：刪除節點與子樹，不 cascade 或偷偷改 Ref；root 永不可刪。
- `Update`：使用按 `NodeKind` 封閉的 `NodeUpdate`，沒有任意 field/value 字串後門；
  node ID 不變。
- `Move`：移動既有子樹並保留全部 ID；拒絕錯 parent、錯 sequence、外部 target
  與移入自身後代。
- `Anchor`：只有 `Start/End/Before(NodeRef)/After(NodeRef)`；Before/After 必須屬於
  同一 parent 與 canonical semantic sequence。具名頂層集合只接受 End，再按
  canonical key 放置。

所有操作先在 clone 套用、重建 sidecar，再執行 `check_document`。結構、manifest
或語言 validation 失敗時，輸入 source、manifest 與 ID counter 均不變。

## 4. Validation、diff 與 trace

`check_language`／`check_language_with_libraries` 從 compile pipeline 抽出，不執行
lowering 或 phon codegen；`compile_system` 復用同一 source validator。
`check_document` 再加 ID、parent、Ref owner/target/kind 與 sidecar invariant。

`LanguageDiff` 以 stable ID 對齊。插入或刪除造成的 sibling index 位移不算 Move；
只有 parent 改變或既有 sibling 的相對次序改變才算 Move。`PrimitiveRecord` 保存
operation、target、parent/anchor、before/after local snapshot、allocated/deleted/moved
IDs、diagnostics 與完整 diff。

## 5. Requirement-to-evidence

| 要求 | 證據 |
|---|---|
| sidecar round-trip、digest mismatch、無猜測恢復 | `language/tests/identity_sidecar.rs` |
| typed NodeRef/field resolver | resolver 正例與錯 kind field 反例 |
| rename 保 identity 與 origin Ref | `rename_preserves_identity_and_rewrites_stable_origin_display` |
| delete+insert 是生滅 | `insert_delete_insert_is_birth_death_not_update` |
| form 改變、function/identity 保留 | `update_form_keeps_sign_identity_but_changes_surface` |
| surface 不變但 Sem 改變可見 | `semantic_change_is_observable_when_surface_is_identical` |
| 多重繼承局部更新 | `updating_one_parent_preserves_other_inheritance_links` |
| rule 居所移動保 RuleId | `move_rule_between_homes_preserves_rule_identity` |
| invalid anchor、dangling delete rollback | `invalid_anchor_and_dangling_delete_fail_without_mutating_source` |
| insert/delete 後 stable anchor 仍指同一節點 | `stable_anchor_survives_prior_insert_and_delete` |
| std/natural/plugin 節點不可由 caller edit | `library_owned_target_is_never_editable` |
| 相同 source/edit 的 document 與 record 決定性 | rename 測試的 repeated `EditOutcome` equality |

本機封板入口為 `scripts/verify-step13.ps1`。它先檢查 submodule gitlink／worktree、
rustfmt、Clippy、linker、WASM target，再執行指名測試、workspace、Tshiatūn 與 WASM
build；缺少基礎設施時寫入 `target/verification/step13-summary.json` 並回傳 exit 2。

仍刻意延後：derived-token downward feature forwarding、context 後 filler-rule 重跑、
component/sense graph 尚不存在的 source node、跨 statement transaction、ChangeSet
serialization/replay、Atomic Rewrite、Recipe、Goal 與 Evolution graph。
