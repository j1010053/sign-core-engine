# `Def` 路徑封閉清單與 `feature:` 分工（P71）

> **狀態:已裁定**(擁有者 2026-08-01),**Phase 1 實作完成**(2026-08-02)。
> 增修 A(§7,寫入通道)、B(§8,`feature[…]` selector)、C(§9,`prag` 支援
> `feature:`)均已裁定並落地;§3 的量測數字已由 A4 重新量測取代,見 §7.5。
> 承 P30(資料層永不含邏輯,只存名字引用)、P48/P69(body 形狀)、
> `case_when與context_fragment_v2.md`、`共時lang_FP_expression_typed_case_constraint_v2.md`。
> 本檔記錄裁定、量測與切法,供實作直接執行。

---

## 1. 裁定

> **外部的裸宣告必須是被規定的;自行宣告的欄位只能放在 `feature:`。**

拆成兩條可檢查的規則:

| # | 規則 |
|---|---|
| **R1** | `Def` 的路徑必須落在一份**封閉清單**內。清單由引擎與套件規格共同規定,`.lang` 作者不得擴充。 |
| **R2** | 作者自己要的屬性一律寫在 `<dim>: feature:` 下——先宣告(含值域),再賦值。 |

**Phase 2 = M4 完成之後**:套件如何**自行宣告**多段座標路徑(見 §3 ③),延到那時再設計。
Phase 1 期間座標清單硬編在引擎側。

---

## 2. 現況:`valid_dim` 是一個開放的逃生口

`crates/language/src/system.rs` 的 `Def` 驗證分兩路,**任一成立即放行**:

```rust
let valid_meta = sign_metadata && match def.path.as_str() {
    "entrenchment" | "lexicalized" | "origin" | "components"
    | "provenance" | "lifecycle" | "source_package" => …逐項驗值…,
    _ => false,
};
let valid_dim = path_dimension(&def.path).is_some()          // ← 開放
    && (def.path == "phon" || 有非空的 <dim>.<field> 尾段)
    && parse_path(&def.path).is_ok();
if !valid_meta && !valid_dim { …DEF_INVALID_PATH_OR_VALUE… }
```

`valid_meta` **已經是 R1 想要的形狀**(封閉清單 + 逐項驗值)。
`valid_dim` 則只檢查「路徑長得像 `<dim>.<field>`」——**欄位名不查、值不查**。

### 2.1 後果(皆為實測)

- `reanalyze{target: category}` 從這個口寫 `syn.category`,而**沒有任何語意層讀它**;
  範疇實際上住 `belongs`。語法化最核心的動作曾是空操作(已由 P69 修正,見
  `function分支語意與選擇層_v1.0.md`)。
- guard 讀得到裸 `<dim>.<field>` def,但**欄位名打錯是靜默 `false`**
  (`$self.syn.bogus == x` → `Ok(false)`;只有維度名錯才報 `Err`)。作者得不到訊號。
- 對照 `feature:`:未宣告報 `FEATURE_UNDECLARED`,值超出值域報
  `FEATURE_VALUE_OUT_OF_DOMAIN`。**同樣 guard 讀得到,但多了兩道檢查**——
  故凡 guard 要讀的東西,`feature:` 嚴格優於裸 def。

---

## 3. 量測:關掉 `valid_dim` 會動到什麼

方法:把 `valid_dim` 限縮成只認 `phon` / `phon.realization` / `sem.roles`,
跑 `cargo test --workspace`,收集 `DEF_INVALID_PATH_OR_VALUE` 的路徑。

結果 **~55 條路徑**,分三類:

### ① 義項內容(併入 `senses`)

| 路徑 | 次數 | 來源 |
|---|---|---|
| `sem.gloss` | 6 | 測試 fixture |

**已裁定**:`gloss` 退出 Def,一律走 `sem: senses:`。`senses` 的 gloss
**維持開放字串**,結構 Phase 2 再固定。

### ② 作者自造的屬性(改走 `feature:`)

`syn.category`(306)、`prag.illocution`(306)、`prag.identifiability`(12)、
`syn.telic`(11)、`sem.kind`(7)、`sem.ref`(6)、`prag.perspective`(6)、
`syn.state`(3)、`sem.frame`(3)、`sem.content`(3)、`sem.actor`(3)、`sem.action`(3)、
`syn.level`(2)、`syn.choice`(2)、`sem.relation`(2)、`sem.event`(2)、
`syn.number`/`syn.inherited`/`syn.licensing.register`/`sem.referent`/`sem.profile`/
`sem.polarity`/`sem.exponent`/`sem.constant`/`prag.purpose`(各 1)

多數住測試 fixture,當通用填充物用(`sign x: syn: category = noun`)。
`syn.category` 那 306 次尤其如此——P69 之後範疇已改走 `belongs`,這些 fixture
只是需要「一點內容」。

### ③ std 套件的類型學座標(Phase 2)

各 **610 次**:`syn.voice.passive`、`syn.valency.causative`、`syn.tam.{present,past,future,perfectivity}`、
`syn.pronoun.clusivity`、`syn.possession.alienability`、`syn.numeral.classifier`、
`syn.evidential.direct`、`syn.argument.subject.omission`、`syn.adposition.{preposition,postposition}`、
`sem.time.{present,past,future}`、`sem.roles.beneficiary`、`sem.reference.{indefiniteness,identifiability,gender}`、
`sem.quantification.classification`、`sem.possession.alienability`、`sem.person.clusivity`、
`sem.number.plural`、`sem.event.serialization`、`sem.causation`、`sem.aspect.perfectivity`、
`prag.reference.subject`、`prag.evidence.direct`;另 `syn.typology.dataset`(305)。

來源 `crates/language/lib/std/grambank/code/syntax.lang`:

```lang
trait GB020_Absent:
    belongs GB020_DefiniteOrSpecificArticles
    syn:
        typology.grambank.GB020 = 0
        determination.definite-article = unavailable
```

**`feature:` 表達不了**:特徵是單段名字 + 宣告的 enum 值域;這些是多段路徑、
帶連字號、值含 `?`/`0`/`1`/`unavailable`。

值本身**其實是封閉的**(三態),缺的是**宣告機制**——25 個參數散在 trait 裡,
沒有一處說「這個座標系有哪些軸、每軸值域為何」。這正是 Phase 2 的題目。

---

## 4. Phase 1 實作切法

### 4.1 `gloss` 併入 `senses`

- `sem.rs` 的 `fields` 投影排除 `gloss`;`SemNode::field("gloss")` 改為**投影自
  `senses`**(唯讀)。選投影而非要求所有讀者改,是因為 DTO 的 `fields` 有 v1 相容
  承諾(`skip_serializing_if`),且順帶補上一個既有缺口:`drift` 改義項後,
  `field("gloss")` 與 guard 都看得到。
- fixture 的 `sem: gloss = X` → `sem: senses: core = X`。
- **前車之鑑**:`sem.gloss` **不是**無人讀取的遺留。曾誤以為可直接刪除,實測
  6 條測試轉紅(`construction_semantics::form_and_meaning_derived_together` 等),
  因為 `SemNode::field()` 只查 `features`/`fields`,**完全不看 `senses`**。

### 4.2 `valid_dim` → 封閉清單

比照 `valid_meta` 改寫:

```
DEF_PATH_ALLOWLIST =
    引擎自有: "phon"(音韻形式)、"phon.realization"、"sem.roles"(內部 context 標籤)
  ∪ 套件座標: §3 ③ 的前綴(Phase 1 硬編;Phase 2 改為套件可宣告)
```

不在清單上的 `<dim>.<field>` → 錯誤,**訊息必須指向 `feature:`**
(「自造欄位請寫在 `<dim>: feature:` 下並宣告值域」),否則作者只會看到一句
「invalid Definition」而不知道正解。

### 4.3 fixture 遷移

§3 ② 全部改成宣告過的 feature。`syn: category = noun` 這類純填充物,
建議統一換成一個 std 已宣告的特徵(如 `syn: feature: number = singular`),
避免每個 fixture 各自發明。

### 4.4 順序與驗證

1. 4.1(gloss)→ 全綠
2. 4.3(fixture 遷移)→ 全綠
3. 4.2(關閉逃生口)→ 全綠

先關逃生口會讓 §3 ② 的 fixture 一次全紅,難以逐條確認遷移是否等價。

**突變測試**至少涵蓋:清單漏掉某條引擎自有路徑(應紅)、未宣告的 feature 賦值
(應報 `FEATURE_UNDECLARED` 而非放行)、值超出值域(應報
`FEATURE_VALUE_OUT_OF_DOMAIN`)。

---

## 5. P 系列決策

| # | 內容 |
|---|---|
| **P71** | **`Def` 路徑封閉、自造欄位一律走 `feature:`**。①`Def` 的合法路徑限於一份封閉清單(引擎自有 + 套件座標),`.lang` 作者不得擴充;現行 `valid_dim` 的「任意 `<dim>.<field>` 放行」為逃生口,已實測導致 `reanalyze` 寫入無人讀取的欄位、guard 欄位名打錯靜默 `false`。②作者自造的屬性只能寫在 `<dim>: feature:` 下,先宣告值域再賦值——`feature:` 已有 `FEATURE_UNDECLARED` 與 `FEATURE_VALUE_OUT_OF_DOMAIN` 兩道檢查,而裸 def 一道也沒有。③`sem.gloss` 併入 `sem: senses:`,`senses` 的 gloss **Phase 1 維持開放字串**。④**Phase 2(M4 完成後)**:設計套件自行宣告多段座標路徑的機制(std:grambank 的 25 個類型學參數為首要對象),之後座標清單由硬編改為宣告驅動 |

> **§7 增修 A(P71-A)已裁定**(2026-08-01):①②的「封閉清單」**同時約束
> synchronic rule 目標路徑**;`gloss` 非法為規則目標(A2)。細節見 §7。

---

## 6. 逐文件修補清單

- `CLAUDE.md` §0.1 決策登記:P71 權威指向本檔 §5;文件表補一列。
- `crates/language/src/system.rs`:`valid_dim` → 封閉清單;新增指向 `feature:` 的診斷訊息。
- `crates/language/src/sem.rs`:`fields` 排除 `gloss`;`field()` 投影 `senses`。
- `specifications/case_when與context_fragment_v2.md`:guard 讀得到的東西以
  `feature:` 為正解,加註指向本檔。
- **(§7 A1)** `crates/language/src/synchronic.rs`:`rule_target_violations` 驗
  普通規則的目標路徑;新診斷碼 `RULE_TARGET_NOT_ALLOWED`(`FeatureRule` 不走此路,
  其出口是 `feature:` 既有的兩道檢查)。
- **(§8 B1)** `crates/changeset/src/lib.rs`:`feature[<dim>.<name>]` selector +
  `NodeUpdate::FeatureAssignment`(只改值,才進得了 `.chg` 的 `field = value` 語法)。
- **(§9 C1)** `crates/language/src/parser.rs` 與 `system.rs`:`feature:` 開放至
  `prag`;`FEATURE_DIMENSION_UNSUPPORTED` 適用範圍縮為 `phon`。
- **實作過程中發現並修掉的既有缺陷**(非 P71 引入,但被 P71 的遷移揭露):
  `Def` 是少數**不帶 `SourceLocation`** 的項目型別,而
  `FeatureDecl`/`FeatureValue`/`Sense`/… 都帶且參與 `PartialEq`。
  `diff_vector` 與 3-way merge 直接比整個項目,於是**別處插刪造成的行號位移**
  會讓內容未變的 sign 被判成改過(diff 多算維度分量、merge 無中生有 Content 衝突)。
  已加 `SignItem::without_source_location` / `SignDef::content_eq` /
  `TraitDef::content_eq` 並接到兩處比較點。同理,`IdentityIndex::node` 原以嚴格
  相等比 `NodeKind`,使 `FeatureRule` 印成 `rule` 後讀不回自己,已改為
  `Rule`/`FeatureRule` 可互換(與 `kind_keyword`/`parse_kind` 既有的共用約定一致)。

### 6.1 出口證據

`crates/language/tests/p71_closed_list.rs`(9 案)——封閉清單的正反例、訊息指向
`feature:`、三維皆關、`gloss` 兩種寫法皆拒、規則目標與分支目標、`feature:` 兩道
檢查;每條否定斷言均配正向控制組。`crates/changeset/tests/feature_selector.rs`
(4 案)——`feature[…]` 定位/改值/維度限定/與 `def[…]` 互不冒認。

§7.4 的突變測試已實跑:漏掉引擎自有路徑 `phon` → 318 紅;封閉清單一律放行 →
5 紅;規則目標不檢查 → 3 紅。

---

## 7. 增修 A(P71-A):寫入通道不只 `Def`

> **狀態:已裁定**(擁有者 2026-08-01,A1–A4 照草稿通過,**A2 採用**)。
> 本節不改 §1 的裁定,只補上 §3 量測方法看不見的兩條寫入通道。

### 7.1 為何 §3 的量測會漏

§3 的方法是「收集 `DEF_INVALID_PATH_OR_VALUE` 的路徑」。該診斷只在
**來源 `Def`** 上觸發,因此以下兩條同樣寫進 `<dim>.<field>` 路徑空間的通道
**在方法上不可見**,不是清點疏漏:

| 通道 | 實例 | 現行驗證 |
|---|---|---|
| **① synchronic rule 目標**(維度區塊下的 `field => value`) | `sem: gloss => HOUND` | **無**(實測) |
| **② `sem.senses[key].*` Def 路徑** | `sem: senses[core].gloss = dog` | 走 `valid_dim`,§3 未列 |

**通道 ① 的證據**(`crates/language/src/synchronic.rs:811`):

```rust
let path = format!("{}.{}", dim.keyword(), parsed.field);
let changed = sign.project(dim, registry).get(&path) != Some(value.as_str());
… Some(Patch::for_dim(dim).set(&parsed.field, &value))
```

規則目標即 `Patch` 的 Def 路徑,`parsed.field` 是作者原文;
`synchronic::validate_rule` 只驗 guard 與 slot 引用,**不驗目標路徑**。

**實測對照(四例,含正向控制組)**:

| 寫法 | 結果 |
|---|---|
| `sem: totally_invented => wat` | **靜默接受** ← 逃生口 |
| `sem: feature: undeclared_feat => wat` | `FEATURE_RULE_UNDECLARED` |
| `sem: feature:` 值超出 `enum(sg, pl)` | `FEATURE_RULE_VALUE_OUT_OF_DOMAIN` |
| `sem: feature:` 值在值域內(控制組) | 乾淨通過 |

即:**R2 的 `feature:` 出口在規則側已經驗得很完整**(兩個既有診斷碼),
唯獨裸 Def 規則目標一道也沒有。這與 §2 對 `Def` 的診斷是同一個形狀的洞。

### 7.2 若不補,§4.2 會留下側門

§4.2 只約束 `Def`。照原樣落地後,作者仍可用規則繞過封閉清單:

```lang
syn:
    category => noun      /* 規則目標,不受清單約束 → 照樣寫入 syn.category */
```

前門關上、側門敞開,且側門寫入的正是 P69 才剛修掉的 `syn.category`。

### 7.3 增修條款

| # | 條款 |
|---|---|
| **A1** | 封閉清單**同時約束 `Def` 路徑與 synchronic rule 目標路徑**。於 `synchronic::validate_rule` 增驗目標路徑,新診斷碼 `RULE_TARGET_NOT_ALLOWED`,訊息與 §4.2 同樣**必須指向 `feature:`**。 |
| **A2** | `gloss` 不在清單上,故 **`gloss => X` 為非法規則目標**。義項內容住 `senses:`;共時規則作用於**已宣告的 feature**;改義項是**歷時**操作(`drift`)。 |
| **A3** | `sem.senses[key].*` 亦不在清單上,一併併入 `senses:` 區塊(§4.1 同一條):`senses[core].gloss` → 義項節點;`senses[core].concept` 等內容 → `feature:`。 |
| **A4** | §3 ② 的清單須**重新量測以涵蓋規則目標**;fixture 遷移(§4.3)的範圍相應擴大到規則目標。 |

**A2 的理由**:m1pp fixture 的 `gloss => HOUND` 是為了取得一個可觀測的 sem 變化,
用以斷言「filler 規則先於遞迴語意組合」。該排序斷言與 gloss 無關,改以**已宣告的
sem feature** 為目標即可原樣保住;共時規則改寫詞彙義項本身在語言學上亦不成立。

**若不採 A2**(即允許規則寫義項),則需為 `Patch` 增設寫入 `SignItem::Sense` 的 op
——歷時側已有對稱物 `NodeUpdate::SenseGloss`,故此路可行,但超出 §4.1「純投影」的
切法,應另立條款而非默認。

### 7.5 A4 重新量測(取代 §3 的數字)

§3 的數字是**執行次數**(同一 fixture 被多條測試重複解析),不是原始碼站點數,
故高估甚多。以原始碼站點重新量測(`.lang` 檔 + Rust 內嵌字串,計入巢狀
`feature:` / `roles:` / `senses:` / `slots:` 區塊,避免把子區塊內容誤算為裸 Def):

| 面向 | 原始碼站點 | 備註 |
|---|---|---|
| 裸 `Def`(測試) | **~133 次 / ~45 條路徑** | `syn.category` **50 次**(非 §3 的 306) |
| 非 phon 規則目標(測試) | **~30 次 / ~22 條路徑** | §3 完全未計 |
| 套件(std + natural) | 全為**多段座標** + 4 條單段(`prag.clause-type`/`prag.identifiability`/`prag.illocution`/`prag.perspective`)+ `sem.causation` | **裸規則目標 0 條** |

**兩項與 §3 不符的結論**:

1. §3 ② 把 `prag.illocution`、`sem.actor`、`sem.referent` 等列為「作者自造」,
   但它們**同時是套件內容**(cxg `schema.lang` / natural `grammar.lang`)。
   ②③ 無法以「單段 vs 多段」區分。
2. 套件側**已經是 R2 相容的**——座標走多段路徑,其餘一律走 `feature:` / `roles:`
   區塊,沒有任何裸規則目標。故關閉逃生口**不會動到 std 內容**,
   亦不產生 library lock digest churn。

### 7.4 修訂後的順序(取代 §4.4)

1. §4.1(gloss + `senses[key]` 併入 `senses:`)→ 全綠
2. §4.3(fixture 遷移:**Def 與規則目標兩者**)→ 全綠
3. §4.2(關閉 `Def` 逃生口)→ 全綠
4. **A1**(關閉規則目標逃生口)→ 全綠

3 與 4 分兩次切,理由同 §4.4:一次全紅無法逐條確認等價。

**突變測試**在 §4.4 三項之外增加:規則目標寫不在清單上的路徑(應報
`RULE_TARGET_NOT_ALLOWED` 而非靜默接受)、規則目標寫已宣告 feature(應通過
——正向控制組,防止「因為路徑不存在而過」的假綠燈)。

---

## 8. 增修 B(P71-B):`feature[…]` 具名 selector

> **狀態:已裁定**(擁有者 2026-08-01)。

### 8.1 問題

R2 要求自造欄位一律移入 `feature:`。但 `feature:` 下的賦值在 AST 是
`SignItem::FeatureValue`,**不是 `Def`**,而 `.chg` 的具名 selector 只有
`def[path]` / `slot[name]` / `sense[name]` / `role[name]` / `belongs[name]` /
`trait_use[name]` / `case` / `branch` / `when` / `then` / `else` / `leaf` / `edge`
——**沒有 `feature[…]`**。

實測後果:遷移 §3 ② 之後,`.chg` 將無法以人手可寫的形式定址任何作者欄位。
量測顯示 `def[syn.category]` 是全 `.chg` 面**最常被定址的 Def 路徑**(29 處)。

`FeatureValue` 節點本身是健全的——有 `NodeKind::FeatureValue`、有
`EditableField::FeatureValue`、進 diff 與 reconstruct;缺的**只有具名入口**。
唯一替代是 `node(feature_value, @id)` 寫死穩定 id,而
`crates/changeset/src/lib.rs` 對 `belongs[…]` 的註解已載明該形式
「機器排出來的形式,人手寫不出,rebase 後也讀不出意圖」。

### 8.2 裁定

| # | 條款 |
|---|---|
| **B1** | 新增具名 selector **`feature[<dim>.<name>]`**,定位 `SignItem::FeatureValue`。 |
| **B2** | 鍵採**維度限定**形式(非裸 name):`feature:` 下同名特徵可分屬不同維度,裸 name 有歧義;且與 `def[syn.category]` 同形,使 §3 ② 的遷移在 `.chg` 側僅是 `def` → `feature` 的字面替換。 |
| **B3** | 特徵**宣告**節點(`FeatureDeclaration`,可改 `FeatureDomain`)本次不開具名入口——現無消費者。待有需求再比照 B1 補 `feature_decl[…]`,不預先造無消費者語法。 |

---

## 9. 增修 C(P71-C):`prag:` 亦支援 `feature:`

> **狀態:已裁定**(擁有者 2026-08-01)。

### 9.1 問題

R2 要求自造欄位一律寫在 `<dim>: feature:` 下,但
`sem_roles_self_phon_realization_v1.md` §1 明訂
**「`feature:` 只可出現在 `syn:` 或 `sem:`」**,並由 parser 與
`FEATURE_DIMENSION_UNSUPPORTED` 兩處強制。

於是 **prag 維的 R2 沒有目的地**:一旦 R1 對 prag 生效,作者的 prag 內容
(如 fixture 的 `prag: register => formal`)將**沒有任何合法寫法**;
std 的 prag 內容之所以倖存,只因它們是被允許的套件座標。

### 9.2 裁定

| # | 條款 |
|---|---|
| **C1** | `feature:` 的合法維度擴充為 **`syn` / `sem` / `prag`**。`sem_roles_self_phon_realization_v1.md` §1 同步修訂。 |
| **C2** | **`phon` 維持不支援**:其 Def 是 UR/模板(`phon` / `phon.realization`,本就在封閉清單上),規則屬 DSL 音變語言,不是 enum 值域欄位——R2 在 phon 沒有缺口。 |
| **C3** | `FEATURE_DIMENSION_UNSUPPORTED` 保留,僅適用範圍縮為 `phon`。 |

C1 使 R2 **完備**:每個作者可寫的維度都有一條「先宣告值域、再賦值/寫規則」的
通道,`FEATURE_UNDECLARED` 與 `FEATURE_VALUE_OUT_OF_DOMAIN` 兩道檢查隨之覆蓋
prag——這正是 §2.1 指出裸 Def 所缺的兩道。
