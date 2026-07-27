# 架構修補09 — phon 命名 block / Lexurgy 對齊(P46)

> **P 系列權威**:本檔定稿 **P46**。承 P44(維度隔離)、P45(具名可定址節點)。
> owner 於 2026-07-24 裁定 **取徑 A**:phon 的規則命名/塊語法**對齊 Lexurgy/tshiatūn `.qy`**
> (`name:` 前綴 + `Then:`/`Else:` 塊),而非 P45 的 `@name` 後綴。範圍限 **phon 維**——
> syn/sem/prag 的 P43 三分 Else 是 per-sign 守衛,另一機制,不受影響。

## 背景:else/then 的 operand 是 block

引擎 IR(`tshiatun/crates/dsl/src/ast.rs`):
```
RuleBlock = Simultaneous(Vec<Stmt>)     # 葉:多語句同時套用
          | Sequential(Vec<RuleBlock>)  # Then: 逐 block commit 接力(全跑)
          | FirstMatching(Vec<RuleBlock>)# Else: 第一個「match」的 block 整組勝出、其餘不跑
          | Propagate(Box<RuleBlock>)     # 迭代到 fixpoint
```
- **`Then:`** = block 依序,前一 block commit 後下一 block 讀更新後 word,全跑。
- **`Else:`** = 對每個作用範圍,**第一個 match 的 block 整組套用、其餘跳過**(`break`);
  match(非輸出改變)決定——`a => a` 也算 match、擋掉後續 Else(`exec.rs`、tutorial 04)。
- **operand 是 block(可含多語句、可巢狀),不是單一 rule。**

## 真實 Lexurgy/tshiatūn 語法(權威)

出處:`tshiatun/crates/dsl/tests/block_ir.rs`、`parser.rs`、`examples/*.qy`。
```
lenition:                 # 命名 rule = block(name: 前綴;名可含連字號 dock-tone:)
    stage: word
    a => b
    Then:                 # Sequential 邊界(縮排子 block)
        u => o
        o => c
    Else: c => e          # FirstMatching 邊界(inline 形)
```
- `Then:`/`Else:`(首字大小寫皆可)後接 inline 語句或縮排子 block;邊界前語句 = 第一 block。
- `propagate` 是修飾:header(`harmony [vowel] propagate:`)或邊界(`Then: propagate:`)。
- **無 `block` 關鍵字**;`name:` 命名 rule **就是** block。

## 現行 `.lang` else/then 有損處(此提案要補)

現行:`Rule { body: String, else_chain: Vec<String>, then_chain: Vec<String> }`。

| # | 有損 | 引擎有、`.lang` 無 |
|---|---|---|
| L1 | **無巢狀/混用**:else/then 扁平且互斥 | `Sequential`/`FirstMatching` 可互相巢狀 |
| L2 | **branch = opaque String**(`;` 切語句),語句無法定址 | block 是結構、語句是節點 |
| L3 | **無 `propagate`** | header/邊界修飾 |
| L4 | **命名慣例**:P45 `@name` 後綴 vs Lexurgy `name:` 前綴 | — |
| L5 | **主/支不對稱**:主 body 是 Rule 物件、分支是裸字串 | 全部是 RuleBlock |

> 扁平單層的**語意**是對的(else_chain→Lexurgy Else=FirstMatching;then→Sequential);
> 有損是**結構性**:巢狀、語句級定址、propagate、命名、主/支對稱。

## P46 決策 + 分期實作

### slice 1(已落地 2026-07-24)
- **phon `name:` 前綴(inline)**:`lenition: a => b` → 具名 rule(`parser.rs::lexurgy_name_prefix`,
  保留字 Scan/stage/Then/Else/realization/case/when/propagate 除外;名可含連字號)。
- **printer**:phon 具名 rule canonical 用**前綴** `name: body @stage`(其他維維持 P45 `@name` 後綴)。
- **codegen**:具名 phon rule 排放 Lexurgy `name:` 標籤(非合成 `rN:`)。
- **定址**:`sign("x").rule["lenition"]`(複用 P45 keyed 定址;name 由前綴設)。
- 測試 `codegen.rs::phon_named_rule_uses_lexurgy_name_prefix_and_label`;workspace 290 綠、
  無 golden churn、clippy 0。

### slice 2(已落地 2026-07-24)
- **結構化 `PhonBlock` IR**(`Leaf`/`Then`/`Else`/`Propagate`,1:1 對映引擎 `RuleBlock`),
  掛為 `Rule.phon_block: Option<PhonBlock>`——**避開新 SignItem 的 36-arm cascade**,且
  `else_chain`/`then_chain` 保留給共用的 P43 路徑(phon rule 對 synchronic 為 no-op)。
- **parser**:phon 裸 `name:` 頭 + 縮排 body → `parse_phon_block`(遞迴;`Then:`/`Else:` inline
  或縮排;同層不得混)。**printer/codegen** 遞迴排放 `.qy` block。round-trip 穩定。
- 測試 `codegen.rs::phon_structured_block_then_and_else_codegen_flat`;workspace 291 綠。
- **界線(已於 slice 4 解除)**:當時**巢狀 Then/Else 由引擎限制**——tshiatūn 舊 parser 只收
  **flat 單層** Then/Else,codegen 出的巢狀 `.qy` 被引擎拒。**L1 部分解**(單層 ✓)。
  → **slice 4 補齊**(見下)。

### slice 3(已落地 2026-07-27)
- **phon block 語句級定址 + 四原語(解 L2)**:phon block 內每一條語句(`Leaf` 一行)與
  每一個子 block(`Then`/`Else` 一 element)皆為穩定可定址節點,經 insert/delete/update/move
  編輯。定址**全遞迴**、沿用選擇器 `.leaf[k]`/`.then[n]`/`.else[n]`(owner 裁定);phon 規則的
  flat `then_chain`/`else_chain` 為空,故有 `phon_block` 時 `.then`/`.else` 路由進 block,
  否則維持既有 flat 行為(未具 phon_block 的 rule 不受影響)。
- **identity**(`identity.rs`):`NodeKind::PhonStatement`/`PhonBlockNode`、`AddressSegment`
  `PhonLeaf`/`PhonThen`/`PhonElse`/`PhonPropagate`;`enumerate_phon_block` 遞迴走訪(僅
  `phon_block` 為 `Some` 時,決定性=位址序,P26)。`Propagate` 透明遞迴(S4 再補其編輯)。
- **changeset**(`lib.rs`):`DetachedNode::PhonStatement`/`PhonBlockNode`、phon 導覽 helper
  (`split_phon_address`/`walk_phon_block(_mut)`/`phon_container_block(_mut)`/`phon_statement_at_mut`)、
  四原語具現化(insert/delete/update/move)、`resolve_path_child` 的 `leaf`/`then`/`else` 路由、
  `kind_at`/`child_addresses`/`address_list_position`/`sequence_tag`/`kind_keyword`/`parse_kind`
  補齊。leaf 插入 `.chg` 語法 `leaf <stmt>`;statement update 複用 `body` 欄位(`RuleBranchBody`)。
- 測試 `changeset/tests/phon_block_edits.rs`(11 案:定址/四原語/巢狀 depth-2/round-trip+決定性/
  越界與 wrong-kind 負例;mutation-tested)。workspace 302 綠、clippy 0、引擎零觸動、wasm 綠。
- **界線**:sub-block 的**從源插入**(整個新 `Then`/`Else` 子樹)延後——move 既存 sub-block
  已支援(detach→reattach,無需 `.qy` 解析);bootstrap 空 rule 成 block 亦延後。

### slice 4(已落地 2026-07-27)= **L1 完整解**
- **codegen 出大括號 `{ }` 巢狀 `.qy`**,接上引擎(tshiatūn `wuc-claudecode` / PR #1)的
  grouped-block parser。巢狀 Then/Else **端到端貫通**:`.lang` 巢狀 `PhonBlock` →
  `codegen::emit_phon_block` 對**複合(compound)元素**包 `{ }`(leaf 元素維持裸露)→ 引擎
  brace parser 收下。**扁平單層維持無括號、與舊輸出逐字相同**(零 golden churn)。
- 對映:`Then([Leaf, Else([…])])` → `a => b` + `Then: { … }`;首元素若為複合(S3 move 可致)
  → 開頭 `{ … }` group(引擎 GroupOpen)。`Propagate` 目前透明遞迴(propagate 關鍵字待 S4)。
- 實作:`codegen.rs::emit_phon_block` + `is_grouped_element`。測試
  `language/tests/phon_grouped_codegen.rs`(4 案:巢狀→braces+引擎收/扁平無括號/round-trip/
  首元素複合開 group;mutation-tested)。workspace 306 綠、clippy 0、golden 零 churn。
- **界線(已於 S4 解除)**:`Propagate` 尚未排 `propagate` 修飾詞(語意暫失)。

### S4(已落地 2026-07-27)= **L3 解**
引擎 `.qy` 的 propagate 有**兩處**修飾詞,S4 兩者皆補上 `.lang` 對應:

| 位置 | `.qy`/`.lang` 語法 | 語意 | `.lang` 承載 |
|---|---|---|---|
| header | `name propagate:` | 整條 rule 迭代到 fixpoint | `Rule.propagate: bool` |
| boundary | `Then propagate:` | 只重複**該邊界引入的那個 element** | `PhonBlock::Propagate` |

- **修好三個既有缺陷(非單純補功能)**:此前 (1) `PhonBlock::Propagate` 經 printer/codegen
  **靜默丟棄**(規則不再迭代 = 語意腐蝕);(2) `.lang` 寫 `Then propagate:` 會被當成**普通語句**
  塞進 Leaf、巢狀被壓平;(3) 寫 `name propagate:` 會**摧毀 block 結構**(降成扁平 rule)。
  三者皆為靜默錯誤,現皆已修並各有回歸測試。
- **`Propagate` = 修飾詞,不是層級**(關鍵設計):它**不佔位址節段**——`AddressSegment::PhonPropagate`
  (S3 引入)**移除**,`enumerate_phon_block`/`walk_phon_block`/`push_phon_children` 透明穿過。
  理由:`sync_identity_descendants` 以 `(address, kind)` 重用 id,若 propagate 佔節段,
  **切換 propagate 會讓底下每條語句換新 id**,「同一條語句」的身分斷裂(違 P25/P26)。
  透明後 `then[1].leaf[k]` 定址不因 propagate 而變,切換 0 身分churn。
  (安全性:S3 之前無任何路徑能產生 `Propagate`,故無既存 sidecar 含該節段,移除免遷移。)
- **編輯**:`update <rule>.propagate = true|false`(header)與 `update <block-node>.propagate = …`
  (boundary,就地 wrap/unwrap)。`EditableField::Propagate`、`NodeUpdate::Propagate(bool)`;
  `.chg` dump/parse round-trip。負例:對扁平 else/then 鏈 rule 設 propagate → 明確拒絕。
- **顯式拒絕**:block **首元素**帶 Propagate(僅 S3 move 可致)在 `.qy` 無處掛修飾詞 →
  `CodegenError::LeadingPropagateUnsupported`,**不默默丟棄**。
- 測試:`language/tests/phon_propagate.rs`(8 案)+ `changeset/tests/phon_propagate_edits.rs`
  (7 案,含 **identity 穩定性** property)。**mutation-tested 5 種**(codegen/printer 丟 boundary
  修飾詞、codegen 丟 header 修飾詞、首元素靜默丟棄、Propagate 重新佔節段)全數被抓。
  workspace **321 綠**、clippy 0、引擎零觸動(166)、wasm 綠;golden 僅 `Rule` 新欄位
  `propagate: false` 三行純新增(無語意變動)。

### 尚未落地(staged)
- **sub-block 從源插入 / bootstrap 空 block**(S3 界線)。
- **submodule 重釘**:slice 4 已把 gitlink 釘到引擎 `wuc-claudecode` 的 brace-parser commit
  (PR #1 分支)以維持本分支自洽;PR #1 merge 到引擎 main 後,再重釘到 merge commit。

## 相容性
- **P44**:block 屬 phon 維。相容。
- **P45**:phon 用前綴、其他維用後綴(dim-aware printer);keyed 定址機制共用。**組合**。
- **P43**(syn/sem/prag Else):不受影響(per-sign 三分,另一機制)。
- **P24/P25/P26**:name 是節點欄位值,樹上封閉、決定性;dump 序數釘穩定 id。
- 向後相容:未具名 phon rule 與現行扁平 else/then 鏈保留;`name:` 前綴 opt-in。

## 待裁(S2 前)
1. 保留舊扁平 else/then 鏈 vs 以結構化 block 取代並遷移?(建議保留、漸進。)
2. Leaf 內多行 = Simultaneous(同時)還是隱含 Then?(建議對齊引擎:同 Leaf = Simultaneous;
   巢狀 Then 需顯式 `Then:`。)
3. `propagate` 語法形。
