# `Def` 座標系的值域宣告

> **推遲出處**:P71《`Def` 路徑封閉清單與 `feature:` 分工 v1.0》§1、§3 ③
> ——「Phase 2 = M4 完成之後:套件如何**自行宣告**多段座標路徑」。
> **相關**:P6(欄位粒度優先序)、P75(`?` = 可以沒有值)、逐包解析(見
> `OntologyRegistry::inherited_values`)。

---

## 1. P71 留下的洞

P71 §3 ③ 原文:

> 值本身**其實是封閉的**(三態),缺的是**宣告機制**——25 個參數散在 trait 裡,
> 沒有一處說「這個座標系有哪些軸、每軸值域為何」。這正是 Phase 2 的題目。

具體形狀:`std:grambank` 的域確實存在,但住在**套件資料層**而不是 `.lang`:

```
crates/language/lib/std/grambank/data/features.tsv
GB020    Are there definite or specific articles?    nominal-determination    0|1|?    …
                                                                             ^^^^^ 域
```

引擎合併值時**不會去查 `features.tsv`**——那個域對 AST 層不可見。Phase 1 只做到
「路徑必須在封閉清單內」,沒做到「值必須在該路徑的域內」。

---

## 2. 為什麼現在不做

Phase 2 的題目(套件如何宣告座標系)比任何單一需求都大。現在為了下面那個具體缺口
自己拉一條域的通路,等於搶在 Phase 2 前面蓋一半,而且蓋的形狀多半與 Phase 2 最後
的設計不合。

---

## 3. 連帶欠的債:泛型 `Def` 的並列分歧是順序相關的

值的合併已改為**逐包解析**——每個掛載的 trait 先在自己那層解完,sign 只看得到
解完的包(見 `OntologyRegistry::inherited_values`)。並列的包對同一個 feature
給不同值時取**候選聯集**,結果可交換,故不需要任何優先序。

但那個聯集**只有 `FeatureValue` 用得上**,因為它的值是集合、裝得下「未定案」。
泛型 `Def` 的值是單一字串,沒有地方放候選,於是逐包解析對它退化成「後掛的贏」:

```lang
trait A:
    syn:
        tam.present = 0

trait B:
    belongs A
    syn:
        tam.present = 1      # 明確特化

trait C:
    belongs A                # 對這個欄位一個字都沒說

sign s:
    belongs B
    belongs C                # → 得到 0;換邊寫 belongs C / belongs B → 得到 1
```

`C` 只是繼承了 `A`,卻把 `B` 的特化蓋掉。**順序相關。**

### 3.1 現在不救火的理由

- **活案例 0**。全庫掃描:並列包在同一 `Def` 路徑上真分歧 = 0 次。庫裡的
  grambank 衝突全是**鏈**(`GB020_Absent` 覆寫父 trait `GB020_DefiniteOrSpecificArticles`),
  鏈在逐包解析下由包內部解掉,碰不到這個問題。
- **母體正在縮小**。P71 §3 ② 已裁定作者自造的屬性(`syn.category` 306 次、
  `prag.illocution` 306 次、`syn.telic`、`sem.kind`……)一律改走 `feature:`。
  遷完之後 `Def` 只剩引擎自有(`phon`、`phon.realization`、`sem.roles`、metadata)
  與類型學座標,而後者全是鏈。
- **診斷有覆蓋**。並列分歧會出 `ONTOLOGY_DEF_CONFLICT_RESOLVED`(Warning),
  且該警告已改為讀**實際的合併結果**,不再自己用閉包序算一次贏家
  (回歸測試:`tests/conflict_warning_matches_reality.rs`)。

### 3.2 Phase 2 落地時要做什麼

座標系的域一旦可宣告,`Def` 就有了與 `FeatureValue` 對等的型別資訊,這筆債幾乎
免費清掉:

1. 讓 `Def` 的合併看得到該路徑的域
2. 並列分歧時比照 `FeatureValue` 取**候選聯集**,而非後掛的贏
3. 未定案的 `Def` 需要一個表示法——`FeatureValue` 用 `values: Vec<String>`,
   `Def` 若沿用同形,要一併處理 `.chg` 定址(`update … .value = X` 的向後相容)
   與 canonical form(單值必須逐位元不變,否則 library lock digest 會漂)
4. 移除本檔 §3 的已知限制記載

---

## 4. 驗收

- 上面 §3 那段 `.lang` 兩種掛載順序得到**相同**結果
- 單值 `Def` 的 canonical form 逐位元不變(digest 不漂)
- 既有 `.chg` 的 `update … .value = X` 語意不變
