# 歷時 `.chg` 授權教學 v1

`.lang` 保存**共時**語言知識；`.chg` 是對 `.lang` 的一次**可 replay 的編輯**。
這一章從最小的一句編輯開始，走完 selector、位置、四原語、原子性與身分配號。

> 本檔每個 `chg` 區塊都由 `crates/changeset/tests/tutorial_chg.rs` 實際
> parse → resolve → replay，並比對教學宣稱的結果。改教學要一併改指示註解。

---

## 0. 邊界：`.chg` 改什麼、不改什麼

`.chg` 只對 **caller 的 `LanguageDocument`** 執行 Insert／Delete／Update／Move
四個原語。它**不**改 effective libraries、`CompiledSystem`、`DerivedToken` 或
surface——那些是推導期的產物，不是語言狀態。resolve 之後的 ChangeSet 只含四原語；
所有表層便利語法（`clone`、block payload、路徑段定址）都會降階掉，不留痕。

本章的 base 語言是這一份：

<!-- chg-test: base -->
```lang
Symbol d
Symbol o
Symbol g
Symbol k
Symbol a
Symbol t

Class vowel {o, a}

trait LocalNoun:

sign dog:
    belongs LocalNoun
    syn:
        feature:
            number = enum(sg, pl)
            number => sg / [LocalNoun]
    phon:
        /dog/

sign kat:
    belongs LocalNoun
    phon:
        /kat/
```

---

## 1. 檔案骨架與三道 digest

一份完整的 `.chg` 長這樣：

```text
changeset evo:demo:
    schema = conlang.changeset/v1
    base_source = sha256:4dce31cb…
    base_identities = sha256:39dc5028…
    library std:core@0.1.0 sha256:89ed0791…
    library std:cxg@0.1.0 sha256:6af29a7d…
    library std:grambank@1.0-subset.1 sha256:9f8a01ab…
    library std:grammaticalization@0.4.0 sha256:b54ed634…

    #0:
        <操作>
```

前言鎖住三件事：base `.lang` 的原始碼、base 的 identity manifest、以及每個
stdlib package 的內容。任一項對不上，resolve 在 replay 之前就拒絕
（`CHANGESET_BASE_SOURCE_MISMATCH`／`CHANGESET_BASE_IDENTITIES_MISMATCH`／
`CHANGESET_LIBRARY_LOCK_MISMATCH`）。

**這些 digest 全是衍生值，不得手改。** 要更新只能重生——`tutorials/en-standard-reconstruction/`
的 README 記著一次教訓：有人把兩行 sha256 手改成算不出來的值，那份材料就變成
`resolve` 不過的死檔。

所以本章往下的 `chg` 區塊只寫**語句部分**；前言由工具依 base 算出來。這正是
digest 該有的使用方式。

---

## 2. 第一句編輯

語句以 `#N:` 標記（`N` 由 0 起算），其下縮排的是操作。整份文件的註解用
`/* … */`——`#` 已經被語句標記佔走了。

<!-- chg-test: base=self:1; ns=evo:a -->
```chg
    /* 新增一個 trait */
    #0:
        insert into language at end:
            trait Nocturnal:
```

replay 之後的 `.lang`：

<!-- chg-test: result -->
```lang
Symbol d
Symbol o
Symbol g
Symbol k
Symbol a
Symbol t
Class vowel {o, a}

trait LocalNoun:

trait Nocturnal:

sign dog:
    belongs LocalNoun
    syn:
        feature:
            number = enum(sg, pl)
            number => sg / [LocalNoun] @stage word
    phon:
        /dog/

sign kat:
    belongs LocalNoun
    phon:
        /kat/
```

有兩處和你寫的 base 不一樣，都不是 `.chg` 做的，而是 `.lang` 的**正規形**：
Symbol 群後面的空行沒了，規則補上了預設的 `@stage word`。canonical source 是
唯一的比較基準，教學後面的預期輸出都是這個形式。

---

## 3. 單一文法原則

```text
.chg = 框架動詞(insert / update / delete / move / clone / at / selector)
      + .lang block(逐字)
```

框架只負責**定位**與**動作**；括號裡的內容永遠是 `.lang` 片段，由既有的 `.lang`
parser 解析。所以「能 insert 什麼」＝「`.lang` 能寫什麼關鍵字」，一一對應，
`.chg` 不自創任何節點型別。

payload 的**首關鍵字**決定插進去的是什麼：

| block 首關鍵字 | 產生 | 合法 target |
|---|---|---|
| `Symbol NAME`／`Class NAME {…}` | DslDeclaration | `language` |
| `trait NAME:` | TraitDef | `language` |
| `sign NAME:` | SignDef（**配全新身分**） | `language` |
| `syn:`／`sem:`／`prag:` | 該維的 items | sign |
| `phon:` | 模板／Tshiatūn 規則／`realization:` | sign |
| `belongs X` | Belongs | sign |
| `slots:`／`slot_features:`／`feature:`／`roles:` | 對應 items | sign 的該維 |
| `else …`／`then …` | rule 的分支 | rule |

因為是同一套 parser，phon 的 Tshiatūn 規則直接就能寫：

<!-- chg-test: base=self:1; ns=evo:b; expect=g => k / _ # @stage word; expect=/dog/ -->
```chg
    #0:
        insert into sign("dog") at end:
            phon:
                g => k / _ #
```

原本的 `/dog/` 模板留著，規則接在它後面。

### 一個 block 可以有多個 item

多 item 的 block 會 **fan out** 成 N 個 `Insert`，同一句、依來源序：

<!-- chg-test: base=self:1; ns=evo:c; expect=agent [LocalNoun]; expect=theme [LocalNoun] -->
```chg
    #0:
        insert into sign("dog") at end:
            syn:
                slots:
                    agent [LocalNoun]
                    theme [LocalNoun]
```

resolve 後會看到兩個 `insert into node(sign, @evo:root:10) at end:`，各帶一個
slot——正規形是每個 primitive 一個 block。

---

## 4. selector：怎麼指到節點

| 寫法 | 指向 |
|---|---|
| `language` | 文件根 |
| `sign("dog")`／`trait("LocalNoun")` | 依名字 |
| `sign("dog").def[phon]` | keyed 路徑段（`.def[path]`／`.slot[name]`／`.role[name]`） |
| `sign("dog").rule[0]` | 序數路徑段（`.rule[n]`／`.else[m]`／`.then[m]`／`.realization[k]`／`.block[n]`） |
| `sign("dog").rule["名"]` | 具名標籤（`@name` 宣告過的 rule／case／branch） |
| `node(sign, @evo:root:10)` | 穩定 NodeId — **canonical 形式** |

**名字與路徑只是授權期的方便寫法。** resolve 會把它們全部釘成
`node(<kind>, @<ns>:<ordinal>)`，dump 出來的一律是穩定形。理由很實際：序數對
重排敏感、名字對 rename 敏感，而一份要能長期 replay 的 `.chg` 不能依賴這兩者。

無名節點（rule、branch、case）就是靠路徑段定址的。往一條既有規則追加 `else`
分支：

<!-- chg-test: base=self:1; ns=evo:h; expect=else number => pl -->
```chg
    #0:
        insert into sign("dog").rule[0] at end:
            else number => pl
```

`sign("dog").rule[0]` 是 `number => sg / [LocalNoun]` 那條，resolve 後釘成
`node(rule, @evo:root:13)`。

找不到目標時的錯誤會指名是哪一句——rebase 最常見的衝突就是這個：

<!-- chg-test: base=self:1; ns=evo:k; error=unknown sign "wolf" -->
```chg
    #0:
        delete sign("wolf")
```

```text
CHANGESET_STATEMENT_0_SELECTOR: unknown sign "wolf"
```

---

## 5. 位置：`at <determiner>`

`at start`／`at end`／`at before <sel>`／`at after <sel>`。

但要分清楚兩種 list：

| list | `at end` 的意思 |
|---|---|
| **canonical-unordered**（Sign／Trait／Distribution） | **佔位符**——引擎忽略它，真實順序由名字排序算出來 |
| **ordered**（sign 的 items、branch 鏈、realization 分支） | 字面 append |

下面這句同時改名和把 trait 升為 global，注意結果裡 `cat` 跑到 `dog` **前面**去了：

<!-- chg-test: base=self:1; ns=evo:e; expect=global trait LocalNoun:; expect=sign cat: -->
```chg
    #0:
        update sign("kat").name = cat
        update trait("LocalNoun").global = true
```

沒有人要求它移動，是 canonical 排序讓 `cat` < `dog`。所以對 Sign／Trait 而言
`at end` 從來不代表「放到最後」。

sign 的 items 則是字面順序，`at start` 真的會放到最前面：

<!-- chg-test: base=self:1; ns=evo:j; expect=agent [LocalNoun] -->
```chg
    #0:
        insert into sign("dog") at end:
            syn:
                slots:
                    agent [LocalNoun]
    #1:
        move sign("dog").slot[agent] to sign("dog") at start
```

`move <node> to <parent> at <anchor>`——注意是 `to`，不是 `under`。

---

## 6. `update`：純量欄位

`update <selector>.<field> = <value>`。目前接上表層的欄位：

| 目標 kind | field |
|---|---|
| Sign／Trait | `name` |
| Trait | `global` |
| Definition | `path`、`value` |
| Rule／FeatureRule | `body`、`dim`、`stage` |
| else／then branch | `body` |
| Slot | `name`、`optional` |
| Belongs | `target` |
| RealizationBranch | `template`、`guard` |
| Case | `selection`（`case`／`when`） |

改一個 sign 的底層音韻模板：

<!-- chg-test: base=self:1; ns=evo:d; expect=/dok/; absent=/dog/ -->
```chg
    #0:
        update sign("dog").def[phon].value = /dok/
```

值的型別會驗——`update trait("LocalNoun").global = maybe` 會被擋下來，錯誤訊息
會告訴你這裡只收 `true`／`false`。

---

## 7. `delete` 與 `clone`

`delete <selector>` 刪整棵子樹：

<!-- chg-test: base=self:1; ns=evo:f; absent=sign kat:; expect=sign dog: -->
```chg
    #0:
        delete sign("kat")
```

`clone <sign-selector> as <name>` 是授權糖，降階成**一個** `Insert`：

<!-- chg-test: base=self:1; ns=evo:g; expect=sign hound:; expect=sign dog: -->
```chg
    #0:
        clone sign("dog") as hound
```

resolve 後會看到它其實是 `insert sign under node(language, @evo:root:0) at end:`
帶著整棵深拷貝。因為走的是 `Insert`，clone 產物會**重配 SignId／RuleId／NodeId**
——它是新實體，身分與來源完全獨立，來源一個位元都不動。

`Delete` ＋ `Insert` 必然換 ID；`Update`（rename）保 ID；`Move` 保整棵子樹的 ID。
要保身分就別用「刪掉重建」。

---

## 8. 原子性：一句 vs 多句

**一句之內**可以有多個操作。它們全部定址**該句起始時**的狀態，一起套用，
最後只驗一次最終態：

<!-- chg-test: base=self:1; ns=evo:multi; expect=/dok/; absent=sign kat: -->
```chg
    #0:
        update sign("dog").def[phon].value = /dok/
        delete sign("kat")
```

中途允許暫時性的不一致（`statement_may_temporarily_dangle`），只要句末合法就好。

**跨句**則是循序的：後一句看得到前一句的結果。所以下面第 1 句才敢 `belongs`
一個第 0 句剛剛才建立的 trait：

<!-- chg-test: base=self:1; ns=evo:i; expect=trait Nocturnal:; expect=belongs Nocturnal -->
```chg
    #0:
        insert into language at end:
            trait Nocturnal:
    #1:
        insert into sign("dog") at end:
            belongs Nocturnal
```

把這兩個操作塞進同一句會失敗——`belongs Nocturnal` 在句首狀態裡找不到那個 trait。

---

## 9. 身分配號：新節點屬於誰

replay 時新建的節點掛在**這份 changeset 的 namespace** 底下，不是 base 的。
上面 `move` 那個例子 resolve 出來是：

```text
    #1:
        move node(slot, @evo:j:0) to node(sign, @evo:root:10) at start
```

`@evo:root:10` 是 base 帶來的 dog，`@evo:j:0` 是這份 changeset（namespace
`evo:j`）新建的 slot。identity manifest 因此看得出每個節點是哪一次演化引入的，
`.chg` 的 replay 也才會是決定性的——同一份 base ＋ 同一份 `.chg` 逐位元同樣的結果。

---

## 10. 舊寫法與正規化

`statement 0:` 是舊形，**仍然接受**，但 dump 一律排成 `#0:`。這和 `.lang` 把
`key = value` 正規化成 `key: value` 是同一個作法：非 canonical 的輸入正規化成
不動點，`dump → parse → resolve → dump` 逐位元穩定。

---

## 11. 目前的邊界

以下在設計稿裡有、但**尚未落地**，遇到會明確拒絕而不是靜默近似：

- 非 SignContext 的 typed case（Feature／Syn／Sem／Prag／Phon）branch insert
- realization-branch insert（父 Realization 節點還沒有定址段）
- 符號式確定詞（`before else`／`after guard "…"`／`#n`）——目前用 `.rule[n]`／
  `.else[m]` 路徑段等效表達
- 部分 struct 值的 update 欄位（SlotConstraint／FeatureValue／RoleBinding／
  SlotMap／Constraint／SignApplication／CaseBranch）
- `set distribution[key] = value`
- Tier-2 維度片段整替（`update <sign>.<dim>:` = delete 舊 + insert 新）
- flat phon rule 的直接 insert——要先用
  `update <phon-rule>.phon_block:` 顯式 bootstrap 成 structured，不做 silent 轉換

完整清單與理由見
[`docs/implementation/chg_authoring_insert_update_v0.md`](../docs/implementation/chg_authoring_insert_update_v0.md)。

---

## 12. 另一種 `.chg`：定義文件

`.chg` 還有第二種模式——**函數定義文件**。它以 `package` 開頭而不是
`changeset`，因為定義是函數、不綁任何 Language，所以**沒有 base digest**：

```text
package std:grammaticalization:
    schema = conlang.functions/v1

function VerbToTense(verb [Verb], tense):
    drift(verb, sense: core, gloss: tense)
```

參數帶 slot 風格的約束（`[Verb]`），body 的語意由既有的 `case:`（選一）、
`when:`（收集）與純序列（全跑）承載——Recipe／Goal 不是關鍵字。定義文件裡不能
寫 statement，可 replay 的編輯屬於 changeset 文件。這一塊的細節見
`crates/changeset/tests/function_definitions.rs` 與 `function_guards.rs`。

---

## 13. 從哪裡繼續

- [`共時lang語法教學_v1.md`](共時lang語法教學_v1.md)：`.chg` 的 payload 就是
  `.lang` 片段，所以那份是本章的前置。
- [`CLI操作教學_v1.md`](CLI操作教學_v1.md)：`conlang evolve` 產生的就是 `.chg`。
- [`en-standard-reconstruction/`](en-standard-reconstruction/README.md)：一份
  由工具生成的真實 `.chg`，也是「digest 不得手改」那條規則的案發現場。
