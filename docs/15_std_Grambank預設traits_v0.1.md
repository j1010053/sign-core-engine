# std Grambank 預設 traits（v0.1）

## 1. 結論與資料版本

stdlib 已依架構修補06的 package 邊界拆成 `code/`、`data/`、`config/`，並新增
Grambank v1.0 的 **25 項功能驗證子集**。這一批同時包含適合記在整體 grammar
profile 的 inventory 參數，以及能由具體 construction 證成的行為參數；涵蓋名詞組、
代詞、格與語序、TAM、否定、複雜謂詞、語態／配價、疑問、證據性與論元省略。

版本固定在 [Grambank v1.0 Zenodo release](https://doi.org/10.5281/zenodo.7740140)
及其 [versioned CLDF source](https://github.com/grambank/grambank/tree/v1.0/cldf)；參數
問題與 coding procedure 以 [official feature browser](https://grambank.clld.org/parameters)
為準。資料與網站內容標示為 CC BY 4.0。本 repo 只保存 ID、問題名稱、值域與 engine
mapping，不重新散布語言觀測列或統計分布。

## 2. 套件結構

```text
crates/language/lib/std/
├─ core/
│  ├─ code/ontology.lang
│  ├─ data/categories.tsv
│  └─ config/{package.conf,exports.tsv}
└─ grambank/
   ├─ code/syntax.lang
   ├─ data/{features.tsv,README.md}
   └─ config/{package.conf,exports.tsv}
```

- `code/`：唯一可進 parser/compiler 的 trait、`belongs`、Slot、Rule 與四維 Def；
  是否採用由參數的語意決定，不表示每個 Grambank value 都必須帶執行內容。
- `data/`：問題名稱、值域、來源與 mapping 查表，不含執行邏輯。
- `config/`：package version、enabled、priority 與不隨小版更動的 stable export ID。
- `stdlib::load_default` 依 priority 決定性載入；`ontology::std_ontology` 是相容入口。

## 3. 值語意與 trait 慣例

每個二元參數有一個未知根 trait 與兩個明確 value trait：

```lang
trait GB107_BoundVerbalNegation:
    belongs GrambankSyntaxFeature
    syn:
        typology.grambank.GB107 = ?

trait GB107_Absent:
    belongs GB107_BoundVerbalNegation
    syn:
        typology.grambank.GB107 = 0
        negation.standard.bound-verb = unavailable

trait GB107_Present:
    belongs GB107_BoundVerbalNegation
    syn:
        typology.grambank.GB107 = 1
        negation.standard.bound-verb = available
```

這裡保留四個不同狀態：

| 狀態 | 表示 |
|---|---|
| 沒有任何 GB107 trait | 這個 Language/Sign 沒有陳述該參數 |
| `belongs GB107_BoundVerbalNegation` | Grambank code `?`，資料不足 |
| `belongs GB107_Absent` | 明確 code `0` |
| `belongs GB107_Present` | 明確 code `1` |

因此 missing 不會被誤當 false。若同時繼承 `_Present` 與 `_Absent`，沿用 M1++ 的
衝突契約：依 `belongs` 優先序決議，並產生帶 winner/shadowed provenance 的 warning。

## 4. 如何描述行為

Grambank value 先表示語言層的類型學觀察；可執行行為則由 `std/cxg` 的抽象 construction
schema 表示，具體 form 實現另存於 `std/cxg/code/realizations.lang` 或 natural package。
兩層以繼承共同掛到作為 coding evidence 的 construction，但不能把 profile 上的 value
trait 所帶內容無條件注入每個 construction。

這裡不新增 `copula`、`order`、`meaning` 等保留關鍵字。`Copula` 是 ontology category，
`copula [Copula]` 是既有通用 Slot 語法；form pole 仍由 `phon+syn`、meaning/function pole
仍由 `sem+prag` 表示。

推薦的語言層紀錄：

```lang
sign GrammarProfile:
    belongs GB107_Present
    belongs GB132_Present
    belongs GB322_Absent
```

若一個 construction 本身就是正值的 coding evidence，可同時掛 `_Present` trait 與
對應 schema；`_Absent` 與 `?` 是整體知識狀態，只應掛在 grammar-profile：

```lang
sign NegativeClause:
    belongs GB107_Present
    belongs PrefixNegation
    syn:
        strategy => licensed / [GB107_Present]
```

`compile_system → evaluate_sign` 後，`NegativeClause` 同時具有
`syn.typology.grambank.GB107 = 1`、`syn.negation.standard.bound-verb = available`，
而本地 rule 會以 Matched 記錄寫入 `syn.strategy = licensed`。std trait 目前須以
`belongs` 使用；裸 `GB107_Present` macro 仍只會在 user Language 內查找。

GB117 尤其要分清 observation 與 construction requirement。code `1` 表示語言中至少有
某種 predicate-nominal equative 使用 copula，並不表示所有名詞謂語句一律強制使用。
具體 construction 依自己的證據選擇三個互斥 schema 之一：

```lang
trait RequiredCopulaPredicateNominal:
    syn:
        slots:
            copula [Copula]

trait OptionalCopulaPredicateNominal:
    syn:
        slots:
            copula [Copula]?

trait ZeroCopulaPredicateNominal:
```

`?` 在這裡只標記該 construction 的 filler optionality；它不是 Grambank 未知值。後者仍
以 `belongs GB117_PredicateNominalCopula` 表示。英語正式核心的具體 copular construction
選 required slot；其他語言或同語言的另一 construction 可另選 optional／zero schema。

## 5. 25 項映射

表內 path 是 engine 對官方問題的保守行為映射；不是 Grambank 新增欄位，也不宣稱
參數間的因果關係。

| ID | 範疇 | 主要行為 path |
|---|---|---|
| GB020 | 定指／特指冠詞 | `syn.determination.definite-article`、`sem.reference.identifiability` |
| GB021 | 不定冠詞 | `syn.determination.indefinite-article`、`sem.reference.indefiniteness` |
| GB028 | 包含／排除式 | `syn.pronoun.clusivity`、`sem.person.clusivity` |
| GB030 | 第三人稱代詞性別 | `syn.pronoun.third-person-gender`、`sem.reference.gender` |
| GB044 | 名詞複數形態 | `syn.number.noun-plural`、`sem.number.plural` |
| GB057 | 數詞分類詞 | `syn.numeral.classifier`、`sem.quantification.classification` |
| GB059 | 可讓渡／不可讓渡領屬 | `syn.possession.alienability`、`sem.possession.alienability` |
| GB068 | 性質詞謂語動詞化 | `syn.predication.property-concept` |
| GB070 | 非代詞核心論元格 | `syn.alignment.core-nominal-case` |
| GB074 | 前置詞 | `syn.adposition.preposition` |
| GB075 | 後置詞 | `syn.adposition.postposition` |
| GB082 | 現在時形態 | `syn.tam.present`、`sem.time.present` |
| GB083 | 過去時形態 | `syn.tam.past`、`sem.time.past` |
| GB084 | 未來時形態 | `syn.tam.future`、`sem.time.future` |
| GB086 | 完成／未完成體形態 | `syn.tam.perfectivity`、`sem.aspect.perfectivity` |
| GB103 | 受益者應用式 | `syn.valency.benefactive-applicative`、`sem.roles.beneficiary` |
| GB107 | 動詞黏著標準否定 | `syn.negation.standard.bound-verb`、`sem.polarity.standard-negation` |
| GB117 | 名詞謂語繫詞 | `syn.predication.nominal-copula`、`sem.predication.nominal-link` |
| GB118 | 連動構式 | `syn.complex-predicate.serial-verb`、`sem.event.serialization` |
| GB132 | 及物句無標語序為動詞居中 | `syn.word-order.transitive.verb-medial`、`prag.information-structure.transitive-order` |
| GB147 | 詞彙動詞上的形態被動 | `syn.voice.passive` |
| GB155 | 動詞詞綴／附著詞使役 | `syn.valency.causative`、`sem.causation` |
| GB262 | 句首是非問句助詞 | `syn.interrogative.polar-particle.initial`、`prag.illocution.polar-question` |
| GB322 | 直接證據語法標記 | `syn.evidential.direct`、`prag.evidence.direct` |
| GB522 | 語境可推知時省略 S/A | `syn.argument.subject.omission`、`prag.reference.subject` |

## 6. 刻意界線

- 這是 25/195 的功能子集，不宣稱覆蓋完整 Grambank。
- 不匯入個別語言 coding、共現機率或自動類型學評分；那是 `data/` provider／自動生成器
  的後續工作，不是手動造語的合法性限制。
- 不把 synchronic coding 當成歷史原因；無年代、attestation、ChangeSet 或
  entrenchment/lexicalization 動力學。
- std package 現可在 trait 中配置 Slot 與 Rule；`config/package.conf` 的
  `rule_namespace` 是其唯一的來源身分（現有為 `std:core`、`std:grambank`），所以
  trace/source map 的 RuleId 是 `namespace:ordinal`，不會與 local rule 或其他 package
  的同序號相撞。現有 25 個 Grambank value traits 保持 observation；可執行 schema 放在
  `std/cxg`。SlotMap 仍是 construction 套用介面，不用 Grambank 專屬語法表示。
- GB132 只表示「verb-medial」，不額外推斷 A–P 的相對順序。
- GB082–084 的 `0` 只否定「動詞上的專用顯性形態」，不否定相應時間語義或助動詞／
  粒詞策略；GB107 的 `0` 同樣不表示語言沒有否定。
- GB074 與 GB075 並非互斥；同一語言可以同時具有前置詞與後置詞。

## 7. Requirement-to-evidence

| 要求 | 實作證據 | 測試證據 |
|---|---|---|
| 修補06資料夾分層 | `lib/std/core|grambank|cxg/{code,data,config}` | package config/export 對齊測試 |
| 20–30 個主要特徵 | `features.tsv` 25 個 distinct ID | 精確斷言 25 且每項三個 trait 可解析 |
| 官方 code 不失真 | root=`?`、Absent=`0`、Present=`1` | missing/unknown/0/1 四態投影 |
| trait 可繼承與覆寫 | `belongs` + dimensional Def | public compile/evaluate、local override、衝突 provenance |
| 能驅動行為 | value category + behavior Def | category guard Matched 與近失敗 Unmatched |
| 維度貼合 | syn/sem/prag 各自 path | 跨範疇 profile 的三維 projection 斷言 |
| std 不污染使用者資料 | std 只進 OntologyRegistry | `system.language().dump()` 不含 std，ontology 可查到 export |
| 決定性與回歸 | priority + name 固定順序 | 兩次 load/dump 相同、全套 language/Tshiatūn 回歸 |
