# `.lang`／`.chg`／weightDB std package MVP 規劃（v0.1）

> **狀態**：提案，待實作
>
> **盤點基準**：2026-08-07
>
> **類型**：實作架構與落地路線
>
> **權威邊界**：本文件不新增 P 系列語意裁決。套件載入與資料邊界以
> [《演化專案結構與套件載入》](../architecture/演化專案結構與套件載入_v0.1.md)、
> [《架構修補06》](../architecture/架構修補06_插件服務與DSL_API_v0.1.md)及既有
> specifications 為準；完成狀態以 verification 的可觀測證據為準。

---

## 1. 摘要

MVP 不建立一個混合用途的 `weightDB`。目前系統已有兩種名稱相近、用途不同的
權重契約，必須分開管理：

1. `.chg` goal/recipe 選擇權重：`goal -> recipe -> weight`；
2. 音位生成先驗：`segment -> weight`。

`.lang` std 基礎已足以開始 MVP，主要缺口是共用 ontology 與 construction schema
尚未完全支援歷時 recipes；`.chg` std 目前只有 Future／Perfect 的最小範例；音位
先驗 loader 已存在，但 embedded std packages 尚未提供 `segments.tsv`。

本規劃的目標套件集合為：

| Package | 狀態 | MVP 定位 | 載入方式 |
|---|---|---|---|
| `std:core` | 已有、擴充 | 跨套件共用 ontology | 預設 |
| `std:cxg` | 已有、擴充 | construction schema 與 realization | 預設 |
| `std:grambank` | 已有、補 metadata | 類型學 feature subset | MVP 維持現況 |
| `std:change-core` | 新增 | 只展開為既有 primitive edits 的共用 functions | 遞移依賴 |
| `std:grammaticalization` | 已有、重整 | 語法化 goals、recipes、paths、weights | 預設 |
| `std:phoneme-prior-balanced` | 新增 | 可選、非實證宣稱的音位冷啟動先驗 | 明確 opt-in |

---

## 2. 不可破壞的邊界

### 2.1 三種資料各自負責一件事

| 層 | 內容 | 不負責 |
|---|---|---|
| `.lang` | 共時 form-meaning／trait／construction 模型 | 不保存抽樣權重，不描述歷史操作 |
| `.chg` function package | 作者期可解析的 goal／recipe；執行後展開成 primitive edits | 不擁有專案 snapshot，不在 replay 時重新選 recipe |
| package `data/` | weights、paths、feature tables、provenance | 不成為語言 identity，不暗中修改 graph |

因此，std package 中的 `.chg` 是 `conlang.functions/v1` 函式庫，不是綁定某個
`base source`／`identity digest` 的專案 working copy。

### 2.2 std package 是分析內容，不是能力開關

`std:*` 是隨產品提供、可被專案 import 與 lock 的 package；它不得成為某項後端功能
是否存在的判斷。primitive edit、parser、replay、GraphStore 等能力仍屬 engine/API。

具體語言的詞彙、音位 inventory 與 phonotactics 留在專案 `.lang`；不把一份具體 IPA
清單塞進 `std:core`，以免污染所有語言。

### 2.3 replay 不得依賴重新抽樣

WeightDB 只參與作者期候選排序或選擇。選定並 commit 後：

- `.chg`／changeset 保存已解析的選擇或 primitive trace；
- replay 不重新查 weights、不重新抽樣；
- library lock 固定實際使用的 package code/data；
- 選擇證據另記 seed、候選集合、table digest 與最終選項，供診斷與重現作者決策。

---

## 3. 現況盤點

### 3.1 `.lang` std packages

- `std:core` 已輸出 Predicate、Verb、Nominal、SemanticFrame、AgreementBearer、
  Pragmatic 等共用 traits。
- `std:cxg` 已依賴 `std:core`，涵蓋 determination、number、possession、adposition、
  intransitive/transitive clause、copular、negation、polar question、passive、serial 等。
- `std:grambank` 已依賴 `std:core`，提供一組固定 Grambank feature subset。

現有 manifest：

- [`std:core/config/package.conf`](../../crates/language/lib/std/core/config/package.conf)
- [`std:cxg/config/package.conf`](../../crates/language/lib/std/cxg/config/package.conf)
- [`std:grambank/config/package.conf`](../../crates/language/lib/std/grambank/config/package.conf)

### 3.2 `.chg` std package

`std:grammaticalization` 目前只有：

- `VerbToTense` recipe；
- `Future` goal；
- `Perfect` goal；
- GO／WANT／COME -> FUTURE 與 FINISH -> PERFECT 的 path data；
- Future／Perfect 各自只有一個 recipe、權重均為 `1.0`。

目前 manifest 的 `requires` 為空，但 function signature 使用 `[Verb]`，而 `Verb` 由
`std:core` 提供；這是 MVP 必須先修正的 dependency 缺口。

相關檔案：

- [`config/package.conf`](../../crates/language/lib/std/grammaticalization/config/package.conf)
- [`code/recipes.chg`](../../crates/language/lib/std/grammaticalization/code/recipes.chg)
- [`code/goals.chg`](../../crates/language/lib/std/grammaticalization/code/goals.chg)
- [`data/paths.tsv`](../../crates/language/lib/std/grammaticalization/data/paths.tsv)
- [`data/weights.tsv`](../../crates/language/lib/std/grammaticalization/data/weights.tsv)

### 3.3 兩種 weight loader

| 用途 | 發現規則 | Schema | 現況 |
|---|---|---|---|
| goal/recipe | package data path 以 `/weights.tsv` 結尾 | `goal<TAB>recipe<TAB>weight` | 已有 std 資料與選擇器 |
| 音位先驗 | package data path 以 `/segments.tsv` 結尾 | `segment<TAB>weight` | loader 已有；std 資料缺席 |

goal/recipe parser 已檢查 duplicate、非有限值、負數與同 priority 歧義。音位 prior
目前允許後載入 package 覆蓋前者，且同一檔案重複 segment 會最後一筆勝出；MVP 應
收緊為可診斷、可追溯的行為。

---

## 4. 目標 package 設計

### 4.1 依賴圖

```text
std:core
├── std:cxg
├── std:grambank
└── std:change-core
    └── std:grammaticalization
        └── std:cxg（使用 construction-level recipes 時）

std:phoneme-prior-balanced（獨立、明確 opt-in）
```

`std:grammaticalization` 的 MVP manifest 至少應宣告：

```ini
requires = std:core, std:change-core, std:cxg
```

實際依賴仍由 `LibraryCatalog` 遞移解析並進 library lock。若 construction-level
recipes 尚未進第一批，可先不依賴 `std:cxg`，但使用其 stable ID 前必須補上。

### 4.2 `std:core`

只增加能被多個 domain packages 共用的 trait，不放具體詞彙、音位或語言特定資料。

MVP 候選：

- `Auxiliary`、`BoundMorpheme`；
- `TAM`、`Polarity`、`Voice`、`Valency`；
- `Person`、`Number`、`Gender`、`Case`；
- `Evidentiality`；
- `Agent`、`Patient`、`Experiencer`、`Source`、`Goal` 等共用 semantic roles。

新增前需先檢查是否已有等價 stable ID。stable ID 一旦發佈不得靜默重新命名；需要
修名時使用顯式 alias/migration。

### 4.3 `std:cxg`

第一批補足會被 grammaticalization 與現有 Grambank subset 直接使用的 schema：

- `TAMConstruction`；
- `AuxiliaryTAMConstruction`；
- `BoundTAMConstruction`；
- `DitransitiveClauseConstruction`；
- `CausativeConstruction`；
- `ApplicativeConstruction`。

第二批再加入 relative、complement、coordination。歷時操作中必須區分：

1. 既有 construction 節點內部的 slot／constraint 變化；
2. 建立新 construction 節點的 constructionalization。

兩者需要不同 function 名稱、primitive trace 與驗收案例。

### 4.4 `std:grambank`

MVP 不直接匯入完整 Grambank。先完成：

- 資料集版本、來源、授權與 subset 產生方式；
- `GBxxx -> std:core/std:cxg stable ID` mapping；
- 無法可靠映射的 feature 標成 report-only；
- subset 升級 golden fixture；
- package lock 能反映資料表變動。

### 4.5 新增 `std:change-core`

這個 package 提供跨 domain 重用的高階 functions，但每個 function 都必須只展開為
engine 已支援且已驗證的 primitive edits。第一版以現有 `drift`、`reanalyze`、
`entrench` 能力組合：

- `SemanticBleaching`；
- `CategoryShift`；
- `EntrenchmentIncrease`；
- `VerbToAuxiliary`；
- `FreeToBoundMarker`。

建議結構：

```text
lib/std/change-core/
├── config/package.conf
├── config/exports.tsv
└── code/
    ├── lexical.chg
    ├── morphosyntax.chg
    └── construction.chg
```

尚未具有 primitive API、preview、錯誤語義與 replay 測試的 split／merge／fusion／
borrowing，不得只靠 package function 模擬後宣稱支援。

### 4.6 重整 `std:grammaticalization`

目前 Future／Perfect 各只有一個候選，weight sampling 尚未產生實質選擇。MVP 將 path
data 轉成帶 guard 的可競爭 recipes：

| Goal | Recipe | Source guard |
|---|---|---|
| Future | `MovementToFuture` | GO／COME 類 movement source |
| Future | `VolitionToFuture` | WANT／volition source |
| Future | `DeicticToFuture` | deictic movement source |
| Perfect | `FinishToPerfect` | FINISH／completive source |
| Perfect | `PossessiveToPerfect` | possessive/resultative source |

每個 recipe 必須具備：

- source trait／semantic guard；
- guard 不成立時的明確診斷；
- 已套用狀態的 idempotence guard；
- primitive-only resolved trace；
- canonical source、replay、rebase 與 identity 測試；
- 與 `paths.tsv`、`weights.tsv` 的 referential-integrity 驗證。

### 4.7 新增 `std:phoneme-prior-balanced`

此 package 是可選的人工平衡冷啟動先驗，不宣稱代表 PHOIBLE 或任何自然語言樣本。

```text
lib/std/phoneme-prior-balanced/
├── config/package.conf
├── config/exports.tsv
└── data/
    ├── segments.tsv
    └── README.md
```

`segments.tsv` 使用既有契約：

```tsv
segment	weight
a	1.0
i	1.0
u	1.0
p	0.8
t	0.8
k	0.8
```

`README.md` 至少記錄：

- 資料性質：人工平衡或實證；
- 來源與授權；
- 產生／清理方法；
- Unicode 與 IPA key 規則；
- 版本與維護者。

若 package manifest 尚不接受 data-only package，先補 data-only package 能力；不要
為通過 loader 而加入沒有語意的假 `.lang` 檔。`exports.tsv` 可為只有表頭的空 export
表，前提是 catalog validator 明確允許。

---

## 5. WeightDB 契約

### 5.1 Goal/recipe weights

資料跟隨定義 goal／recipe 的 domain package 發版：

```tsv
goal	recipe	weight
Future	MovementToFuture	1.0
Future	VolitionToFuture	0.8
Future	DeicticToFuture	0.6
```

MVP validator 必須保證：

- goal 與 recipe export 實際存在；
- `(goal, recipe)` 在單一來源中不重複；
- 同 priority package 不得對同一 key 提供歧義值；
- weight 為 finite、非負數；
- 每個可選 goal 至少有一個 guard 成立且權重大於零的候選；
- 權重不要求加總為 `1.0`，sampler 在選擇時正規化。

### 5.2 Segment prior weights

```tsv
segment	weight
t͡ʃ	0.4
a	1.0
```

MVP 需將 parser 收緊為：

- 同一檔案 duplicate segment 直接報錯；
- IPA key 必須符合指定 Unicode NFC 政策，不在 replay 中靜默正規化；
- 空 segment、非有限值與負數報錯；
- 跨 package override 使用顯式 priority；同 priority 衝突報錯；
- 至少一個正權重項目才可用於抽樣；
- multi-codepoint segment 保持單一 key。

### 5.3 分層與來源顯示

有效生成分佈沿用應用層既有優先序，並在 UI／DTO 顯示每一項的實際來源：

1. per-request override；
2. project manual weight；
3. imported/provider weight；
4. selected package prior。

來源資訊至少包含：

- layer；
- package ID 與 version；
- data path；
- source digest；
- 被哪一層覆蓋。

phoneme projection 保持 report-only，不得自動進入抽樣分佈。

---

## 6. 歷史語意與可重現性

### 6.1 作者期選擇證據

任何 seeded goal selection 至少保存以下診斷資料：

```text
algorithm
seed
ordered candidate IDs
effective weights
package/data digests
selected candidate ID
```

這些資料用於重現「為何選到這個 recipe」，但不是 replay 時重新抽樣的指令。

### 6.2 Commit 邊界

commit 前允許：

- 重新載入 package data；
- 更換 seed；
- 重新排名／選擇候選；
- 保留無效 authoring draft。

commit 後必須：

- 固定 resolved function call 或 primitive edits；
- 固定 base source／identity 與 library lock；
- replay 不依賴 weightDB 是否仍可取得；
- 失敗不得產生部分 graph mutation。

### 6.3 Identity 與 construction history

- std exports 使用 stable ID，不以顯示名稱作 identity；
- package 更新不得讓既有 sign／trait identity 被重新配對；
- construction node 內部變化與 constructionalization 使用不同事件語意；
- 舊 snapshot、parent node 與磁碟物件保持 immutable。

---

## 7. 實作階段

### S0：契約收斂

- 修正 `std:grammaticalization.requires`；
- 文件化兩種 weight schema 與命名；
- 補 data-only package catalog 測試；
- segment parser 增加 duplicate、NFC、priority conflict 驗證；
- DTO 增加精確 weight provenance；
- 確認 library lock 涵蓋 function 與 data source digest。

### S1：`.chg` 共用層

- 建立 `std:change-core`；
- 搬出或重寫可共用 recipes；
- 補 stable function exports；
- 加入 positive、guard-failure、idempotence、replay 測試。

### S2：語法化 vertical slice

- Future 建立至少三個可競爭 recipes；
- Perfect 建立至少兩個可競爭 recipes；
- 讓 `paths.tsv`、goals、recipes、weights 交叉驗證；
- UI 顯示候選來源、有效權重、seed 與選擇理由；
- 採用 proposal 後只進 pending `.chg`，不自動 commit。

### S3：音位 prior vertical slice

- 新增 `std:phoneme-prior-balanced`；
- new-project template 提供明確 opt-in；
- 顯示 package/manual/request 各層來源；
- 驗證固定 seed 精確重現與跨 seed 統計分布；
- 驗證 phoneme projection 不影響 sampler。

### S4：`.lang` ontology 補齊

- 只依已落地 recipes 補 `std:core` traits；
- 補 TAM／auxiliary／bound marker 等 `std:cxg` schemas；
- 加 Grambank mapping 與資料 provenance；
- 做 canonical roundtrip、stable identity 與 library upgrade fixture。

---

## 8. 測試與驗收矩陣

| 面向 | 必要測試 | 完成標準 |
|---|---|---|
| Package catalog | requires、cycle、priority、data-only、export kind | 選擇順序決定性，錯誤可診斷 |
| `.lang` | parse、canonical roundtrip、stable ID、舊 fixture | package 升級不靜默改 identity |
| `.chg` functions | resolve、guard、primitive trace、idempotence | 不產生隱藏 mutation |
| Goal weights | missing/dangling、duplicate、all-zero、fixed seed | 同 lock＋seed 得相同選擇 |
| Segment prior | duplicate、NFC、multi-codepoint、priority | 鍵不被錯拆，衝突不靜默覆寫 |
| Distribution | manual/request override、source report | 每個有效值可追到來源 |
| Replay | 移除 weight package 後重播 committed change | 不查 DB、不重新抽樣，結果相同 |
| Stochastic evidence | 固定 seed golden＋多 seed 分布測試 | 精確重現與分布行為同時受測 |
| Graph safety | guard／rebase 失敗 | 磁碟與 graph 都無部分寫入 |

至少保留三條端到端案例：

1. Future proposal：同 seed 重現同 recipe，採用後只進 pending，commit 後可離線 replay；
2. package prior：載入 `segments.tsv`、手動覆寫一項、request 再覆寫一項，UI 顯示正確來源；
3. constructionalization：建立新 construction 節點，與既有節點內部修改產生不同 trace。

---

## 9. 相容性與遷移

- 現有 `std:core`／`std:cxg`／`std:grambank` stable IDs 不改名；
- `std:grammaticalization` 新 dependency 以次版本升級並更新 lock；
- 現有 `VerbToTense` 可保留為相容 wrapper，內部委派新 functions；
- 既有 committed `.chg` 依原 library lock replay，不強制改寫；
- 新 weight validator 只在重新載入／編輯 package data 時阻止歧義，不改寫舊 object；
- package 從 embedded catalog 遷至 filesystem discovery 時，ID、version、digest 與排序語意保持一致。

---

## 10. 明確不納入 MVP

- 不建立 SQL／獨立 database service；GraphStore 仍是專案持久化來源；
- 不把 goal weights 與 segment prior 合併成通用三欄／動態 schema；
- 不在 `std:core` 放具體 IPA inventory、自然語言詞彙或 phonotactics；
- 不直接打包授權或版本尚未釐清的 PHOIBLE／Index Diachronica 衍生資料；
- 不讓 package function 偽裝尚未存在的 primitive API；
- 不讓 phoneme projection 暗中回灌 sampler；
- 不在 committed replay 中重新選 recipe。

---

## 11. MVP 完成定義

以下條件全部成立才視為完成：

1. `.lang` std stable IDs 可 roundtrip 且升版不改 identity；
2. `std:grammaticalization` 的依賴完整，Future／Perfect 有實質競爭 recipes 與 guards；
3. 兩種 weight schema 有各自 validator、provenance 與錯誤碼；
4. `std:phoneme-prior-balanced` 可明確 opt-in，且不影響未選擇它的舊專案；
5. 同 package lock、同 seed、同候選順序產生相同作者期選擇；
6. committed `.chg` 在 weight package 不存在時仍可完全 replay；
7. 統計 projection 只作報表；
8. 所有 guard、parse、selection、rebase 失敗均不產生部分寫入。

