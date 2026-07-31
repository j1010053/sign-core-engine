# 架構修補 02:Trait 機制(v0.1)

> **性質**:對設計鏈的修補;已納入 repo,決策編入 **P 系列**(見 §8)。
> **依賴**:修補 01(Grammar Store / 臨時韻律域)是本修補的上游——Trait 的 phon block 攜帶的規則,住在 Grammar Store 的 layer 索引內。
> **編號註記**:§6 原建議編號 I13–I15 與實作層撞號,重編為 **P5–P7**;
> **權威 = 《架構修補彙整 01–04》§1 總表**(P7 已廢止 → P14;P6 由 P9/P14 修訂),本檔 §8 為初編僅供對照。
> **修正註記**:§2 的 priority/位置衝突語意由**修補 03 修正**——Rule Order 與 Trait Priority
> 完全分離(Rule 永遠依書寫順序;Priority 僅管 compile 合併),出入處以修補 03 為準。
> Grammar Store 一詞由修補 03 更名為 **Compiled Grammar**。

---

## 0. 一句話定位

**Trait = sign 內容的可共享、有槽位的展開模板(macro 語意,非 mixin)。**
它不是新本體——沒有 trait 就沒有語言,sign 才是真正的語言單位。trait 是 DRY 在語言本體上的應用:任何能寫進 sign 的內容都可以抽成 trait 供多個 sign 共用。

---

## 1. 語法(進 docs/02 記法表)

### 宣告
```
trait <名稱>
    <block 1 內容>
    ==
    <block 2 內容>
    ==
    <block 3 內容>
    ...
```

- `==` 是**強制分割點**:把 trait 切成具名 block,各 block 內部保持宣告順序。
- Block 命名:**`<名稱>[1]`、`<名稱>[2]`、…**——複用 DSL 既有序數尾槽記法(`<syl>[2]`),不引入新符號。

### 使用
```
sign {
    <名稱>[1]
    <sign 自己的規則>
    <名稱>[2]
}
```

- **所有 block 必須全部顯式寫入,否則 compile error**。要嘛完整展開、要嘛完全不用;不存在半用靜默略過。
- 使用端顯式決定每個 block 的插入位置——**位置本身就是語意**,消滅了大多數順序衝突(你把 `TR[1]` 放在自己規則前 = trait 先、我可覆蓋;放在後 = 我先、trait 可覆蓋)。

### 修改與 compile 語意
- **宣告後可修改** trait 的內容(設計時活引用,共享更新)。
- **compile 時確定**:sign 以「compile 當下 trait 的最後修改版本」展開;compile 後 sign 只看到展開後的最終序列,trait 的概念消失。
- compile = snapshot——和執行語意的 commit 同哲學。

### 全域 trait(Global)
```
global trait <名稱>  ← 所有 sign 預設自動引用、自動展開
```
- Global = 預設自動引用的 trait,**無特殊本體**,只是 priority 最低的普通 trait。
- Grammar Store 的全域音系規則 = 一組 global trait。這統一了修補 01 的 Grammar Store 和本修補的 trait——Grammar Store = 所有 global/trait 的規則按 priority+order 解析後的合成規則系統。

---

## 2. Priority:衝突解析(欄位粒度)

**Priority 階梯**:global(最低)< 引用的 trait < sign 內顯式寫出(最高)。

**衝突判定在欄位粒度**,非整個 trait:
- 不同 trait 管不同欄位 → **疊加(union)**,兩者都生效。
- 同欄位衝突 → 高 priority 覆蓋低 priority。
- 同 priority 平手 → 退到**位置**(使用端寫入順序)決勝;若仍無法消歧 → compile warn,明確要求使用端指定。

Priority **不是執行時動態競爭**,是 compile 時靜態解析。大多數情況位置已解決衝突,priority 是備援。

---

## 3. Priority 階梯 = 適用範圍軸 = Life Cycle 軸

Priority 階梯同時是規則的**生命週期搬遷軸**——規則的一生就是在這個階梯上移動:

```
global(低優先、寬範圍)
  ↕ Generalize / Fossilize
trait(中)
  ↕ Generalize / Fossilize
individual sign(高優先、窄範圍)
```

- **Fossilize(化石化,下移)**:規則從 global/trait 下沉到 individual sign,適用範圍縮、優先升 → 修補 01 的 Lexicalization 宏。compile 的永久展平 = 化石化的實作。
- **Generalize(泛化,上移)**:sign 內的規則被抽取成 trait、再升為 global,適用範圍擴、優先降。類推擴展、規則泛化、schema 化皆為此向。

兩者是同一軸上的反向移動——B 的搬遷宏:
- `Fossilize` = trait/global 規則移入 sign 並從上層刪除。
- `Generalize` = sign 規則抽取成 trait 或升為 global。

---

## 4. Trait 統一的三個既有機制

| 既有機制 | 在 Trait 框架下的身分 |
|---|---|
| 受控本體範疇(docs/07 §9):多 sign 共享的 syn 標記 | 受控本體 = 一組預定義 trait,範疇值 = trait 引用 |
| Schema sign([σσ] 模板、animacy):高固著抽象 sign | Schema = 被多 sign 引用的 trait(非獨立 sign);Generalize 宏的產物 |
| Grammar Store / Global 約束(修補 01):全域音系規則 | Global trait 的集合;Grammar Store = priority+order 解析後的合成結果 |

三個機制合一,不重複定義。

---

## 5. 實作邊界(防 trait 變 God 機制)

**Trait 攜帶宣告式資料,不攜帯任意計算**。規則是宣告(配價/音系規則/sem 傾向),不是「決定如何運算的程式」——否則 trait 滑向可執行 mixin 系統。判準同 sign:存事實,不存過程。

---

## 6. 逐文件修補清單

### docs/02 語法規格
- **記法表新增**:
  - `trait <名稱>` / `global trait <名稱>` 宣告語法。
  - `==` 強制分割點。
  - `<名稱>[n]` block 引用(說明複用既有序數尾槽記法)。
- **strata 章補充**:trait 的 phon block 攜帶的規則按 layer(stem/word/phrase)標記,與修補 01 的 strata 層級錨定銜接。
- **規則語言本身零修改**。

### docs/04 執行語意
- **Compile 語意新增節**:trait 展開的靜態解析(priority 欄位級、位置消歧、全 block 強制完整性)在 compile 時完成,早於執行。
- Parallel Match→Commit 的執行語意:接收的是**已展開的 sign**,trait 在此不存在——compile 和執行嚴格分離。

### docs/05 M0 實作參照
- §9 決策表追加:
  - **I13**:Trait = sign 內容的 macro 展開模板;所有 block 強制顯式寫入;compile 時確定,sign 以最後修改版展開。
  - **I14**:Priority 為欄位粒度,compile 靜態解析;Global = priority 最低的 global trait,無特殊本體。
  - **I15**:Priority 階梯 = Life Cycle 軸;Fossilize/Generalize 為反向移動,由 B 的搬遷宏執行。

### docs/07 分層結構檔(C)
- §1 sign 骨架:`dims` 的來源說明:欄位值可來自 trait 展開(compile 後消失)或 sign 直接寫出;trait 不是 dims 的新型別,是 compile 前的來源機制。
- §9 受控本體:範疇定義為一組預定義 trait;使用範疇 = 引用對應 trait。
- 刪除「schema sign 是一種 sign」的說法,改為「schema = 被多 sign 引用的 trait,Generalize 的產物」。

### docs/08 A/B 分配
- B 需求清單:搬遷宏擴為明確雙向——`Fossilize`(下移)和 `Generalize`(上移),同一軸反向。
- B3 動力學:類推擴展/規則泛化/schema 化 = Generalize 宏的不同語言學表現。

### docs/09 Sign 引擎
- §1 Sign 骨架:dims 在 compile 前可包含 trait 引用(TraitId);compile 後只剩展開值。sign 保留 TraitId 供反向索引查詢,純實作問題。
- §3 Builder:compile 步驟在 Builder adopt 後執行(展開 trait → 最終 sign)。

### 架構修補 01
- Grammar Store 定義補充:Grammar Store = 所有 global trait 的 phon 規則,按 priority+order 解析後的合成規則系統(非獨立本體,是 global trait 的計算結果)。
- 三個搬遷宏對應:Phonologization = 音變進 global trait(AddRule);Morphologization = global trait 規則下移進構式 trait(Fossilize 的一種);Lexicalization = trait 規則展平進 sign(Fossilize 到底)。

---

## 7. 實作插入點(配合 M0 步驟二進度)

| 時點 | 動作 |
|---|---|
| **步驟 2–3(現在)** | 零插入——trait 是 compile 層的概念,引擎執行層不知道 trait 存在 |
| **步驟 4(parser)** | DSL parser 認得 `trait`/`global trait` 宣告與 `[n]` block 引用;parse 出 TraitDef AST(不展開) |
| **步驟 4 同時** | compile pass:trait 展開 → 最終 sign AST;全 block 完整性驗證;priority 欄位級解析;此 pass 在 parser 之後、引擎執行之前 |
| **步驟 4 之後** | 引擎(步驟 5–6)接收的永遠是已展開的 sign——對引擎完全透明 |
| **Sign 引擎實作時** | TraitId 反向索引;Generalize/Fossilize 宏的 store 操作 |

---

## 8. P 系列決策(本文件新增;權威同修補01 §4 之制度)

| 編號 | 決策 |
|---|---|
| P5 | **Trait = sign 內容的 macro 展開模板**:`==` 分 block、`<名>[n]` 引用;所有 block 強制顯式寫入(全用或全不用);compile 時以最後修改版展開,compile 後 trait 對引擎不存在;trait 存宣告式資料、不存任意計算(§5 防 God 機制) |
| P6 | **Priority 欄位粒度、compile 靜態解析**:global(最低)< trait < sign 顯式(最高);不同欄位疊加、同欄位高覆蓋低;`global trait` 無特殊本體 = 預設自動引用的最低優先 trait。*(runtime 語意由修補03/P9 修正:Priority 僅決定 compile 合併,Rule Order 不受影響)* |
| P7 | **Priority 階梯 = Life Cycle 軸**:`Fossilize`(下移:global→trait→sign,範圍縮優先升;Lexicalization=到底)/ `Generalize`(上移:sign→trait→global;類推擴展/規則泛化/schema 化)為同軸反向搬遷宏,屬 B;統一受控本體範疇、schema、全域約束三機制(§4) |
