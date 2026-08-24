# 共時 `.lang` 語法與資料貼合度（v0.1）

## 1. 邊界

`.lang` 是一個語言節點的**共時 construction-type inventory**：分類、四維內容、
slot/valence、共時規則與少量 sign 狀態。歷史語料中的實際使用例不是 construction
type；年代、文本位置、原文/正規化、分析者、可信度與引用來源應由後續
Evidence/Attestation 側表承載，歷時改變則由 ChangeSet 承載。

這條邊界沿用歷史語言紀錄的 type/token 分工：Language 可保存輕量 `origin` 指標，
但不把整份詞源史或一次 attestation 複製進每個 Sign。

## 2. 完整表面

```lang
sign NP:
    belongs CommonNoun
    origin = sign(proto_np)
    provenance = grammaticalized
    lifecycle = active
    entrenchment = 0.75
    lexicalized = true
    syn:
        slots:
            det [Determiner]?
            head [CommonNoun]
        map det autofill article
        map det internalize
        map head rename nucleus
        map head optional false
        licensing.register = general
    phon:
        /{$slot.det}{$slot.head}/
    sem:
        senses[core].concept = DEFINITE_NP
        profile = {$slot.head}
```

Slot mapping 是平坦、動詞式語句，避免再造第二套巢狀物件格式：

| 語法 | typed 操作 | 效果 |
|---|---|---|
| `map x preserve` | `Preserve` | 保留原外部 slot 名 |
| `map x rename y` | `Rename` | 外部名稱改為 `y`，內部語意/模板仍以 `x` 定址 |
| `map x autofill sign_name` | `AutoFill` | 由庫內 Sign 固定填入 |
| `map x internalize` | `Internalize` | 角色仍在內部，但不再對呼叫端暴露 |
| `map x optional true/false` | `Optional` | 覆寫 required/optional |

source mapping 與呼叫端 Rust mapping 合成後，先完整驗證未知 slot、同類重複操作、
名稱碰撞、未填的 internal required slot、autofill sign 與範疇，再原子套用。

## 3. 語法—AST—行為貼合矩陣

| 資料 | `.lang` 表面 | AST/typed view | compile/runtime | 貼合度 |
|---|---|---|---|---|
| DSL 音段宣告 | Language 區前 verbatim | `dsl_decls` | 交 Tshiatūn | 完整；刻意不由 language 重解 |
| DSL 韻律域宣告 | DSL 區 `Prosody LEVEL < …` | `dsl_decls` | 交 Tshiatūn | 完整；小寫 `prosody = …` 已廢棄 |
| distribution override | `distribution:` | 有序 key/value | 目前資料保存/round-trip | 語法完整，抽樣消費者屬後續 E |
| trait/sign/global trait | colon+縮排、`==` | `TraitDef/SignDef/Block` | compile pipeline | 完整 |
| 分類/繼承 | `belongs Name` | `SignItem::Belongs` | ontology closure/diagnostics | 完整 |
| macro 引用 | `Name`、`Name[n]` | `TraitUse` | expansion | 完整 |
| 四維 scalar/structured path | 維度內 `path = value` | `Def` + `Path` | projection/patch/rules | 完整；lhs 支援 `.`、`[key]`、`~tier` |
| phon UR/template | `/…/`、`{$slot.NAME}` | `Def("phon")` | construction + phon runtime | 完整 |
| valence | `slots:` + `NAME [Trait]?` | `Slot` | filler licensing/partial apply | 完整 |
| slot mapping | `map SLOT OP [ARG]` | `SlotMapOp` | 原子驗證/application | 完整 |
| syn/sem/prag/phon 規則 | rule + `then/else` + `@stage` | `Rule` | 三分求值/Tshiatūn | 完整（不含巢狀 Then/Else） |
| sign metadata | 頂層五欄 | typed accessor | validation；動力學不在此 | `origin/provenance/lifecycle/entrenchment/lexicalized` 完整 |

## 4. 刻意不偽裝成「已支援」的資料

| 項目 | 目前處理 | 原因/後續落點 |
|---|---|---|
| attestation 年代、文本、頁行、原文/正規化、confidence | `.lang` 拒絕未知頂層欄位 | Evidence/Attestation，避免 type/token 混淆 |
| 完整歷時詞源與 constructionalization 事件 | 只留 `origin` 輕量指標 | M2 ChangeSet/Evolution graph |
| component DAG、fusion/transparency | 尚無表面語法 | runtime/store 型別尚未落地；不先造無消費者語法 |
| typed Sense/Derivation graph | Path 可保存結構化 scalar 名稱，但 `SemNode` 仍是 fields+roles | 待 Sem 型別與 diff/validation 同步落地後再升格 |
| productivity/entrenchment 動力學 | 只存 entrenchment/lexicalized 現值 | usage event/population update 不屬當前共時 runtime |
| 文化說明與自由註解 | 不進形式本體 | Annotation sidecar |

因此，「可 round-trip」不等於「引擎已有語言學行為」。矩陣只有在 AST、validation
與實際消費者同時存在時才標為完整；只有資料位置的項目會明列為 data-only。

## 5. 證據

- `crates/language/tests/fixtures/synchronic_complete.lang`：完整共時 surface fixture。
- `crates/language/tests/lang_surface.rs`：canonical round-trip、typed metadata、origin
  cycle、五種 mapping、source→runtime 與歷史欄位拒絕。
- `crates/language/tests/m1pp_system.rs`：公共入口、四維 token、Rust SlotMap 與遞迴 filler。

## 6. std trait 與類型學資料

`crates/language/lib/std` 依修補06分成 `code/`（可編譯宣告）、`data/`（查表與來源）、
`config/`（啟用、priority、stable export）。`core` 保存一般本體；`grambank` 保存
Grambank v1.0 的 25 項功能驗證子集。

Grambank 參數是語言層級的類型學觀察，不是會自行執行的普遍規則。因此 std value
trait 只含 `belongs` 與四維 Def；語言本身的 construction/rule 可用它作分類或 guard。
建議將整體編碼掛在專用 grammar-profile sign；若某 construction 正是判定依據，也可
把同一 value trait 掛在該 construction。未掛 trait、明確 `0`、資料不足 `?` 三者不
合併。完整映射、限制與 requirement-to-evidence 表見 docs/15。
