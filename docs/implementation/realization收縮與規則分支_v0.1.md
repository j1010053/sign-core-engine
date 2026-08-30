# realization 收縮與規則分支 v0.1

> **狀態:語法核心已定,實作進行中(2026-08-30)。** 本檔 §2 的四條決策**擬編 P93–P96**,
> 待擁有者確認編號後回填《架構修補彙整》。§5 的補充形機制**未定**,不得當作已裁定引用。
>
> 上游依賴:Tshiatūn `3105363`(插入語法 `* => er / _ #`、方向敏感邊界、1→N 擴張)。

---

## 1. 定調:哪個 part 承擔什麼

這是本檔的根。所有決策都是它的推論。

| part | 承擔 |
|---|---|
| **sign** | 語法範疇(`belongs` + syn feature)、**儲存的形** |
| **構式** | 槽的排列(深層模板)、範疇的**計算**(`=>` 規則) |
| **realization** | **音段變換**,條件寫在範疇上 |

**終局是 realization 只剩規則。** 這一輪到不了——會逼出那一步的語料案例(語序交替、補充形)全部卡在
§5 的待決議,故**模板分支本輪保留**。

### 1.1 為什麼不是新增 `block` 機制

先前提案是在 realization 內新增位置類 `block`(Stump 的 PFM rule block)。**否決**:
引擎已有兩套位置機制——構式的 `syn: slots:`(句)與 `word::derive` 的 `Component::Ring`(詞)
——而詞綴本來就是 sign(`sign suffix_ap:`),`Stage` 的 stem/word/phrase 是**規則套用階段**
而非範疇分界。**詞句平等已是既成事實**,再加第三套位置機制等於重蹈 P86/P88 收掉的重複。

**位置類 = 構式 slot。** 納瓦霍動詞 11 個前綴位 = 一個有 11 個槽的構式,與及物構式有
agent/patient 兩個槽是同一件事。PFM 反對「詞素是獨立詞項」的那些論證(累積實現、延展實現、
零實現、非串接)在構式模型裡不適用——slot 是**構式內部的角色**,不是獨立詞項。

### 1.2 語料證據

`en-standard/grammar.lang` 八條 realization 全數盤點,**沒有一條**在做音韻條件化的同位異形;
七條在做「這個槽該填什麼」,只是沒有語法可寫,只好整份模板複寫:

| 行 | sign | 分支實際改了什麼 |
|---|---|---|
| 212 | `she` | 整詞換 `/she/` → `/her/` |
| 374 | `EnglishCountNounForm` | 給 stem 加 `s` |
| 441 | `EnglishSVIntransitive` | 給 predicate 加 `s` |
| 473 | `EnglishSVOTransitive` | 給 predicate 加 `s` |
| 496 | `EnglishCopularPredication` | **`{$slot.copula}` → 字面 `is`** |
| 523 | `EnglishDoNegation` | **`{$slot.auxiliary}` → 字面 `does`** |
| 547 | `EnglishPolarQuestion` | **`{$slot.auxiliary}` → 字面 `does`** |
| 570 | `EnglishPeriphrasticPassive` | **`{$slot.auxiliary}` → 字面 `is`** |

三個硬證據:

1. **同一詞彙事實寫了兩遍**:`is` 硬編在 496 和 570,`does` 硬編在 523 和 547。
2. **詞位存在,屈折形不存在**:`sign be:` / `sign do:` 是 sign,`is` / `does` 是模板裡的字串常量。
3. **硬編是被 `+s` 逼出來的**:`{$slot.X}s` 對 `walk` 可行,對 `be`/`do` 產出 `*bes`/`*dos`,
   所以到 copula 和 auxiliary 就退回硬編。

**根因**:名詞有屈折構式(`EnglishCountNounForm` 有 `stem [Noun]` 槽),**動詞完全沒有**,
所以動詞形態只能做在小句構式上。

---

## 2. 已定決策(擬 P93–P96)

### P93 分支 body = phon block body

realization 的 case 分支底下,寫的就是一個 `phon:` 區塊的 body:**一個深層模板 + 若干規則**。

```
phon:                              realization:
    /pa/                               case:
    p a => b a @stage stem                 <guard>:
                                               /{$slot.stem}er/
                                               a => ä
```

語意統一為 **分支 =(base, rules)**:base 預設為 sign 的深層模板,rules 預設空。
**現行只有模板的分支 = `(模板, [])`,是新語意的特例。**

實作上**不新增 `Expression` variant**:`Expression::DimFragment { dim, items }` 已存在,
其排除 phon 的理由(「Phon uses its existing pure-template representation」)在本決策後失效,
解除排除即可。分支 body 的解析器直接復用 phon block body 的解析器,**零新文法**。

### P94 模板管語序,規則管音段變換

| | 職責 | 為何不可互換 |
|---|---|---|
| **模板** | 槽的**排列**——構式的形式極 | 規則不能重排槽(`EnglishPolarQuestion` 的倒裝) |
| **規則** | 對排好的形做**音段變換** | 模板寫不出 ablaut(`sing`→`sang`) |

上游 Tshiatūn `82764ce` 之後,加綴已可寫成插入規則(`* => er / _ #`),故**保留模板的理由
不再是「規則做不到加綴」,而是語序**。

### P95 多音段實現必須寫成單一 token

```
* => er  / _ #     ✅
* => e r / _ #     ❌ InvalidRewrite
```

這是**上游硬約束**,不是實作偏好。Lexurgy 在 `element/Sequence.kt:73` 對序列做硬性長度檢查,
長度不符丟 `InvalidTransformation`;空格分隔的 `e r` 是兩個 element,對上一個 `*` 必然拋錯。
`er` 與 `e r` **不等價、也不該等價**。

**conlang-engine 的產生器把模板降階成規則時,多音段一律輸出單一 token。**

### P96 詞界一律來自顯式陳述

**詞素邊界 `+` 由作者手寫在模板裡,引擎不推斷。**

```
/{$slot.nominal}+{$slot.marker}/     ← 兩個詞素
/{$slot.c1}a{$slot.c2}a{$slot.c3}a/  ← 三個槽,一個詞根(k-t-b),無縫
```

**已實測可用,零實作**:`+` 原樣通過 `template_references`(它只管 `{...}`)、進 `build_phrase`、
產生 Stem 括號,`@stage word` 規則在縫上命中。

否決自動發縫的兩案:

- **從空白推斷**——正是 P85 剛移除的「首段猜測」同一模式(從表面形式推斷結構)。
- **一律發縫**——對非串接形態**可證明地錯**:阿拉伯語詞根三槽一詞素,中綴使一個詞素被切成兩截。

詞素結構是**分析主張**,不是槽排列的函數。`word::derive` 自動發 `+` 不是因為它較聰明,
而是**那條路的結構本來就是顯式的**(`Component::Ring` 是呼叫端建的樹)。統一成一句話:

> **詞界一律來自顯式陳述,只是記法不同——一邊是樹,一邊是模板。**

### 2.1 附帶必須記載:循環 / 後循環分層

**目前無任何文件記載,不寫下一個人會把實現規則塞進 `@stage stem`。**

```rust
material.phon = if let Some(realized) = token.realized_phon_input() { ... }  // construction.rs:1753
```

外層構式拿到的是內層**實現後**的形。故:

| 層 | 何時跑 | 對象 |
|---|---|---|
| **realization 規則** | 逐構式,展開槽之後 | 該構式的產物 |
| **`@stage` 一般音變** | Tshiatūn 最後一趟 | 整個 phrase |

前者**循環**(cyclic),後者**後循環**(post-cyclic)。這是 Lexical Phonology 的標準分層。
`{$slot.X}s` 的 /s/~/z/~/əz/ 同位異形歸後循環,架構上乾淨。

---

## 3. 變更清單

### A. 語言核心

| # | 檔案 | 內容 |
|---|---|---|
| A1 | `lib.rs` | `Expression::DimFragment` 解除 phon 排除 |
| A2 | `parser.rs:327` | `is_fragment_context` 納入 `PhonContext`,復用 phon block body 解析器 |
| A3 | `system.rs:1722` | `PhonContext` 型檢接受 fragment 形式 |
| A4 | `system.rs:4356` `realize_phon` | 分支命中後以 base 展開,再跑分支帶的規則 |
| A5 | `printer.rs` | 印分支內的模板 + 規則 |
| A6 | `construction.rs` | **不動**——手寫 `+` 原樣通過(P96) |

### B. 診斷

| # | 碼 | 條件 | 級別 |
|---|---|---|---|
| B1 | `TEMPLATE_ADJACENT_SLOTS_FUSED` | 相鄰 `{$slot.X}{$slot.Y}` 無分隔 | Warning(融合對非串接形態合法) |
| B2 | `REALIZATION_RULE_WITHOUT_BASE` | 分支只有規則但無深層模板 | Error |
| B3 | `REALIZATION_RULES_INERT` | 分支帶規則卻對展開形無作用 | `CaseRecord.diagnostic_code`(執行期) |

### C. changeset / `.chg`

| # | 位置 | 內容 |
|---|---|---|
| C1 | `lib.rs:5072` `root_case`/`root_case_mut` | 定址多一層:branch → rule |
| C2 | 原語編輯 | 插入/更新/刪除分支內規則(`RuleId` 現成,containment 變了) |
| C3 | `identity.rs` | 分支內規則進內容雜湊 |
| C4 | `restore.chg` | 語料改動後 rebless |

### D. 語料遷移

| # | 對象 | 改成 | 狀態 |
|---|---|---|---|
| D1 | **新增** `EnglishFiniteVerbForm` | 吃 `stem [Verb]`,規則分支處理**可派生**屈折 | 可做 |
| D2 | #3 / #4 小句構式 | 去掉 `{$slot.predicate}s`,predicate 改填 D1 的產物 | 可做 |
| D3 | #2 `EnglishCountNounForm` | 規則分支處理 `mouse`→`mice` | 可做 |
| D4 | `EnglishPluralNP`、std:cxg 的 `PrefixNegation` / `SuffixNegation` | 三處相鄰槽補 `+` | 可做(B1 上線即抓到,**全數真陽性**) |
| D5 | #1 `she`/`her` | **不動**,合法的詞位內部替換 | — |
| D6 | #5–#8 硬編 `is`/`does` | **本輪不動** | **卡 §5** |

**D6 為何不做**:四條全涉及 `be`/`do` 的補充形。搬進屈折構式的分支只是把違反從小句層挪到
屈折層——`is` 仍不是 sign,範疇仍不在它身上。**做一半的錯移比不動更難清。**

### E. 文件

| # | 檔案 | 內容 |
|---|---|---|
| E1 | 本檔 | P93–P96 + 定調 + 循環分層 |
| E2 | `共時lang語法教學_v1.md:163` | realization 段改寫 |
| E3 | `chg_authoring_insert_update_v0.md:81,224` | realization 定址表 |

### F. 測試

| # | 內容 |
|---|---|
| F1 | 手寫詞界:有縫則 `@stage word` 命中、無縫則不命中(P96 迴歸守衛) |
| F2 | 分支帶規則的 parse / roundtrip / 求值 |
| F3 | 巢狀構式的循環順序(內層實現形餵外層) |
| F4 | B1–B3 正負例 |

---

## 4. 上游驗證紀錄(Tshiatūn `3105363`)

本設計依賴的上游能力,已在 worktree 實測:

| 探針 | 輸入 → 輸出 |
|---|---|
| `i => a / _ n g` | `sing` → `sang`(ablaut) |
| `* => er / _ #` | `sang` → `sanger`(純加綴) |
| `a => e / M _ n` 然後 `* => er / _ #` | `Mann` → `Menner`(**一分支兩規則,依序**) |
| 上者反序 | 同樣 `Menner` |

邊界方向(`_ #` vs `# _`)、插入後 morph 括號連續不重疊、`p => *` 與 `* => *` 舊語意——均未退化。

---

## 5. 待決議

### ★ 補充形機制(填充者選擇)

**問題**:「be + 三單 = is」是詞位事實。按 §1 的定調,`is` 應是 sign:

```
sign is:
    belongs EnglishCopula
    syn:
        finite_form = third_singular
    phon:
        /is/
```

範疇在 sign 上、形在 sign 上,realization 不必決定任何事。但**選中它需要填充者選擇**,
而現在 `SlotFiller::sign(slot, name)` 由呼叫端指名。

**已知條件**:

- **構式軸的選擇已經存在**——`System::derive_candidates(category, fillers, mapping, context)`
  遍歷所有滿足該範疇的構式、逐一試填,不相容者**靜默跳過**而非報錯。
- **槽約束機制已經存在**——`syn: slot_features:`。
- 缺的是同一套邏輯搬到**填充者軸**。

| 選項 | 內容 | 代價 |
|---|---|---|
| **1** | 填充者選擇:補充形成為 sign,構式依槽約束比對選中 | 新入口,邏輯是 `derive_candidates` 的鏡像;**架構正確** |
| **2** | 詞位儲存屈折形,構式讀取 | 需要 phon 值的 feature;是「word 延伸」模型的變形,已判定為病灶 |
| **3** | 不做,補充形留在構式分支當字串 | 現況;範疇繼續散在 realization 裡 |

**阻塞**:D6(#5–#8)、以及「realization 只剩規則」的終局。

這是**已知缺口,非疏漏**。補充形是 blocking 的詞彙層樣貌(儲存形阻斷派生形,Elsewhere
Condition);可派生形態不需要填充者選擇,補充形需要。

### 🔴 A2 副作用:`when:` 會在 phon 合法

`is_fragment_context` 同時守著 `when:`(`CaseSelection::Accumulate`)。phon 成為 fragment
context 後 `when:` 就進得來。帶規則之後累積語意**未必錯**(多分支各貢獻規則、全部套用 =
PFM 的 rule block),但這是獨立語意決定。

**本輪取保守:`when:` 維持排除 phon**,把 fragment 閘門與 `when:` 閘門拆成兩個判斷。
日後若裁定 phon 可累積,是一行改動。

### 🔴 A4:分支規則在哪跑

(a) 展開後、`build_phrase` 前自成一趟,或 (b) 排進 `phon_program` 最前面。
(b) 復用多,但巢狀推導時會套到整個 phrase 而非只有本構式的產物。

**本輪取 (a)**,與 §2.1 的循環分層一致。`word.rs:138` 的「小規則組」已有同形作法可參照。

---

## 6. 不做 / Phase 2

- **語序交替拆成獨立構式**(#7 倒裝)——卡在同一批補充形案例
- **reduplication**——上游 qy 的 capture 尚未降階實作;1→N 擴張的輸出端必須是字面符號序列,
  「複製匹配到的內容」是另一套機制
- **Romanizer 分層**——`Deromanizer:` 在所有規則前、`Romanizer:` 在所有規則後,上游已備;
  realization 產出的是音韻串,書寫形式是更下游的事
