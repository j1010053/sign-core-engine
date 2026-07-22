# M1+ 共時完整性、英語格位與歷時入口（v0.1）

> **狀態**：Step 13 source interface 與 Step 14 Primitive-only ChangeSet 已分別由
> docs/20、docs/22 實作；本文保留需求推導與共時邊界。`syn: slot_features:` 現可對
> stored sign 與 derived token 執行 occurrence-local constraint、由 deep/base 狀態重跑
> Syn→Sem→Prag，再重選 realization。Atomic Rewrite、Recipe、Goal、Evolution node 與
> History 仍不在本階段。
>
> **目的**：把已封板的 M1++ 共時能力，與 M2 歷時層真正需要的可編輯來源
> 介面分開。英語格位段落只定義 occurrence-context 的契約；它不是新的 `.lang`
> 關鍵字，也不是把格位寫回詞彙 sign 的方案。

---

## 1. 權威與術語

《架構2.0總鳥瞰》Step 13 現已依《架構修補05》P23 更正；早期曾列出的
`insert/delete/replace/rename/move/set/unset` 七個名稱不是七種權威原語。實作契約為：

```text
insert(parent, anchor, subtree)
delete(node)
update(node, field, value)
move(node, new_parent, anchor)
```

`rename`、`set`、`unset` 是 `update` 的受限形；`replace` 要明確選擇是保持
身分的 `update`，還是有生滅語意的 `delete + insert`。四原語只操作 **Language
source**，絕不操作 `CompiledGrammar`、`CompiledSign`、`DerivedToken` 或 phonological
surface。

《架構修補07》P39 的 `Patch` 仍有其位置：它是四維 local Def 的 typed、不可變
`update` facade。它不能取代結構性 Primitive Edit，因為它不能插入／刪除／搬移
sign、trait、block、rule、slot、role 或 feature declaration。

資訊流 B 的完整方向固定為：

```text
Goal -> Recipe candidates -> AtomicRewrite -> PrimitiveEdit -> Language'
     -> recompile -> Compiled Grammar' -> runtime -> Surface'
```

上三層只展開、最底層才執行。Step 14 已實作 Primitive-only ChangeSet interpreter；
Goal、Recipe、Atomic Rewrite 仍未實作。本文件主要說明它們共同依賴的
`PrimitiveEdit -> Language'` 邊界。

---

## 2. 已有的共時基礎

下表是「可作歷時輸入」的能力，不把它誤寫成歷時功能已完成。

| 能力 | 現有共時介面 | 對歷時的意義 |
|---|---|---|
| source root | `Language::new()` 的 canonical empty root | 從空 Language 建立分支時有合法掛點。 |
| source AST | `Language`、Trait、Block、Sign、Def、Rule、slot、feature、role、realization、metadata | Primitive 會改的是真正語言知識，而不是 compiled artifact。 |
| canonical text | `Language::parse`／`Language::dump` | 歷時前後狀態可讀、可比較、可再 compile。 |
| compile/runtime | `compile_system`／`compile_with_libraries` 與 derive/phon runtime | 可檢查 source edit 是否真的改變共時導出。 |
| 四維內容 | phon/syn/sem/prag projection、規則、construction token、Sem roles | 歷時 edit 有明確的內容靶點，不能任意跨維寫入。 |
| validation | `ValidationReport`、ontology、feature、role、construction、rule、origin 檢查 | 可作未來 `check_language` 的既有檢查來源。 |
| 局部 patch/diff | `Patch::{phon,syn,sem,prag}`、`Patch::apply`、per-dim `diff` | 可重用為 `update` 的一個子案例。 |
| source provenance | Rule namespace、RuleRecord、部分 source line、package provenance | 歷時 trace 可以沿用既有 rule/source 資訊。 |
| occurrence-local slot feature | `syn: slot_features:` + frozen probe + stored/derived occurrence re-evaluation | construction 可將已驗證的 syn enum constraint 給單次 filler occurrence；stored sign 從 effective base、derived token 從 deep baseline 重跑，皆不寫回來源。 |

共時資料與衍生資料必須維持這條界線：

```text
Language source --compile--> CompiledSystem --derive--> DerivedToken/surface
       ^                                                  |
       +---------------- PrimitiveEdit only --------------+
```

`DerivedToken`、`RealizedPhonInput` 與 surface 都是一次推導的暫態觀察值；它們不能
成為 Primitive Edit 的隱藏寫入目標。

### 2.1 2026-07-22 M1+ 完整性稽核

本輪不是只看 surface。稽核逐項追過 source/IR、繼承與 selection、狀態轉移、四維
token、provenance/trace、serialization、公共 API 與擴大回歸。結果如下：

| 面向 | 判定 | 證據或尚缺契約 |
|---|---|---|
| parser／printer／library IR | 通過 | feature、role、realization、`$self`、`slot_features`、namespace 與 library selection 均有 round-trip／反例。 |
| ontology／effective Sign | 通過 | 修正 generic Def 與 typed feature 的單一優先序、繼承 slot 可見性、非-global std trait 驗證及 warning 保留；循環、重名、衝突皆為 coded diagnostic。 |
| 四維 construction/runtime | 通過 | filler rules → construction → Syn/Sem/Prag token rules → pure phon input → Tshiatūn；Sem projection、optional slot 與 enum runtime domain 已補反例。 |
| English direct stored-filler case | 通過（窄契約） | 動詞 assignment → occurrence `slot_features` → 同一 `she` sign 的 `she/her` realization；snapshot、source immutability、SlotMap rename、衝突與決定性皆驗證。 |
| occurrence assignment trace | 部分完成 | binding 的 source line、結果 snapshot 與 filler source provenance 均保留；`SystemDerivation` 尚無一筆 first-class record 直接串起 predicate assignment、binding 與 target occurrence。 |
| derived-token feature forwarding | 未完成 | 外層不能把新 occurrence constraint 向下送入已形成的 derived token；目前明確拒絕，不靜默產生錯誤 surface。 |
| contextual filler rules | 未完成 | occurrence feature 注入後不重跑 filler 的 Syn/Sem/Prag 規則，只重選 read-only phon realization。 |
| typed Patch 嚴格性 | 部分完成 | `try_set`／`try_unset`／parser 為 fallible；便利 API `set`／`unset` 對動態非法 path 仍會 panic。`diff` 保證 local dimension 的觀察值相等，不保證 AST 表示、重複項與原順序逐位元相同。 |
| valence 契約 | 部分完成 | construction runtime 以 typed slots／residual slots 為 valence；generic Def 仍可寫入 legacy `syn.valence = 2`，validator 尚未禁止它冒充可執行 valence。 |
| phon source trace | 部分完成 | compiled global phon source map 可回到 rule／branch `.lang` 行；推導時動態排放的 sign-local phon rules 尚未把新 source map 接回 `SystemDerivation`。 |
| 本機完整閘門 | 基礎設施未齊 | language 179/179、Tshiatūn 157/157、MSVC rustfmt check 通過；GNU preflight 因缺 rustfmt、Clippy、WASM target 正確回報 exit 2，不能宣稱全閘門 exit 0。 |

因此目前可說「M1/M1++ 主 runtime 與本輪英語格位窄契約回歸綠燈」，不能說「所有
M1+ 擴充與進入歷時所需 source interface 已封板」。

---

## 3. 第一版歷時編輯的資料範圍

### 3.1 只編輯 caller Language

`CompiledSystem` 同時保留：

- `language()`：caller 所提供、可序列化的 Language source；
- `effective_language()`：dependency/std/natural/plugin overlay 加入後、經 compile
  pass 使用的有效輸入。

**M2 第一版的 ChangeSet 只能修改 caller Language。** 它不得直接修改
`effective_language()`，也不得原地修改 `std:*`、`plugin:*` 或
`natural:en-standard` 的 embedded package source。

理由如下：

1. effective language 是載入、繼承與排序後的衍生視圖；改它會把 compile artifact
   當 source；
2. library package 有自己的 namespace、version 與 export 契約，不能由某一個專案的
   Evolution node 靜默覆寫；
3. 第一版 replay 必須只有一份明確、可持久的被改寫 source，才能比較 `Language`
   與 `Language'`。

若要以標準英語參考庫作祖先，專案必須先擁有自己的 caller Language 基底（手寫、
明示複製，或未來的 materialize/import 操作）。直接對
`natural:en-standard` overlay 套 ChangeSet 不在第一版範圍。

### 3.2 不是歷時 target 的資料

- Compiled grammar/program、phon source map、cache；
- construction 的一次 `DerivedToken`、filler snapshot、occurrence context；
- Tshiatūn 的 Word、步驟記錄與 surface；
- UI annotation、時間、文本出處、社會資料與外部 State；
- std/自然語言參考套件的內嵌 source。

它們可由 source、ChangeSet、library selection 或 project State 重建，但不作
Primitive 的直接持久化寫入面。

---

## 4. Step 13 已補齊的 source interface

M1++ 已有資料內容；Step 13 補上「可安全改寫」的來源 API。下列項目是實作所採用的
最小契約；完成證據與目前公開 API 見文件 20。

### 4.1 持久且可比較的身分

目前 `SignId`／`RuleId` 在單一 in-memory Language 內有決定性配發，但 canonical
dump 不攜帶它們，且 printer 會規範化具名容器順序。這不足以支援歷時 replay 的
身分保持。

歷時 source interface 必須定義：

- 穩定的、node-scoped ID allocation（例如 `EvolutionNodeId:counter`）；
- `update`／`move` 後未改節點保留同一 ID；
- `delete + insert` 產生不同 ID；
- 至少可定址的 Sign、Trait、Block、Rule、Def、Slot、Feature、Role、Realization
  branch 等節點身分；
- ID 與 canonical Language/replay snapshot 的持久化策略。

最後一點必須明確選擇：要嘛 ID 成為 canonical source 的一部分，要嘛 Evolution
node 保存一個與 source 一起重播、且不靠 vector index 重建的 identity ledger。不能
依名稱或 printer 排序後的位置猜回身分。

### 4.2 Ref 必須指向穩定身分

P24 要求引用是節點欄位裡的 Ref 值，而非另一套可變圖邊。現有 `origin` 與
`Component::Sign` 仍以名稱字串解析，rename 後可能懸空；其他 Ref wrapper 也尚未有
完整 source resolver。

Primitive 前必須把下面規則定死：

- Ref 指向 stable ID，或有等價、可重播的顯式 resolver；
- rename 只改顯示名稱，不能改變 referent；
- edit 後檢查懸空 Ref、origin chain cycle、component DAG cycle；
- library/external Ref 的 namespace 與第一版不可修改套件邊界一致。

### 4.3 Path resolver 與 anchor

既有 `parse_path` 只驗證語法。歷時還需要一個 typed resolver：

```text
Path / NodeRef -> resolved node + parent + editable field kind
```

它必須拒絕未知、歧義、跨容器或型別不符的 target。序列插入使用：

```text
Anchor = Start | End | Before(NodeId) | After(NodeId)
```

不可用純 index；前一條 replay edit 的 insert/delete 會令 index 漂移。Block 是可被
操作的語義節點，不是 `==` 的字面分隔符。

### 4.4 四原語的純函數面

第一版至少需要等價於下列 Rust 層介面；這不是 `.chg` 表面語法草案：

```text
apply_edit(source, edit, allocator) -> Result<Language', EditError>

PrimitiveEdit =
  Insert { parent, anchor, subtree }
| Delete { node }
| Update { node, field, value }
| Move   { node, new_parent, anchor }
```

要求：

- 不可變輸入：失敗時原 Language 逐位元不變；
- `Update` 僅改被指名 field，保留 node ID；
- `Move` 保留整個子樹 ID；
- child kind、parent kind、anchor ownership 和 Ref 型別都先驗證；
- Patch 的 cross-dimension isolation 必須保留；結構性 edit 也不得成為繞過四維
  validation 的後門。

### 4.5 獨立的 `check_language`

`compile_system` 可作最終共時可執行性檢查，但 Step 13 不能把「嘗試 codegen」當成
唯一 AST invariant checker。需要公開、無副作用的：

```text
check_language(&Language) -> ValidationReport
```

至少覆蓋唯一 ID/name、Ref 完整性、圖無環、trait/block 完整性、slot/feature/role
schema、rule stage、可編輯 path，以及現有 ontology/construction 規則。它要回傳分級
diagnostic，而不是 panic。Step 14 的 transaction 才會在每個 statement commit 後呼叫它。

### 4.6 diff 與 trace

現有 per-dimension Sign patch diff 不足以解釋歷時。需要 Language-wide、以 stable ID
對齊的 diff/trace，至少記錄：Primitive、target、before/after、new/deleted/moved ID、
anchor 和 validation result。這是資訊流 B「每步 trace + 前後狀態 diff」的觀察面。

---

## 5. 英語格位：occurrence-context 契約

### 5.1 範圍

此契約描述當代標準英語參考庫的最小 nominative/accusative 模型。它不是完整英語
格系統：不在此加入 possessive/genitive、wh-case、介詞支配、case stacking、格位
競爭、一般 paradigm engine 或新的 `.lang` 關鍵字。

`case` 是 Nominal **occurrence** 的 typed `syn.feature`，不是詞彙 Noun sign 永久
擁有的一個會被句子改寫的值。類別仍由 `belongs` 表示，不能用 `syn.class` 代替。

### 5.2 四個參與者

| 參與者 | 責任 | 不得做的事 |
|---|---|---|
| predicate / verb source sign | 宣告它能分配的 `subject_case`／`object_case` feature；例如 finite transitive predicate 分別分配 nominative 與 accusative。 | 不得直接改寫 noun/pronoun source sign。 |
| clause construction | 將 predicate 的 assignment 與 `subject`／`object` slot 關聯，檢查 selection，為每個 filler occurrence 建立暫態 context。 | 不得把 context 寫回 lexical source 或反向改 filler。 |
| nominal source realization | 接受已約束的 occurrence context，以同一 source sign 的 `phon.realization` 選出該 occurrence 的 form。 | 不得把 surface 當成新的 lexical source sign。 |
| phon runtime | 只處理已展開、純 phon 的 realization input。 | 不得看見 case metadata、slot label 或 `$self`／`$slot`。 |

`natural:en-standard` 現有 source schema 與 direct stored-filler runtime 已驗證這個
狹義路徑：predicate `see` 有 `subject_case`／`object_case` enum，
`EnglishSVOTransitiveClause` 以 `slot_features:` 將兩個 assignment 寫入各自的暫態
occurrence，而 `she` 只以 `$self.syn.case` 的 realization guard 選模板。同一個 `she`
同時作 subject 與 object 時，subject snapshot 為 nominative、phon 為 `/she/`，object
snapshot 為 accusative、phon 為 `/her/`，整個 clause surface 為 `she sees her`。
`she` 的 source projection 仍不含 `syn.case`，兩個 snapshot 的 provenance 都是
`StoredSign("she")`；衝突的固定格值會在 phon 前拒絕。這是 direct stored filler 的
證據，不等於所有 filler 類型或完整英語格位皆已驗收。

### 5.3 動詞分配（government）

動詞的 lexically stored grammar 資訊是「分配什麼」，不是「某個名詞目前是什麼格」。
最小契約如下：

```lang
trait EnglishFiniteTransitiveCaseAssigner:
    belongs EnglishNominativeSubjectCaseAssigner
    belongs EnglishObjectCaseAssigner
    syn:
        feature:
            object_case = accusative
```

這裡的值必須來自已宣告的有限 enum domain。若某 predicate 不帶某個 assignment
feature，construction 不得自行補造一個格值；只有明確不要求該 role 的 construction
才可不檢查它。

### 5.4 構式傳遞：每次 occurrence 都有獨立 context

概念上的 `OccurrenceContext` 是 construction runtime 的暫態值，不是新的 source
container，也不是 `.lang` keyword：

```text
OccurrenceContext {
  deep_construction: construction provenance (persistent NodeRef only after Step 13),
  slot: subject | object | ...,
  filler_source: lexical/derived provenance (persistent NodeRef only after Step 13),
  required_syn_features: { case: nominative | accusative, ... },
  provenance: predicate assignment + construction slot
}
```

一個 lexical sign 可在不同 derivation、甚至同一較大 construction 的不同 slot 中
出現多次；每次 occurrence 必須各自帶 context。`john`、`dog`、`she` 的 source 不因
曾當 subject 或 object 而累積、覆蓋或遺留 `syn.case`。

已支援的 direct stored-filler 傳遞順序為：

```text
evaluate predicate and direct stored-filler source rules
-> create direct stored-filler occurrence material
-> resolve predicate assignment by internal construction slot
-> apply slot_features to a direct stored-filler occurrence clone
-> run that clone's read-only phon realization guard
-> freeze the realized filler material into FillerSnapshot and compose the four-dimensional clause token
-> run construction/token rules
-> expand pure phon input
-> Tshiatūn phon runtime
```

這是既有「filler snapshot 不可變、context 是 constraint、phon 前完成 realization」
原則在格位上的具體化；它不引入任意 cross-dimension write 或 `link:`。這裡的
occurrence clone 不是新的 public source container；它只讓 realization guard 讀到
`$self.syn.case`，不會在注入格位後重新執行 filler 的 syn/sem/prag 規則。

### 5.5 名詞與代詞 realization

nominal form 的 deep construction identity 應保持單一；格位只選擇 occurrence 的
surface realization。

- 一般 proper/common noun 可是 **syncretic**：nominative 與 accusative 選到相同
  phon template，仍保留兩個不同的 typed occurrence constraints；
- 已驗證的第三人稱女性代詞範例是單一 `she` source sign：nominative branch 為 `she`，
  accusative branch 為 `her`。它們是顯式 lexicalized allomorph inventory，由 source
  sign 的 realization branch 依 occurrence-local `$self.syn.case` 選擇，而不是兩個
  會互相改寫的 lexical signs；
- surface 選擇後仍要先產生純 `RealizedPhonInput`，再交給 Tshiatūn；surface 不保存
  到 Language，也不生成「這次是 object 的 she/her」新 source sign；
- 尚未有通用 paradigm/allomorph relation type 前，不把明列詞形誤稱為完整可生成的
  英語屈折系統。

### 5.6 source immutability 與錯誤界線

下列不變量是格位方案的核心：

1. predicate、lexical noun/pronoun 與 construction source sign 都不可被一次 derive
   就地改寫；
2. `case` 值只存在於本次 nominal occurrence/token 的已驗證 context；
3. filler snapshot 只讀，construction 不得反向寫入它；
4. context 與 lexical/derived 固定 feature 衝突時，必須在 phon 前報
   `SlotFeatureConflict`；
5. 未宣告 feature、domain 外值、未知 slot/path 或錯誤 assignment 是 Error，不得
   落入 realization fallback；
6. 已宣告但缺少的純量值依既有 rule 語義是 Unmatched；只有明確的 Else/default
   branch 才能處理它。

這些規則使「動詞分配格」與「名詞實現格」可以分開：前者是 lexical/constructional
licensing，後者是 token-local realization；兩者都不把使用事件誤存成語言知識。

### 5.7 Step 14 封板後的 occurrence 語義

`slot_features` 現在同時支援 stored sign 與 derived-token filler。每個 derived token
私下保存 composition 後、context 與 token rules 前的 deep baseline；外層 occurrence
注入 constraint 時一律從 baseline clone，依 Syn→Sem→Prag 重跑，而不是在舊 Patch 上
再次疊加。所有 RHS 先讀同一批 frozen probe，整批驗證成功才原子提交，故書寫順序不會
污染結果；同一 target 重複綁定以 `SLOT_FEATURE_DUPLICATE_TARGET` 拒絕。

`slot_feature_bindings.rs` 已驗證 derived token 在不同 nominative／accusative occurrence
中得到不同 Syn、Sem／Prag 與 phon realization，同時保留原 token、SignId、construction
provenance 與來源 sign 不變。`OccurrenceRecord` 記錄 probe、constraint、是否重跑、
probe／committed RuleRecords 與 realization，讓 downward forwarding 可以由 trace 稽核。

本輪工作區與 Tshiatūn 回歸均為 0 failed、0 ignored、0 filtered；完整工具閘門仍須將
Clippy lint 與 WASM build 綁定到實際可用的 linker／target，基礎設施缺件不得誤報通過。

---

## 6. Step 13 到資訊流 B 的實作順序

資訊流 B 進入 `PrimitiveEdit -> Language'` 前，以下項目必須逐項落地；目前皆不能以
現有的 in-memory `SignId`、`parse_path` 或 per-dimension Patch 替代：

| 必要介面 | Step 13 所需的明確契約 | 目前狀態 |
|---|---|---|
| stable IDs | 可持久、node-scoped ID；update/move 保身分，delete+insert 產生新身分 | 已完成：versioned sidecar + SHA-256 exact binding |
| `NodeRef` | source field 指向 stable identity；rename 不改 referent，並可檢查懸空／環 | 已完成：document Ref binding；未存在的 component/sense graph 不預造 |
| typed `Path` resolver | `Path`／`NodeRef` 解析為唯一 editable node、parent 與 field kind | 已完成：NodeRef anchor + atomic typed field resolver |
| `Anchor` | `Start`／`End`／`Before(NodeId)`／`After(NodeId)`，不用可漂移的 index | 已完成 |
| 四 Primitive | immutable、fallible `Insert`／`Delete`／`Update`／`Move` 只改 caller `Language` | 已完成：`conlang-changeset` |
| `check_language` | 無副作用、獨立於 codegen 的 AST/source invariant diagnostics | 已完成並由 compile path 復用 |
| Language diff/trace | stable-ID 對齊的 before/after、anchor、validation result | 已完成：`LanguageDiff`／`PrimitiveRecord` |
| ChangeSet | ChangeSet-owned allocator、statement transaction、replay、lazy recompile | 已完成：`conlang.changeset/v1` 與 identity sidecar v2 |

### Step 13a：先建立 source interface

1. 決定 ID 的持久化與 Evolution-node namespace；
2. 補齊 node/ref model；
3. 實作 typed path resolver 與 stable anchor；
4. 實作四個純 Primitive Edit；
5. 實作 `check_language` 和 Language-wide diff/trace；
6. 為 identity、anchor、rollback、rename/ref、source immutability 建立反例。

### Step 13b：Primitive Edit 封板

每種 primitive 都要證明 `Language -> Language'`；同時斷言未被 target 的 node ID
不變、source dump 可再 parse、`check_language` 成功，以及修改後可重新 compile。

### Step 14：ChangeSet interpreter（已完成）

本步加入 ChangeSet-owned allocator、statement transaction、commit 後
`check_document`、dirty/lazy compile、serialized ChangeSet 與 deterministic replay。成功出口是
caller Language 經 ChangeSet 變為 `Language'`，重新 compile 後可以觀察到相應的
共時差異；不是直接改 compiled program。

### Step 15 以後：上層歷時功能

Atomic Rewrite、Recipe、Goal、Weight DB、Evolution node、State 與抽樣都建立在上述
source interface 之上。sense/derivation-edge、完整 component graph、entrenchment
動力學、通用 morphology/paradigm 與外部 SemanticBackend 只在各自的 Atomic Rewrite
需要時再擴充，不先偷渡進 Primitive 層。

---

## 7. 進入歷時前 checklist

- [x] 四原語以《架構修補05》P23 為唯一原語契約；
- [x] ChangeSet 的 target 明確限制為 caller Language；
- [x] every editable source node 有可持久的 stable identity；
- [x] Ref 不依名稱或 vector index 維持身分；
- [x] Path 可解析到唯一 typed target；anchor 只用 stable node identity；
- [x] Insert/Delete/Update/Move 全為 immutable、fallible source transformation；
- [x] `check_language` 不依賴 codegen 且回傳分級 diagnostics；
- [x] structure-aware diff/trace 可描述每個 primitive 前後狀態；
- [x] update/move、delete+insert、anchor 漂移、失敗 rollback、Ref rename、重 compile
  都有最小正反例；
- [x] direct stored filler 的英語格位 occurrence `slot_features` 不改動 predicate 或
  noun/pronoun source，且只在 phon realization 前的 token runtime 存活；
- [x] derived-token downward forwarding 已實作；同一 token 可在不同 outer occurrence
  接收不同 constraints，原 token、SignId 與 nested provenance 不變；
- [x] context 注入後從 `DeepTokenState` 重跑 Syn→Sem→Prag，再重選 filler realization；
  RHS 使用 frozen probe，duplicate target 在 commit 前原子拒絕；
- [x] std/natural/plugin package source 沒有被 caller 的歷時 edit 直接覆寫。

Step 13 source-edit 與上述兩個共時 context 缺口均已完成；Step 14 的 statement
transaction、ChangeSet replay 與 lazy compile 見 docs/22。此清單不代表語法化、語意漂移、
語言接觸或完整英語形態學已完成。
