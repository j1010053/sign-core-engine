# 架構修補 10 — 歷時 function 層(Recipe/Goal)與其載入(P47–P55)

> **決策來源**:本檔提出並定稿 **P47–P55**；現行權威為《架構修補彙整 05–11》§1。
> 承 P16(Atomic Rewrite 定案 12 項)、
> P20–P28(《修補05》primitive 與檔案格式)、**P29–P34**(《修補06》插件/服務)、
> I22(colon+縮排)、P46(《修補09》phon 命名 block)。
> 定案時間 2026-07-27;步驟 15 三刀(15a/15b/15c)已落地為本檔的實作基礎。

## 0. 修正目的

《修補05》§9 的分層記法是**草案**(原文自註「真正 `.chg` 表面語法須另案決定」),
且**自相矛盾**——同一件事在 `.chg` 頂層寫 `rewrite reanalyze(…)`、在 Recipe body 裡
卻寫裸 `reanalyze(…)`。若照抄,步驟 16–17 落地時得維護兩套解析。

本檔把層②③④ 的**呼叫、定義、載入**三面一次定死,並以既有機制為準——
**不新增載入機制**(P29 已定 auto-discovery)、**不新增關鍵字**(P29 已定 Recipe/Goal
非關鍵字)、**不新增檔案格式**(沿用 `.chg` + 套件三層目錄)。

---

## 1. 呼叫面【P47】

**層① 是語句,層②③④ 一律是函數呼叫。**

```
# 層①(步驟 14 已封板,一字不動):關鍵字開頭、無括號
update sign("go").syn.category = aux
delete sign("kobo")

# 層②③④:一律 name(位置參數, key: value, …)
reanalyze(sign("go"), target: category, to: aux)   # ② 內建 12 項
VerbToTense(sign("go"), tense: FUTURE, result_category: aux) # ③ Recipe
Future(sign("go"))                                 # ④ Goal
```

依據:《總鳥瞰》裡層②③④ **本來就都是函數**(Rewrite→`Vec<PrimitiveEdit>`、
Recipe→`Vec<AtomicRewrite>`、Goal→`Vec<Recipe 候選>`);只有層① 是機器讀的原語。

- **層級由「名字解析」決定,不靠關鍵字前綴**。故 Recipe/Goal 落地時 **parser 零改動**。
- 帶整個 sign 的呼叫(`create`/`lexicalize`/`adopt`)尾接 `:` + 縮排 `.lang` block,
  形狀比照既有 `insert into …:`。
- **降階與 `clone` 同構**:呼叫只活在**未解析層**,`resolve` 即降成 `Vec<PrimitiveEdit>`;
  **`ResolvedChangeSet` 維持 primitive-only**(步驟 14 契約),dump/round-trip/三道 digest
  全部沿用,零改動。
- **`.lang` 的 sign 引用(application)具名參數一併統一為 `key: value`**——構式套用
  本來就是「帶具名參數的函數呼叫」。舊 `key = value` 仍接受,canonical printer 排 `:`
  (非 canonical 正規化為不動點,P21 既有契約)。

> **已實作**(步驟 15c + 後續一刀):`changeset/src/call.rs`、
> `changeset/tests/rewrite_calls.rs`、`language/tests/application_named_arguments.rs`。

---

## 2. Recipe/Goal 的身分【P48】

**Recipe/Goal 不是語言概念,是 `code/` 的檔案分工**(承 P29「Goal/Recipe 只是歷時
function layer code,非關鍵字」)。`code/recipes.chg`、`code/goals.chg` 只是人類的
歸檔方式,parser 不認得這兩個詞。

**body 的執行語意由既有的 `case`/`when` 承載**,不新增 layer 標記:

| body 形狀 | 語意 | 慣稱 |
|---|---|---|
| 純序列(無 case/when) | **依序全跑** | Recipe |
| `case:` | **第一個 Matched 的分支**(`CaseSelection::FirstMatch`) | 確定性分支 |
| `when:` | **所有 Matched 依序合併**(`CaseSelection::Accumulate`) | **Goal 的候選列舉** |

`when:` 對 Goal 特別貼合:Goal 的純函數半契約是
`Goal.candidates(…) → Vec<(Recipe, weight)>`——回傳**所有**符合的候選,不是第一個。
**抽樣是呼叫時的引擎行為,不寫在 body**(《修補05》§9:「Goal 的隨機為隱含語意」)。

- **function 之間可互相呼叫**(層級靠名字解析,無額外程式碼)。
- **必須偵測循環呼叫**(A→B→A)並明確報錯——這是**終止性**要求,與分層無關。
- 若日後真需要顯式層標記,以 `@layer goal` **選填、預設 recipe** 加入(`.lang` 既有
  `@name`/`@stage` 後綴慣例),既有檔案零改動。

---

## 3. 定義語法與居所【P49】

定義住在**套件的 `code/*.chg`**(P29 三層目錄),語法全部沿用既有慣例,零發明:

```
package std:grammaticalization:
    schema = conlang.functions/v1

function VerbToTense(verb [Verb], tense, result_category):
    drift(verb, sense: core, gloss: tense)
    reanalyze(verb, target: category, to: result_category)
    entrench(verb, delta: 0.3)
    sound_change(verb, body: "V => * / _ #")
```

| 記號 | 出處 |
|---|---|
| colon + 4-space 縮排 | I22(已取代 `{ }`) |
| `verb [Verb]` 參數約束 | `.lang` slot 寫法(`agent [Nominal]`);授權走既有 `belongs` 閉包 |
| `name(args)` / `key: value` | P47 |
| `/ guard`(需要時,置於 header 尾) | `.lang` 規則既有(`field => value / guard`) |

**參數約束取代大部分 guard**:`verb [Verb]` 即「這條路徑只吃動詞」,不必再寫
`/ verb.syn.category == verb`;guard 留給參數型別表達不了的條件。

**定義文件模式**:定義是**函數,不綁特定 Language**,故套件內 `code/*.chg`
**沒有** `base_source`/`base_identities`——以 `package` 頭與可 replay 的 `changeset`
頭區分,解析時只收 `function`、不收 `statement`。

---

## 4. 載入:沿用 P29,**否決顯式 import**【P50】

**auto-discovery,無 `import`/`use` 語句**(P29 原文)。理由與既有機制:

- 可重現性(P26)**已由既有 library lock 解決**:`.chg` prelude 早有
  `library <pkg>@<ver> sha256:<digest>`,且 **std 套件自動全載自動入鎖**。
  recipe 套件只要是套件,digest 自動被釘 → 同一份 `.chg` 不可能配到不同的 recipe 內容。
- **ChangeSet 永不引用套件內部路徑**(P29):引用走 **export 表的穩定 ID**。
- **MVP 為編譯期靜態註冊**(P31,WASM-safe):`include_str!` 嵌入,與現行 std 套件同構;
  兩個 crate 維持**無檔案 IO**。
- 衝突:**priority 四層**(未啟用 < std < plugin < 本地);同名同 priority →
  **warn + 強制消歧 `套件::符號`**(P29,非靜默選一,亦非一律報錯)。

> **明確否決**:本檔草擬階段曾提出 prelude 加 `import <ns> sha256:` 與呼叫端
> `DefinitionSource` trait,**與 P29/P31 抵觸,已捨棄**。留此記錄以免重蹈。

### 4.1 現況缺口(必補四項)

| # | 缺口 | 現況證據 |
|---|---|---|
| 1 | `LibraryPackage.data_path` 是**單數** `String`,而 `code_paths` 是 `Vec<String>` | grammaticalization 需要 `paths.tsv`(+日後 weights.tsv),塞不進去 |
| 2 | `LibraryExportKind` 只有 `Trait`/`Sign` | function 無法 export → auto-discovery 找不到 |
| 3 | `code/` 只 `include_str!` 載 `.lang` | P29 明寫 `code/ *.lang + *.chg`,`.chg` 那半未實作 |
| 4 | `.chg` parse 強制要 prelude 三 digest | 定義檔無 base(§3 定義文件模式) |

四項都是**補既有機制的洞**,非新架構。

---

## 5. Recipe 展開:**接力**,非快照【P51】

Recipe body 的多步 **逐條展開並套用到暫存文件,再展下一條**。

**否決快照展開**(全部對同一 base 算)的理由:寫不出 H&K 路徑的真實語意——
`GO→未來` 的**磨損**作用在**重分析之後**的助動詞形式上;且若某條 recipe 先 `create`
一個 sign,後續 rewrite 必須引用它。

**仍是純函數**:同樣的 `(params, Language)` 進去,出來的 `Vec<AtomicRewrite>` 相同,
只是內部以暫存文件推進;golden 照常可做(《總鳥瞰》line 171「固定 Language fixture」)。

---

## 6. 路徑庫 = code 機制 + data 路徑表【P52】

docs/08 的「Heine & Kuteva 斜坡路徑資料庫,30–50 條起步」**不是 30–50 個 function**。
30–50 條路徑(GO→未來、WANT→未來、COME→未來…)**機制完全相同**,只有來源概念與
目標語意不同。依 P29 的 code/data 判準(「會被 compile 的在 code;只被查表讀值的在 data」):

```
std/grammaticalization/
    code/recipes.chg    ← 路徑機制：參數化 sequence function
    code/goals.chg      ← 功能入口：`when:` 候選 function
    data/paths.tsv      ← 30–50 條:來源概念 / 目標語意 / 預設 δ …
```

**官方加新路徑 = 加 data 一行**,滿足 P29「不改引擎」且比「加 .chg 檔」更省。

---

## 7. 外部服務接點:`expand` 帶 `ServiceContext`【P53】

現行 `expand(rewrite, document) → Vec<PrimitiveEdit>` 是**同步純函數**,與 P30/P33/P34 相撞:

- **P30**:SemanticBackend **必後端** —— `drift` 的新語意值本該由它算(15b 暫由呼叫端傳)。
- **P33**:要求「允許語句中途暫停、commit 仍整句原子」——同步純函數**沒有暫停點**。
- **P34**:首次執行呼叫服務並記入 History;**replay 一律讀 History 不重呼叫**。

**裁決:現在就把簽名開好,實作留空。**

```rust
pub fn expand(rewrite, document, services: &ServiceContext) -> Result<Vec<PrimitiveEdit>, _>
```

`ServiceContext` 現階段 = 「只含 History 查表、無 live 呼叫」的空殼(零功能)。
**理由**:不現在定,將來 12 項的簽名要全部改一次,連同 P47 的降階入口與屆時已寫好的
所有 recipe。加參數是廉價的;改 12 個簽名 + 既有呼叫端不是。

---

### 7.1 `fuse` 的 component 引用【P54,已補】

稽核步驟 15 時發現:`fuse` 只把 `left` 記進 `origin`,`right` 僅被驗證存在後丟棄,
故 `fuse(a,b)` 與 `fuse(a,c)` **產出完全相同**——《修補05》§4.3 要求的
「component 引用」形同未實作。當時判定補欄位屬架構層而**暫不擅自發明**,
改以測試釘住缺口(補上時會主動失敗)。本次回寫補上:

- `SignDef::components()` / `with_components(&[SignRef])`,存為頂層 metadata Def
  `components = sign(a), sign(b)`(比照既有 `origin`/`provenance`/`lifecycle`)。
- 驗證:**至少兩個**成分,少於兩個明確診斷(單一來源該用 `origin`)。
- `fuse` 同時寫入 `origin`(左成分)與 `components`(兩成分)——兩者職責不同。

> 展開 golden **不受影響**:golden 釘的是「用了哪個原語、作用在哪種節點」,
> 加 metadata 不改變 `insert Sign into Language at End` 這個形狀。這正是當初
> 刻意不把節點內容寫進 golden 的用意。

---

## 8. P 系列決策(本文件新增)

| # | 決策 |
|---|---|
| **P47** | **層①=語句、層②③④=函數呼叫**:一律 `name(位置參數, key: value)`,**層級由名字解析決定不靠關鍵字**;呼叫是未解析層的糖,resolve 即降成四原語,`ResolvedChangeSet` 維持 primitive-only(P26/步驟14 契約不動);`.lang` sign application 具名參數一併統一為 `key: value`(舊 `=` 接受、printer 排 `:`) |
| **P48** | **Recipe/Goal 非關鍵字,是 `code/` 檔案分工**;body 語意由既有 `case`(選一)/`when`(收集候選)/純序列(全跑)承載,**不新增 layer 標記**;function 可互相呼叫,**必須偵測循環**;日後若需顯式層標記,`@layer` 為選填、預設 recipe |
| **P49** | **定義住套件 `code/*.chg`**,語法 = `function Name(參數 [約束]) [/ guard]:` + 縮排 body;參數約束沿用 slot 寫法並取代大部分 guard;**定義文件模式**(`package` 頭、無 base digest、只收 function 不收 statement) |
| **P50** | **載入沿用 P29 auto-discovery,明確否決顯式 `import`**;可重現性由既有 `library …@ver sha256:` lock 提供(std 自動入鎖);引用走 export 穩定 ID 不碰內部路徑;MVP 編譯期靜態註冊(P31),crate 維持無檔案 IO;衝突走 priority 四層 + warn 強制消歧。**必補四缺口**:data 路徑複數化、`ExportKind::Function`、`code/` 收 `.chg`、定義文件模式 |
| **P51** | **Recipe body 接力展開**(逐條展開並套用到暫存文件),**否決快照展開**;仍為純函數,golden 可做 |
| **P52** | **路徑庫 = 1 個參數化 function(code) + 30–50 條路徑表(data)**,非 30–50 個 function;依 P29 code/data 判準;加新路徑 = 加 data 一行 |
| **P53** | **`expand` 現在就加 `ServiceContext` 參數**(實作留空):外部服務(SemanticBackend)、暫停恢復(P33)、History record–replay(P34)的統一接點;避免日後一次改 12 個簽名 |
| **P55** | **`.chg` 表面語法收斂**:①**語句標記 `statement N:` → `#N:`**(`#` 讀作編號,去除每筆交易的冗詞);②**註解統一為 `/* … */`**——`.qy` 與 `.lang` 早已如此(擁有者 2026-07-12 定案,因 **`#` 在 `.qy` 被詞界 D19 佔用**),只有 `.chg` 用 `#` 當註解,三者不一致;改註解後 `#` 正好空出來當標記。序號**必須保留**(P34 History 側表以「語句序號+呼叫序」為鍵)。舊形 `statement N:` 仍接受、dump 一律排新形(非 canonical 正規化為不動點,同 P47 對 `.lang` 具名參數的作法)。動到步驟 14 已封板的表面語法,但**契約語意零改變**(交易邊界、三道 digest、primitive-only 全不動) |
| **P54** | **sign 增 `components` metadata**(至少兩個 `sign(x)`,逗號分隔):兌現《修補05》§4.3 對 `fuse` 的「component 引用」要求。與 `origin` **職責不同**——`origin` 是**單一**來源(衍生自誰,split/adopt 用),`components` 是**線性組合的各成分**(au = à + le)。缺它時 `fuse(a,b)` 與 `fuse(a,c)` 產出相同(第二成分只驗證存在就丟掉);驗證拒絕少於兩個成分(單一來源請用 `origin`) |

---

## 9. 相容性

- **步驟 14 契約**:`ResolvedChangeSet` 仍 primitive-only;dump/round-trip/三道 digest 零改動。
- **P29–P31**:完全沿用(本檔不新增載入機制;四缺口是補洞)。
- **P34 意圖/事實分離**:docs/08 的「macro↔primitive 雙向映射是一等資料」由
  **Evolution_node 側表**(步驟 16)承載,**不進 ChangeSet 本文**——與 P47 的
  primitive-only 不衝突。
- **P16**:12 項 Atomic Rewrite 仍是**封閉內建集**;使用者/plugin 可寫的是
  Recipe/Goal 層(本檔),不得新增 rewrite。
- **引擎(tshiatūn)**:phon-only,與本檔完全無關,零觸動。

## 10. 已知技術債(非阻礙,但已量測)

`apply_edit` 對**每個 primitive** 都做全量 `dump()` + `parse()` + `check_document`,
成本 **O(文件大小)**。實測(release):

| signs | 每次 edit |
|---|---|
| 200 | 8.3 ms |
| 500 | 16.5 ms |
| 1000 | 31.1 ms |

乾淨線性(≈0.03 ms/sign/edit)。外推 10,000 signs ≈ 310 ms/edit;一條 4 步 recipe
(接力,約 6–8 原語)≈ 2 秒;演化樹 replay 100 節點 ≈ 分鐘級。

**非架構阻礙**,修法明確且局部:把驗證由「每個 primitive」改為「每個 **statement 交易**」
(步驟 14 已有交易概念),並以增量維持 canonical 取代 dump+reparse。
步驟 16–17 開發不受影響;**M3 自動演化 + E 抽樣前必須做**。

## 11. 待裁(不阻礙本檔實作)

1. Goal 的 `seed` 放 prelude(整份共用)還是每個呼叫各帶?——做 Goal 時再定。
2. 定義檔要不要也鎖 library(guard 引用 ontology 範疇)?——建議**不驗**,guard 到
   invoke 時才在實際 base 上求值,定義檔即完全 base-independent。
3. Weight DB 的資料格式與 E1 先驗的關係——屬模組 E(docs/10),設計層。
