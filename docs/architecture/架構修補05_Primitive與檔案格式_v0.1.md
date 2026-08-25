# 架構修補 05:Primitive 與檔案格式(v0.1)

> **依賴(一律以此二者為準)**:《架構 2.0 總鳥瞰 v1.0》、《架構修補彙整 01–04 v1.0》(P1–P19 權威表)。
> **本修補提出**:P20–P28(見 §11)。現行權威為《架構修補彙整 05–11》§1；
> 本檔保留詳細理由、格式草案與實作插入點。
> **範圍**:Language / ChangeSet 兩份檔案所需的實作 primitive,及其連帶的架構約束。

---

## 0. 修正目的

回答一個問題:**Language 與 ChangeSet 各自需要哪些 primitive?** 答案分兩種:

| 檔案 | primitive 是什麼 | 需求來源(倒推法) |
|---|---|---|
| **Language**(宣告式,被編譯) | **宣告的原子種類**(AST 節點型別) | Compile 六 pass 要能處理;12 個 Atomic Rewrite 要有東西可改 |
| **ChangeSet**(命令式,被直譯) | **操作的原子種類**(Primitive Edit + 定址) | 12 個 Atomic Rewrite 必須全部展得開 |

同時確立一條新的架構約束:**音變 DSL 為獨立可分軟體**(P20)——它約束其餘所有決策。

---

## 1. 音變 DSL 的獨立性【P20】

### 1.1 定位

**音變 DSL(Word 表徵 + 音變規則語言 + 單規則執行語意 + CLI)必須能作為獨立軟體發布與使用**,不依賴 Language / Sign / ChangeSet 的任何概念。

**獨立使用契約**:
```
(規則檔, 詞表) ──▶ DSL 引擎 ──▶ 詞表′
```
即 M0 步驟 1–7 的既有能力,不需任何 2.0 概念。

### 1.2 依賴方向(硬約束,CI 檢查)

```
changeset  ──▶  language  ──▶  dsl
                                 │
                                 └─ 禁止反向:dsl 不得 import Sign / Trait / Language / ChangeSet 任何型別
```

`dsl` crate 只知道 **Word**(臨時韻律域,P1)、音段、tier、韻律階層、音變規則。**它不知道 sign 存在**——這給了 P1 一個 crate 邊界層級的硬支撐:Word 之所以是「臨時韻律域」而非「詞位」,正因為 DSL 根本不認識詞位。

### 1.3 兩條路徑,同一個引擎(關鍵接口)

```
路徑 A(DSL 獨立):  規則檔 ──────────────────────────▶ DSL 引擎 ──▶ 表層
路徑 B(2.0 完整):  Language ─Compile─▶ Compiled Grammar
                                        └─ phon 側 = DSL 規則集 ──▶ DSL 引擎 ──▶ 表層
```

**接口定義**:**Compiled Grammar 的 phon 側 = DSL 引擎可直接吃的規則集 + stage 索引**。兩條路徑最終都到同一個 DSL 引擎,只是規則來源不同。

### 1.4 免費的雙軌迴歸測試(重要紅利)

8.1–8.6 已有路徑 A 的版本(M0)。步驟 11 完成後,同一組音變寫成 Language 檔跑路徑 B,**兩條路徑的表層必須逐字相同**。這一個測試同時驗證三件事:

1. **Compile 的正確性**(Language → Compiled Grammar 沒有語意漂移);
2. **DSL 的獨立性**(同一引擎在兩條路徑上行為一致);
3. **M0 引擎核心存活**(修補 01 的核心宣稱)。

且成本為零——M0 版本已存在。**列為步驟 11 的出口定義**。

### 1.5 規則的兩個域(釐清)

| 域 | 例 | 歸屬 | 求值對象 |
|---|---|---|---|
| **phon 規則** | `a => ə / _#` | **dsl** crate | Word |
| **syn/sem 規則** | `valence => 2 / _[+move]`(修補03) | **language** crate | Sign |

**同樣的 `=>` `/` `else` 語法,不同的求值域**。DSL 不認識 `valence`。語法共用靠依賴方向:**dsl 定義核心規則/條件文法;language 擴充它**(加 Path anchor,§3)——依賴方向仍是 `language → dsl` ✓。

### 1.6 產品意義

DSL 可獨立發布(Lexurgy 的替代品),形成先發布引擎建立使用者、再推完整工作台的路徑。此定位使 M0 成為一個**可交付的產品**,而非僅是內部階段。

---

## 2. IR dump 與 Compile pipeline【P21】

### 2.1 採 MLIR 的 round-trip 原則

MLIR 的核心性質——文字形式同時作為輸入與輸出;**由於沒有隱藏狀態,單獨執行一個 pass 的結果與在完整 pipeline 中執行該 pass 完全相同**;IR 形式可以手寫,變換易於追蹤。

**三條必守性質**:
1. **Round-trip**:`text → IR → text` 恆等。golden test = 純文字比對;**可手寫 IR 餵給任一 pass**。
2. **無隱藏狀態**:單獨跑一個 pass = 在完整 pipeline 裡跑它。這是「分開 debug」(鳥瞰 §3)真正成立的前提。
3. **同一格式跨所有 pass**。

### 2.2 Language 的 dump 格式不必發明

**它就是 Language 源文字的 canonical form**。推導出 pipeline 的形狀——**每個 pass 都是 `Language → Language`,只有最後一個 pass 產出 Compiled Grammar**(相當於 MLIR 的 progressive lowering):

```
① Source Language     使用者寫的:有 trait / global / 引用
      ↓ Trait Expansion
② Expanded Language   trait 已展開(P5:compile 後 trait 不存在)
      ↓ Name Resolution + Priority Resolution
③ Resolved Language   名稱已解析、priority 已消失(P6/P9)
      ↓ Stage 排序
④ Ordered Language    stage dispatch 已定序(P18)
      ↓ Codegen
⑤ Compiled Grammar + Compiled Sign    ← 唯一的異質產物(P8)
```

①–④ **全是合法的 Language**(同一文字語法的逐步降階)。立即能力:

- **每 pass 獨立測**:手寫 ② → 餵 Priority Resolution → 比對 ③。不必跑前面的 pass。
- **text diff 看變化**:`diff ②.lang ③.lang` = 這個 pass 做了什麼的人類可讀報告。
- **任一 stage 當 fixture**:Atomic Rewrite 測試需要的 Language fixture = 任一 stage 的 dump。
- **P5 變成可驗證**:「compile 後 trait 不存在」= ② 的 dump 裡不得出現 trait 宣告或引用 → 一個 golden test 就能抓。

### 2.3 Canonical printer(必須現在做)

`Language → text` 必須**確定性**:欄位順序固定、空白規範化、集合排序。否則 golden test 會被無關的格式差異弄紅。**步驟 8 就要有,事後補很痛。**

---

## 3. 條件語法擴展【P22】

### 3.1 `/` 已存在,不發明新語法

DSL 的環境語法 `/ _#`、`/ [+voice] _` 即條件;修補03 的 `valence => 2 / _[+move]` 已將其擴至 syn 側。歷時 function 的 Goal/Recipe **分層 IR**可直接復用 `/` 的條件語意。以下只是 IR 示意，`goal`／`recipe` **不是 `.chg` 關鍵字**：

```
Function GO_Future [layer=Recipe](verb: SignRef)
    / verb.syn.category == VERB

Function Future [layer=Goal](target: SignRef)
    candidate GO_Future(target)        / target.sem.concept == GO
    candidate WANT_Future(target)      / target.sem.concept == WANT
    else Auxiliary_Future(target)
```

`a => ə / _#` 的 `/` = 「在這個環境下」;`GO_Future(target) / target.sem.concept == GO` = 「在這個條件下它是候選」。**同一記法、同一意思**,只是匹配對象從 Word 換成 Language。零新語法。

### 3.2 `else` = Elsewhere Condition

語言學上即 Pāṇini 原則 / Elsewhere Condition(具體阻斷一般)——我們在 priority 階梯已引用過。相容性上,docs/02 §13 開放項的「`Else:`/`defer` 映射」**就此關閉**。

```
a => ə / _#
  else ɐ / _[+cons]
  else e                    # 無條件 = elsewhere 預設
```

**語意**:`else` 群組是 **disjunctive、單趟**——第一個匹配者勝出,其餘分支不跑。

**必須寫進執行語意的區別**(否則實作必錯):

| 寫法 | 語意 |
|---|---|
| `A / c1` 換行 `B / !c1` | **兩條規則**;A 跑完後 B 檢查**已被 A 改過**的結構(feeding 可能發生) |
| `A / c1 else B` | **一條規則兩分支**;只有一支跑,B 看**原始**結構 |

兩者不等價。這正是 `else` 存在的理由。

### 3.3 非線性:tier 相對的 adjacency(DSL 域)

現行 `/ _#` 假設線性字串,但 Word 是多 tier 的。**自體段音韻學的核心洞見:相鄰性是 tier 相對的**。

```
[+voice] _ [+voice]      # 音段 tier 的相鄰(現行)
<tone>H  _ <tone>H       # 聲調 tier 的相鄰 —— 中間可隔數個音節
_~H                      # 焦點聯結到 H
_~<tone>[1]              # 聯結到聲調 tier 的第一個元素
```

**零新概念**:`~`(聯結)、`<tier>`、`[n]`(序數)全是既有記法。改的只是「模板可含 tier 引用,adjacency 依該 tier 解讀」——**字串只是預設 tier**。這也是把 Scan 已解決的「序列相鄰」(8.6 Meeussen)下放給一般規則環境,不是新機制。

### 3.4 Path 表達式(Language 域)

```
Function X [layer=Recipe](cxn) / cxn.syn.alignment.ergative
Function Y [layer=Goal](t) / t.sem.concept == GO & !t.syn.has(AUX)
```

`.` 欄位存取。

> **P80 縮減(已落地)**:原另有 `[key]` slot／序數存取,與 trait 的
> `TR[1]`、選擇器的 `<syl>[2]` 同一記法。**已自 language 域的 Path 移除**;
> 那兩處記法本身(trait 引用的序數尾槽、Scan 的序數)不受影響。
>
> 本節第一個範例原作 `cxn.slot[agent].syn.animate`——「construction 的 agent
> slot 的填充者的 animate 欄位」。該形**現已不可表達**:`.lang` 側對應的寫法是
> `$slot.agent.syn.animate`,但 function guard 的主體是**參數**而非 slot
> (求值前被代換成 `$self`,見P79),故 function 層目前沒有定址 slot 的
> 記法。這是 ⑨ 與 ⑧ 的交界,列為未決。

### 3.5 統一文法(兩個域,一套 parser)

```
Condition := LinearTemplate            # _# / [+voice] _ / <tone>H _ <tone>H   (dsl)
           | Path (Op Value)?          # 路徑測試 / 存在測試                    (language)
           | Condition & Condition
           | Condition | Condition
           | ! Condition

Path := Anchor ( '.' Name )*           # 欄位
```

> **P80 縮減(已落地)**:本產生式原有 `'[' Key ']'`(slot／序數)與
> `'~' TierRef`(聯結)兩種段,已移除。量測依據見修補13 §6:全庫 201 個
> `$` 引用與所有 Def lhs **零處**用到這兩種段;`.lang` 的查找端
> (`project(dim).get(&path)`、`FillerSnapshot::scalar`)是**字串鍵比對**,
> 不解讀結構,故三種段的表達力完全相同;P71 之後自造欄位一律走 `feature:`
> (單一識別字),多段路徑的唯一來源是套件座標前綴;Step 13 的欄位定址本來
> 就只收具名段。
>
> **只縮減 language 域的 Path。** 序數與 tier 在它們真正有語意的地方不受
> 影響——`.qy` 的 `LinearTemplate`(`<tone>H _ <tone>H`)、Scan 的序數
> `[n]`(D16)、旋律層的 tier 記法,都是 dsl 域的獨立文法。

**差別只在 Anchor 與求值上下文**:

| 域 | Anchor | 求值 | 結果 | crate |
|---|---|---|---|---|
| **Word**(規則環境) | `_` 焦點、`#` 界、tier 引用 | 帶焦點位置 | `Position → Bool`(所有為真處套用) | dsl |
| **Language**(歷時 function 守衛) | 參數名(`target`/`cxn`) | 無焦點 | `Bool`(適用與否) | language |

環境與守衛**不是兩種東西**:條件都是「對某 context 的布林測試」,環境的 context 多一個焦點位置。**dsl 定義核心文法(LinearTemplate + 組合子 + else),language 擴充 Path anchor**——依賴方向合規(P20)。

### 3.6 三道界(防長成通用查詢語言)

- **無量詞**:不給 `forall`/`exists`/聚合(`count`/`sum`)。需要「所有 slot 皆 animate」→ 寫成引擎提供的具名謂詞(`cxn.slots.all_animate`),不給任意量化。
- **無計算**:`==`/`in`/`has`/比較即止;不給算術、字串操作、函數呼叫。
- **無副作用、必終止**:純函數;路徑深度有限;無遞迴。

保住三個 debug 前提:可 golden、可靜態檢查、可 dump。

---

## 4. Primitive Edit:四原語【P23】

### 4.1 定案

```
insert(parent, anchor, subtree)      插入節點
delete(node)                          刪除節點(含子樹)
update(node, field, value)            改欄位,**保持節點身分**
move(node, new_parent, anchor)        移動子樹,**保持節點身分**
```

**權威先例**:GumTree(AST diff 領域最廣引用的框架)的編輯腳本正是四動作——insert、delete、update、move。

**修補04 的七項為非正交**:
| 修補04 項 | 實為 |
|---|---|
| `rename` | `update(node, "name", v)` |
| `set` | `update(node, field, v)` |
| `unset` | `update(node, field, None)` |
| `replace` | `update(node, field, subtree)`(保持身分)**或** `delete`+`insert`(換身分)——**語意不同,由使用者選** |

### 4.2 `update` 獨立存在的理由:身分保持

研究界踩過此坑——許多 delete/insert 配對其實應是 update;move 與 update 的品質是既有方法不準確的主因,正因它們涉及身分保持,不能用 delete+insert 化約。

**這直接撐起上層的語言學語意**:

| Primitive | 身分 | 語言學語意 | 對應 Atomic Rewrite |
|---|---|---|---|
| `update(sign.phon, …)` | **SignId 不變** | 這個詞的音變了,仍是同一個詞 → D 的 diff **對齊得上** | `sound_change` |
| `delete(sign)` + `insert(new)` | **SignId 換了** | 舊詞消失、新詞出現 → D 的 diff 顯示為**生滅** | `delete` + `create` |

若把 update 化約為 delete+insert,SignId 斷裂,D 的 diff 對齊鍵(docs/07 v0.1.1)整個失效。

### 4.3 12 個 Atomic Rewrite 的展開檢核

| Atomic Rewrite | 展開 | primitive |
|---|---|---|
| `sound_change` | 加規則入 global trait 某 stage / 或 UR 重構 | insert(rule) / update(sign.phon) |
| `drift` | 改 sense 語意表示(值由 SemanticBackend 算) | update(sense.meaning) |
| `derive_sense` | 加 sense + 加衍生邊 | insert(sense) + insert(edge) |
| `lexicalize_sense` | 衍生邊 → opaque | update(edge.transparency) |
| `reanalyze{target}` | 改 valence/category/slot;Boundary 時改成分切分 | update;(Boundary)+ move/insert/delete |
| `entrench` / `attrit` | 固著度 ±δ | update(entrenchment) |
| `lexicalize` | token 固化為新 sign | insert(sign 子樹) |
| `create` / `delete` | sign/sense 生滅 | insert / delete |
| `split` | 新 sign + 搬部分 sense + origin 指回 | insert + move + update(refs) |
| `merge` / `fuse` | 搬 sense + 刪 / 建融合 sign + component 引用 | move + delete / insert + update |
| `adopt` | 從他節點複製 sign(新 id + origin) | insert(展開時讀 donor Language) |
| `fossilize` / `generalize` | 規則在居所階梯間搬移 | **move**(rule 跨 Global↔Trait↔Sign) |

**12/12 展得開,四原語封閉。** 但成立依賴 P24(引用模型)。

---

## 5. 引用模型:Ref 是屬性值,不是圖邊【P24】

Language 不是純樹——origin、衍生邊、component、trait 引用皆為跨節點引用。**若視為圖邊,四原語(樹操作)不足**,得加 link/unlink。

**決策**:引用一律是**節點欄位裡的 `Ref` 值**——衍生邊住來源 sense 的欄位、component 住複合 sign 的欄位(這正是既有的單一資訊源決定,docs/09 §4)。

```
update(sign(go).origin, SignRef("foreign::3"))
```

如此「改引用」= `update(field, Ref)`,**樹模型保住、四原語封閉**。GumTree 的四操作能成為業界標準,正因它們在樹上封閉。

**引用型別**:`SignRef` / `SenseRef` / `RuleRef` / `TraitRef` / `ConceptRef`——一律以穩定 ID 定址。

**不變量**(`check_language`,§7.3):懸空 Ref 偵測、component DAG 無環、origin 鏈無環。

---

## 6. 定址語言:Path + 錨點相對插入【P25】

### 6.1 定址 = 條件語法的 Path(復用,不另造)

```
update( sign(go).syn.category, AUX )
insert( trait(VerbCommon).block[2].rules, after: rule(r7), <新規則> )
move(   rule(r3), from: global(CorePhon).block[1].rules,
                  to:   sign(go).cophonology.rules, anchor: end )
```

**同一 Path 文法:條件裡是測試、primitive 裡是定址——一套 parser 兩用。**

### 6.2 插入定位用錨點相對,非純索引

```
anchor := start | end | after: <Ref> | before: <Ref>
```

**理由**:規則序列在 replay 中會增刪,**索引會漂移**;錨點(指向穩定 ID)不會。這對 replay 的可重現性是必須的。

---

## 7. 直譯器執行語意【P26】

### 7.1 決定性 ID 配發(replay 的死穴)

`insert(sign)` 要配新 SignId。**若 ID 配發不確定,replay 兩次得到不同 Language,所有 diff 對齊與 origin 鏈全毀。**

> **每個 Evolution_node 的直譯過程,ID 配發必須純序列性**(如 `node_id:counter`,或由 ChangeSet 行號派生),同一 ChangeSet replay N 次產出**逐位元相同**的 Language。**禁止隨機 / 時間戳 / 雜湊亂序 ID。**

### 7.2 語句級交易

一條 ChangeSet 語句展開的 `Vec<Edit>` **原子套用**——全成或全不動(中途失敗回滾)。否則半套用的 Language 無法重現。

### 7.3 後驗證

每語句 commit 後跑 `check_language()`:懸空 Ref、component/origin 環偵測、trait block 完整性(P5)、stage 合法性。**仿 `check_word` 的分級回報(error/warn/info),不 panic**——與 M0 的 I 系列規範一致。

### 7.4 重編譯觸發

commit 後標 dirty;**下次導出前才重 compile**(lazy,同 lazy reparse 哲學)。Compiled Grammar 是純函數產物,可丟棄重算(P8)。

### 7.5 `adopt` 的跨節點讀

直譯器持有「已 materialize 節點的唯讀視圖」;受 docs/06 的**無環約束**(引用只能指向時間上已 materialize 的狀態)。

---

## 8. Trait Block 的 AST 表示:Block 節點法【P27】

**採選項 A**:

```
Trait { name, blocks: Vec<Block> }
Block { items: Vec<Item> }        # Item = Definition | Rule
```

- **`==` 不是分隔符 token,是 Block 節點的邊界**;parser 遇 `==` 切換到下一個 Block。
- `insert` 可操作 **Block 層**(加一個 block)或 **Item 層**(在某 block 內加規則)。
- 若 `==` 僅為詞法符號(選項 B:Separator 節點),primitive 將無法定址 block 層級。

**理由**:Block 是語義單位,Separator 是語法噪音。

---

## 9. ChangeSet 的產物與 Language 的對偶性【P28】

### 9.1 產物是 Language,不是 AST

```
Language 源文字(.lang)
    ↕ round-trip(canonical printer / parser)
Language 資料結構(in-memory tree)   ← Primitive Edit 操作這個
    ↓ Compile
Compiled Grammar                   ← Engine 讀這個
```

Primitive Edit 操作 **Language 資料結構**,不是 Compiled Grammar 的 AST。直譯器執行完 → Language 資料結構 → canonical printer → **Language 源文字,與輸入同格式**,可再 parse / 再 compile / 再 diff。這是「每步 trace + 前後狀態 diff」的歷時 debug 手法成立的前提。

### 9.2 Canonical Empty Language(必須存在)

**從空寫出完整 Language = 對一個空根做一連串 insert。** 若無根節點,第一個 insert 無處掛靠。

> Language 資料結構必須有一個**永遠存在的根節點**(canonical empty Language),等同 MLIR 的 `builtin.module`。

### 9.3 對偶性(浮現的結構同構)

> **Language 源文字 ≡ 「對 canonical empty Language 做一串 insert 的 ChangeSet」。**

把一份 Language 檔的每個宣告翻譯成 `insert`,得到一份合法 ChangeSet;執行它對空 Language,得回原本那份 Language。含義:

- **兩份檔案格式互為對偶**:Language = 狀態的宣告;ChangeSet = 狀態變遷的操作;**用相同的概念集構建**。
- **共時/歷時是同一東西的兩個視角**:Language = 已 apply 完的 ChangeSet 的靜態快照。
- **「從既有語言建分支」= clone Language + apply ChangeSet**——不需特殊 clone 機制,就是四原語的組合。
- **root Evolution_node 的來源解決了**:它不是 ChangeSet 演化出來的,**它就是一份 Language 檔,直接給定**;之後所有後代節點才是 ChangeSet 的產物。

### 9.4 兩檔案的結構同構

```
Language 檔案                ChangeSet 檔案
  定義區:trait / global  ↔    定義區:具名歷時 function + layer metadata
  使用區:sign           ↔    執行區:呼叫序列
```

兩側都是「可參數化的具名定義 + 使用」——共時側的 trait 是宣告的 macro、歷時側的 Recipe-layer function 是操作的 macro。Goal/Recipe 是 AST/執行器的 function 分層名稱，**不是 source keyword**。**檔案框架、命名解析、參數綁定、展開機制可共用實作**,只有「展開成什麼」不同。

**差異**:trait 有 `==` block + 插入位置語意(宣告的位置有意義);Recipe-layer function 是線性序列,不需 block 槽位(操作本就是一串)。

---

## 10. 檔案格式草案

### 10.1 Language(`.lang`)

```
# ── 定義區(Definition,`=`,無序,Merge Strategy 合併)──
feature voice   = (+, -)
symbol  m       = [+sonorant, +nasal]
class   vowel   = {a, e, i, o, u}

# ── 分佈覆寫(E 的覆寫層,稀疏;docs/10)──
distribution {
    /k/ = 0.15
}

# ── Global trait(預設自動引用,priority 最低;P5/P6)──
global trait CorePhonology {
    a => ə / _#              @stage word
    ==
    n => m / _[+labial]      @stage stem
}

# ── Trait(macro 模板;== 分 Block,P27)──
trait VerbCommon {
    syn.provides = VERB
    syn.requires = [agent, patient]
    ==
    valence => 2 / _[+move]           # syn 規則(language 域)
}

# ── Sign(真正的語言單位)──
sign go {
    VerbCommon[1]                      # block 1 顯式插入(P5:全 block 強制)
    phon = /go/                        # 底層形 UR
    sem.senses = [ sense s1 { concept = GO } ]
    VerbCommon[2]                      # block 2 顯式插入
    entrenchment = 0.8
}
```

### 10.2 ChangeSet(`.chg`)

下列為 **IR 草案記法，不是 lexer 關鍵字表**；`Function`、`Invoke` 只用來顯示
function 身分與 Goal/Recipe 分層。真正 `.chg` 表面語法須另案決定。

```
# ── 定義區 ──
Function GO_Future [layer=Recipe](verb: SignRef) / verb.syn.category == VERB {
    drift(verb.sem, direction: bleaching)
    reanalyze(verb, target: Category, to: AUX)
    entrench(verb, delta: +0.3)
    sound_change(rule: <reduction>)
}

Function Future [layer=Goal](target: SignRef) {
    GO_Future(target)        / target.sem.concept == GO
    WANT_Future(target)      / target.sem.concept == WANT
    else Auxiliary_Future(target)
}

# ── 執行區(四層任一,層級介入 P17 的檔案體現)──
Invoke Future [layer=Goal](target: sign(go))         # ④
Invoke GO_Future [layer=Recipe](verb: sign(go))      # ③
rewrite reanalyze(sign(go), target: Category, to: AUX)   # ②
edit    update(sign(go).syn.category, AUX)          # ①
```

**Goal 的隨機為隱含語意**(不寫在 body):body 宣告「有哪些候選 + 條件」(純函數,可 golden);**呼叫**時引擎自動:過濾候選 → 查 Weight DB → seeded 抽樣 → 執行選中 Recipe。Goal 兩個面各自可測:

| 面 | 契約 | 測法 |
|---|---|---|
| `Goal.candidates(target, Language, State)` | → `Vec<(Recipe, weight)>` | **純函數 golden**,不含隨機 |
| `Goal.invoke(…, seed)` | → 選中的 Recipe | seeded,同 seed 同結果 |

保住 P12 分工:Goal 決定可能性(body 條件)、Weight DB 決定機率(引擎查表)、抽樣器選擇(seed)。**其餘隨機(挑 target、要不要發生、數量)由自動演化系統在呼叫端處理**,不進 Goal body。

### 10.3 Language 的 AST 節點型別(五組)

| 組 | 節點 |
|---|---|
| **① 定義類**(`=`,無序) | 受控範疇引用 |
| **② 規則類**(`=>`,有序,帶 `@stage`) | `Rule { env, action, stage, else_chain }`——**必須有 RuleId**(fossilize/generalize 要 move 它,定址要指它) |
| **③ 容器類** | `global trait` / `trait`(`Vec<Block>`)/ `sign`;sign 內:phon(UR)、sem(senses + 衍生邊)、syn(provides + slots)、prag、entrenchment、origin、provenance、construction 的 `cophonology`(P4 雙來源閂) |
| **④ 引用類**(Ref 值,P24) | `SignRef` / `SenseRef` / `RuleRef` / `TraitRef` / `ConceptRef` |
| **⑤ 分佈類** | `distribution`(E 的覆寫層,稀疏;是節點資料、要被 diff 與繼承) |

**不在 Language 內**:旁註層(Annotation)——引擎不可見,存專案檔的節點側表(docs/07 §5c)。

---

## 11. P 系列決策(本文件新增)

| 編號 | 決策 |
|---|---|
| **P20** | **音變 DSL 為獨立可分軟體**:`dsl` crate 只知 Word,**禁止 import Sign/Trait/Language/ChangeSet**(CI 檢查);依賴方向 `changeset → language → dsl`;獨立契約 `(規則檔, 詞表) → 詞表′`;**Compiled Grammar 的 phon 側 = DSL 可直接吃的規則集**;**雙軌迴歸**(同一音變經路徑 A/B 表層須逐字相同)列為步驟 11 出口;規則兩域:phon 規則屬 dsl、syn/sem 規則屬 language,語法共用靠 language 擴充 dsl 的文法 |
| **P21** | **IR dump = Language canonical form + progressive lowering**:採 MLIR round-trip 原則(text↔IR 恆等、無隱藏狀態、同格式跨 pass);compile 的 ①Source→②Expanded→③Resolved→④Ordered 全是合法 Language,僅 ⑤ Codegen 產出 Compiled Grammar;**canonical printer 為步驟 8 必做**(確定性:欄位序、空白、集合排序) |
| **P22** | **條件語法擴展**:`/` 復用(環境=守衛,同一記法);**`else` 鏈 = Elsewhere Condition**(disjunctive 單趟、第一匹配勝出;與「兩條有序規則」的 feeding 語意明確區別;關閉 docs/02 §13 的 `Else:` 開放項);**tier 相對 adjacency**(模板可含 tier 引用,字串只是預設 tier);**Path 表達式**(`.` 欄位;原有的 `[key]` slot 與 `~` 聯結兩種段已由 **P80**移除,見 §3.5 附註);**三道界**:無量詞(需要時用引擎提供的具名謂詞)、無計算、無副作用必終止 |
| **P23** | **Primitive Edit = 四原語** `insert / delete / update / move`(GumTree 先例);修補04 的 `rename/set/unset/replace` 皆為 `update` 的參數化;**`update`/`move` 保持節點身分**——D 的 diff 對齊鍵靠它,`update(sign.phon)`=同一詞音變 vs `delete+insert`=生滅,語意不同;12 個 Atomic Rewrite 全數展得開 |
| **P24** | **引用 = Ref 屬性值,非圖邊**:origin/衍生邊/component/trait 引用一律為節點欄位裡的 Ref 值(沿單一資訊源);「改引用」= `update(field, Ref)`;**這是四原語在樹上封閉的前提**;不變量:懸空 Ref、component DAG 無環、origin 鏈無環 |
| **P25** | **定址 = Path 文法 + 錨點相對插入**:條件與定址同一套 parser(測試 vs 定址);插入位置用 `start/end/after:/before:` **錨點**而非索引(replay 中索引會漂移,錨點不會) |
| **P26** | **直譯器執行語意**:**決定性 ID 配發**(純序列性,如 `node_id:counter`;禁隨機/時間戳;同 ChangeSet replay N 次逐位元相同)+ **語句級交易**(一語句展開的 Vec\<Edit\> 原子套用,全成或全不動)+ **後驗證** `check_language()`(仿 `check_word` 分級回報,不 panic)+ **lazy 重編譯**(commit 標 dirty,導出前才重 compile)+ adopt 的跨節點唯讀視圖受無環約束 |
| **P27** | **Trait Block = Block 節點法(選項 A)**:`Trait { blocks: Vec<Block> }`,`==` 是 Block 邊界而非分隔符 token;insert 可操作 Block 層或 Item 層 |
| **P28** | **ChangeSet 產物 = Language(非 AST)+ Canonical Empty Language + 對偶性**:直譯器產物經 canonical printer 回到與輸入同格式的 Language 源文字;**必須有永遠存在的根節點**(等同 MLIR `builtin.module`),否則從空 insert 無處掛靠;**Language 源文字 ≡ 對空 Language 的一串 insert 的 ChangeSet**——兩檔案格式互為對偶、共時=已 apply 的歷時快照、建分支 = clone+apply(無需特殊 clone 機制)、**root Evolution_node 就是一份直接給定的 Language 檔** |

---

## 12. 逐文件修補清單

### CLAUDE.md
- §2 不變式追加:「`dsl` crate 不得 import 任何 Language/Sign 型別(P20)」「ID 配發必須決定性,replay 逐位元可重現(P26)」「引用是 Ref 屬性值,非圖邊(P24)」。
- §4 可移植性規範:加一條 crate 依賴方向的 CI 檢查(`changeset → language → dsl`,禁反向)。

### docs/02 語法規格
- 記法表新增:`else` 鏈;環境模板的 tier 引用(adjacency 隨 tier);Path 表達式(`.`/`[]`/`~`);`@stage` 標記。
- **§13 開放項:`Else:`/`defer` 映射可關閉**(P22)。
- 新增節:條件語法的兩域(Word/Language)與 anchor 差異;三道界。

### docs/04 執行語意
- 新增「disjunctive 套用」節:`else` 群組單趟、第一匹配勝出;與「兩條有序規則」的 feeding 語意明確區別。
- 新增「Compile pipeline」節:①–⑤ progressive lowering;每 pass dump。

### docs/05 M0 實作參照
- **§8 開發順序**:步驟 11 出口加入**雙軌迴歸測試**(P20 §1.4)。
- 新增節:crate 邊界與依賴方向(P20)。
- §9 決策表加交叉引用:P20–P28 住本檔 §11。

### docs/06 演化圖(D)
- Replay 語意補:**產物是 Language**;root 節點 = 直接給定的 Language 檔(P28)。
- 補:決定性 ID 配發是 replay 可重現的前提(P26)。

### docs/09 Sign 引擎
- §4 五連接關係:明確為 **Ref 屬性值**(P24),非圖邊。

### docs/10 統計引擎(E)
- 覆寫層 = Language 的 `distribution` 節(§10.3 ⑤)。

### docs/12 邏輯分層
- 引擎層即時子層:補 DSL 的獨立契約(路徑 A)與 2.0 契約(路徑 B)。

---

## 13. 實作插入點(配合 M0 步驟 7 進行中)

| 時點 | 動作 |
|---|---|
| **步驟 7(現在)** | 零插入。**但可順手確立 crate 邊界**:確認 `dsl` crate 沒有任何對未來 language 型別的預留依賴(P20) |
| **步驟 8** | Language 資料結構 + **canonical empty root**(P28)+ **canonical printer**(P21)+ 五組 AST 節點(§10.3)+ IR dump |
| **步驟 9** | Language Parser:`==` 切 Block(P27)、`@stage`、Path、`else` 鏈;**language 擴充 dsl 的條件文法**(P20 §1.5) |
| **步驟 10** | Compile pipeline ①–⑤,每 pass dump 同格式 Language(P21);priority 欄位級解析 |
| **步驟 11** | Compiled Grammar(phon 側 = DSL 規則集,P20 §1.3);🔑 **雙軌迴歸測試**(P20 §1.4) |
| **步驟 13** | Primitive Edit 四原語(P23)+ Path 定址(P25)+ 錨點相對插入 |
| **步驟 14** | ChangeSet Interpreter:決定性 ID + 語句級交易 + `check_language` + lazy 重編譯(P26) |
| **步驟 15** | Atomic Rewrite 12 項的展開(§4.3 檢核表為 golden 來源) |
| **步驟 17** | Goal/Recipe 函數 + 隱含抽樣(§10.2);Goal 的 candidates/invoke 兩面各自測 |
