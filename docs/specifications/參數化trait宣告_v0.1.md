# 參數化 trait 宣告(P76 提案)

> **狀態:提案,未裁定。**
> 承 P5(trait = macro 模板,全 block 或全不用)、P22(三道界:無量詞、無計算、無副作用)、
> P41(slot 宣告)、P71(封閉清單與 feature 分工)。
> 緣起:`package/盤點_lang-chg-data可表達的資料結構_v0.1.md` §8.2。

---

## 1. 一句話

> **讓 trait 宣告帶型別參數,消費端實例化時填入具體範疇名。**

```lang
trait Agreement<C, T>:
    syn:
        slots:
            controller [C]
            target [T]

sign SubjectVerb:
    belongs Agreement<Nominal, Predicate>
    /* 展開後等價於:
       syn:
           slots:
               controller [Nominal]
               target [Predicate]
    */
```

---

## 2. 為什麼需要

### 2.1 現有不對稱

`.chg` function **已經有具型別的參數**:

```chg
function VerbToTense(verb [Verb], tense):
    drift(verb, sense: core, gloss: tense)
    reanalyze(verb, target: category, to: Aux)
    entrench(verb, delta: 0.3)
```

`FunctionParam { name, constraint: Option<String> }`(`function.rs:276`)。
消費端 `VerbToTense(sign("go"), "future")` 時,引擎檢查 `go` 是否 `[Verb]`。

**共時側的 trait 沒有對應物。** `TraitDef { name, global, blocks }` 是封閉的;
slot/role 的 constraint 寫死具體範疇名,沒有參數可換。

這個不對稱的後果見 §2.2。

### 2.2 窮人版泛型(現狀)

`specifications/syn_feature與單一DeepConstruction_Sign規範_v0.1.md` §7.2
面對「std:cxg 要不要假設所有語言的 `number` 同一個 domain」時,
列出的可選策略只有兩條:

1. std 宣告跨語言穩定的 feature;
2. **std 只提供 slot schema,讓 natural package 自己宣告 domain 並補上具體 agreement rule。**

語言有二數/三數/paucal,`number` 採策略 2。這就是**泛型的窮人版**:
通用套件出形狀,規則丟給消費端,因為沒有辦法寫「controller 填什麼型別由你決定」。

有了型別參數:

```lang
/* std:cxg 提供 */
trait Agreement<C, T>:
    syn:
        slots:
            controller [C]
            target [T]

/* natural:xx 實例化 */
sign SubjectVerbAgreement:
    belongs Agreement<Nominal, Predicate>
    syn:
        feature:
            number = enum(sg, pl)
            person = enum(1, 2, 3)
```

§7.2 的策略 2 升級成**策略 1 的泛型版**:通用套件把形狀**與**欄位型別一起給出,
消費端只需填入自己語言的分類節點;值域與規則仍由消費端宣告。

### 2.3 實數:可參數化的部分有多大

`package/盤點_lang-chg-data可表達的資料結構_v0.1.md` 附錄 A:

| | 數量 |
|---|---|
| slot + role 宣告總數 | 72 |
| constraint = `[*]`(不限型別) | 11 (15%) |
| constraint = 具名範疇 | 61 (85%) |

那 61 個具名 constraint,泛型能把它們從「寫死的範疇」變成「參數」。
不用泛型時它們硬綁在一套分類體系;用了之後同一份構式 schema
能被**不同理論或不同語言的分類體系**實例化。

---

## 3. 語法

### 3.1 trait 宣告:加角括號

```lang
trait Name<P1, P2, ...>:
    /* body 裡的 slot/role constraint 可引用 P1, P2 */
```

- 參數名的詞法:大寫開頭的 identifier(與現有範疇名同域)。
- 參數數量:≥ 1。無上限,但實務上 1–3 個。
- 同名參數:parse error。

### 3.2 帶 bound 的參數

```lang
trait Agreement<C: Nominal, T: Predicate>:
    syn:
        slots:
            controller [C]
            target [T]
```

bound 意義:實例化時填入的範疇必須是 bound 的**子範疇**
(= `category_is_a(填入, bound)` 為真;用的是同一個 `ontology.rs:156` 的閉包判定)。

**Phase 1 限制**:bound 只接受**單一範疇名**,與現行 `SlotConstraint::Category(String)` 一致。
不支援交集(`C: A & B`)、聯集(`C: A | B`)或否定(`C: !A`)。
理由與日後路徑見 §6。

無 bound 的參數等同 bound = `[*]`(`AnySign`)。

### 3.3 trait body 中引用參數

型別參數**只能出現在 `SlotConstraint` 位置**——即 slot/role 的方括號內:

```lang
trait X<T>:
    syn:
        slots:
            subject [T]          /* OK:constraint 位置 */
            object [T]           /* OK:同一參數可用多次 */
            modifier [Adjective] /* OK:仍可混用具體範疇 */
```

不可出現的位置:
- `belongs` 的目標(分類邊不參數化——否則 ontology 樹形會在 compile 前無法確定);
- `feature:` 的 `name` 或 `values`(值域參數化是 §8 未來工作);
- `Def` 的 `path` 或 `value`。

嘗試在非法位置使用型別參數 → parse error,
訊息:「type parameter `T` can only appear in slot/role constraints」。

### 3.4 實例化:在 `belongs` 或 `trait_use` 處

**在 sign 中**:必須全填。

```lang
sign X:
    belongs Agreement<Nominal, Predicate>

sign Y:
    Agreement<Noun, Verb>    /* trait_use 形式 */
```

- 參數數量必須精確匹配宣告;多或少 → compile error。
- 每個實參必須是已知範疇名(含套件匯入的);未知 → `ONTOLOGY_UNKNOWN_TRAIT`。
- 帶 bound 時:實參必須是 bound 的子範疇;否則 →
  `TYPE_PARAM_BOUND_UNSATISFIED` (compile error,新增)。

**在 trait 中**:可以部分填,未填的參數向外傳播(§3.6)。

### 3.5 無參數 trait:零改動

```lang
trait Noun:       /* 照舊,完全向後相容 */
```

無角括號 = 無型別參數 = 今天的行為。**現有所有 `.lang` 零修改、零 churn。**

### 3.6 參數傳播:trait 可以部分填入

一個 trait 在 `belongs`/`trait_use` 另一個帶參數的 trait 時,可以只填部分參數;
未填的參數成為**自己的型別參數**,由更外層的消費端填入。
這與 sign 的 FP 模型中 slot 的 `residual` 傳播(§3 of `共時lang_FP_expression_typed_case_constraint_v2.md`)
是同一個設計:未填的東西往上傳,直到某一層全部填完。

```lang
trait Agreement<C, T>:
    syn:
        slots:
            controller [C]
            target [T]

/* C 固定,T 傳播:SubjectAgreement 自己帶一個型別參數 T */
trait SubjectAgreement<T>:
    belongs Agreement<Nominal, T>

/* 消費端全填 */
sign EnglishSVA:
    belongs SubjectAgreement<Predicate>
    /* 展開鏈:
       SubjectAgreement<Predicate>
       → Agreement<Nominal, Predicate>
       → controller [Nominal], target [Predicate] */
```

規則:

| 場景 | 行為 |
|---|---|
| **trait 中部分填** | 合法;未填的參數按位置成為外層 trait 的型別參數 |
| **trait 中全填** | 合法;外層 trait 不需要型別參數(除非自己另有) |
| **sign 中部分填** | **compile error**;sign 是終端,不能再傳播 |
| **sign 中全填** | 合法 |

**限制:只有 trait 可以傳遞參數,sign 不行。**
理由:sign 是實例(ontology 的葉節點),而 ontology 邊必須在 sign 層全部解析完畢(§4.1)。
如果 sign 也能帶自由參數,填入時機只能是 runtime(slot filling),
但型別參數是靜態的分類約束,不是 runtime 值——兩者語意不同。

### 3.7 傳播的位置語法

未填的參數**按位置**映射到外層 trait 的參數列表:

```lang
trait Inner<A, B, C>:
    syn:
        slots:
            x [A]
            y [B]
            z [C]

/* 填 A,傳播 B 和 C */
trait Outer<P, Q>:
    belongs Inner<Nominal, P, Q>
    /* P 對應 B, Q 對應 C */
```

外層 trait 的參數數量 = 內層未填的參數數量(加上外層自己額外宣告的)。
名字可以不同(`P` vs `B`);映射按位置,不按名字。

也可以用**具名語法**讓映射更清楚(待定,見 §10):

```lang
trait Outer<P, Q>:
    belongs Inner<A = Nominal, B = P, C = Q>
```

---

## 4. 語意:展開即替換

泛型不引入新的運行時概念。語意完全歸約到**compile 期的文字替換**:

> **規則 G1:**
> `belongs T<A₁, A₂, …>` 展開為:先把 T 的 body 中所有型別參數 Pᵢ
> 替換為對應的實參 Aᵢ,再執行現行的 `belongs` 語意
> (建成員邊 + 繼承內容,`inheritance_order`)。

> **規則 G2:**
> `trait_use T<A₁, A₂, …>` 同上,但走 `expand_item_sequence`(inline,不建成員邊)。

替換只觸及 `SlotConstraint` 位置。替換**在 compile 展開之前**發生,
不影響 `inheritance_order` / `sign_categories` / `categories_satisfy` 的任何邏輯。

**這正是 P5「trait = macro 模板」的直接延伸**——
macro 展開今天已經是文字級別的(`compile.rs:125` 的 `expand_item_sequence`);
型別參數只是讓 macro 可以帶引數。見 §5 的 P5 界線確認。

### 4.1 在 ontology 中的身分(方案 B:lazy)

**裁定:ontology 邊只在 sign 實例化全部參數後才建立。**

帶自由型別參數的中間 trait(部分填入)**不產生 ontology 邊**;
只有 sign(或全填的 trait)展開時,整條 belongs 鏈才一次解析完畢,
最終的 ontology 邊指向**原始 trait 名**。

```
trait Agreement<C, T>           ← 模板,不建邊
trait SubjectAgreement<T>
    belongs Agreement<Nominal, T>   ← 部分填,T 仍自由,不建邊
sign EnglishSVA
    belongs SubjectAgreement<Predicate>
    → 展開鏈:SubjectAgreement<Predicate> → Agreement<Nominal, Predicate>
    → ontology 邊:EnglishSVA → Agreement(原始名), EnglishSVA → SubjectAgreement(原始名)
```

規則:

- `sign_categories(EnglishSVA)` 含 `Agreement` 和 `SubjectAgreement`。
  `[Agreement]` 的 slot constraint 對所有實例化都成立。
- 兩個 sign 各 `belongs Agreement<X, Y>` 和 `belongs Agreement<A, B>`,
  它們都屬於 `Agreement`,但 **slot constraint 不同**(因為替換的結果不同)。
- 帶自由參數的 trait(`SubjectAgreement<T>`)在 ontology 中**沒有獨立節點**,
  直到某個 sign 填滿它的所有參數時才間接建邊。

**為何選 lazy(方案 B)**:

| 方案 | 行為 | 問題 |
|---|---|---|
| A(eager) | 每個 `belongs` 立刻建邊,自由參數造 placeholder 節點 | ontology 樹在 compile 前不可預測;`[SubjectAgreement]` 的 slot constraint 含自由參數,無法檢查 |
| **B(lazy)** | 邊只在全填時建 | 中間 trait 不可直接被 `[SubjectAgreement]` 的 slot constraint 引用,但實務上這些中間 trait 本就是抽象模板,不應作為 slot 約束的目標 |

方案 B 的代價(可接受):不能寫 `slot x [SubjectAgreement]` 來約束「任何填了 SubjectAgreement 的 sign」,
除非有某個 sign 確實 `belongs SubjectAgreement<SomeType>`——此時 ontology 中才有 `SubjectAgreement` 節點。
這是正確的:如果沒有任何 sign 實例化這個 trait,它就不應該是有效的分類。

### 4.2 傳播展開語意

當 trait 部分填入另一個 trait 的參數時,展開分兩階段:

**階段 1:trait 宣告(compile 期,不建 ontology 邊)**

```
trait SubjectAgreement<T>:
    belongs Agreement<Nominal, T>
```

此時 `Agreement<Nominal, T>` 的 body 被部分替換:
- `C` → `Nominal`(已填)
- `T` → 保持為自由參數

結果:`SubjectAgreement` 的展開內容含 `controller [Nominal]`, `target [T]`。
`T` 是自由的,所以 **ontology 不建邊**(方案 B)。

**階段 2:sign 實例化(compile 期,建 ontology 邊)**

```
sign EnglishSVA:
    belongs SubjectAgreement<Predicate>
```

展開器遞迴:
1. `SubjectAgreement<Predicate>` → `T = Predicate`,展開 SubjectAgreement 的 body
2. body 中有 `belongs Agreement<Nominal, T>` → `T` 已填為 `Predicate` → `Agreement<Nominal, Predicate>`
3. 展開 Agreement 的 body:`controller [Nominal]`, `target [Predicate]`
4. 此時所有參數已填滿 → 建 ontology 邊

最終 sign 的項目向量中含:`controller [Nominal]`, `target [Predicate]`。
ontology:`EnglishSVA` → `SubjectAgreement`, `EnglishSVA` → `Agreement`。

**與菱形的交互:**

當同一個 trait 經兩條路徑到達同一個 sign 時(菱形繼承),去重規則是
**同一次宣告 vs 兩次引用**:

| 情形 | 展開路徑 | 行為 |
|---|---|---|
| 菱形(D→B,C→A,sign 引用 D 這一次中 A 被走到兩次) | 同一次頂層引用的子樹 | **去重**——同一宣告,走到兩次 |
| 並列引用(sign 寫了兩次 `belongs X`) | 兩次頂層引用 | **要求顯式處理**(rename 或顯式合併) |

實作:`expand_item_sequence` 遞迴時已帶 `active` 堆疊;在一次頂層引用的子樹內
記住 `(trait, block)` 已展開,即可去重。兩個並列的 `belongs`/`trait_use` 是
兩次頂層引用,不共用那份記錄。

### 4.3 最小例:從頭到尾

```lang
trait Schema<C>:
    syn:
        slots:
            head [C]

sign NominalProjection:
    belongs Schema<Noun>
    syn:
        feature:
            definiteness = enum(def, indef)

sign VerbalProjection:
    belongs Schema<Verb>
```

compile 之後:

- `NominalProjection` 有 slot `head [Noun]`,是 construction(P42,≥1 slot)。
  它的 `sign_categories` 含 `Schema`。
- `VerbalProjection` 有 slot `head [Verb]`,也是 construction。
  它的 `sign_categories` 也含 `Schema`。
- `[Schema]` 的 slot constraint 對兩者都成立。

---

## 5. 與 P 決議的關係

### 5.1 P5 界線確認:參數化宣告仍在「宣告式」內

P5 的原文:

> trait 是「sign 內容的 macro 展開模板」,**存宣告式資料、不存任意計算**。

型別參數是**宣告的一部分**,不是計算:

| 面向 | P5 的界線 | 型別參數 | 是否踩線 |
|---|---|---|---|
| 全 block 強制 | trait_use 要嘛整個 trait、要嘛所有 block 都寫齊 | 不改:展開仍是全 block | ❌ 不踩 |
| 不存計算 | body 裡不能有函式呼叫、迴圈、條件求值 | 替換是文字級別,不求值 | ❌ 不踩 |
| macro 展開 | compile 期 inline | 加了一步「先替換參數」再 inline | ❌ 不踩 |

**結論:P5 不需修訂。** 型別參數是 macro 帶引數,不是計算帶入 trait。

### 5.2 P22 三道界

P22:無量詞、無計算、無副作用,condition 必終止。

型別參數不引入:
- 量詞(不是「對所有 T」,是「用這個 T」);
- 計算(替換是靜態的);
- 副作用(ontology 邊建在原始 trait 名上,不新增節點)。

**結論:P22 不需修訂。**

### 5.3 P71 封閉清單

`Def` 路徑的封閉清單不受影響——型別參數不產生新的 `Def` 路徑。

### 5.4 P41/P42 slot 與 construction

slot 宣告的形狀不變:`Slot { name, constraint, optional }`。
`constraint` 從「寫死的名字」變成「可能是參數替換後的名字」,但替換發生在
compile 之前,compile 看到的仍然是 `SlotConstraint::Category(String)`。
**P41/P42 不需修訂。**

---

## 6. `.chg` 側的相容性

### 6.1 slot/role 定址:零影響

`.chg` 的 `slot[name]` selector 按**欄位名**定址,不按 constraint 型別定址。
泛型改的是 constraint(從寫死變成替換後的結果),不改欄位名。

```chg
/* 對 NominalProjection 的 head slot 操作——和今天一模一樣 */
statement 0:
    update sign("NominalProjection").slot["head"] ...
```

### 6.2 belongs selector:用原始 trait 名

§4.1 已決定 ontology 邊指向原始 trait 名。所以:

```chg
/* 改寫 NominalProjection 的 belongs */
statement 0:
    update sign("NominalProjection").belongs["Schema"]
```

注意是 `belongs["Schema"]`,不是 `belongs["Schema<Noun>"]`。

### 6.3 reanalyze:不受影響

`reanalyze(x, target: category, from: Verb, to: Aux)` 操作的是 `belongs` 邊的
**目標名稱**。泛型 trait 的 belongs 目標名稱就是原始 trait 名(`Schema`),
reanalyze 的 `from`/`to` 照用。

### 6.4 function 參數:已有的可對照

`.chg` function 的 `FunctionParam { name, constraint: Option<String> }` 與
本提案的 trait 型別參數 `TraitTypeParam { name, bound: Option<String> }` 是
**同一形狀的東西用在不同地方**:

| | `.chg` function param | `.lang` trait type param |
|---|---|---|
| 宣告 | `function F(x [Verb])` | `trait T<C: Verb>:` |
| 約束 | `Option<String>` = 單一範疇 | `Option<String>` = 單一範疇 |
| 檢查時機 | function 呼叫時 | `belongs`/`trait_use` 展開時 |
| 判定邏輯 | `category_is_a` | `category_is_a`(同一個) |

**這不是巧合,是刻意的對稱。** 歷時側已經有的東西,共時側補上。

### 6.5 rename 機制的管線順序

泛型 trait 的展開與 slot rename 機制(獨立於 P76)有管線依賴:

> **rename 必須在參數替換之後求值。**

理由:rename 的去重/衝突判定依賴 constraint 的具體值。
`controller [C]` 在替換前兩份看起來一模一樣,替換後才分得出
`controller [Nominal]` vs `controller [Accusative]`。

管線順序:

```
解析型別參數 → 遞迴替換(自由參數填入) → 展開項目向量 → rename map 求值 → 衝突檢查
```

rename map 的語法掛在**引用**上(不是 trait 宣告上):

```lang
sign BasqueTransitive:
    belongs SubjectAgreement<Predicate>
    belongs ObjectAgreement<Predicate>
    SubjectAgreement[0] { controller -> erg_controller }
    ObjectAgreement[0]  { controller -> abs_controller }
```

rename 之後,`.chg` 用**新名字**定址:`slot["erg_controller"]`,
不用 `slot["SubjectAgreement.controller"]`。這與今天 `SlotMapOp::Rename` 的行為一致:
改名等於改契約,但那是消費端自己顯式做的動作。

`rewrite_local_slot_refs_in_items` 已涵蓋全部文件內引用點,
rename 之後文件內部是自洽的。

---

## 7. 實作要點

### 7.1 資料結構

```rust
// crates/language/src/lib.rs

/// Trait 型別參數。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitTypeParam {
    pub name: String,
    /// 上界:實例化時填入的範疇必須是此範疇的子範疇。
    /// `None` = `[*]`(AnySign)。
    pub bound: Option<String>,
}

pub struct TraitDef {
    pub name: String,
    pub global: bool,
    pub type_params: Vec<TraitTypeParam>,  // 新增;空 = 非泛型
    pub blocks: Vec<Block>,
}
```

### 7.2 parser 改動

`parse_trait` 在讀到 trait name 之後、`:` 之前,嘗試讀 `<…>`:

```
trait Name<P1: Bound1, P2>:
         ^              ^
         |              |
         parse_type_params (新增)
```

角括號內是逗號分隔的 `name` 或 `name: bound` 對。

### 7.3 compile 改動

`expand_item_sequence`(`compile.rs:125`)需要擴充為**遞迴替換**:

1. 進入 `belongs T<A₁, …>` 或 `trait_use T<A₁, …>` 時,
   把 T 的 body 中所有型別參數 Pᵢ 替換為對應的實參 Aᵢ。
2. 如果 Aᵢ 本身是外層的自由參數(trait 傳播的情形),
   替換結果仍含自由參數名——此時**不建 ontology 邊**(方案 B)。
3. 遞迴展開內層的 `belongs`/`trait_use`;
   外層已填入的參數隨替換帶進內層。
4. 只有當所有參數都已解析為具體範疇名(無自由參數)時,才建 ontology 邊。

**管線順序**:參數替換 → 展開項目向量 → rename map 求值(§6.5)→ 衝突檢查。

菱形去重:在一次頂層引用的遞迴中,`active` 堆疊記住已展開的 `(trait, block)` 對,
同一對再遇到時跳過(去重)。兩個並列的 `belongs` 是兩次頂層引用,不共用 `active`。

### 7.4 驗證(新增 diagnostic)

| diagnostic | 層級 | 條件 |
|---|---|---|
| `TYPE_PARAM_ARITY_MISMATCH` | Error | belongs/trait_use 給的實參數 ≠ 宣告的參數數(扣除已傳播的自由參數) |
| `TYPE_PARAM_BOUND_UNSATISFIED` | Error | 實參不是 bound 的子範疇 |
| `TYPE_PARAM_DUPLICATE_NAME` | Error | 同一 trait 宣告中兩個參數同名 |
| `TYPE_PARAM_UNUSED` | Warning | 宣告了參數但 body 中沒有引用它(含傳播的內層引用) |
| `TYPE_PARAM_NOT_IN_CONSTRAINT` | Error | 在非 constraint 位置使用型別參數 |
| `TYPE_PARAM_SIGN_HAS_FREE` | Error | sign 的展開結果仍含自由型別參數(sign 必須全填) |
| `SLOT_NAME_COLLISION` | Error | 兩個並列引用展開後產生同名 slot,需要 rename map 顯式處理(此 diagnostic 屬 rename 機制,非 P76,但管線上依賴 P76 的替換結果) |

### 7.5 printer 改動

canonical printer 在 trait name 後印 `<P1: Bound1, P2>`。
無型別參數時不印(向後相容,零 churn)。

---

## 8. 日後擴充:bound 的布林組合

Phase 1 的 bound 只接受單一範疇:

```lang
trait X<C: Nominal>:    /* OK */
trait X<C: Nominal & Animate>:    /* Phase 1 不支援 */
```

**P22 不擋這條路。** P22 的三道界是:

1. 無量詞 → `&`/`|`/`!` 不是量詞,是布林運算子;
2. 無計算 → bound 是宣告時的靜態約束,不在 runtime 求值;
3. 無副作用 → 型別檢查不產生副作用。

如果日後有消費者(例如 `<C: Nominal & Animate>` 表示 controller 必須同時是
Nominal 和 Animate),需要的改動是:

1. `SlotConstraint` 從 `Category(String) | AnySign` 擴充為
   `Compound(Vec<(BoolOp, String)>) | AnySign`;
2. `category_is_a` 的呼叫處改為對 compound 的每一項取 `&`/`|`;
3. parser 的 `parse_slot` 認 `&`/`|`/`!`。

**這些改動只在 `SlotConstraint` 的定義和解讀處,不涉及 ontology、
inheritance、或 `.chg` 定址——所以可以獨立追加,不破壞 Phase 1。**

---

## 9. 不做什麼

| 項目 | 理由 |
|---|---|
| `belongs` 目標的參數化(`belongs T<P>` 裡的 T 也可以是參數) | 會讓 ontology 樹形在 compile 前無法確定 |
| 值域的參數化(`feature: name = enum(P)`) | 與 P71 Phase 2(命名值域)同源,另案 |
| 高階型別參數(`trait X<T<U>>:`) | 超出 P5 的「宣告式」界線 |
| 預設實參(`trait X<C = Nominal>:`) | 無消費者,不預造(P71-B3) |
| `global trait` 的泛型 | global trait 是 phon-rule macro,自動引用;帶型別參數時無法自動填實參 |
| **sign 層的部分填入**(sign 帶自由型別參數) | sign 是 ontology 葉節點,方案 B 要求所有參數在 sign 層全部解析;型別參數是靜態分類約束,不是 runtime 值 |
| **rename 機制**(slot name collision 的處理) | 不涉及泛型也會發生(菱形繼承);屬引用機制,P76 只保證展開產物是一般的項目向量 |

---

## 10. 待裁定與已裁定

### 已裁定

| # | 問題 | 裁定 |
|---|---|---|
| 1 | ontology 身分 | **方案 B(lazy)**:ontology 邊只在 sign 實例化全部參數後才建立。帶自由參數的中間 trait 不產生 ontology 邊(§4.1) |
| 2 | 參數傳播 | **只有 trait 可以部分填入**;sign 必須全填(§3.6)。自由參數按位置傳播到外層 trait |
| 3 | rename 歸屬 | **不屬於 P76**;菱形繼承不用泛型也會撞(§9)。管線上 rename 在參數替換之後(§6.5) |
| 4 | 兩階段語法下參數位置 | **寫在 `belongs` 上**。參數是分類身分,不是位置控制;與方案 B 相容(§10 待裁定 4) |

### 待裁定

1. **canonical form**:`belongs Agreement<Nominal, Predicate>` 是否應該在
   canonical printer 中保留型別實參?如果保留 → library lock digest 會因
   重構 trait 參數順序而 churn;如果不保留 → canonical form 丟失資訊。
   建議:**保留**,因為型別實參是語意的一部分(影響 slot constraint)。

2. **是否給 P76 編號**:本文件的裁定如果通過,需要一個 P 編號。
   P75 是目前最高;建議 P76。

3. **具名 vs 位置傳播**:§3.7 提了兩種寫法。位置語法簡單但
   重構時容易漏;具名語法清楚但囉嗦。建議先只做位置,
   具名作為語法糖日後按需追加。

4. ~~兩階段語法下型別參數的位置~~ → **已裁定:寫在 `belongs` 上。**
   `belongs Agreement<Nominal, T>` + `Agreement[0]` 帶內容。
   理由:參數是「屬於哪一種 Agreement」的身分,是分類層事實;
   `[n]` 是位置控制,不該承載身分。與方案 B(lazy ontology)相容:
   `belongs` 行帶著完整實參,建邊時資訊就在手上。
   代價:展開時要回查同一 trait 名的 `belongs` 參數,但查找是本地的(同一容器內)。
