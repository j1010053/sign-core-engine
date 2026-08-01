# `Def` 路徑封閉清單與 `feature:` 分工（P71）

> **狀態:已裁定**(擁有者 2026-08-01),**實作未開始**。
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

---

## 6. 逐文件修補清單

- `CLAUDE.md` §0.1 決策登記:P71 權威指向本檔 §5;文件表補一列。
- `crates/language/src/system.rs`:`valid_dim` → 封閉清單;新增指向 `feature:` 的診斷訊息。
- `crates/language/src/sem.rs`:`fields` 排除 `gloss`;`field()` 投影 `senses`。
- `specifications/case_when與context_fragment_v2.md`:guard 讀得到的東西以
  `feature:` 為正解,加註指向本檔。
