# Sem Roles、`$self` 與 phon realization

本規範把一個 construction 的深層 pairing、其可變 feature 條件，以及最後的 phon 實現分開保存。`SignId` 指向深層 construction；不同數、格、語用環境選擇不同 realization，並不產生另一個深層 sign。

## 1. 已宣告的 enum feature

`feature:` 只可出現在 `syn:` 或 `sem:`。欄位先宣告 enum domain，之後才可有固定值或 rule 寫入。

```lang
trait EnglishAgreementBearer:
    syn:
        feature:
            number = enum(singular, plural)
            person = enum(first, second, third)

sign Example:
    belongs EnglishAgreementBearer
    syn:
        feature:
            number = plural
    sem:
        feature:
            number = enum(singular, plural)
            number => $self.syn.number
```

`NAME = enum(...)` 是 declaration；`NAME = VALUE` 是 assignment；`NAME => EXPR` 是 feature rule。未宣告的欄位、domain 外值，以及跨 dimension 的 `unify` 都是 compile error。衝突 declaration 依 library priority、繼承距離、`belongs` 順序和 local source 順序選出 winner，並保留 winner/shadowed warning；role contract 則不使用此覆寫規則。

`syn.class` 不是 feature。類別是 `belongs` 的 ontology membership；例如 copula 應寫 `belongs Copula`，而不是 `syn.class = copula`。

## 2. Frame trait 與 semantic roles

frame identity 由 trait 表達，結構參數由 `roles:` 表達。`frame = "Transfer"` 這類自由字串不屬於 executable Sem。

```lang
trait Semantic:
trait SemanticFrame:
    belongs Semantic
trait TransferFrame:
    belongs SemanticFrame
    sem:
        roles:
            agent [Animate]
            theme [Entity]
            recipient [Animate]?

sign GivingConstruction:
    belongs TransferFrame
    syn:
        slots:
            subject [Animate]
            object [Entity]
            indirect [Animate]?
    sem:
        roles:
            agent = {subject}
            theme = {object}
            recipient = {indirect}
```

`NAME [Trait]?` 是 role declaration；`NAME = {slot}` 是 binding。相同 declaration 可去重；同名 role 的 constraint 或 optionality 不同是 `ROLE_SCHEMA_CONFLICT` error。binding 可依一般 `=` precedence 覆寫抽象預設。填入 filler 的語意類型必須滿足 role constraint；partial application 可暫缺 role，但 saturated token 不可缺 required role。

`SemNode` 保存 `types`、typed `features`、遞迴 `roles`、以及 source/provenance。舊有 gloss、concept、sense 等非可執行標籤應放在 package `data/` 或 annotation，不作 Sem 的自由執行欄位。

## 3. 唯讀 `$self`、frozen `$slot`

規則只寫自己的 dimension。`$self` 讀取目前已提交的 token，`$slot` 讀取 filler rule 已完成後的 immutable snapshot。

```lang
syn:
    feature:
        target = enum(singular, plural)
        target => $slot.subject.syn.number
sem:
    feature:
        target = enum(singular, plural)
        target => $self.syn.target
prag:
    realized-number => $self.sem.target
```

可用 read/guard：

```lang
$self == [TransferFrame]
$self.syn.number == plural
$slot.subject == [Human]
$slot.subject.syn.number == plural
```

token 固定依 `Syn → Sem → Prag` 執行。因此後段看得到前段 commit；Syn 讀 Sem/Prag 時只看 deep/base 值。Then 分支看得到同 dimension 的先前 commit；Error 保留已 commit 的步驟並中止後續 Then，不會落入 Else。每個 `RuleRecord` 保存 `$self`／`$slot` read、typed value、source line 與 rule namespace。

沒有 `link:`，也沒有任意跨 dimension assignment。

### 3.1 `slot_features:`：構式內的 occurrence feature 傳遞

`slot_features:` 只出現在 `syn:`，位於 `slots:` 與 `feature:` 同級。它不是把整個
slot 複製到另一個 slot，而是由 construction 在組合 token 前，替某個 filler 的**本次
occurrence**設定一個已宣告的 Syn enum feature：

```lang
sign TransitiveClause:
    syn:
        slots:
            subject [Nominal]
            predicate [Transitive]
            object [Nominal]
        slot_features:
            subject.case = $slot.predicate.syn.subject_case
            object.case = accusative
```

語法固定為：

```text
TARGET_SLOT.TARGET_FEATURE = ENUM_LITERAL
TARGET_SLOT.TARGET_FEATURE = $slot.SOURCE_SLOT.syn.SOURCE_FEATURE
```

- 左側指定要被約束的 filler occurrence；右側可以是 enum literal，或另一個已完成
  source filler 的單一 Syn feature。
- target/source 都以 construction 的 internal slot name 解析；caller 使用 SlotMap
  rename 不會改變語法相依關係。
- target filler 必須已宣告該 feature，值也必須在 enum domain；未宣告、domain 外值、
  unknown slot、缺少 required source value 或既有固定值衝突都是 Error。
- optional target 沒有填入時略過；source 缺值不能以 Else 掩蓋。
- 寫入的是 occurrence-local clone，永不修改 lexical sign。完成後才凍結為
  `FillerSnapshot`，供普通 `$slot` rule 唯讀存取。
- 第一版不傳 phon、sem、prag、category、role、slot identity 或任意結構；也不允許
  construction 反向改寫 filler。既有 derived token 尚不接受新的 downward feature。

因此把它稱為「傳遞 slot 訊息」只在窄義上正確：它傳遞的是**由 slot 定位、經 enum
驗證的 Syn 純量值**，不是通用 message bus。普通 rule 中的 `$slot` 仍然只是讀取已
凍結 snapshot。

## 4. 同一 deep sign 的 context 與 phon realization

`DerivationContext` 是 constraint，而不是 priority override。它只能指定已宣告的 feature，並會在 token rule 之後再次檢查，避免 filler/rule 結果默默覆蓋 context。

```rust
let plural = DerivationContext::new()
    .feature(Dim::Syn, "number", "plural");
let derivation = system.derive_with_context(
    "EnglishCountNounForm", fillers, &SlotMap::identity(), plural,
)?;
```

```lang
sign EnglishCountNounForm:
    syn:
        slots:
            stem [Noun]
        feature:
            number = enum(singular, plural)
    sem:
        feature:
            number = enum(singular, plural)
            number => $self.syn.number
    phon:
        /{stem}/
        realization:
            /{stem}s/ / $self.syn.number == plural
            else /{stem}/
```

`realization:` branch 是完整 phon template。guard 只能讀 `$self` 與 frozen `$slot`，不能寫 Syn/Sem/Prag。按書寫順序 first-match；Error 不進 fallback；所有 guard Unmatched 時使用 deep/default template。選中後先展開 `{slot}`，若仍有 `{...}`、`$self`、`$slot` 或非 phon metadata，即為 error。

展開結果是透明的 `RealizedPhonInput(String)`。只有這個純字串被交給 Tshiatūn；realization trace、source line 和 reads 留在 language runtime。surface 從不存入 Language 或 Sign。已實現的 transient token 可保留純 input 以便作為下一個 construction 的 filler，但它不是 phonological surface。

phon template 內的空白是結構性的詞界，不是事後排版：

```lang
/{subject} {predicate} {object}/   /* 三個 phonological words */
/{stem}s/                         /* 同一詞內的黏著實現 */
```

`build_phrase` 將空白建立為 `MorphUnit::Word` 邊界，phrase rule 以 `##` 匹配跨詞環境；
`surface_phrase` 在音變後自動把仍存在的詞界輸出為空格。language runtime 不再刪除
這些空格。大小寫、標點與一般 orthographic pretty-print 仍不屬於 phon surface。

## 5. Semantic JSON v1

未來 LLM 邊界只使用版本化 DTO，不提供 backend、provider、prompt 或網路 API。

```json
{
  "schema": "conlang.semantic/v1",
  "root": {
    "source": {"sign": "GivingConstruction"},
    "types": ["EventFrame", "TransferFrame"],
    "features": {"boundedness": "bounded"},
    "roles": {
      "agent": {
        "source": {"sign": "john"},
        "types": ["Entity", "Animate", "Human"],
        "features": {},
        "roles": {}
      }
    }
  }
}
```

`SemanticDocumentV1::to_json` produces deterministic key/type ordering. `from_json` rejects unknown fields and schema versions. `CompiledSystem::validate_semantic_document` revalidates trait existence, enum domains, role contracts, required roles and source package format, then returns a detached `SemNode`; it never mutates `Language`.

## 6. Boundary

- 不新增 `form:`／`meaning:` wrapper；form pole 仍是 `phon + syn`，meaning/function pole 仍是 `sem + prag`。
- 不新增 `link:`、generic cross-dimension write、phon edit/morphology operation DSL、Sense/Concept graph 或 LLM backend。
- Grambank 保留 `0/1/?` 類型觀察。GB117 的 copula requirement 由 concrete construction 的 required `copula [Copula]` slot 實現，不把 Grambank observation 本身誤作語言專屬 spell-out rule。
