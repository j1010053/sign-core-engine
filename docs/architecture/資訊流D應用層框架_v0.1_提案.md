# 資訊流 D(查詢／UI)參考框架 — 應用層(步驟 21–22)

> **狀態:提案,未定案。** D-a–D-e 待裁定;本檔不新增 P 編號。
> 承 `邏輯分層架構_v0.1` §1.2/§3/§4(該檔自陳應用層是「**目前設計最空白的一層**」)、
> `架構2.0總鳥瞰_v1.0` §2 流 D、`演化圖本體論_v0.1` §6(差異度/互通度)。
> 受既有裁定約束:**C1**(Builder 產四原語)、**A**(State 只在撰寫時被讀)、
> **R1–R6**(專案結構)、**P70**(候選/選擇分離)、**P60/P64**(immutable node)。

---

## 0. 一句話

**流 D 幾乎不含新語言學。** 它的工作是**組裝**既有能力並維持視圖一致性;
真正需要新設計的只有兩件:**互通度/方言群組**(§6.2 已定為「可替換函數」,
先定接口)與**增量失效**(唯一有狀態的部分)。

---

## 1. 現況:可直接組裝的東西

| Query 需求 | 現況 |
|---|---|
| `diff_vector(a, b)` | **已實作**(`changeset::diff`;五分量 phon/syn/sem/prag/structural + 生滅) |
| `phoneme_stats(node)` | **已實作**(`stats::project_phoneme_freq`,步驟 19;投影本就是為此而生) |
| `lexicon(filter, sort)` | Language 的 sign 投影 + 過濾 |
| `derivation_family(sign)` | 遍歷既有 `origin` + `Sense`/`SenseEdge` |
| 演化樹視圖 | `EvolutionGraph` 節點 + 邊 |
| 旁註層 | `nodes/<id>/annotation/`(**已實作**) |
| 候選詞面板 | `generate::ranked()`(步驟 18,手動模式入口) |
| 追蹤視圖 | `GoalSelectionTrace` / `ProposalSelectionTrace` / `RuleRecord` |
| 統計面板 | 同 `phoneme_stats` |
| **互通度 / 方言群組** | **無**——唯一真正的新設計 |

**專案檔**已由 R1–R6 定案:`project.toml` / `packages.lock.json` / `views/<name>.json` /
`data/` + 既有 `objects/` `nodes/`。D1 已裁定:以 `conlang-persistence` 為準,
`邏輯分層` §3.4 的「SQLite 專案檔」作廢。

---

## 2. 關鍵拆分:**純函數 core + 薄有狀態殼**

`邏輯分層` §3.2 已經把 Query 定成「計算視圖、回答查詢;接收 View Config + Override
**作為參數**(**無狀態純函數**)」。唯一有狀態的是 §3.2 末句的**增量失效表**。

據此拆兩個 crate:

```
conlang-query   純函數投影組裝。受 §4 可攜性約束(可 wasm、可單元測、無 fs)
conlang-app     薄有狀態殼:失效表、Undo/Redo、專案讀寫。**不受** §4 約束(需 fs)
```

**為什麼要拆**:把純的那半獨立出來,才能在 wasm 前端直接跑,且不被 fs/undo 的
狀態污染。這與 `language`/`persistence` 的既有分工同形——語意 crate 純資料,
filesystem host boundary 獨立。

---

## 3. Command API

### 3.1 硬約束:**必須降階為四原語**

這是 **C1 的直接推論**。Builder 已經這樣了(構造 `AtomicRewrite` → `rewrite::expand`
→ `Vec<PrimitiveEdit>`)。Command 若繞過四原語直接改 `Language`:

- 沒有 `.chg` 紀錄 ⇒ 改動**不可 replay**、進不了演化圖;
- 三道 digest 失去意義;
- Undo 就得自己另存快照(見 §5)。

```
command::adopt_proposal(id)             → generate::build       → Vec<PrimitiveEdit>
command::run_evolution(node, changeset) → 既有 ChangeInterpreter
command::apply_rule(rule, scope)        → AtomicRewrite::SoundChange → expand
command::set_view_config(config)        → views/<name>.json     ← **不進 Language**
command::set_override(override)         → views/<name>.json     ← **不進 Language**
```

### 3.2 硬界線:派生視圖**永不回寫資料層**

`演化圖本體論` §19 的鐵律:

> 資料層只存 ChangeSet 事實 + nativization 屬性;互通度、方言界線、pidgin/creole
> 顯示分類,皆為上層對連續事實施加閾值/邏輯的**派生視圖**,**永不回寫資料層**。

故上表最後兩列刻意與前三列**不同去向**:View Config 與 Classification Override
寫 `views/`,不產生任何 `PrimitiveEdit`。R4 已裁定一套一檔。

Classification Override 的理由(`邏輯分層` §1.2):語言/方言界線本質是**社會政治
判斷**,不是語言距離能回答的(馬其頓語 vs 保加利亞語)。資料層只存連續的差異向量;
Override 是詮釋層。

---

## 4. Query API

全部**無狀態純函數**,View Config / Override 一律走參數:

```rust
query::lexicon(&system, &filter, &view)            -> LexiconView
query::phoneme_stats(&language, &inventory)        -> WeightTable      // 已有
query::derivation_family(&language, sign)          -> DerivationDag
query::diff_vector(&a, &b)                         -> DiffVector       // 已有
query::intelligibility(&a, &b, &measure)           -> f64              // ← 新,見 4.1
query::dialect_groups(root, threshold, &override_) -> Vec<Group>       // ← 新
query::search(&system, &query, kind)               -> Vec<Hit>
```

### 4.1 互通度:**只定接口,不定公式**

`演化圖本體論` §6.2 已經定調:

> 此「可替換函數」設計同 DSL 的 strategy 模組化(D28):本體**不綁死**
> 「互通度怎麼算」,只定接口。

【M】對稱版 = 分層差異向量的**加權函數**。但「詞彙差異最傷互通、規則性音變其次」
這個加權**是語言學判斷,不是引擎事實**——與 §6.4(引擎不定義評分合成公式)、
`ContactIntensity::default_factor`(標明是預設)是同一類。

故:

```rust
pub trait IntelligibilityMeasure { fn score(&self, diff: &DiffVector) -> f64; }
```

引擎提供一個**明確標示為預設**的加權實作,呼叫端可換。【N】有向版(A 懂 B ≠ B 懂 A,
依 `contact_history`)待 multi-agent,同接口換實作即可——而 `contact_history`
**已於步驟 20 就位**。

### 4.2 方言群組 = 閾值 + Override

`dialect_groups` 先以 `intelligibility` 對演化樹分群,再套 Override
(強制合併/分割/自訂標籤)。兩者都是參數,故同一棵樹可同時有「從語言學看」
與「從政治認可看」兩套視圖——`邏輯分層` §1.2 明列此需求,R4 的一套一檔正是為此。

---

## 5. Undo/Redo:immutable 模型下**不需要另存快照**

`邏輯分層` §3.3 原文:「Command 執行前後**各取快照**;批次演化為粗粒度 undo
(整個 ChangeSet 執行前快照)」。

那是為可變狀態寫的。但 P60/P64 之後**節點本來就是 immutable snapshot**,
演化是**新增**節點而非就地改。所以:

| | 規格原文 | immutable 模型下 |
|---|---|---|
| undo 一次演化 | 取回執行前快照 | **移動「當前節點」指標**到 parent |
| redo | 重放 | 指標移回 |
| undo 未提交的編輯 | — | 丟棄尚未成為節點的 `PrimitiveEdit` 緩衝 |

也就是說 Undo/Redo 是**指標移動 + 未提交緩衝**,不是狀態回滾。這讓 undo 天然
O(1) 且不佔額外空間,也不會與 `objects/` 的內容定址重複儲存。

**要小心的**:`views/` 與 `data/` 的編輯**不在**這條線上(它們不產生節點)。
那些要不要 undo、怎麼 undo,是 D-c。

---

## 6. 增量失效:唯一有狀態的部分

演化之後哪些投影要重算。若沒有它,每次 UI 更新都要重跑 replay/diff——那是
`邏輯分層` §3.2 點名的成本。

最小可用形狀:以**輸入**為鍵,而非以查詢為鍵。

| 輸入 | 影響 |
|---|---|
| 某節點的 Language 變了 | 該節點的 lexicon / stats / derivation_family;**所有**與它比較的 diff/互通度 |
| `views/<name>` 變了 | 只有該 view 的 dialect_groups / 著色 |
| `data/` 或 package 變了 | 抽樣相關(不影響已固化的語言內容) |
| **State 變了** | **什麼都不必重算**——裁定 (A):replay 不讀 State |

最後一列是裁定 (A) 的直接紅利,值得寫進失效表當註解:**State 改了不必重算任何
既有視圖**,因為它只影響「下一次生成什麼」。

---

## 7. 待裁定

| # | 議題 | 傾向 |
|---|---|---|
| **D-a** | `query` / `app` 兩 crate 拆分是否採用;`query` 是否納入 §4 可攜性約束(禁 fs、wasm 綠) | 採用;納入。純的那半能在 wasm 跑是 UI 的前提 |
| **D-b** | Command 的粒度:一個 Command = 一個 statement 還是一整份 changeset? | 一份 changeset(對齊 §3.3 的「粗粒度 undo」與節點邊界) |
| **D-c** | `views/` 與 `data/` 的編輯要不要進 Undo/Redo? | 不進引擎的 undo 線;若要,由 UI 自己做編輯歷史——它們不產生節點 |
| **D-d** | 失效表以輸入為鍵(§6)還是以查詢為鍵? | 以輸入為鍵;查詢種類會長,輸入種類穩定 |
| **D-e** | 互通度【M】的預設加權由誰定?引擎給一組標示為「預設」的係數,還是完全不給、強制呼叫端提供? | 給預設但標明——同 `ContactIntensity::default_factor` 的處理 |

---

## 8. 建議順序

1. **`conlang-query` 純函數層**——先做已有能力的組裝(lexicon / stats /
   derivation_family / diff),出口:同輸入同輸出、Override 只影響視圖**不進 digest**;
2. **互通度 + 方言群組**——接口 + 預設實作 + Override 套用;
3. **Command API**——降階為四原語,出口:每個 command 都留下可 replay 的 `.chg`;
4. **失效表 + Undo/Redo**——指標移動模型;
5. **步驟 22 UI**(Tauri/WASM),步驟 20 欠的「State 的 UI 顯示」在此還。

1–2 完全不碰寫入路徑,可獨立驗證;3 才動到 Command,風險集中在那一步。
