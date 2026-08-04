# 資訊流 D(查詢／UI)參考框架 — 應用層(步驟 21–22)

> **狀態:提案,未定案。** D-a–D-f 待裁定;本檔不新增 P 編號。
> **v0.2(2026-08-04)**:外部審查後重寫 §5(Undo)、§6(失效),並拆開 Command 與
> ChangeSet 的邊界。三處原文為誤,誌於 §9。
> 承 `邏輯分層架構_v0.1` §1.2/§3/§4(該檔自陳應用層是「**目前設計最空白的一層**」)、
> `架構2.0總鳥瞰_v1.0` §2 流 D、`演化圖本體論_v0.1` §6(差異度/互通度)。
> 受既有裁定約束:**C1**(Builder 產四原語)、**A**(State 只在撰寫時被讀)、
> **R1–R6**(專案結構)、**P70**(候選/選擇分離)、**P60/P64**(immutable node)、
> **P30**(資料層永不含執行邏輯)、**P26**(statement 交易原子性)。

---

## 0. 一句話

**流 D 幾乎不含新語言學。** 它的工作是**組裝**既有能力並維持視圖一致性;
真正需要新設計的只有三件:**互通度/方言群組**(§4,只定接口)、
**工作階段歷史**(§5,唯一真正有狀態的部分)、**快取身分**(§6)。

---

## 1. 現況:可直接組裝的東西

| Query 需求 | 現況 |
|---|---|
| `diff_vector(a, b)` | **已實作**(`changeset::diff`;五分量 phon/syn/sem/prag/structural + 生滅) |
| `phoneme_stats(node)` | **已實作**(`stats::project_phoneme_freq`,步驟 19;投影本就是為此而生) |
| `lexicon(filter, sort)` | Language 的 sign 投影 + 過濾 |
| `derivation_family(sign)` | **兩張不同的圖,不是一次遍歷**——見 §1.1 |
| 演化樹視圖 | `EvolutionGraph` 節點 + 邊 |
| 旁註層 | `nodes/<id>/annotation/`(**已實作**) |
| 候選詞面板 | `generate::ranked()`(步驟 18,手動模式入口)。**依賴 State**,見 §6 |
| 追蹤視圖 | `GoalSelectionTrace` / `ProposalSelectionTrace` / `RuleRecord` |
| 統計面板 | 同 `phoneme_stats` |
| `search` | **MVP = 純線性掃描**;索引化見 §6.3 |
| **互通度 / 方言群組** | **無**——唯一真正的新語言學設計 |

### 1.1 `derivation_family` 踩在兩張圖上

| 圖 | 承載 | 範圍 |
|---|---|---|
| **義項衍生** | `SignItem::SenseEdge { to, from, kind, transparency }` | **單一 sign 內部**——`SemNode::of_sign` 只走 `reg.effective_sign(sign).items`(`sem.rs:109`) |
| **sign 世系** | `metadata.rs:117` 的 `origin() -> Option<SignRef>` | **跨 sign**,單一來源 |

`SenseEdge` 已是一級節點且 parser/printer 皆接(`parser.rs:1291`、`printer.rs:433`),
進 digest——**資料是齊的**。但兩張圖的定址空間不同:前者是義項名、後者是 `SignRef`。

故 `derivation_family` 是**兩段遍歷的接合**,接合點是「某個 sign 的某個義項」。
出口必須同時涵蓋兩段,否則會出現「同 sign 內看得到、跨 sign 斷掉」這種只有部分綠的假象。

**專案檔**已由 R1–R6 定案:`project.toml` / `packages.lock.json` / `views/<name>.json` /
`data/` + 既有 `objects/` `nodes/`。D1 已裁定:以 `conlang-persistence` 為準,
`邏輯分層` §3.4 的「SQLite 專案檔」作廢。

**R15(2026-08-04)**:`project.toml` 的 import 表**合法**——P29/P50 的
「無顯式 import」限 `.lang`/`.chg`(界線見《修補06》§8.5)。步驟 21 實作專案讀寫時
**須同時掛一條顯式拒絕**:`.lang`/`.chg` 內出現 `import` → 診斷並指向專案層 import 表。
沒有它,這條界線只是約定,而「順手也讓 `.chg` 寫一行」是很自然的下一步。

---

## 2. 關鍵拆分:**純函數 core + 薄有狀態殼**

`邏輯分層` §3.2 已經把 Query 定成「計算視圖、回答查詢;接收 View Config + Override
**作為參數**(**無狀態純函數**)」。

據此拆兩個 crate:

```
conlang-query   純函數投影組裝。受 §4 可攜性約束(可 wasm、可單元測、無 fs)
conlang-app     薄有狀態殼:工作階段歷史、快取、專案讀寫的**協調**。不受 §4 約束
```

### 2.1 為什麼是 crate,不是 module

有一種常見建議是「先做 module 邊界,等依賴圖證明有必要再拆 crate」。
**在本專案不成立。**

CLAUDE.md §4 的可攜性是 **crate 粒度的 CI 閘門**
(`cargo build -p <crate> --target wasm32-unknown-unknown`)。一個同時需要 fs 的
crate,裡面的「純 module」**無法被驗證**——整個 crate 對 wasm 就建不起來。

`language`/`changeset` 純、`persistence` 持 fs,這個既有分工正是這條規則的產物。
**在本專案,純度就是靠 crate 邊界檢查的**,不是靠約定。

### 2.2 `conlang-app` 不是第二個 persistence

`conlang-persistence`(P60/P64)**擁有**檔案格式、fs I/O、驗證與交易。
`conlang-app` 只做**協調**(orchestration/facade):

```
conlang-persistence   格式、I/O、驗證、交易        ← 唯一擁有者
conlang-app           串接 persistence/query/command;工作階段歷史;快取
                      **不得自行定義第二套檔案格式或 loader**
```

寫成這樣是因為 §1 的專案檔(`project.toml`、`views/`、`data/`)全是新檔案,
最容易在 app 側長出一套平行的讀寫規則。

---

## 3. Command API

### 3.1 Command 不是一個東西——**三類,規則完全不同**

原案把 `set_view_config` 與 `adopt_proposal` 並列為 `command::*`,再說
「每個 command 都留下可 replay 的 `.chg`」——**自相矛盾**,前者根本不產生
`PrimitiveEdit`。故型別上就分開:

| 類 | 去向 | replay | digest | undo 線 |
|---|---|---|---|---|
| `LanguageCommand` | `PrimitiveEdit` → `.chg` → 節點 | ✅ | ✅ | §5.2 |
| `ViewCommand` | `views/<name>.json` | ❌ | ❌ | §5.3 |
| `ProjectDataCommand` | `data/`、`project.toml` | ❌ | ❌ | §5.3 |

修正後的規則:

> **每個 `LanguageCommand` 都必須降階為四原語並可固化為 `.chg`;
> View/Data command 寫各自的外部資料檔,不進語言 replay。**

### 3.2 硬約束:`LanguageCommand` 必須降階為四原語

這是 **C1 的直接推論**。Builder 已經這樣了(構造 `AtomicRewrite` → `rewrite::expand`
→ `Vec<PrimitiveEdit>`)。若繞過四原語直接改 `Language`:改動不可 replay、
進不了演化圖、三道 digest 失去意義。

```
LanguageCommand::adopt_proposal(id)   → generate::build       → Vec<PrimitiveEdit>
LanguageCommand::run_evolution(..)    → 既有 ChangeInterpreter
LanguageCommand::apply_rule(rule, sc) → AtomicRewrite::SoundChange → expand
ViewCommand::set_view_config(..)      → views/<name>.json
ViewCommand::set_override(..)         → views/<name>.json
ProjectDataCommand::set_weight(..)    → data/weights.tsv
```

### 3.3 Command 與提交邊界:**現行模型已經是三層**

原案 D-b 問「一個 Command = 一個 statement 還是一整份 changeset」。**問法本身是錯的**
——那三個概念早已分層,且已實作:

```rust
pub struct ResolvedStatement { ordinal: u64, edits: Vec<PrimitiveEdit> }
```
(`changeset/src/lib.rs:2078`;`ChangeSet` = `Vec<ResolvedStatement>`)

| 層 | 是什麼 | 現況 |
|---|---|---|
| **Command** | 使用者意圖 | 待做(本檔) |
| **Statement** | 一次交易;1 意圖 → N 原語 | **已實作**,步驟 14 封板含回滾/部分保留 |
| **ChangeSet** | 提交邊界 = 一條演化邊 | **已實作** |

故正確的關係是:

```
LanguageCommand  →  CommandResult { edits, diagnostics, preview }
                 →  一個 ResolvedStatement(交易單位,P26 原子性)
數個 Statement   →  commit  →  一份 ChangeSet  →  一個新節點
```

這讓 UI 得以:預覽後再提交、多步操作合併成一個歷史節點、單一 command 失敗不污染
pending buffer。**不需要新機制,只需要別把 Command 直接等同於 ChangeSet。**

### 3.4 硬界線:派生視圖**永不回寫資料層**

`演化圖本體論` §19 的鐵律:

> 資料層只存 ChangeSet 事實 + nativization 屬性;互通度、方言界線、pidgin/creole
> 顯示分類,皆為上層對連續事實施加閾值/邏輯的**派生視圖**,**永不回寫資料層**。

Classification Override 的理由(`邏輯分層` §1.2):語言/方言界線本質是**社會政治
判斷**,不是語言距離能回答的(馬其頓語 vs 保加利亞語)。資料層只存連續的差異向量;
Override 是詮釋層。R4 的一套一檔正是為此。

---

## 4. Query API

全部**無狀態純函數**,View Config / Override 一律走參數:

```rust
query::lexicon(&system, &filter, &view)      -> LexiconView
query::phoneme_stats(&language, &inventory)  -> WeightTable      // 已有
query::derivation_family(&language, sign)    -> DerivationDag     // 兩段,§1.1
query::diff_vector(&a, &b)                   -> DiffVector        // 已有
query::intelligibility(&input, &measure)     -> IntelligibilityScore  // ← 新,4.1
query::dialect_groups(&input, &strategy)     -> Grouping          // ← 新,4.2
query::search(&system, &query, kind)         -> Vec<Hit>          // MVP 線性掃描
```

### 4.1 互通度:只定接口,但接口要**裝得下它自己承諾的東西**

`演化圖本體論` §6.2 已定調:本體**不綁死**「互通度怎麼算」,只定接口。

原案的接口是 `fn score(&self, diff: &DiffVector) -> f64`。**不夠**——同一段文字
緊接著說有向版要依 `contact_history`,而該接口讀不到方向、接觸史、社會暴露。
「同接口換實作即可」不成立。

```rust
pub struct IntelligibilityInput<'a> {
    pub source: &'a Language,
    pub target: &'a Language,
    pub diff:   &'a DiffVector,
    pub context: &'a IntelligibilityContext,   // 含 contact_history
}

pub trait IntelligibilityMeasure {
    fn id(&self) -> MeasureId;
    fn score(&self, input: &IntelligibilityInput<'_>) -> IntelligibilityScore;
}

pub struct IntelligibilityScore {
    pub value: f64,
    pub measure_id: MeasureId,     // 0.73 是哪套模型算的
    pub symmetric: bool,
}
```

**⚠ 接口一旦拿 `context`,就同時決定了它在失效表的哪一列。**
`contact_history` 住在 `EvolutionState`,而 State 是**撰寫時、雜湊外**的(裁定 A)。
故有向互通度是**讀 authoring 狀態的 Query**,結果隨 State 改變——它屬於 §6.1 的
**authoring-derived** 那一列,不是 replay-derived。這兩題是同一題,不能分開裁。

#### 結果必須帶 `measure_id`

裸 `f64` 會讓 UI 顯示一個看似客觀的「A 與 B 互通度 73%」,但那只是某個 heuristic。
帶身分是本專案既有模式——`EffectiveDistribution::provenance()` 就是「每一項查得出
來自哪一層」。同理。

#### 預設加權的歸屬:**公式是邏輯,係數是資料**

「詞彙差異最傷互通、規則性音變其次」是**語言學判斷,不是引擎事實**——同 §6.4
(引擎不定義評分合成公式)、`ContactIntensity::default_factor`(標明是預設)。

但「交給官方 package 提供」要照 **P30** 切開:

| | 歸屬 | 依據 |
|---|---|---|
| 加權**公式**(如何合成) | **Rust 側,經 PluginRegistry 註冊** | P30:「資料層永不含執行邏輯,只存名字引用」 |
| 加權**係數**(各分量幾分) | package 的 `data/` | 裁定 W:權重表是 data |

若 MVP 必須內建一組,命名應為 `exploratory_heuristic_v1` 而非無來源的 `default`。

### 4.2 方言群組:閾值分群**本身不是算法**

互通度不必然傳遞:`A~B` 高、`B~C` 高、`A~C` 低。同一組閾值下,
connected components / clique / hierarchical / 沿樹切邊會給出**不同答案**。
故原案的 `dialect_groups(root, threshold, override)` 看似已定義,實則最關鍵的
群組語意仍未定。

```rust
pub trait DialectGroupingStrategy {
    fn group(&self, graph: &EvolutionGraph, m: &dyn IntelligibilityMeasure) -> Grouping;
}
```

**MVP = `TreeEdgeCut`**:只在演化樹的 parent–child 邊上按閾值切斷。
它最貼合 EvolutionGraph 的樹狀本體,且**天然迴避一般圖的非傳遞分群問題**。
`ConnectedComponents` / `HierarchicalClustering` 之後再加。

#### Override 是**分類指派**,不是 merge/split(擁有者 2026-08-04)

先前草案把 Override 設計成「強制合併 / 強制分割」,並因此得先裁兩條規則
(merge 是否取傳遞閉包、split 是否優先於 merge)。**那兩個問題是設計選錯而
自造的**——merge/split 是**關係**運算,關係之間才會互相矛盾:

```
A+B、B+C、A|C  同時存在 → 結果取決於套用順序,可能無解
```

改用**分類指派**語意後,問題不是被回答,而是**不存在**:

```rust
pub struct GroupingOverride {
    /// node → group。**sparse**:未列者一律用 strategy 算出的結果。
    assignments: BTreeMap<NodeId, GroupId>,
    /// group → 顯示名。**純展示**,不影響群組身分。
    labels: BTreeMap<GroupId, String>,
}
```

指派是**函數**不是關係,故:

| 原問題 | 分類指派下 |
|---|---|
| merge 傳遞閉包? | 不適用——沒有 merge |
| split 優先於 merge? | 不適用——沒有 split |
| 一個節點能否同屬多群? | 否,型別上就不可能(一個 `NodeId` 一個 `GroupId`) |
| 衝突怎麼辦? | **不可能衝突**,結果由建構保證唯一 |

也因此管線少一段——原案需要的 `validate consistency` 只是為了收拾 merge/split
的矛盾:

```
1. strategy 算出基礎分群
2. 套用 assignments(sparse 覆寫)
3. 套用 labels(純顯示)
```

**這也是本專案既有的慣用語意**:`belongs` 就是分類指派——一個 trait 宣告自己
屬於哪個分類,引擎從來沒有「把兩個分類合併」這種運算。而且它更貼合領域:
語言學家不說「把馬其頓語和保加利亞語合併」,而說「馬其頓語**歸入**某個群」——
`邏輯分層` §1.2 舉的正是這個例子。

**唯一的取捨(誠實記下)**:「合併 G1 與 G2」在指派語意下要逐一寫入成員,
而非一個動作;且日後新增的節點若本該落入 G2,**不會**被舊的合併自動吸收。
後者其實是優點(覆寫不該靜默捕獲未來的節點),前者是 UI 的事——
**介面仍可提供「合併」按鈕,它寫下的是 N 筆指派**。儲存語意與操作語意分離。

D-f3(結構 vs 展示分兩類)因此更自然:`assignments` 管身分、`labels` 管顯示。

---

## 5. Undo/Redo:**工作階段歷史,不是演化圖邊的反向遍歷**

> **本節為 v0.2 全面改寫。** v0.1 寫「undo = 移動當前節點指標到 parent、
> redo = 指標移回」,**是錯的**。誌於 §9。

### 5.1 為什麼原案不可實作

`EvolutionGraph` 的節點**只有 parents,沒有 children 索引**:

```rust
struct Node { parents: Vec<Edge>, .. }      // evolution.rs:139
```

而且節點 id **由其 parents 的 id 算出**(`evolution.rs:32`:「無環是結構保證,
不是檢查」)。故:

- **redo 不是語意模糊,是不可實作**——「子節點」在此資料結構中不是可查詢的方向,
  要找得掃全圖 O(N);
- **undo 也已經多選一**——`parents` 是 `Vec` 不是 `Option`(全 parent merge),
  「移到 parent」本身就沒有唯一答案。

v0.1 那張表兩列都錯。

### 5.2 先分清**使用者在做什麼**,不是分清機制

引擎對 Language 改動只有一種機制:`PrimitiveEdit → ChangeSet → 新節點`。
`EvolutionGraph` 的節點產生入口只有 `add_root`(`evolution.rs:381`)與
`commit`(`evolution.rs:462`),**沒有「就地編輯一個節點」**——唯一的 mutation
是 `set_label`,而 label 是雜湊外的。

但使用者在做的是**三件不同的事**,undo 語意各不相同:

| | 活動 | pending 表示 | undo 是什麼 |
|---|---|---|---|
| **(A)** | **專案編輯**:造原始語時連加 50 個詞、修 gloss、調規則 | **一份寫到一半的 `.chg`** | 編輯那份檔案(移除最後一條 statement) |
| **(B)** | **演化**:Proto → Old → Middle | `graph.commit()` 落成節點 | app history stack + active-node(§5.3) |
| **(C)** | **修改祖先**:發現 Proto 有錯字 | `rebase`(`evolution.rs:651`,步驟16) | 另一條線,不在本節 |

**(A) 若也走 commit,演化樹會被 authoring 噪音淹沒**——50 個詞變成 50 個
「演化事件」,而那在語言學上什麼都不是。故 (A) 必須停在 pending,不落節點。

#### (A) 不需要新格式,更不需要新的專案槽

未提交的編輯 = **一個基底節點 + 一疊 statements**,而 `.chg` 的定義正好就是這個:
`change_set_prelude` 以三道 digest 釘住基底,`statements` 逐條 append,
且 **prelude 單獨即可 parse + resolve**(零條 statement 的 `.chg` 合法)。

若另立一個 `working/` 格式存「未提交的編輯」,等於**兩種東西說同一件事**
(違反實作原則 3 單一資訊源),且 commit 時還要做一次格式轉換。
用 `.chg` 則沒有轉換:pending buffer 長大成熟,**直接就是那條 trunk edge 掛的
changeset**。

**故 R1–R6 不必補洞。** 「目前開著哪份 `.chg`」是 UI 狀態,不是專案結構。

#### 連帶結論:(A) 根本不需要 app 維護 undo stack

它是**一份文件的編輯歷史**,與 `views/`/`data/` 那條線同型。
真正需要 history stack 的只有 (B)。

### 5.3 (B) 的分層:三個不同的東西

| | 是什麼 | 誰擁有 |
|---|---|---|
| **EvolutionGraph** | 語言的歷史／本體關係 | `changeset`(immutable) |
| **Navigation history** | 使用者這次經過哪些節點 | `conlang-app`(工作階段) |
| **Undo stack** | 本次階段要撤銷哪些 commit | `conlang-app`(工作階段) |

```
undo:  從 command/navigation history stack 取上一個 active node
redo:  從 redo stack 取剛撤銷的
```

**不是** `undo = graph.parent` / `redo = graph.child`。

immutable node 仍然帶來真正的紅利:**undo 不需要複製 Language 快照**
(節點本來就是 snapshot,演化是新增而非就地改)。但那不等於不需要一個小型
history stack——原案把「不需複製快照」誤推成「不需要歷史」。

### 5.4 (B) 的 undo:葉節點是真的刪得掉的

> **v0.2 草擬期曾把這裡寫成「只能雜湊外標記 + 顯示層過濾」,那是把一個
> 缺陷當成前提。缺陷已修(2026-08-04),見 §5.4.1。**

| | 情形 | undo |
|---|---|---|
| **(a)** | 尚未 `save` | 丟棄記憶體中的節點即可 |
| **(b)** | 已 `save`,**無子節點** | `graph.remove_node` + `store.remove_node`——**真的刪掉** |
| **(c)** | 已 `save`,**有子節點** | 結構上不可能刪:子節點 id 由 parents 的 id 算出。得先處理子節點 |

(c) 不是政策而是結構事實,兩側都硬擋(`EvolutionError::NodeHasDependents` /
`StoreError::NodeHasDependents`)。

**`manifest` 標記「未採用」永遠不可行**:id = `node_id(snapshot, parents,
nativization)`,加欄位就成了另一個節點。這條與缺陷無關,是內容定址的直接後果。

#### 5.4.1 修掉的缺陷:`save` 是 append-only 且**靜默**

原本:

```rust
/// Append every graph node to the content-addressed store.
pub fn save(&self, graph: &EvolutionGraph) -> Result<(), StoreError>
```

`save` 只走 `graph.ids()`,**從不刪除 store 裡有、graph 裡沒有的節點**;
而 `load` 以 `fs::read_dir(nodes/)` 為準。故「從圖裡移除節點 → `save`」
會 `Ok(())` 然後下一次 `load` 把它**無聲讀回來**——使用者以為撤銷了一次演化,
重開檔案又看到它。

修法**不是**讓 `save` 自動剪除:那樣一個只持有部分圖的呼叫端就能不可逆地清空
store。破壞性操作要顯式:

| | 行為 |
|---|---|
| `save` 遇到 store 有、圖沒有的節點 | `StoreError::StaleNode`,訊息指向 `remove_node`;**檢查在寫入前**,擋下時 store 未被動過 |
| `EvolutionGraph::remove_node(id)` | 只准葉節點;移除 root 時**釋放其 identity namespace**(否則同一份 `.lang` 加不回來) |
| `GraphStore::remove_node(id)` | 只准葉節點;刪整個 `nodes/<id>/`;**不動 `objects/`**(內容定址跨節點共用,孤兒 object 無害,回收另計) |

出口:`persistence/tests/store_roundtrip.rs` 5 案,突變 5/5 首輪全紅。

#### 對 (A)/(B) 分工的影響

(a) 仍是最省事的窗口,故 §3.3 的「多步操作合併成一次歷史節點」依然有價值;
但它**不再是 undo 可行的前提**——已落盤的葉節點現在也撤得乾淨。
真正撤不掉的只有 (c),而那是使用者已經在它之上繼續演化的節點,
本來就不該無聲消失。

### 5.5 三條 undo 線

```
(A) 專案編輯      → 編輯開著的 .chg(文件編輯歷史,app 無需 undo stack)
(B) 演化 commit   → app history stack + active-node 移動(§5.3–5.4)
    views/data    → 一般文件編輯歷史
```

可以都由 app 提供,但**不能假裝是同一條線**。

---

## 6. 失效與快取

> **本節為 v0.2 全面改寫。** v0.1 寫「State 變了 → 什麼都不必重算」,**是錯的**。
> 誌於 §9。

### 6.1 State 失效必須拆兩類

v0.1 的理由是「State 只影響下一次生成什麼」。**那恰好是必須失效的理由**:
既然下一次生成會變,生成面板就必須重算。而 §1 表裡的「候選詞面板」正是
`generate::ranked()`,其權重經 `ContactInfluence` 讀 State 而來。

| 類 | 內容 | State 改變 |
|---|---|---|
| **Replay-derived** | lexicon、diff、已固化節點的 Language、manifest digest | **不失效**(裁定 A:replay 不讀 State) |
| **Authoring-derived** | `ranked()` 候選與排序、generator preview、未提交的 goal selection、**有向互通度**(§4.1) | **必須失效** |

正確表述:

> **State 不使任何已固化節點、manifest digest 或 replay-derived view 失效;
> 但使依賴 State 的 authoring proposal/cache 失效。**

### 6.2 快取身分 ≠ 依賴失效

v0.1 提「以輸入為鍵」,方向對但不足以實作:「某節點的 Language 變了 → **所有**與它
比較的 diff 失效」在 N 個節點下是 O(N) 的 pair cache 觸碰。

本專案的節點是 immutable 且**內容定址**(`objects/<sha256>` 已實作),
故**不必維護「誰受誰影響」表**,只要 key 完整:

```
LexiconKey = (language_digest, filter_digest, view_digest)
DiffKey    = (language_digest_a, language_digest_b, diff_config_digest)
GroupKey   = (graph_digest, measure_id, threshold, override_digest)
```

輸入 digest 改變 → 自然 miss,舊項之後垃圾回收即可。故拆成兩件:

| | 用途 |
|---|---|
| **Cache identity** | 正確性。完整 input digest tuple 為鍵 |
| **Dependency invalidation** | **只**用於 UI subscription(該重畫哪個面板),不負責正確性 |

### 6.3 `search` 不得偷偷成為第二個有狀態的東西

線性掃描是純函數;索引不是(有狀態、需失效、可能持久化、wasm 下不宜全量重建)。
故明定:

```
MVP     : 純線性掃描,住 conlang-query
未來    : 可選索引,住 conlang-app 的快取層——不得回流進 query
```

### 6.4 `data/` 與 package 是**不同等級的事件**

v0.1 把兩者寫成同一列(「抽樣相關」)。錯得不只是分類:

| 事件 | 影響 |
|---|---|
| **generator `data/` 改變** | authoring proposal cache 失效;已固化 Language 不變 |
| **鎖定的 package digest 改變** | **不是失效,是拒絕**——`.chg` 的第三道 digest(library lock)在 replay 前就會擋下(步驟 14 已實作)。需要重新 resolve / migrate / 建立新基線 |
| **`views/<name>` 改變** | 只有該 view 的 `GroupKey` 改變 |

把 package 改版與「調一個推薦權重」寫成同一列,會誤導成同等級事件。
實際上前者現行機制已**主動拒絕**,不容默默重算。

---

## 7. 待裁定(D-a–D-f,v0.2 重開)

| # | 議題 | 傾向 |
|---|---|---|
| **D-a1** | `conlang-query` 的公開 API 是否禁 fs / 時鐘 / 亂數 / 隱式全域狀態(即納入 §4)? | **是** |
| **D-a2** | 立即物理拆 `query`/`app` 兩 crate,還是先 module 邊界? | **立即拆**——§2.1:§4 是 crate 粒度閘門,module 內的「純」無法驗證 |
| **D-b** | Command 是否只產生 `edits/preview`,ChangeSet 由 commit 建立(而非「一 Command = 一 ChangeSet」)? | **是**;§3.3 顯示三層已實作,只需別再等同 |
| **D-c0** | (A) 專案編輯的 pending 狀態是否就是**一份使用者可見、可命名的 `.chg`**(而非 app 內部緩衝、更非新的 `working/` 格式)? | **是**——§5.2:`.chg` 已表達「基底 + statements」,另立格式違反單一資訊源 |
| **D-c1a** | commit(產生節點)與 persist(寫入 store)是否分離,UI 延後 `save` 到顯式「儲存」? | **是**,但只為省事,不再是 undo 的前提(§5.4) |
| ~~D-c1b~~ | ~~已落盤的節點被 undo 後,標記記在哪?~~ | **問題取消 2026-08-04**:它預設了「刪不掉」,而那是 `save` 的缺陷不是設計前提。缺陷已修——葉節點真的刪得掉(§5.4.1),不需要「已放棄」標記 |
| **D-c2** | (B) 的 undo 由 app history stack 承擔(而非 graph parent 遍歷)? | **是**——§5.1:children 在現行結構中不可查 |
| **D-c3** | `views/` 與 `data/` 編輯是否走**獨立**的第三條 undo 線? | **是**;不進引擎 undo 線 |
| **D-d** | 派生視圖快取是否以完整 input digest tuple 為鍵,事件失效只用於 UI 通知? | **是**(§6.2) |
| **D-e1** | 互通度接口是否納入 `context`(方向、`contact_history`)? | **是**;連帶把它歸入 authoring-derived(§6.1) |
| **D-e2** | 官方是否提供**具名** heuristic profile(公式走 Registry、係數走 package data)? | **是**,依 P30 切開 |
| **D-e3** | 結果是否必須攜帶 `measure_id`? | **是**(同 `provenance()` 之例) |
| **D-f1** | 分群是否為可替換 strategy,MVP = `TreeEdgeCut`? | **是** |
| ~~D-f2~~ | ~~merge 取傳遞閉包嗎?split 優先於 merge 嗎?~~ | **已裁定 2026-08-04:改用分類指派語意,問題消失**(§4.2)。merge/split 是關係運算故會互相矛盾;指派是函數,結果由建構保證唯一 |
| **D-f3** | `assignments`(身分)與 `labels`(顯示)是否分成兩類? | **是**;指派語意下更自然 |

---

## 8. 建議順序(v0.2 調整)

原案把互通度/方言群組排在 Command API 之前。**改為先封應用層主幹**——
互通度與分群是最不確定的語言學視圖,而 command/commit/history 是 UI 與專案存儲的地基。

1. **Query 型別 + 純函數組裝**——lexicon / stats / derivation_family(§1.1 兩段)/ diff;
   出口:同輸入同輸出、Override 只影響視圖**不進 digest**;
2. **CommandResult / statement / commit 邊界**——三類 Command 分型,
   `LanguageCommand` 降階四原語;出口:每個 `LanguageCommand` 留下可 replay 的 `.chg`;
3. **(A) 的 `.chg` 工作副本 + (B) 的 active-node history**(§5.2–5.5 三條 undo 線);
   節點移除的兩側 API 已於 2026-08-04 補上(§5.4.1),此步只需接線;
4. **內容定址 query cache**(§6.2);
5. **互通度接口**(§4.1,含 `measure_id` 與 authoring 失效歸屬);
6. **分群 strategy + Override**(§4.2,MVP `TreeEdgeCut`);
7. **步驟 22 UI**(Tauri/WASM),步驟 20 欠的「State 的 UI 顯示」在此還。

1 完全不碰寫入路徑,可獨立驗證;2–3 才動到 Command 與歷史,風險集中在那兩步。

---

## 9. 誌誤(v0.1 → v0.2)

| # | v0.1 原文 | 實情 |
|---|---|---|
| 1 | 「State 變了 → **什麼都不必重算**」 | 與本檔 §1 表自相矛盾:候選詞面板 = `generate::ranked()`,其權重經 `ContactInfluence` 讀 State。「只影響下一次生成」**正是**生成面板必須重算的理由 |
| 2 | 「undo = 移動當前節點指標到 parent;redo = 指標移回」 | `EvolutionGraph` **無 children 索引**(`evolution.rs:139`),節點 id 由 parents 算出;redo **不可實作**。且 `parents` 是 `Vec`,undo 本身已多選一 |
| 3 | 「每個 command 都留下可 replay 的 `.chg`」 | 與同節的 `set_view_config`/`set_override` 直接衝突——那兩個不產生 `PrimitiveEdit`。Command 必須先分類 |
| 4 | 三條 undo 線按**機制**分(commit / pending edit / views·data) | 應按**使用者活動**分。原分法漏掉 (A) 專案編輯——設計期加 50 個詞不該變成 50 個演化節點,而原文沒有它的位置(§5.2) |

另有兩處**不足**(非錯,但寫得像已解決):
`IntelligibilityMeasure` 只拿 `DiffVector`,裝不下它自己承諾的有向版(§4.1);
`dialect_groups` 的「閾值分群」在非傳遞的互通度下不是唯一算法(§4.2)。

共同成因:**把「已有零件」誤當成「已有設計」**。零件確實都在(diff、origin、
SenseEdge、immutable node),但接合處的語意沒定就寫成了現況表。

### 9.1 兩次「把缺陷當成前提」(v0.2 草擬期)

**① 「已落盤的節點刪不掉,只能雜湊外標記 + 顯示層過濾」。**
我查到 `persistence` 沒有 delete API、`save` 是 append-only,就把現況當成設計前提,
還替它想好了標記要記在 `NodeConfig.preferences`(原 D-c1b)。

擁有者一句「這很明顯就是個 bug 要改」點破:**沒有 delete API 不是約束,是缺口**。
`save` 明明收整張圖卻對移除視而不見,`load` 又以目錄內容為準——兩者合起來讓
「撤銷一次演化」靜默失效。修掉之後 D-c1b 這個問題本身就消失了(§5.4.1)。

毛病與 `演化專案結構與套件載入_v0.1.md` §6 記的三次同型:**先替現狀構造理由**。
差別是這次現狀本身就是錯的,於是理由構造得越周全,離正解越遠。

**② 從需求直接跳到新機制。**

發現 (A) 專案編輯無位置後,曾主張「pending buffer 要跨階段存活 ⇒ R1–R6 缺一個
`working/` 槽」。**錯**——`.chg` 已經表達「基底 + statements」,且零條 statement
的 `.chg` 合法。新格式不但多餘,還會違反單一資訊源。

毛病是**從需求直接跳到新機制,漏掉「先問既有格式能不能表達它」這一步**。
與 `演化專案結構與套件載入_v0.1.md` §6 記的三次同型:先有結論再找理由。
