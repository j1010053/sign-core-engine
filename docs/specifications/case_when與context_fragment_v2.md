# `case`、`when` 與 Context Fragment（V2）

> 本檔規範**共時側**（`.lang`）。歷時 function 層（`code/*.chg` 的 body）沿用同一組
> 關鍵字，其規範見 `function分支語意與選擇層_v1.0.md`（P69）；兩側的逐條對齊表在
> 該檔 §3.4。歷時側另有 `choose:`（列舉候選、不執行），共時側無對應物。

## 封閉的 Context 型別

V2 的 fragment context 是編譯器封閉型別，不開放 `.lang` 自行註冊：

- `<SignContext>` 可包含完整 Sign body vocabulary，也可使用 trait；trait 仍只展開成 Sign fragment。
- `<SynContext>` 只接受 `syn` 的 slot、feature、constraint、Def、Rule 與同型 context expression。
- `<SemContext>` 只接受 `sem` 的 feature、roles、Def、Rule 與同型 context expression。
- `<PragContext>` 只接受 `prag` 的 Def、Rule 與同型 context expression。
- `<PhonContext>` 沿用 `realization:` 的純 phon template／projection；不能展開 trait。

匿名 fragment 不建立新的 Sign 實體。合併後仍是原 Sign，保留 SignId、source provenance
與 deep baseline。只有 `<SignContext>` 能使用 trait；維度 context 不能把 trait 降格成
Syn／Sem／Prag fragment。

## `case:`：first matching

`case:` 依來源序評估，第一個 Matched branch 的結果經 context 對應的 merge 路徑合併後
即停止。Unmatched 才繼續；Error 立即中止且不進 `else`。

```lang
syn:
    case:
        $self.syn.number == singular:
            feature:
                exponence = suffix
        else:
            feature:
                exponence = zero
```

## `when:`：frozen matching、ordered merge

`when:` 是累加式 expression，但 matching 與 merge 是兩個分離階段：

1. 建立一次合併前的 frozen Sign snapshot。
2. 所有非 `else` guard 都只讀同一份 snapshot；先前命中的 fragment 對後續 guard 不可見。
3. 任一 guard 為 Error 時，在 merge 前中止，沒有 branch、fragment 或 trace 被部分提交。
4. 全部 guard 結果確定後，才把所有 Matched fragment 按來源序合併；同 path 衝突沿用
   trait merge 的 stable later-wins。
5. `else` 只在沒有任何普通 branch Matched 時命中。

> **guard 讀得到的東西一律以 `feature:` 為正解**(P71,見
> `Def路徑封閉清單與feature分工_v1.0.md`)。裸 `<dim>.<field>` 的欄位名打錯**曾是靜默
> `false`**(只有維度名錯才報 `Err`),作者得不到訊號;`feature:` 則有
> `FEATURE_UNDECLARED` 與 `FEATURE_VALUE_OUT_OF_DOMAIN` 兩道檢查。P71 Phase 1 起
> 自造欄位的裸 Def 路徑已被封閉清單擋下,`feature:` 是唯一出口;
> **增修 D(§10,2026-08-12)起 guard 讀的路徑也受同一份清單約束**
> ——既不在清單上、也沒宣告成 feature 的欄位名報 `RULE_GUARD_NOT_ALLOWED`
> (typed `case:` 分支的 guard 則是 `CASE_INVALID_GUARD`),不再靜默。

```lang
sign cumulative:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = on
            outcome = enum(base, first, second)
            outcome = base
            leaked = enum(no, yes)
            leaked = no
        when:
            $self.syn.trigger == on:
                feature:
                    outcome = first
            $self.syn.outcome == first:
                feature:
                    leaked = yes
            $self.syn.trigger == on:
                feature:
                    outcome = second
```

此例第一、第三支 Matched，合併後 `outcome = second`。第二支必須 Unmatched，因為它看到的
`outcome` 是 frozen snapshot 中的 `base`，不能看見第一支尚未合併的 `first`。

`when` branch 必須回傳匿名 `<SignContext>`、`<SynContext>`、`<SemContext>` 或
`<PragContext>` fragment；不能回傳完整 Sign application。feature、role 與 phon
realization 的 scalar 選擇仍使用 `case:`。

## Lowering、trace 與來源編輯

`case` 與 `when` 在 IR 共用 `TypedCase` 與同一 fragment merge executor，並用
`CaseSelection::{FirstMatch, Accumulate}` 保留可觀察語意。`CaseRecord` 同樣記錄 selection、
branch、Matched／Unmatched／MoreSpecificBlocked、來源行與 diagnostic code。

identity sidecar 為 Case、CaseBranch 與 fragment 內項目配置 stable NodeId。Primitive
Edit／`.chg` 可用 typed `selection = case|when` 更新 Case；Update 保留 Case NodeId，
而 fragment 內節點仍可使用 Insert／Delete／Update／Move。

## Requirement-to-evidence

| Requirement | Evidence |
|---|---|
| Syn／Sem／Prag context parse、print、compile、runtime | `fp_v2::when_guards_share_one_frozen_pre_merge_snapshot` |
| 前一命中不得 feeding 後一 guard | `fp_v2::when_guards_share_one_frozen_pre_merge_snapshot` |
| 多支命中依來源序 merge、later-wins | `fp_v2::when_guards_share_one_frozen_pre_merge_snapshot` |
| `else` 僅在零普通命中時使用 | `fp_v2::when_else_uses_the_same_external_default_policy` |
| 任一 guard Error 時在 merge 前原子中止 | `fp_v2::when_guard_error_aborts_before_any_fragment_commit` |
| 維度 fragment stable identity round-trip | `identity_sidecar::dimension_when_fragment_items_round_trip_with_stable_identity` |
| `case`／`when` typed Update 保留 Case ID | `primitive_edits::case_selection_is_a_typed_identity_preserving_update` |
