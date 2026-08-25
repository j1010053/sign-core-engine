# Sem Roles、`$self` 與 phon realization

本規範把一個 construction 的深層 pairing、其可變 feature 條件，以及最後的 phon 實現分開保存。`SignId` 指向深層 construction；不同數、格、語用環境選擇不同 realization，並不產生另一個深層 sign。

## 1. 已宣告的 enum feature

`feature:` 可出現在 `syn:`、`sem:` 或 `prag:`(`prag` 由 P71-C 於 2026-08-01 納入;`phon` 仍不支援,其內容是 UR/模板與 DSL 音變規則)。欄位先宣告 enum domain，之後才可有固定值或 rule 寫入。

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

`NAME = enum(...)` 是 declaration；`NAME = VALUE` 是 assignment；`NAME => EXPR` 是 feature rule。declaration 可帶尾綴 `?`（`NAME = enum(...)?`）＝**這條 feature 可以沒有值**，與 `slot`／`role` 的 `?` 同形同義；沒有 `?` 而讀取時沒有值是**執行期 Error**，不再靜默 `Unmatched`（P75，見 `feature缺席語意與optional標記_v1.0.md`）。`?` 貼在 assignment 上是 parse error。未宣告的欄位、domain 外值，以及跨 dimension 的 `unify` 都是 compile error。衝突 declaration 依 library priority、繼承距離、`belongs` 順序和 local source 順序選出 winner，並保留 winner/shadowed warning；role contract 則不使用此覆寫規則。

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
            agent = {$slot.subject}
            theme = {$slot.object}
            recipient = {$slot.indirect}
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
        /{$slot.stem}/
        realization:
            case:
                $self.syn.number == plural:
                    /{$slot.stem}s/
                else:
                    /{$slot.stem}/
```

`realization:` branch 是完整 phon template。guard 只能讀 `$self` 與 frozen `$slot`，不能寫 Syn/Sem/Prag。按書寫順序 first-match；Error 不進 fallback；所有 guard Unmatched 時使用 deep/default template。選中後先展開 `{$slot.NAME}`，若仍有 `{...}`、`$self`、`$slot` 或非 phon metadata，即為 error。

展開結果是透明的 `RealizedPhonInput(String)`。只有這個純字串被交給 Tshiatūn；realization trace、source line 和 reads 留在 language runtime。surface 從不存入 Language 或 Sign。已實現的 transient token 可保留純 input 以便作為下一個 construction 的 filler，但它不是 phonological surface。

phon template 內的空白是結構性的詞界，不是事後排版：

```lang
/{subject} {predicate} {object}/   /* 三個 phonological words */
/{$slot.stem}s/                         /* 同一詞內的黏著實現 */
```

`build_phrase` 將空白建立為 `MorphUnit::Word` 邊界，phrase rule 以 `##` 匹配跨詞環境；
`surface_phrase` 在音變後自動把仍存在的詞界輸出為空格。language runtime 不再刪除
這些空格。大小寫、標點與一般 orthographic pretty-print 仍不屬於 phon surface。

### 4.1 `+`：詞幹域接縫（std 規範，2026-08-17）

空白之外，template 內的 **`+` 是詞幹域接縫**，與詞界並列為第二種結構性標記。
兩者都不是排版，都會跨界進入 Tshiatūn。

```lang
/{subject} {predicate}/     /* 空白 → MorphUnit::Word */
/auf+{stem}t/               /* +   → MorphUnit::Stem  */
```

管道三層都已存在，**`.lang` 側無須任何引擎改動即可使用**：

| 層 | 行為 | 位置 |
|---|---|---|
| 純度檢查 | 擋 `$self`／`$slot`／`{`／`}`／`/`，**不擋 `+` 與空白** | `system.rs` `realize_phon` |
| 詞建構 | `pword.split('+')` → 每段一個 `MorphUnit::Stem` bracket；空段報 `InvalidPhrase{reason:"empty stem domain"}` | `tshiatun/crates/dsl/src/build.rs` |
| 規則環境 | `+` = `Element::StemBoundary`；另有 `#`(詞界)、`##`(韻律詞接縫)、`\|\|`(語句邊緣)、`.`(音節界) | `tshiatun/crates/dsl/src/ast.rs` |

三個必須知道的性質：

1. **`+` 影響分段**。每個 component 獨立 tokenize，故 `t+s` 不會被宣告過的多碼位符號
   `ts` 吞掉。漏寫 `+` 不只是丟失環境，可能直接切錯音段。
2. **`+` 不是音節界**。音節化跑一次完整 pword，`.` 才是音節界。
3. **`+` 會出現在表層**。`surface_phrase` 把仍存在的 stem seam 輸出回 `+`（比照詞界輸出為
   空格）。故 surface 字串帶接縫標記，消費端須知它是**形態註記，不是音段**。

**規範：接縫是語言學宣告，引擎不推。** `substitute` 照 template 字面展開，不會替 slot
接縫自動補 `+`。同一個構式的相鄰槽，哪一個接縫是獨立詞幹域、哪一個是黏合的，
必須由構式作者判斷並寫進 template：

```lang
/auf+sagt/     /* 對：可分前綴是獨立詞幹域，屈折 -t 黏合 */
/auf+sag+t/    /* 錯：把屈折後綴當成獨立詞幹域 */
/aufsagt/      /* 錯：前綴的域邊界對音韻隱形，且 tokenize 可能切錯 */
```

英語的 level-1（`-ity`，觸發重音移動）與 level-2（`-ness`，不觸發）是同一個判斷。

**已知落差**：隨附的 `crates/language/tests/fixtures/german_present.lang` 寫的是
`/{prefix}{stem}{suffix}/`，未標接縫，故 `auf-` 的域邊界目前對音韻隱形。
這是庫內容的欠帳，不是引擎能力的欠帳。

**`+` 的第二個作用（別漏）**：它同時決定 **`stage: stem` 的規則跑在哪些片段上**。
`stage_domains`（`tshiatun/crates/dsl/src/exec.rs`）把 `Stage::Stem` 映到逐 stem bracket、
`Stage::Word` 映到逐韻律詞、`Stage::Phrase` 映到整句——**這已經是層級音韻，且一趟跑完**。
所以漏寫 `+` 不只是少一個可引用的環境，是**整個 stem 層級失效**（全詞被當成一個域）。
多層嵌套循環的上限與相關提案見《架構修補13》。

### 4.2 觀察：deep template 是 realization 的無條件分支（未裁定）

**現況是一件事被拆成兩個 `SignItem`**：

```lang
phon:
    /she/                       ← SignItem::Def(path = "phon")
    realization:                ← SignItem::Realization
        case:
            $self.syn.case == accusative:
                /her/
```

語意上 `/she/` 就是那個 case 的 `else`——`construction.rs` 選不到分支時回
`token.phon_form()`，也就是 deep template。三個佐證：

1. **default 已經是強制的**：construction 缺 phon template 直接報 `CONSTRUCTION_PHON_MISSING`。
   不是「可有可無的預設值」，是「函數必須有值」。
2. **兩個 item 之間沒有順序語意**：printer 把 `realization:` 印在 `/she/` **之前**，
   而原始碼順序相反；誰先誰後不影響求值。
3. **拆分本身製造了非法狀態**：realization 是獨立 item，所以它可以有 **0 個**
   （由 `REALIZATION_EMPTY` 攔）也可以有 **2 個**（曾使 P21 round-trip 破，
   2026-08-18 起於 parser 攔下）。若 phon 形只有一個 item，這兩種狀態在型別上不存在。

**可能的形狀**（提案，未裁定）：一個 sign 的 phon 形是**一個** form function，
`條件 → 模板`，且必含一個無條件分支。無交替的詞就是常數函數，`phon: /dog/`
保留為表層糖、既有 `.lang` 一字不改。收益是上述三種非法狀態由型別消滅，
而不是靠逐條 parser/check 檢查。

**與《架構修補13》的關係**：獨立。13 談的是跨界回傳，這裡談的是同一側的 item 切分。

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
