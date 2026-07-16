# 架構修補 03：Trait 與 Compiled Grammar（v0.2）

**依賴**：修補 01（Grammar/Grammar Store 分離、臨時韻律域）、修補 02（Trait 機制）。
**編號註記**：本修補決策編為 **P8/P9**；**權威 = 《架構修補彙整 01–04》§1 總表**（P9 由 P18 修訂），文末決策表為初編僅供對照。

---

# 修正目的

本修補解決兩個問題：

1. Trait 在架構中的本體定位。
2. 共時文法（deep → surface grammar）與執行層的責任分離。

本修補完成後：

* Trait 成為共時語言知識的主要組織方式。
* Grammar 不再等於執行資料。
* 原 Grammar Store 更名為 **Compiled Grammar**。

---

# 一、Trait 的定位

Trait 定義為：

> **Sign 內容的可共享模板（compile-time reusable definition）。**

Trait 不是新的語言本體，也不是執行期物件。

Trait 可以包含任何原本能寫入 Sign 的內容，例如：

* phon
* sem
* syn
* prag
* metadata
* rule

因此：

> **凡 Sign 能宣告的內容，都可以抽離成 Trait。**

Trait 不限制維度，也不預設任何語言學分類。

名詞、動詞、形容詞等僅作為標準庫 Trait，而非引擎內建概念。

---

# 二、Compile 前與 Compile 後

Trait 僅存在於 Compile 前。

Compile 流程：

1. Parser 建立 Trait / Sign AST。
2. Trait 可被修改。
3. Compile 展開所有 Trait。
4. 合併產生最終 Sign。
5. 建立查詢索引。
6. 建立 Compiled Grammar。

Compile 完成後：

* Trait 不再參與語意解析。
* Engine 不讀取 Trait。

Compile 後僅保留：

* Sign
* Compiled Grammar
* Trait 查詢索引（Compile Artifact）

索引僅供：

* IDE
* 搜尋
* 類別查詢
* Builder 最佳化

不得影響語意。

---

# 三、Trait Priority

Rule Order 與 Trait Priority 為兩個完全不同概念。

## Rule Order

Rule 永遠依照書寫順序執行。

例如：

```text
rule A
rule B
rule C
```

執行順序固定為：

A → B → C

不得因 Trait 或 Priority 改變。

---

## Trait Priority

Trait Priority 僅決定：

> **Compile 時，多個 Trait Block 進入同一 Sign 的合併順序。**

Trait 不修改自身 Rule Order。

Priority 不參與 Runtime。

Compile 完成後即消失。

---

# 四、Definition 與 Rule

DSL 區分兩種語句。

## Definition

Definition 描述語言知識。

使用：

```text
=
```

例如：

```text
feature sonorant = (+,-)

symbol m = [+sonorant]

class vowel = {a,e,i,o,u}

valence = 1
```

Definition 不具有執行順序。

Compile 時依欄位 Merge Strategy 合併。

---

## Rule

Rule 描述狀態轉換。

使用：

```text
=>
```

例如：

```text
a => ə / _#

valence => 2 / _[+move]
```

Rule 永遠具有執行順序。

Rule 一律依照書寫順序執行。

是否為 phon、syn、sem 不影響此性質。

---

# 五、Compiled Grammar

原 Grammar Store 更名為：

> **Compiled Grammar**

Compiled Grammar 定義為：

> **Compile 後供引擎執行的共時文法表示（compiled grammar representation）。**

Compiled Grammar 不是語言知識本身。

語言知識存在於：

* Global
* Trait
* Sign

Compile 後：

Global、Trait、Sign 中的 Rule 經：

* Priority
* Rule Order
* Layer
* Stage

解析後生成 Compiled Grammar。

Engine 僅讀取 Compiled Grammar。

因此：

Deep → Surface Grammar（語言知識）

≠

Compiled Grammar（執行表示）

---

# 六、Compiled Grammar 的責任

Compiled Grammar 僅負責：

* Runtime Rule Lookup
* Layer Dispatch
* Stage Dispatch
* Rule Execution

不得保存：

* Trait
* Priority
* Compile Metadata

Compiled Grammar 應視為 Builder 的編譯產物，而非語言模型。

---

# 七、Trait 查詢索引

Compile 同時建立 Trait 索引。

例如：

```
Trait
↓

Referenced Signs
```

以及：

```
Sign
↓

Referenced Traits
```

索引用途：

* Find References
* IDE 跳轉
* Trait 搜尋
* 限定詞類查詢

索引屬於 Compile Artifact。

不得參與 Engine 執行。

---

# 八、整體流程

```
Grammar Source

(Global
 Trait
 Sign)

        │
        ▼

Compile

        │

 ├── Trait Expansion
 ├── Priority Resolution
 ├── Rule Order Preservation
 ├── Trait Index Generation

        ▼

Compiled Grammar
+
Compiled Sign

        │
        ▼

Execution Engine

        │
        ▼

Deep → Surface
```

---

# 九、本修補影響

本修補完成後：

* Trait 成為共時語言知識的主要重用機制。
* Rule Order 與 Trait Priority 完全分離。
* Grammar Store 正式更名為 Compiled Grammar。
* Compiled Grammar 僅代表 Engine 的執行表示，不再代表語言知識。
* Compile Artifact（Trait 索引）與 Runtime 執行責任完全分離。


---

# 十、P 系列決策(本文件新增)

| 編號 | 決策 |
|---|---|
| P8 | **Compiled Grammar(Grammar Store 更名 + 責任分離)**:語言知識住 Global/Trait/Sign(Grammar Source);Compiled Grammar = compile 後供引擎執行的共時文法表示(Builder 編譯產物,非語言模型),僅負責 rule lookup / layer dispatch / stage dispatch / rule execution,不得保存 Trait/Priority/compile metadata;Engine 只讀 Compiled Grammar。Trait 查詢索引為 Compile Artifact(IDE/搜尋用),不得影響語意。**修訂 P2 的表述**:「節點第二儲存」= Grammar Source(Global/Trait/Sign),其編譯結果才是引擎所見 |
| P9 | **Definition/Rule 二分 + Rule Order 與 Priority 完全分離**:Definition 用 `=`(描述語言知識,無執行順序,compile 依欄位 Merge Strategy 合併);Rule 用 `=>`(狀態轉換,一律依書寫順序執行,不因 trait/priority 改變,phon/syn/sem 皆然);Trait Priority 僅決定 compile 時多 trait block 進入同一 sign 的合併順序,不參與 runtime,compile 後消失。**修正 P6 的 runtime 語意** |
