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

### 尚未落地(staged)
- **S2 縮排 body + `Then:`/`Else:` 巢狀**:把 phon rule 升級成結構化 block IR
  (`Simultaneous/Sequential/FirstMatching/Propagate`,1:1 對映引擎 `RuleBlock`),取代扁平
  `else_chain`/`then_chain`。**解 L1/L2/L5**。
- **S3 語句級定址 + 四原語**:`rule["lenition"].then[0].stmt[k]` 定址/insert/move。**解 L2**。
- **S4 `propagate` 語法**(header/邊界)。**解 L3**。

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
