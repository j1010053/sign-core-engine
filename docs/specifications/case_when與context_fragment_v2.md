# `case`、`when`、bundle match 與 Context Fragment（V3 第一階段）

> 檔名保留 `v2` 以免既有連結失效；本檔內容已更新為 V3 的第一階段契約。

> 本檔規範**共時側**（`.lang`）。歷時 function 層（`code/*.chg` 的 body）本階段
> 尚未遷移，仍使用它既有的 `case`／`when`／`else` 契約；其規範見
> `function分支語意與選擇層_v1.0.md`（P69）§3.4。歷時側另有 `choose:`
> （列舉候選、不執行），共時側無對應物。

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
即停止。Unmatched 才繼續；Error 立即中止且不進 `base`。`case` 的 fallback
關鍵字是 `base`，不接受 `else`。

```lang
syn:
    case:
        $self.syn.number == singular:
            feature:
                exponence = suffix
        base:
            feature:
                exponence = zero
```

`base` 可直接帶純量結果，也可留白。空白 `base:` 回傳該 context 的既有基底：
Sign 保留目前 Sign、feature 保留目前值、role 保留目前 binding、phon 保留深層模板。

## Bundle match：先宣告欄位，再寫值列

`case [dim.feature, ...]:` 與 `when [dim.feature, ...]:` 先宣告有序的 typed-feature
投影。每個普通 branch 寫一列同欄數的值；`_` 表示該欄不參與這一列的判定：

```lang
case [syn.tense, syn.person, syn.number]:
    [present, first, singular]: am()
    [present, third, singular]: is()
    [present, _, _]: are()
    base: be()
```

- 每列的所有非 `_` 欄位是 AND；context 的其他欄位不影響匹配。
- 值的 enum domain 由同位置的 feature 推導，因此列中不用重寫完整路徑。
- 欄位必須是目前 effective Sign 已宣告的 typed feature；重複欄位、欄數不符、
  未知欄位與值域外的值都是編譯錯誤。
- 缺失或尚未決定的欄位不匹配常量；`_` 不讀該欄。
- `case` 仍是來源序 first-match。較早列涵蓋較晚列時，編譯器發出
  `CASE_BUNDLE_BRANCH_SHADOWED` warning。
- bundle 列一旦匹配，才執行該列結果；結果失敗是 Error，不會再嘗試下一列或
  `base`。

上例若位於 `paradigm BE:`，結果使用統一的 SignApplication 拼法；零參 `be()` 回傳
同一個 stored Sign 與 SignId。Paradigm 的完整契約、partial application、deferred
autofill 與消費者介面見 `Paradigm與SignApplication_V3.md`。

## `when:`：frozen matching、ordered merge

`when:` 是累加式 expression，但 matching 與 merge 是兩個分離階段：

1. 建立一次合併前的 frozen Sign snapshot。
2. 所有非 `else` guard 都只讀同一份 snapshot；先前命中的 fragment 對後續 guard 不可見。
3. 任一 guard 為 Error 時，在 merge 前中止，沒有 branch、fragment 或 trace 被部分提交。
4. 全部 guard 結果確定後，才把所有 Matched fragment 按來源序合併；同 path 衝突沿用
   trait merge 的 stable later-wins。
5. `else` 只在沒有任何普通 branch Matched 時命中。`when` 不接受 `base`。

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
| bundle parse／canonical／first-match／`_`／空白 base | `bundle_match::bundle_case_round_trips_and_uses_first_match_wildcards_and_empty_base` |
| bundle 欄數、欄位與 enum domain 負例 | `bundle_match::bundle_header_rows_are_shape_checked_by_the_parser`、`bundle_match::bundle_fields_and_values_are_checked_against_typed_feature_domains` |
| `when` bundle frozen matching 與 `else` | `bundle_match::when_bundle_matching_reads_one_frozen_snapshot_and_else_is_fallback_only` |
