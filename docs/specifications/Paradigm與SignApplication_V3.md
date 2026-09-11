# Paradigm 與 SignApplication（V3）

## 1. 三種本體各做一件事

V3 不把繼承、函式套用與範式選擇塞進同一種節點：

1. `global trait`／具名 trait 是模板與分類的 vertical link；展開後內容進入 Sign，
   trait 本身不成為可發音的 Sign。
2. Sign 是可呼叫值。`sign(arg)` 以 slot 作參數；`sign()` 對零參 Sign 回傳該 Sign
   自身並保留 SignId。尚有必填 slot 時，結果是同一 Sign 的 partial application。
3. `paradigm` 是具名 horizontal link。它觀察 feature bundle，選出一個成員 Sign；
   Paradigm 有自己的 ParadigmId，但求值結果的本體與 SignId 都屬於被選成員。

因此 `BE` 不等於 `am`、`is`、`are` 或 `be`；它是「在什麼條件下連到哪個
Sign」的穩定關係節點。

一份有效語言至多只有一個自動套用的 `global trait`。選擇方式就是宣告本身：只有
被選中的模板寫成 `global trait`，其餘候選寫成普通 `trait`，需要時再以 `belongs`
或 trait mount 顯式使用。若同時宣告兩個以上 `global trait`，編譯以
`GLOBAL_TRAIT_SELECTION_REQUIRED` 拒絕；V3 不另設隱藏的預設值或頂層 selector。

「自動套用」涵蓋整份模板契約：slot、feature／role schema、預設值與非 phon 規則
都會成為每個 Sign 的有效內容，普通 trait 也可直接引用 global 提供的 slot／schema。
值優先序固定為 global（最低）< 顯式 `belongs` 的特定 trait < Sign 本地內容。
global 自己的 `belongs` 祖先亦屬模板繼承鏈；該鏈的 phon 規則經 stage 排序後只編入
Compiled Grammar 一次，從 effective Sign 排除，避免再當作局部 cophonology 執行。

## 2. 語法

```lang
paradigm BE:
    case [syn.tense, syn.person, syn.number]:
        [present, first, singular]: am()
        [present, third, singular]: is()
        [present, _, _]: are()
        base: be()
```

Paradigm 的 body 固定為一個 bundle `case`：

- selection 固定為來源序 `firstmatch`；不接受 `when`。
- `base` 必須是最後一列，而且必須有明確的 Sign application；空白 `base:` 是
  一般 `case` 的 identity fallback，在 Paradigm 中不合法。
- 每列結果必須直接是 Sign application。不可回傳 fragment、`$self`，也不可把另一個
  Paradigm 當成成員。
- Paradigm 不擁有 `belongs`；分類仍屬於成員 Sign。
- 所有成員必須宣告相同的 match-feature domain，且預先套用後留下完全相同的
  residual parameter interface。

列中的非 `_` 值全部以 AND 比對。缺少或未決定的 feature 不匹配常量；`_` 完全不讀
該欄。較早的列涵蓋較晚列時發出 `PARADIGM_BRANCH_SHADOWED` warning。

## 3. Sign 是函式

```lang
sign left_form:
    syn:
        slots:
            value [Entity]
    phon:
        /l{$slot.value}/
```

- `left_form()` 回傳帶有自由參數 `value` 的 partial Sign。
- `left_form(value: {$slot.item})` 綁定具名參數。
- 單一參數仍可用位置 shorthand；多參數必須具名。
- 參數值可為 `$self`、`{$slot.name}` 或另一個 `sign(...)`／`paradigm(...)`。
- `be()` 這類零參呼叫是 identity-preserving，不建立匿名或複製 Sign。

Paradigm 也使用同一套 `SignApplication` 表面。若各分支先回傳 `left_form()`／
`right_form()`，兩者 residual interface 都是 `value`，則 `FORM()` 回傳 partial member，
`FORM(value: {$slot.item})` 則把參數轉送給選中的那一個成員。

## 4. 消費者寫法

### 4.1 Sign-valued `case` 分支

Paradigm 可出現在任何原本接受完整 Sign application 的結果位置：

```lang
case [syn.variant]:
    [selected]: FORM(value: {$slot.item})
    base: fallback(value: {$slot.item})
```

列一旦命中便 committed。選中成員的 constraint、參數或執行錯誤會直接回報，不會把
錯誤偽裝成 Unmatched 再試下一列。

### 4.2 construction deferred autofill

需要先從其他 slot 算出 agreement bundle 時，使用 callable autofill：

```lang
sign Clause:
    syn:
        slots:
            subject [Nominal]
            copula [Copula]
        map copula autofill BE()
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person => $slot.subject.syn.person
            number = enum(singular, plural)
            number => $slot.subject.syn.number
    phon:
        /{$slot.subject} {$slot.copula}/
```

執行順序是：先放入顯式 filler → 以 consumer 規則計算 Syn／Sem／Prag probe → 凍結一次
bundle → 所有 deferred autofill 都讀同一 probe 並各自選成員 → 全部成功後原子提交 →
正式 pipeline 只跑一次。某個 autofill 仍留下必填 residual parameter 時整次失敗。

顯式提供該 slot 會覆蓋 autofill；`autofill` 是預設填充者，不是不可覆寫的強制值。

### 4.3 Rust／Query 消費者

- `CompiledSystem::evaluate_paradigm(name, context)`：直接以 feature bundle 選成員。
- `CompiledSystem::evaluate_application(current, application)`：程式化執行一般
  Sign／Paradigm application，`$self` 與 `$slot` 讀 `current`。
- `CompiledSystem::callable_parameters(name)`：在選成員前取得共同 residual interface。
- `conlang_query::paradigms(system)`：列出 ParadigmId、欄位、分支、canonical
  application 與成員 SignId。`lexicon()` 仍只列 Sign，不製造假的 Paradigm 詞條。

套件以 `paradigm` export kind 與 `paradigms` capability 發布；不能冒充 `sign` export。

## 5. 驗證與可觀察性

靜態錯誤涵蓋：重名／Sign 名衝突、缺少非空 final base、未知成員、非 Sign 結果、
成員 residual signature 不一致、match feature 缺失或 domain 不同、列值超出 domain，
以及 pattern 與成員可接受 feature values 衝突。

identity sidecar 為 Paradigm、Case、CaseBranch、Application 與巢狀 argument 配置穩定
NodeId，member reference 指向實際 Sign。trace 的 `ParadigmCaseRecord` 同時保存 ParadigmId
與被選 SignId，讓「關係改了」和「成員本身改了」可區分。

Paradigm 也參與完整演化生命週期：expert-source reconcile 以內容指紋保留改名前的整棵
identity；多 parent merge 以 ParadigmId 做逐項 3-way，並拒絕合併後才形成的
Sign／Paradigm callable 撞名；snapshot 則把每個 Paradigm 存成獨立內容定址物件。
manifest 的 `paradigms` 為 default-empty 增量欄位，舊 v1 manifest 仍可讀。

DiffVector 將 Paradigm 生滅／內容變更記為 structural event，不冒充成員 Sign 的 leaf
變更；互通度因此能看見水平選擇關係改變。現有 Wire V1 不新增欄位，而是把事件納入既有
`structural` scalar，詞典 schema 與 lexicon 語意不變。

`.chg` 可持久化：

- `paradigm("BE").name`
- Case 的 `.selection` 與 `.match_features`
- CaseBranch 的 `.condition`
- Application 的 `.application`

上述更新在 reconstruct／dump／parse／replay 後保留既有容器與子節點身分。新增或刪除
整個 Paradigm 使用一般四原語 Insert／Delete。

## 6. 本階段刻意不做

- Paradigm 不使用 `when`、匿名 context fragment 或 `belongs`。
- Paradigm 成員不再指向另一個 Paradigm；需要複合選擇時在 consumer 明寫另一層 case。
- 沒有 backtracking／best-match specificity；只有來源序 first-match。
- Paradigm 不建立 synthetic Sign，也不直接進現有 Wire/UI V1 詞典 schema。

## 7. Requirement-to-evidence

| 契約 | 自動化證據 |
|---|---|
| global 唯一性、整份契約自動繼承、最低值優先、繼承 phon 只執行一次 | `crates/language/tests/global_trait_v3.rs`、`codegen.rs` |
| parser／canonical round-trip、final `base`、成員與 pattern 驗證 | `crates/language/tests/paradigm_v3.rs` |
| nullary identity、partial application、參數轉送、公開 callable API | `crates/language/tests/paradigm_v3.rs` |
| frozen deferred autofill、多 autofill、顯式 filler override | `crates/language/tests/paradigm_v3.rs` |
| 教學區塊可直接編譯並選出 stored `is` | `crates/language/tests/tutorial_examples.rs` |
| Paradigm／branch／application identity 與 `.chg` replay | `crates/changeset/tests/reconstruct_roundtrip.rs`、`insert_block.rs` |
| expert-source rename 保留整棵 Paradigm identity | `crates/changeset/tests/identity_reconcile.rs` |
| Paradigm 3-way merge、內容衝突與跨 kind callable 撞名 | `crates/changeset/tests/merge_plan.rs` |
| Paradigm diff、互通度與 Wire V1 structural 投影 | `crates/changeset/tests/diff_vector.rs`、`crates/query/tests/grouping.rs`、`crates/app/tests/ipc.rs` |
| snapshot object round-trip 與舊 manifest 相容 | `crates/persistence/tests/store_roundtrip.rs` |
| Query catalog 不污染 lexicon | `crates/query/tests/paradigms.rs` |
| package export/capability、reference 禁止、跨 package 名衝突 | `crates/language/tests/package_loader_v2.rs` |
| en-standard 的 BE／DO 真實 consumer 與 provenance | `crates/language/tests/library_cxg_english.rs` |
