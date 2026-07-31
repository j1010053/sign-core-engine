# `syn:`／`sem:` `feature:` 與單一 Deep Construction Sign 規範（v0.1）

> 狀態：已實作。parser、effective-sign validation、runtime、`std` 與
> `natural:en-standard` 已依此契約遷移；可執行證據見
> `tests/sem_roles_realization.rs` 與 `tests/library_cxg_english.rs`。
>
> 本文件補充《14_共時lang語法與資料貼合度》與《16_lib_std_cxg與標準英語參考庫》。若既有文件把未宣告的 `syn` path 當成可自由寫入欄位，或把同一 deep construction 的表層變體建成不同 construction sign，以本文件為準。

## 1. 決議摘要

1. **資料欄位必須先有型別宣告，才能保存或產生值。** `syn:` 與 `sem:` 的 `feature:` 都必須宣告為有限 `enum`。
2. `feature:` 是與 `slots:` 同級的語法容器，不是 runtime path 的一部分。宣告為 `syn.feature:number` 的欄位，規則仍以 `syn.number` 或 `$slot.subject.syn.number` 讀取。
3. feature 宣告衝突沿用普通 `=` Definition 的同一套 priority resolution；不得另設 feature 專用優先規則，也不得合併兩個 enum domain。
4. **一個 deep construction 由一個穩定的 `sign` 表示。** 因環境、filler feature、語言 profile 或 allomorphy 而不同的表層實現，必須由該 sign 內的條件規則導出，不得只因表層不同而複製 construction sign。
5. `std` 的 `.lang` 可以用 trait 宣告抽象本體、feature schema、slot schema 或可重用規則片段；但凡是可實例化／可套用的 construction export，必須以 sign 承載 deep construction 身分，再由其規則導出 surface。
6. trait membership 與 enum feature value 是不同機制。類別使用 `belongs`／`== [Trait]`；有限資料值使用 `feature:`／`== value`。

## 2. `syn.feature:` 的表示契約

### 2.1 宣告與賦值

feature 名稱由 library 或語言自行命名，但其值域必須是封閉、有限且具名的 enum：

```lang
trait EnglishAgreementBearer:
    syn:
        feature:
            number = enum(singular, plural)
            person = enum(first, second, third)

sign dog:
    belongs EnglishAgreementBearer
    syn:
        feature:
            number = singular
            person = third
```

同一個 `feature:` 容器容納三種 item：

```text
NAME = enum(VALUE, VALUE, ...)   # schema declaration
NAME = VALUE                    # value constraint/assignment
NAME => EXPRESSION              # rule-produced value
```

辨識依 RHS 的 AST 型別完成，不以命名慣例猜測。

### 2.2 「先宣告」的精確語意

「先宣告」是 semantic visibility，不要求使用者以不穩定的檔案物理順序管理跨 package 依賴。某次賦值或規則寫入合法，當且僅當 compiler 在該 sign 的 effective schema 中能解析到 feature declaration。可見來源為：

1. 已載入 dependency／std／selected natural／plugin package 所提供的宣告；
2. sign 的 `belongs` closure 所繼承的宣告；
3. sign 本地宣告。

因此 compiler 必須分成至少兩個語意步驟：

```text
收集候選宣告
  -> 依既有 priority 選出 effective declaration
  -> 解析並驗證 value/rule assignment
```

以下均為 compile error：

- 找不到宣告便賦值；
- 值不屬於 effective enum domain；
- 規則可能寫出 domain 外的值；
- 對 feature path 寫入結構值、任意字串或未宣告 enum member；
- 將 feature declaration 當成 runtime Patch 變更。

宣告只規定「此欄位可存在及其值域」，不等於每個部分描述都必須立即給值。缺值不是隱含的 `?` enum member；匹配時依規則契約成為 `Unmatched`，要求完整值的 construction validation 則應報 error。

### 2.3 enum 不可變式

- enum 至少有一個 member；member 不得重複。
- feature name 與 member 都是 identifier，不以自由字串承載結構。
- effective declaration 決定唯一 domain；不得把祖先的兩個 domain 做 union。
- Sign、DerivedToken 與 Patch 只能 Set／Unset 值，不能新增 member 或改寫 domain。
- `unify(a, b)` 只允許相同 effective domain；兩個已定且不同的 member 產生 runtime Error。
- 序列化必須保存 declaration origin、effective winner 與值；round-trip 不得把 enum 降格成無型別字串。

## 3. 宣告衝突與 priority

feature declaration 必須重用 `=` Definition 的 winner selection，不另發明一套「feature specificity」。概念上：

```text
effective library load order
  -> inheritance distance / belongs order
  -> local sign
  -> 同一 effective priority 時的穩定來源序（後者勝）
```

實際實作應呼叫與 Def 相同的候選排序／winner 函式，而非複製判斷。既有契約仍成立：caller/local 勝繼承內容、較近祖先勝較遠祖先；同距離時，較後寫的 `belongs` 來源勝；最後以穩定文件序決勝。

例：

```lang
trait TwoNumber:
    syn:
        feature:
            number = enum(singular, plural)

trait ThreeNumber:
    syn:
        feature:
            number = enum(singular, dual, plural)

sign Example:
    belongs TwoNumber
    belongs ThreeNumber
```

`Example` 的 effective `number` domain 由既有 priority／`belongs` 順序選出 `ThreeNumber` 的三數 domain；不產生四值 union，也不因名稱相同便把兩者視為相容。

診斷規則：

- 完全相同 domain：去重，可保留 info/provenance；
- 不同 domain 且可由 priority 決勝：warning，列出 winner、shadowed declarations、package、trait/sign 與來源行；
- winner 無法依既有 Def 規則決定：error；
- declaration 決勝後，effective value 不在 winner domain：error。

此處的 priority 是 compile-time schema resolution，不是 runtime construction competition。

## 4. Trait、Feature、Slot 的責任邊界

| 機制 | 表示內容 | 判斷／操作 |
|---|---|---|
| trait／`belongs` | 類別、本體位置、構式家族、可繼承約束 | `$slot.x == [Nominal]` |
| `syn.feature:` | 該類型適用的有限句法值 | `$slot.x.syn.number == plural` |
| `slots:` | 構式成分、論元、價位與 filler constraint | required／optional、fill、SlotMap |
| Rule／Then／Else | 在 snapshot 上選擇條件實現並產生 patch | `==`、`unify`、Matched／Unmatched／Error |
| phon template/runtime | deep form 到 surface 的排列、拼接及音系實現 | surface、trace、source map |

禁止把下列內容降格成 enum feature：

- category membership（例如 `class = nominal`）；
- constituent／argument slot；
- filler identity、共指、component DAG 或 semantic role edge；
- 任意 constituent order 列表；
- phon template；
- construction 的 stable identity；
- Grambank 的來源、版本或可信度 metadata。

因此 `syn.class = copula` 應由 `belongs Copula` 取代。`number`、`person`、`case` 等封閉形態句法值才是 `feature:` 的主要對象。

## 5. `==` 與 `unify`

不新增 `is` 關鍵字，條件判斷統一使用 `==`：

```lang
$slot.predicate == [Copula]
$slot.subject.syn.number == singular
```

兩種 RHS 使語意可靜態判斷：

- `[Trait]`：對 sign/category closure 做 membership/subsumption；
- enum member：對已宣告 feature 做 typed equality。

欄位對欄位的 agreement 仍使用 `unify`：

```lang
agreement.number => unify(
    $slot.subject.syn.number,
    $slot.predicate.syn.number
)
```

原因是 unification 可以約束尚未定值的一端，而 `==` 只回報目前條件是否成立，不能反向修改 filler。construction rule 只寫自身 token；filler snapshot 不可變。

## 6. 單一 Deep Construction Sign

### 6.1 身分規範

Deep construction 是一個 conventional form–meaning pairing，其穩定身分由一個 `SignId` 承載。該 sign 至少保存：

- 所屬 construction/category traits；
- form pole 的 slot、結構約束與底層／模板資訊；
- meaning/function pole 的 sem/prag 約束；
- 可用的 syn feature declarations 與 deep constraints；
- 從 deep state 選擇 surface realization 的 Rule／Then／Else；
- provenance、來源 package 與原 `.lang` 行號。

推導順序固定為：

```text
解析同一 deep construction sign
  -> context-free 評估 filler，建立 frozen probe
  -> 原子解析 construction 的 slot_features constraints
  -> stored sign 從 effective base、derived token 從 deep baseline 重跑 Syn→Sem→Prag
  -> 重選 filler occurrence realization
  -> 凍結 FillerSnapshot
  -> 組合含 phon/syn/sem/prag 的 DerivedToken
  -> 執行 construction/token rules
  -> 依 finalized token 選擇 construction realization branch
  -> phon runtime 導出 surface
```

surface 是衍生結果，不回寫成另一個 construction sign。

### 6.2 哪些差異仍屬同一 sign

下列差異預設在同一 deep construction sign 內實現：

- singular/plural、person、tense 等 feature-conditioned form；
- phonological allomorphy；
- 同一構式在不同 phonological environment 的表層變體；
- 同一 constituent/semantic role 結構的條件語序；
- overt／zero exponent；
- 能由 profile trait 或 filler feature 選出的實現。

例如 `dog/dogs` 若分析為同一 lexeme/construction 的 productive number realization，應保有同一 deep sign，先以 `number` feature 表示 deep constraint，再由 morphology/phon rule 產生 surface。只有目前缺少通用屈折 primitive 時，才可暫以兩個 lexicalized signs workaround；文件與測試不得把 workaround 宣稱為理論模型已完成。

### 6.3 何時必須拆成不同 sign

只有出現新的 conventional pairing 或獨立 lexical identity 時才拆 sign，例如：

- meaning/function 改變而形成另一個已固化構式；
- valence／semantic roles 改變且不是同一構式的可預測環境實現；
- suppletion 由獨立 lexical sign 填充 paradigm slot；
- constructionalization 建立新的 network node。

單純 surface string、allomorph 或規則分支不同，不足以建立新 construction sign。

## 7. `std` `.lang` 的強制規範

### 7.1 允許的分層

`std` 可保留「抽象範疇／可選具象實作」的檔案分層，但分層是 code organization，不是 construction identity：

```text
code/schema.lang          trait：本體、feature/slot schema、跨語言約束
code/realizations.lang    trait：可重用的 realization rule fragments
code/constructions.lang   sign：一個 deep construction 一個穩定身分
```

- schema trait 不可假裝是可直接 derive 的 construction token。
- realization trait 可以被 deep sign 引用，但只能展開成該 sign 的規則／約束；不得成為另一個 surface-only construction identity。
- 可執行 construction export 的 kind 必須是 `sign`。
- 同一 deep construction 的多個 surface branches 必須留在同一 sign／CompiledSign／RuleRecord provenance 下。

### 7.2 std 不得偷渡語言專屬 feature

generic `std/cxg` 不得在沒有 declaration 的情況下直接讀寫 `syn.number`、`syn.person` 等欄位。可選策略只有：

1. std 宣告真正跨語言且 domain 穩定的 feature；或
2. std 只提供 controller／target slot schema，讓 natural package 宣告 feature domain 並加入具體 agreement rule。

對 `number` 而言，因語言可能具有二數、三數、paucal 或完全不使用 number agreement，預設採策略 2。

Grambank trait 可以表達類型學觀察並授權某個 deep construction constraint。例如 GB117 的「predicate nominal 必須使用 copula」可以讓 profile sign/trait 選用 required-copula constraint；真正的 copula 參與仍必須由 construction 的 required `copula [Copula]` slot 表示，不能只保存 `copula = required` 後便視為已驗證。

## 8. 實作狀態與證據

1. AST 有 typed `FeatureDecl`／`FeatureValue`，parser/printer 支援 `syn:`、`sem:` 的 `feature:`，並保留原 `.lang` 行號。
2. `std:core` 已以 `belongs` 表示分類，不再使用可變的 `syn.class`；Semantic、Frame、Relation 與 Agreement schema 均有 stable export。
3. `std:cxg` 的 slot-aware rule 以 `$slot`／`$self` 讀取 typed feature；generic schema 不預設所有語言都有同一個 number domain。
4. `FeatureRule` 與一般 Rule 使用同一個 stable Stem→Word→Phrase dispatch stream；同 stage 保留來源序。這避免 feature AST 變體繞過 stage precedence。
5. `roles:` 的 frame contract 使用 trait identity；required role 在 token saturated 時強制、partial application 可暫缺。`SemNode` 保存 typed features、recursive roles 與 provenance。
6. 同一 `EnglishCountNounForm` Sign 以 `DerivationContext` 和 `phon.realization:` 產生 `dog`／`dogs`，SignId 不變。規則性的 `-s` 是可執行 realization；`seen` 等不規則形式仍明確標成 lexicalized workaround。
7. `SemanticDocumentV1` 是唯一未來 LLM interchange boundary：schema/version、unknown-field rejection、deterministic key/type ordering 和 detached revalidation 都已測試；沒有 backend 或網路呼叫。

`docs/specifications/sem_roles_self_phon_realization_v1.md` 定義完整 runtime 順序與 syntax；遷移測試先保留反例，再驗證修正後的公開入口。

## 9. Requirement-to-evidence

| 契約 | 最小正例 | 必要反例／觀察 |
|---|---|---|
| 宣告後才能賦值 | inherited `number` declaration + `number = plural` | unknown feature、domain 外值 compile error |
| declaration priority | 衝突 domain 依 `belongs`/package priority 選 winner | 不可 union；diagnostic 含 winner/shadowed provenance |
| typed equality | `number == singular` | 跨 domain 比較 compile error |
| typed unify | 同 domain 未定值＋定值成功 unify | 不同 domain compile error；不同定值 runtime Error |
| Patch 不改 schema | Set／Unset feature value | 新增 enum member／改 domain 被拒絕 |
| 單一 deep identity | 兩個環境分支產不同 surface、SignId 相同 | 只因 allomorph 便產生兩個 construction SignId 應失敗 |
| filler 不可變 | construction 讀 frozen feature 並改 token | filler projection 逐位元不變 |
| std construction export | exported sign 經 compile→derive→surface | surface-only exported sign／trait 不可冒充 deep construction |
| 四維保留 | surface 不同但 sem/prag/provenance 保留 | 只斷言字串不算證據 |
| 決定性 | 同輸入兩次 domain winner、branch、surface、trace 相同 | map iteration 不得改 winner/order |

## 10. 本階段不新增的語法

- 不新增 `is`；使用 `==`。
- 不新增 `class=` 作 category 的平行資料欄位。
- 不以 `form:`／`meaning:` 重包裝既有 `phon+syn`／`sem+prag`。
- 不把 `recipe`／`goal` 當成共時 `.lang` 關鍵字；它們仍是歷時 function 的分層 code 名稱。
- 不以 `?` 充當 enum member。feature optionality／underspecification若要成為一等型別，另案定義。
- 不因本文件直接新增 `where`、`order`、`when` 或 Grambank 專屬關鍵字。
