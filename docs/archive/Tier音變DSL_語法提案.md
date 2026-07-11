> ⚠️ **已歸檔(superseded)**:本提案(v0.1)已被《Tier音變DSL_語法規格_v0.3.md》取代,其 interface 模型已廢除。僅供歷史參考,勿引用。

# Tier / 自體段音變 DSL — 語法提案(v0.1)

搭配架構書 v0.3 §4.1–4.2 的 multi-tier 機制。採用模型:**純自體段(autosegment)+ 明確 interface**。語法刻意設計成 Lexurgy 的超集,既有的 `Feature` / `Symbol` / `Class` / `Syllables:` / `=> / _` / `[]` / `@` / `$` / `.` / `#` 全部沿用,以下只描述新增部分。本提案是起點,細節可再收斂。

---

## 0. 設計不變式(記住這一條就好)

表徵 = **骨架(skeleton)+ N 條 tier + 聯結線(association lines)**。

- **Tier 內規則**:可以**讀**——自己這條 tier 的自體段、錨點(anchor)的骨架/音段特徵、韻律結構;只能**寫**自己這條 tier(掛靠、斷聯、插入、刪除、搬移自體段)。
- **Interface 規則**:唯一能**跨層寫**的建構。兩個方向——投射(project:讀音段/骨架 → 寫 tier)、實現(realize:讀 tier → 寫音段特徵)。一律以 `interface` 標記。

一句話:**條件可以跨層(唯讀),但跨層的「改動」必須寫成 interface。** 這條線同時保住 FST 可分析性——所有非正則的跨層耦合都集中在被明確標記的 interface 寫入,其餘 tier 內規則可分離處理。

兩個已定預設:

- **stability**:錨點被音段規則刪除時,掛在其上的自體段不消失,改為浮游(`on-anchor-loss: float`)。
- **stray(浮游無宿主)**:保留,漂到詞緣(`on-stray: float-to-edge`)。

---

## 1. 韻律階層與錨點

層級(由小到大):`segment < mora < syllable < foot < pword`。

- 匹配子:`<seg>` `<mora>` `<syl>` `<foot>` `<pword>`(把 Lexurgy 的 `<syl>` 推廣到整條階層)。
- 界標:`.` 音節界、`|` 音步界、`$` 詞界。
- 層級特徵:`Feature (mora) …`、`Feature (foot) …`(沿用 Lexurgy 的 `Feature (syllable) …` 寫法)。
- 構成宣告:與 `Syllables:` 平行,新增 `Morae:` 與 `Feet:`。

```
Syllables:  {p,t,k,s}? {l,r}? :: @vowel :: @cons?     # onset :: nucleus :: coda(Lexurgy)
Morae:      @vowel                                     # 核心元音給一個 μ
            @vowel :: @coda                            # weight-by-position:coda 給第二個 μ
Feet:       (<syl> <syl>&[+stress]) ltr                # 由左建「左弱右強」抑揚音步
```

---

## 2. Tier 宣告

精簡式:

```
Tier tone anchor mora {H, M, L} default M
```

完整區塊式(需要選項時):

```
Tier tone:
    anchor mora            # 自體段掛在哪一層(TBU)
    values {H, M, L}       # 符號值 tier
    default M              # 未賦值錨點的填充值
    domain pword           # tier 規則的作用域(省略時 = pword)
    on-anchor-loss float   # 穩定性:錨點被刪時的行為(float | delete | reassociate)
    on-stray float-to-edge # 浮游無宿主:float-to-edge | delete | error;可加 left|right|nearest
```

**符號值 tier vs 特徵值 tier。** 聲調這類用 `values {…}`(離散符號);鼻化、ATR 這類本質是音段特徵的擴散,用 `feature` 宣告,其自體段就是一個特徵值:

```
Tier nasal anchor segment feature +nasal domain pword
Tier atr   anchor vowel   feature +ATR   domain stem
```

`domain` 可用韻律層(`pword`)或型態單位(`stem` / `root` / `word`),供和諧類規則界定範圍。

---

## 3. 表徵與良構條件

- 聯結允許**一對多**(一個聲調鋪在多個莫拉上)與**多對一**(多莫拉共用一調)。
- **浮游**(未掛任何錨點)是合法狀態。記法:`α~x` 表自體段 α 掛於錨點 x;`(α)` 表浮游 α。
- 引擎強制 **No-Crossing Constraint(不交叉約束)**:聯結線不得交叉。`spread` / `shift` / `dock` 產生的新聯結若會造成交叉,該次套用失敗(可設為警告或報錯)。

---

## 4. Tier 內規則

一般式(`ltr` / `rtl` 為掃描方向,省略預設 ltr):

```
<name> [ltr|rtl]:
    tier <tiername>: <operation>
```

單行精簡式:`<name> [rtl]: tier tone: shift 1 mora rightward`

操作原語(全部只寫自己這條 tier;條件處可讀錨點的音段特徵):

| 操作 | 語法 | 語意 |
|---|---|---|
| spread | `spread [<val>] <leftward\|rightward> [blocked-by <cond>] [within <prosodic>]` | 把某自體段的聯結延伸到相鄰錨點 |
| delink | `delink [<val>] <anchor-selector>` | 斷開聯結,自體段變浮游 |
| dock | `dock [<val>] floating <leftward\|rightward\|nearest> [to <anchor-cond>]` | 浮游自體段掛到最近的空錨點 |
| shift | `shift <n> <mora\|syl\|…> <leftward\|rightward>` | 每個自體段整體平移 n 個單位 |
| merge | `merge adjacent-equal` | OCP:相鄰同值自體段合併 |
| insert | `insert <val> at <position-cond>` | 在 tier 上憑空插入自體段(少用,多由 interface 投射代勞) |
| delete | `delete <val> <selector>` | 移除某自體段 |
| default | `default <val> [within <domain>]` | 域內未賦值錨點填預設 |
| relink | `relink <val> <dir> to <anchor-cond>` | delink + dock 的簡寫 |

`blocked-by <cond>` 裡的條件可以是音段特徵(如 `[-sonorant]`)——這是**讀**,合法。

---

## 5. Interface 規則(唯一的跨層寫)

**投射(project):讀音段/骨架 → 寫 tier。**

```
<name> interface project:
    <segment/骨架條件>  =>  tier <t>: introduce <val> on <anchor-expr> [floating]
```

`<anchor-expr>` 可用捕捉:`mora-of($1)` = 捕捉音段 `$1` 所屬的莫拉。

**實現(realize):讀 tier → 寫音段特徵。** 讓 tier 的內容在表層可見,供後續音段規則與 romanizer 使用。**慣例:realize 規則須排在最終 romanizer 之前。**

```
<name> interface realize:
    tier <t>: <tier 配置條件>  =>  <被掛錨點的音段效果>
```

---

## 6. Stability 與浮游解析

- **`on-anchor-loss`**:`float`(預設,錨點刪除 → 自體段浮游)、`delete`(隨錨點一起消失,退回線性行為)、`reassociate`(立即就近重掛)。
- **`on-stray`**:`float-to-edge`(預設,你的選擇——浮游至詞緣保留)、`delete`、`error`。方向 `left|right|nearest`(預設 `nearest`:漂向原錨點側的詞緣)。

浮游解析在每條 tier 規則之後、以及全流程結束時各檢查一次;仍浮游者依 `on-stray` 處理。

---

## 7. 與 strata / 管線整合

- Tier 宣告與 `Feature`/`Symbol`/`Class`/`Syllables:`/`Morae:`/`Feet:` 同置於檔頭宣告區。
- 規則依書寫順序執行,interface 規則與音段規則、tier 規則交錯排列;`Then:` / `Else:` 照常可用。
- 中間 `romanizer-xxx` 之前若要顯示 tier 內容,需先跑對應的 realize。純 tier 內狀態(浮游、聯結)不會自動進入 romanizer 輸出,除非被 realize。

---

## 8. 語法速覽(pseudo-EBNF)

```
decl        := tier-decl | morae-decl | feet-decl | <Lexurgy decls>
tier-decl   := "Tier" NAME ("anchor" LEVEL) (values | feature) opt*
values      := "values" "{" SYM ("," SYM)* "}"
feature     := "feature" ("+"|"-") NAME
opt         := "default" VAL | "domain" UNIT
             | "on-anchor-loss" ("float"|"delete"|"reassociate")
             | "on-stray" ("float-to-edge"|"delete"|"error") EDGE?
rule        := NAME DIR? ":" ( tier-rule | iface-rule | <segment-rule> )
tier-rule   := "tier" NAME ":" operation
operation   := "spread" VAL? WARD ("blocked-by" COND)? ("within" UNIT)?
             | "delink" VAL? SEL | "dock" VAL? "floating" WARD ("to" COND)?
             | "shift" INT UNIT WARD | "merge" "adjacent-equal"
             | "insert" VAL "at" COND | "delete" VAL SEL
             | "default" VAL ("within" UNIT)? | "relink" VAL WARD "to" COND
iface-rule  := "interface" ("project" ":" seg-cond "=>" "tier" NAME ":" tier-write
                          | "realize" ":" "tier" NAME ":" tier-cond "=>" seg-effect)
DIR         := "ltr" | "rtl"
WARD        := "leftward" | "rightward" | "nearest"
LEVEL/UNIT  := "segment" | "mora" | "syllable" | "foot" | "pword" | "stem" | "root"
```

---

## 9. 範例集

### 9.1 聲調產生(tonogenesis)—— 純自體段 + interface 的樣板

```
Tier tone anchor mora {H, M, L} default M

# 投射:清/濁音節首在後元音的莫拉上產生浮游聲調(讀音段 → 寫 tone tier)
tonogenesis interface project:
    onset&[-voice] @vowel$1 => tier tone: introduce H on mora-of($1) floating
    onset&[+voice] @vowel$1 => tier tone: introduce L on mora-of($1) floating

# 純聲調層:浮游調就近著陸
dock-tone: tier tone: dock floating nearest

# 純音段層:濁清合併(對立轉移到聲調層)
devoicing: [+voice]&onset => [-voice]

# 實現:讓聲調在表層可見(供 romanizer 輸出 á / à)
tone-realize interface realize:
    tier tone: H~<mora>$1 => $1 [+hightone]
    tier tone: L~<mora>$1 => $1 [+lowtone]
```

結果:\*pa → pá、\*ba → pà。濁清對立乾淨地從音段層移到聲調層,沒有任何一步同時改兩層。

### 9.2 鼻化和諧 + 阻塞

```
Tier nasal anchor segment feature +nasal domain pword

nasal-spread: tier nasal: spread +nasal rightward blocked-by [-sonorant]
nasal-realize interface realize:
    tier nasal: +nasal~@vowel$1 => $1 [+nasalized]
```

`blocked-by [-sonorant]` 是唯讀跨層條件(合法);擴散只寫 nasal tier;直到 realize 才落到元音成為可輸出的鼻化。

### 9.3 聲調穩定 + 平移(承 9.1)

```
final-vowel-loss: @vowel => * / _ $     # 錨點被刪 → 其調依 on-anchor-loss=float 浮游
redock: tier tone: dock floating leftward   # 浮游調左掛到鄰莫拉(連調 sandhi 產生)
# 或整層右移一個莫拉:
tone-shift rtl: tier tone: shift 1 mora rightward
```

線性模型(含 Lexurgy)在 `final-vowel-loss` 就把聲調弄丟了;tier 的 stability 讓它存活重掛。

### 9.4 補償性延長(用 mora tier)

```
coda-loss: @coda => * / @vowel _ .              # 刪 coda,其 μ 依 stability 浮游
relink-mora: tier mora: dock floating leftward to @vowel   # 孤兒 μ 左掛到元音
mora-realize interface realize:
    tier mora: @vowel$1 with 2 mora => $1 [+long]     # 雙莫拉元音 → 長元音
```

不必寫 `Vː * / _C` 這種硬規則,長化是莫拉重掛的自然結果。

### 9.5 ATR 元音和諧(domain = stem)

```
Tier atr anchor vowel feature +ATR domain stem
atr-default: tier atr: default -ATR within stem
atr-spread:  tier atr: spread +ATR leftward within stem   # +ATR 由詞根向左擴散
atr-realize interface realize:
    tier atr: +ATR~@vowel$1 => $1 [+ATR]
```

### 9.6 抑揚格延長(用 foot tier + interface)

```
Feet: (<syl> <syl>&[+stress]) ltr
iambic-lengthening interface project:      # 讀音步結構 → 寫 mora tier
    <syl>&[+stress head-of foot] : @vowel$1 => tier mora: introduce mora on $1
```

---

## 10. 開放問題 & FST 邊界

- **float-to-edge 的方向消歧**:`nearest` 在兩側等距或跨多音節時的判準需定死(建議:沿原錨點所屬音步的中心方向)。
- **feature-tier 與 symbol-tier 的著陸語意是否需分流**:特徵值 tier 的 `spread` 天然是「延伸同一值」,符號值 tier 的 `dock` 較常是「一對一著陸」,可能需要不同預設。
- **realize 與中間 romanizer 的互動**:同一 tier 在不同演化階段可能要用不同 realize(早期只標調型、晚期標具體調值)。
- **FST 邊界**:tier 內規則(§4)一律走直譯器;§5 的 interface 寫入是被標記的非正則耦合點,FST 後端只吃純音段規則,或對「投射」做交錯編碼近似。這一點對含聲調語言的批次演化與反向重構覆蓋率影響最大,列為架構書 §10 的持續評估項。
