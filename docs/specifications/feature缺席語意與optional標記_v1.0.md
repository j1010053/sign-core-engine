# feature 的缺席語意與 `?` 標記(P75)

> **狀態:已裁定並實作**(擁有者 2026-08-12)。
> 承 P71 增修 D/E(讀取路徑受封閉清單約束)、P43(Else 三分)、
> `sem_roles_self_phon_realization_v1.md` §1(`feature:` 的三種形式)。
> D/E 管「這條路徑**合不合法**」;本檔管「這條合法路徑在這個主體上**沒有值**時怎麼辦」。

---

## 1. 裁定

> **缺席容忍在宣告處宣告,讀取處繼承。**

| # | 規則 |
|---|---|
| **R1** | feature 宣告可帶尾綴 `?`:`NAME = enum(...)?` 表示**這條 feature 可以沒有值**。 |
| **R2** | 讀到**宣告過、但沒有 `?`** 的 feature 而它沒有值 → **執行期 Error**(`RuleStatus::Error`),不是靜默 `Unmatched`。有 `?` → `Unmatched`(可落 `else`),即今日行為,但現在是被授權的。 |
| **R3** | 範圍**限 typed feature**。封閉清單座標(`syn.tam.present` 等)沒有宣告處可掛 `?`,維持缺席容忍;收緊屬 P71 Phase 2 的座標宣告機制。 |
| **R4** | `?` 在 canonical form 上**可省略**:`optional == false` 不印,故**未使用此語法的宣告**其 canonical 逐位元不變、不產生無謂的 library lock churn。(真的需要 `?` 的套件當然會變——實例與代價見 §4.1。) |
| **R5** | `?` 只對**宣告**有意義。貼到賦值(`n = sg?`)是明確的 parse error,不是默默忽略。 |

---

## 2. 為何是 `?`、為何在宣告處

`?` 在本語言裡**已經有且只有一個意思**——「這東西可以不提供」——而且已經在兩個
宣告處用著:

```lang
slots:
    indirect [Animate]?      /* slot 宣告 */
roles:
    recipient [Animate]?     /* role 宣告 */
feature:
    case = enum(nom, acc)?   /* P75:第三個宣告處 */
```

第三個宣告處沿用它,讀者不必學新符號。三條理由:

1. **符號的單一意義**。移到讀取處(`$self.syn.n?`)會讓同一符號有兩個方向:
   宣告處是「可以不提供」,讀取處是「我容忍它不在」。
2. **optionality 屬於型別,不屬於使用點**。這是 FP 模型的直接後果,也讓現有
   41+ 個讀取點**一個都不用改**。與 P71 R2「值域宣告是唯一真相來源」同一條線,
   只是宣告多說一件事:**可不可以沒有**。
3. **讓 `else` 恢復單一語意**。原本 `else` 兼任「守衛不成立」與「讀不到」兩件事;
   `?` 之後 `else` 只表示前者。這句話 spec 其實已經寫過一次
   ——`sem_roles_self_phon_realization_v1.md` §3.1 的「**source 缺值不能以 Else
   掩蓋**」——只是當時只約束了 `slot_features:` 一個角落。

### 2.1 語言早已有半套,只是沒推廣

`read_slot` 的四種結局在 P75 之前就已經是這個形狀:

| 情形 | 結局 | 依據 |
|---|---|---|
| filler 在、有值 | `Value` | — |
| filler 在、**那個 feature 沒值** | ~~`Unmatched`~~ → **P75:視宣告有無 `?`** | 本檔 |
| 槽未填,槽宣告了 `?` | `Unmatched` | 槽的 `?` |
| 槽未填,槽**沒有** `?` | `Error`,訊息帶路徑 | 槽的 `?` |

後兩列說明「宣告處決定缺席容不容忍」早就是本語言的規約。P75 只是把第二列補齊,
並把同一條規約給 `$self`——它原本連類比物都沒有,永遠 `Unmatched`。

### 2.2 作者原本在自己刻 Option

```lang
optional-value = enum(singular, plural, absent)
optional-value => $slot.optional.syn.number
    else optional-value => absent
```

把「沒有值」編碼成值域裡的一個成員。這是語言缺了一個概念時的典型症狀。

---

## 3. 檢查的觸發條件(關鍵:不是「沒有值就錯」)

**只有宣告在該主體上可見、而值不存在時才觸發。** 主體上根本沒有那條宣告時
維持 `Unmatched`。

這一條讓 `?` 保持稀少,也修正了一個直覺上的誤判:「trait 上宣告的 feature,
只要被可能非成員的主體讀到,就都需要 `?`」——**不對**。非成員看不到那條宣告,
所以檢查不觸發。實測即有此例:`function_guards.rs` 的 `x.syn.telic == no` 讀
名詞 `stone`,而 `telic` 宣告在 `stone` 不屬於的 trait 上,故不受 P75 影響,
仍是合法的「缺席即假」。

需要 `?` 的是**另一種**情形:宣告看得到,但值由別處/稍後供給——

| 供給者 | 實例 |
|---|---|
| 外層構式的 occurrence binding(`slot_features:`) | `LocalCaseBearer.case`、`TutorialNominal.case` |
| 呼叫端注入(`DerivationContext::feature`) | `TestCountTransfer` 的 `syn.number` |
| 更後面的 stage(`@stage phrase` 寫、`@stage stem` 讀) | `sem_roles_realization` 的 `Plain.value` |
| 前一條規則(而該規則自己因缺席而未觸發) | tutorial 的 `sem.interpreted_case`(串接自 `syn.case`) |

最後一列是**串接**:一條缺席會沿規則鏈往下傳,所以遷移要跟著鏈走,不是逐條獨立判斷。

---

## 4. 量測與遷移

方法:instrument `read_self`／`read_slot` 的缺席分支,跑全 workspace 測試,
收路徑+主體(P71 §3 的同一套方法;依 §7.5 的教訓報**站點**而非執行次數)。

- 全 workspace **13 次缺席讀取 / 8 條路徑**。
- 其中會觸發 P75 Error 的:**8 個站點**(`syn.case`×2、`syn.mark`×2、`syn.value`、
  `syn.number`、`sem.number`、`sem.interpreted_case`)。
- **套件側(std / natural)= 0**——但這是**測試套件跑得到的範圍**,不是全部,
  見 §4.1。
- `slot-filled`(filler 在、feature 沒值)**實測 0 次**——規約上的洞照樣補齊,
  但補起來零風險。
- 預設翻轉後**只有 1 個測試觀察得到失敗**
  (`slot_feature_bindings::derived_token_downward_case_forwarding_…`),
  其餘站點的 `Unmatched → Error` 不改變任何既有斷言。

遷移共 8 處宣告加 `?` + 教學文件 3 處同步(`tutorial_examples` 的文件對照測試
會抓到 `.lang` fixture 與教學正文的漂移,已一併更新並補一段 `?` 的說明)。

### 4.1 補量測:測試覆蓋之外的套件(**修正 §4 的「套件側 = 0」**)

§4 的執行期量測只看得到**測試套件實際跑過**的路徑。補做兩輪覆蓋全套件的掃描:

1. **靜態**(每個 sign × 每條可見且無 `?` 的宣告,投影裡有沒有值):
   en-standard **37 個 (sign, path) 候選 / 8 條路徑**,std **0**。
   但這份清單**大量偽陽**——由規則在求值期寫入的 feature(`finite_form`、
   `subject_case` 等)在未求值的 sign 上一律顯示為無值。
2. **執行期**(替每個構式自動挑滿足約束的 filler 後 `derive`):
   真正觸發 P75 的 **只有 1 個**——`EnglishCountNounForm` 讀 `$self.syn.number`。

該讀取的值由 `DerivationContext` 注入(`grammar.lang` 的註解即如此寫),
故宣告端 `AgreementBearer.number`(**std:core**)加 `?`。同一 trait 的 `person`
**不加**:兩輪掃描都沒有任何讀取會在它缺席時觸發,不無憑據放寬檢查。

**因此 R4 的「零 churn」在本次不成立**:std:core 的內容變了,其
`library std:core@0.1.0 sha256:` 隨之改變,簽入的
`tutorials/en-standard-reconstruction/restore.chg` 需重新 bless
(`cargo run -p conlang-changeset --example bless_en_standard_restore`,
diff 僅該一行,statements 未變)。

R4 的正確表述是:**沒有用到 `?` 的宣告不會產生 churn**;真的需要 `?` 的套件
當然會變。前者仍成立且是 R4 的目的(避免全庫無謂重印),後者是內容確實改變的
必然結果。教訓:「套件側零影響」不能只靠測試套件的執行覆蓋推得,要對套件本身
做覆蓋掃描。

---

## 5. 落地與出口證據

- `crates/language/src/lib.rs`:`FeatureDecl.optional`。
- `crates/language/src/parser.rs`:`NAME = enum(...)?`;賦值帶 `?` 明確拒絕(R5)。
- `crates/language/src/printer.rs`:`optional == false` 不印(R4)。
- `crates/language/src/synchronic.rs`:`read_self` 的缺席分支查 `required_feature`;
  `read_slot` 的 filler-present 分支查 snapshot 的 `required_features`;
  `absent_feature_message` **必須指出 `?` 是正解**(與 P71 §4.2 同一條原則)。
- `crates/language/src/construction.rs`:`FillerMaterial`/`FillerSnapshot` 增
  `required_features`(宣告住在 filler 上,`$slot` 讀取才判斷得了)。
- 出口:`crates/language/tests/p75_optional_feature.rs`(**10 案**)——值/guard
  兩種讀取的正反例、`else` 接手、封閉清單座標不受影響(R3)、canonical 省略
  (R4,「未用到就不重印」的**直接證據**而非推論;真需要 `?` 的套件仍會變,見 §4.1)、
  round-trip 不動點、賦值拒絕
  (R5)、`$slot` 兩案;每條否定斷言均配正向控制組。
- **突變 5/5 首輪全紅**:①`$self` 檢查永不觸發(2 紅)②忽略 `?` 一律 Error
  (3 紅)③一律印 `?`(1 紅)④parser 吃掉 `?` 但不記錄(6 紅)⑤slot 側不查(1 紅)。
- 覆蓋掃描與 std:core 的 `?`(§4.1)+ `restore.chg` 重 bless。
- 回歸:`cargo test --workspace --exclude langcraft-desktop --tests`
  **987 綠、0 失敗**。

---

## 6. 未納入

- **c-乙:編譯期「必然缺席」警告**。主體是具體 sign 時,「沒有賦值、也沒有任何
  規則寫這條 feature」在 compile 期可判定,可以不必等到執行期才知道。**擱置**
  ——`then` 鏈與 case 分支會讓「必然」的判定必須保守到什麼程度是獨立題目,
  不該綁住主線。有需求再開。
- **封閉清單座標的缺席**(R3):等 P71 Phase 2 的座標宣告機制,屆時 `?` 隨之而來。
