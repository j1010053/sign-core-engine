# `.chg` 授權語法 v0：insert / update over 四原語（已落地）

> **2026-07-30 狀態：核心授權面已實作。** 本文定義 `.chg` 上
> `insert`／`update` 的**授權表層**；phon block 的 source insert／顯式 bootstrap
> convenience 亦已補齊，其餘 §6.5 邊界維持。
> 權威操作仍只有 **Insert／Delete／Update／Move** 四原語（P23）；本語法一律
> **降階**成四原語，resolve／dump 後的 ChangeSet **只含四原語**。Step 14 契約
> （`docs/verification/Step14_ChangeSetInterpreter_封板_v1.md`）不變。
>
> **鐵律**：`insert`／`update` 的 **target 只能是 `.lang` 關鍵字**。`.chg` 不自創任何節點型別或
> payload 語法——框架動詞（`insert`／`update`／`at`）之外，一切內容都是逐字的 `.lang` fragment，
> 由既有 `.lang` parser 解析。故「能 insert／update 的物件」＝「`.lang` 能寫的關鍵字」，一一對應。

---

## 0. 單一文法原則

```
.chg = 框架動詞（insert / update / at / selector / position）  ＋  .lang block（逐字）
```

- **框架**只負責：定位（selector）、位置（position）、動作（insert/update/delete/move）。
- **內容**（block）永遠是 `.lang` 片段，首關鍵字即 `.lang` 關鍵字（`sign`/`trait`/`syn`/`sem`/
  `prag`/`phon`/`slot`/`slots`/`slot_features`/`feature`/`roles`/`constraints`/`map`/`belongs`/
  `case`/`when`/`else`/`then`/`realization` …）。
- 好處：單一 parser；block 的 dump ＝ `.lang` fragment（round-trip 天然對稱）；phon 的 Tshiatūn
  規則免費支援（§5.2）。

---

## 1. 三層物件分類

| 層 | 定義 | 成員 |
|---|---|---|
| **① 公開 insert target** | 使用者可見、可重複、有位置語意、對應 `.lang` 關鍵字 | Symbol／Class／Sign／Trait（→ language）；17 種 sign body item；4 條 branch 鏈 |
| **② container** | AST 中能容納子節點的節點（insert 的 target 側；多數同時是 ① 的 payload） | Language, Sign, Trait, Block, Case, Rule, Realization |
| **③ dump-only** | 為 round-trip 必序列化，但**無公開 insert 語法**，改走 update/set | Distribution；Symbol/Class 以外的 DslDeclaration |

③ 目前唯一的 `Language` 直屬**非構式**欄位是 `distribution: Vec<(String,String)>`，
自然編輯是 upsert，非位置插入：

- `DslDeclaration`：`Symbol`/`Class` 已支援 `insert into language at …:`；`Feature`/`Melody`/`Spell-out`/`Parse` 仍待專屬 `dsl` op 或文字級契約。
- `Prosody` 只使用 Tshiatūn DSL `Prosody LEVEL < …`，作為 `dsl_decls` 的一員；小寫 `prosody = …` 已廢棄。
- `Distribution`（`distribution:` + `key = value`）→ `set distribution[key] = value`（upsert）。

三類仍進 dump 白名單；其中 `Symbol`/`Class` 已可由 `insert into … at …` authoring、dump 與 replay，其餘維持 dump-only。

---

## 2. `insert`：target · position · block

```
insert into <target> at <position>:
    <block>            # 逐字 .lang fragment
```

- **`<target>`**（container selector，皆由 `.lang` 關鍵字衍生）：
  `language` ｜ `sign("NAME")` ｜ `trait("NAME")` ｜ 維度路徑 `sign("NAME").syn`（見 §3 selector）
  ｜穩定形 `node(<kind>, @ns:ord)`。
- **`<position>`**：見 §3。
- **`<block>`**：`.lang` fragment，首關鍵字選 payload 種類：

| `.lang` block 首關鍵字 | 產生節點 | 合法 target |
|---|---|---|
| `Symbol NAME`／`Class NAME {…}` | DslDeclaration；多行 block 依原順序 fan-out | language |
| `sign NAME:` | SignDef（**配全新身分**：SignId/RuleId/NodeId 全刷新） | language |
| `trait NAME:` | TraitDef | language |
| `syn:` / `sem:` / `prag:` | dim fragment（多 items，僅該維，無 belongs/trait/realization） | sign／block |
| `phon:` | `/…/` 模板 ／ phon Def ／ **Tshiatūn 規則** ／ `realization:` | sign |
| `sign:`（匿名） | SignContext fragment（混維＋belongs，合併回 sign，**不建新 SignId**） | sign |
| `slot NAME [F]?` | Slot | sign.syn |
| `slots:` / `slot_features:` | Slot ／ SlotFeatureBinding | sign.syn |
| `feature:` | FeatureDecl/Value/Expression/Rule | sign.\<dim\> |
| `map SLOT OP …` | SlotMap | sign.syn |
| `roles:` | RoleDecl/Binding/Expression | sign.sem |
| `constraints:` | BinaryConstraint | sign（sign 級） |
| `belongs X` | Belongs | sign |
| `NAME[n]` | TraitUse | sign |
| `field => value / guard` | Rule（維度依 target 語境） | sign.\<dim\> |
| `case`/`when …:` | Case（SignExpression） | sign.\<dim\> |
| `realization:` | Realization ＋ branches | sign.phon |
| `else …` / `then …` | RuleElseBranch／RuleThenBranch | rule（互斥，§5.4） |
| `path = value` | Def | sign.\<dim\> |

> `clone <sign> as <name>` 是既有授權糖，降階為單一 `Insert{Sign}`；不新增原語。

---

## 3. 位置確定（position）

### 3.1 selector（定位既存節點）

`language` ｜ `sign("NAME")` ｜ `trait("NAME")` ｜ **authoring path** ｜穩定形
`node(<kind>, @ns:ord)`。resolve 後名字/路徑型 selector 一律釘成 `node(...)`。

**nameless rule/case/branch 定址（已定案，`resolve_path_child` 實作）**——無名節點以
**typed 路徑段**定址，掛在 `sign("x")`/`trait("x")` 之下：

| 路徑段 | 目標 | 選法 |
|---|---|---|
| `.rule[n]` | 第 n 個 Rule/FeatureRule | 序數 |
| `.else[m]` / `.then[m]` | rule 的第 m 個 else/then 分支 | 序數（接在 `.rule[n]` 後） |
| `.realization[k]` | 第 k 個 realization 分支 | 序數 |
| `.case[n]` / `.branch[m]` | 第 n 個 case ／ 其第 m 個 branch | 序數（**待補**） |
| `.block[n]` | trait block | 序數 |
| `.def[path]` / `.slot[name]` / `.role[name]` | Def／Slot／Role | keyed |

例：`sign("dog").rule[0]`（父 rule）、`sign("dog").rule[0].else[0]`（sibling 分支，供
`before/after` 定位）。序數對重排敏感，故 dump 一律釘成穩定 `node(<kind>,@ns:ord)`。

### 3.2 `at <determiner>` 三種語意

| target list | `at end` 意思 | 可用 determiner |
|---|---|---|
| **canonical-unordered**（Sign/Trait/Distribution） | **佔位符**：引擎忽略，真實位置由 **key 排序**算（`end` ≠「最後」） | **只准 `end`** |
| **singleton**（Prosody） | 空時才准放 | 只 `end` |
| **ordered**（Items/Blocks/**CaseBranch/else/then/Realization**） | **字面** append | `first/last`、`before/after <ref>` |

**items group-aware**：sign 的 `items` 依 `item_group` 分區；`start/end` 是**該群**頭尾，`before/after`
的錨**必須同群**（否則 `AnchorInvalid`）。branch 鏈不分群，`end` ＝ 字面最後一支。

### 3.3 有序區塊的符號式 `<ref>`（給 nameless branch）

branch 無名字，`before/after` 若只吃穩定 NodeId 幾乎無法手寫。提供符號式參照，resolve 時釘成
`node(<kind>,@ns:ord)`：

| `<ref>` | 意義 | 穩定性 |
|---|---|---|
| `#n` | 第 n 支（0-based） | 隨重排失效 |
| `guard "<expr>"` / `== <value>` | condition 命中的那支 | **對重排穩定** |
| `else` / `default` | `CaseCondition::Else` ／ realization `guard=None` 的 catch-all | 每鏈至多一 |
| `node(<kind>,@ns:ord)` | 穩定 NodeId | **canonical** |

### 3.4 determiner 分級

| target list | 合法 determiner |
|---|---|
| Sign／Trait／Distribution | **僅 `at end`** |
| Items（group-aware） | `first/last`、`before/after #n\|node(...)`（限同群） |
| CaseBranch | ＋ `before/after guard\|==\|else` |
| else／then 鏈 | ＋ `before/after guard\|#n\|else` |
| Realization 分支 | ＋ `before/after guard\|else` |

### 3.5 有序區塊安全設計（順序＝語意）

first-match（`case:`、Lexurgy `else`）下順序即優先序；`at end` 常是陷阱（插在 catch-all 之後 ＝
dead branch）。故：

1. **first-match 不預設 `end`**：省略位置時預設 `before else`（無 Else 才 fallback `last`）。
2. **出口驗證**：first-match 區塊「Else 之後不得有 guarded branch」→ 插入後 `check_document` 報
   unreachable-branch，把慣例升級為不變式。

---

## 4. `update`：兩層（不需 position）

### 4.1 Tier-1 純量欄位

```
update <selector>.<field> = <value>
```

現有 8 個對應（`update_for`／`dump_update`）：

| target kind | field → NodeUpdate |
|---|---|
| Sign/Trait | `name` → Rename |
| Definition | `path` → DefinitionPath；`value` → DefinitionValue |
| Rule/FeatureRule | `body` → RuleBody |
| Else/Then branch | `body` → RuleBranchBody |
| Slot | `name` → SlotName |
| RealizationBranch | `template` → RealizationTemplate |
| Case | `selection` → CaseSelection（`case`/`when`） |

**待補（20 個 `NodeUpdate` 未接表層）**：TraitGlobal、RuleStage、RuleDimension、SlotConstraint、
SlotOptional、TraitUse、Belongs、FeatureDeclaration、FeatureValue、SlotFeatureBinding、SlotMap、
RoleDeclaration、RoleBinding、RealizationGuard、CaseBranch、SignApplication、Constraint，以及
③ 的 DslDeclaration／Prosody／Distribution。原則：`update_for`（讀）與 `dump_update`（寫）**對稱**補齊。
sign 級 metadata（`origin`/`provenance`/`lifecycle`/`entrenchment`）走 top-level Def path。

### 4.2 Tier-2 維度片段整替（block-valued，糖）

替換整個維度切片時用 block 形式（block 仍是 `.lang` fragment）：

```
update <target>.<dim>:
    <該維 fragment>
```

語意：以 fragment **取代** target 該維 items。降階 ＝ **delete 舊維 items ＋ insert 新 fragment
items**（同一 atomic statement）。`.sign:`（匿名 SignContext）整替混維切片。

---

## 5. block 內容參考（`.lang` 關鍵字語意）

### 5.1 維度子區塊（皆 `.lang` 既有語法）

```
syn:
    slots:
        agent [Nominal]
        theme [Nominal]?                 # ? = optional
    slot_features:                       # SlotFeatureBinding
        agent.person = 3                          # enum literal
        theme.number = $slot.agent.syn.number     # frozen 讀 filler syn
    feature:                             # FeatureDecl/Value/Expression/Rule
        transitivity = enum(transitive, intransitive)
        transitivity = transitive
        class => transitive / [Verb]
    map agent rename actor               # SlotMap
    class => transitive / [Verb]         # syn Rule

sem:
    roles:                               # 僅 sem 內
        agent = actor                             # 綁定
        theme => : case ...                       # RoleExpression
    senses[core].concept = GIVE          # Def（Path）

constraints:                             # sign 級（語意屬 syn）
    agree(agent.number, theme.number)    # predicate(left, right)，恰兩運算元
```

### 5.2 `realization:` vs `phon:`（form 選形 vs 純音韻）

- **`realization:`**：`RealizationBranch { template:/…/, guard:Option }`；guard **唯讀** `$self`／frozen
  `$slot`（跨維讀取 syn 範疇/feature/filler）→ **選一個 phon template**。是 form 端 spell-out／選形橋，
  **讀取／投影**他維、**不改**他維（P44）。phon 的分支（guard/else/case）一律住這裡。
- **`phon:` 直接規則**：`b => p / _ #` → `Rule{dim:Phon}` ＝ **純音韻改寫（Tshiatūn rule）**，
  不讀他維、不分支。codegen 放進 `CompiledGrammar.phon_source` 交 Tshiatūn 跑（雙軌 8.1–8.6 為證）。
- 邊界：匿名 PhonContext fragment 只收 template／full-sign projection，trait 不可展開到 phon；
  檔首 `Feature`/`Symbol`/`Class` 是 dsl_decls（③），非 per-sign phon rule。

### 5.3 `field` 三義

| 語境 | field 是 | 文法 |
|---|---|---|
| Def `field = value` | 一條 **Path** | `anchor`（`.name`\|`[key]`\|`~tier`）* |
| syn/sem/prag rule `field => …` | 一個**已宣告 feature 名** | 單一 identifier |
| phon rule `b => p` | **音韻 pattern**（非 field） | Tshiatūn 改寫式 |

### 5.4 case/when、else/then

- `case:`＝FirstMatch（第一 Matched 勝）；`when:`＝Accumulate（所有 Matched 依來源序合併）。
- `else` 鏈＝Lexurgy 第一匹配 fallback；`then` 鏈＝循序（下一支讀更新後狀態）；**同一 rule 上互斥**。
- 四者皆 ordered，位置語意由 §3.3–3.5 的符號式 determiner 表達。

### 5.5 structured phon 的顯式 bootstrap 與 source insert

Flat rule 不因 `insert` 偷偷改變資料形狀；先用 block-valued update 明確建立 root：

```chg
update trait("Core").block[0].rule["shift"].phon_block:
    a => b
    Then:
        b => c
```

之後 rule／leaf／Then／Else container 都可接受真實 `.lang` phon statement 或完整 sub-block：

```chg
insert into trait("Core").block[0].rule["shift"].then[1] at end:
    c => a

insert into trait("Core").block[0].rule["shift"] at end:
    Then propagate:
        c => b
```

上述表層分別降為 `Update{PhonBlockRoot}` 與 `Insert{PhonStatement|PhonBlockNode}`；
`dump→parse→resolve` 固定點由 `phon_authoring.rs` 驗證。leading boundary root 與
structured → flat 仍無合法便利語法，會明確拒絕。

---

## 6. 降階與前置

| 表層 | 降階為四原語 |
|---|---|
| `insert … sign/trait …` | 1×`Insert{Sign/Trait}` |
| `insert … 維/sign/case/else 片段`（N items） | **N×`Insert{Item/Branch}`**（同一 statement，只驗最終態） |
| `insert … branch` | 1×`Insert{CaseBranch/RuleElse/RuleThen/RealizationBranch}` |
| `update …field = v` | 1×`Update` |
| `update …dim:` fragment | **delete 舊 + insert 新**（同一 atomic statement） |

**兩個底層前置（必先補）：**

1. **§② item／branch 的 dump·parse 對稱**：現 `dump` 只序列化 `Insert{Sign}`，其餘吐空 →
   片段降階出的 `Insert{Item/Branch}` 無法落盤、round-trip 破。**第一步。**
2. **§④ 一句多原語**：片段＝N primitive、整替＝delete+insert，都要「一句多 edit、只驗最終態」。
   原語層已支援（`statement_may_temporarily_dangle` 證過），只差表層 parser 放行。

**驗證零新增**：複用 `item_allowed_in_context(item, dim)` 與 `expression_matches_type` 把 `.chg`
開口接上既有維度驗證。

---

## 6.5 實作進度（2026-07-24，branch `wuc-claudecode`）

已落地（`crates/changeset`，`insert_block.rs` 10 + `update_fields.rs` 3 + `clone_keyword.rs` 5；
crate 320 綠、clippy 0）：

- **通用 insert**：`insert into <target> at <pos>:` ＋ 逐字 `.lang` block → 頂層 `Symbol`/`Class`、`trait NAME:` 或
  **sign body items**。因**重用 `.lang` parser（wrapper-synthesis）**，直接涵蓋整個 `SignItem`
  分類法（slot/rule/**phon Tshiatūn 規則**/feature/Def/…），非逐 kind 硬接。
- **§④ fan-out + 一句多原語**：`resolve_operation → Vec<PrimitiveEdit>`；多 item block 展成
  N 個 `Insert`（同 statement、只驗最終態、來源序）；`parse_statement_body` 依縮排切多 operation
  chunk（indent-0 = 新 operation），單句可含多操作，皆定址 statement 起始態（atomic snapshot）。
- **dump 對稱**：`Insert{Trait}`／`Insert{Item}` 經 wrapper-print 還原；正規形＝每 primitive 一
  block；`dump→parse→resolve→dump` 逐位元穩定。
- **update 欄位（8→14）**：＋`trait.global`／`slot.optional`／`belongs.target`／`rule.dim`／
  `rule.stage`／`realization.guard`（對稱 `update_for`／`dump_update`）。

已落地（續）：

- **nameless 定址定案**：`sign("x").rule[n]`／`.else[m]`／`.then[m]`／`.realization[k]`（§3.1），
  resolve 釘成 `node(<kind>,@ns:ord)`。
- **具名標籤（P 系列取徑 B，owner 裁定；不動 sign 扁平結構）**：`Rule`/`TypedCase`/`CaseBranch`
  各加 `name: Option<String>`，`@name <label>` 後綴宣告（rule 為 `@stage` 之內、case/branch 為 `:`
  之前），printer 僅 Some 時輸出（unnamed 零 golden churn）。keyed 定址 `rule["x"]`／`case["x"]`／
  `branch["x"]`（`["…"]` 或非數字＝keyed，數字＝序數），dump 仍釘穩定 `node(kind,@id)`。
  測試 `named_addressing.rs` 4。
- **else/then branch insert**：`insert into sign("x").rule[n] at <pos>:` ＋ `else <body>`／
  `then <body>` → `Insert{RuleElseBranch/RuleThenBranch}`；`before/after <sibling-path>` 定序；
  dump 對稱 round-trip；**else/then 互斥被強制**（edit 重序列化 re-parse，parse-time 不變式把關）。
  測試 `branch_insert.rs` 4。

已落地（續）：

- **case-branch insert**：`insert into sign("x").case["…"] at <pos>:` ＋ branch block →
  `Insert{CaseBranch}`（wrapper 以 target case 的 keyword/scrutinee 重建語境解析；多 branch fan
  out）；dump 經 SignContext wrapper 還原、round-trip 穩定。**限 SignContext `case:`/`when:`**。
  `case[n]`/`case["x"]`／`branch[m]`/`branch["x"]` 定址已補（§3.1）。
- **structured phon authoring convenience**：`update <phon-rule>.phon_block:` 顯式
  flat→structured；既存 structured rule／leaf／Then／Else 可從 source fragment 插入
  statement 或完整 Then／Else／propagate sub-block。flat rule 的直接 insert 明確拒絕，
  不做 silent bootstrap；resolved dump 往返穩定且 codegen 交 Tshiatūn 實收。

尚未落地（皆有明確原因，非遺漏）：

- **非 SignContext case 的 branch insert**（Feature/Syn/Sem/Prag/Phon typed case）——須依 target
  expected type 重建 typed 語境 wrapper，明確拒絕（`supports SignContext … only`）。
- **realization-branch insert**——realization branch 只在 `realization:` 內解析，且父 Realization
  node 尚無定址段。
- **符號式確定詞**（before else／after guard／#n，§3.3–3.5）——目前以 `.else[m]`/`.rule[n]` 路徑段
  等效表達；符號糖為便利層。
- **剩餘 struct 值 update 欄位**（SlotConstraint/FeatureValue/RoleBinding/SlotMap/Constraint/
  SignApplication/CaseBranch）——各需值語法子 parser。
- **③ set-ops**：`set distribution[key]=v` 及其 dump 白名單。
- **Tier-2 維度片段整替**（`update <sign>.<dim>:` = delete 舊 + insert 新）。

## 7. 實作順序建議

1. **§②** item／branch insert 的 dump·parse 對稱（含 ③ 三項進 dump 白名單）。
2. `insert into … at …:` ＋ block dispatch（先 `syn/sem/prag/phon` 維片段，再 `sign:` 匿名、branch）。
3. 有序 list 的 `before/after guard|else|#n` 確定詞 ＋ §3.5 兩條安全設計。
4. **§④** atomic 多原語 statement。
5. `update` Tier-1 詞彙泛化（`update_for`／`dump_update` 對稱補 28）。
6. `update` Tier-2 維度片段整替；`③` 的 `set distribution[...]`。

不逐 kind 硬幹，而按**降階形狀**收斂成 4 條泛型路徑：容器級 node insert／item 級 insert／branch 級
insert／update 欄位泛化。

---

## 8. 邊界

- 不承諾舊 v1 `.chg` 相容（v1 已硬移除；見
  `docs/verification/Step14_ChangeSetInterpreter_封板_v1.md`）。
- Application 是 expression 內部件、非獨立 insert target（有 `SignApplication` update 但無定址位）。
- 本文為 v0 設計稿；封板前不得宣稱已實作。實作各步以測試出口收尾（CLAUDE.md §0-2）。
