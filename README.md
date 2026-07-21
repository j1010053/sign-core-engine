# conlang-engine — 共時工作台

> **開發者(含 Claude Code)請先讀 `CLAUDE.md`**:專案指引、設計不變式、決策制度、當前任務都在那裡。

conlang(人造語言)工作台的**共時側(架構 2.0)**。本 repo 含**設計文件**與
`crates/language`——語言知識檔(Language IR)、編譯管線、Compiled Grammar 產出、
以及 construction-grammar 四維共時系統。

自體段(autosegmental)**音變引擎**已抽離為獨立產品 **Tshiatūn(切韻)**,以 git
submodule 掛於 `tshiatun/`(GPL-3.0-or-later,規則檔 `.qy`,bin `tshiatun`)。
依賴方向單向:`language → tshiatun/dsl → tshiatun/core`(P20)。

## 兩個 repo / 兩條路徑

```
路徑 A(獨立音變 DSL):  規則檔.qy ─────────────▶ Tshiatūn 引擎 ──▶ 表層
路徑 B(2.0 完整):      Language.lang ─Compile─▶ Compiled Grammar
                                                └ phon 側 = dsl 規則集 ──▶ 同一引擎 ──▶ 表層
```

**雙軌迴歸**(P20 §1.4):同一組音變經路徑 A/B 表層**逐字相同**——8.1–8.6 全綠。

## 目前進度:**M1 + M1++ 完成**(共時四維系統閉環)

`crates/language`(步驟 8–12e)。以 `.lang` 檔承載語言知識,經 ①–⑤ compile pipeline
產出 Compiled Grammar/Sign,再由臨時 Word 建構循環套用導出表層(**Flow A 共時導出**)。

- **步驟 8–9**:Language IR(五組 AST 節點、canonical printer P21、決定性 id P26)+ parser。
- **步驟 10**:compile ①Source→②Expanded→③Resolved→④Ordered(trait 展開、後者勝
  解析、stage 排序;每 pass dump golden)。
- **步驟 11**:⑤Codegen — Compiled Grammar(phon 側 = dsl 可食規則集)+ Compiled Sign;
  🔑 雙軌迴歸 8.1–8.6。
- **步驟 12**:臨時 Word 建構 + 循環套用(stem→word→phrase);詞根+詞綴組合 → 表層。
- **步驟 12a–12e(M1++,共時四維系統;修補07 P38v2–P44)**:
  - **12a** 單一維度中立分類樹(`belongs`)+ 四維 typed projection;最小本體為額外引用的
    stdlib `.lang`(`std/ontology.lang`)。
  - **12b** construction-as-Sign + slots(valence=slots,`?`=optional);組合造詞(德語變位驗收)。
  - **12c** construction semantics(form-meaning pair;`SemNode` 可擴充語意接口)。
  - **12d** 四維同步規則(syn/sem/prag 規則求值於 Sign projection)+ Lexurgy 式
    **`Else`(first-matching)/`Then`(順序組合)**;維度隔離。
  - **12e** typed `Patch`(`Sign × Patch → Sign'`)+ entrenchment 資料欄位(僅介面/欄位)。

**下一步 = M2 歷時**(步驟 13 Primitive Edit 四原語 → 14 ChangeSet Interpreter,🔑 歷時貫通)。

## `.lang` 語法一瞥(colon+縮排,I22)

```
sign give:
    belongs Transitive          /* 分類:單一維度中立 belongs 樹 */
    syn:
        slots:
            agent [Nominal]
            theme [Nominal]     /* [Filler]? = optional */
        class => transitive / [Verb]
            else class => other /* Lexurgy Else:第一匹配 */
    phon:
        /geben/                 /* UR;construction 模板可含 {slot} + 字面素材 */
    sem:
        gloss = GIVE
    entrenchment = 0.5          /* 跨維 meta 欄位(無動力學) */
```

容器頭 `sign`/`trait`/`global trait Name:`;body 統一(trait ≡ sign);維度區塊
`syn:`/`phon:`/`sem:`/`prag:`;`/* … */` 區塊註解可置任意位置。

## 建置與測試

```
cargo test -p conlang-language                              # 工作台(共時側)
cargo test --manifest-path tshiatun/Cargo.toml             # Tshiatūn 引擎
cargo build --manifest-path tshiatun/Cargo.toml \
    -p tshiatun-core --target wasm32-unknown-unknown       # 可移植性(I4)
```

首次取得須初始化 submodule:`git submodule update --init`。

## 文件

規範文件依閱讀順序放 `docs/`:`01`–`13` + 架構修補 `01`–`07`(彙整=P1–P19 權威、
修補05=P20–P28、修補06=P29–P37、修補07=P38–P44 權威)。決策索引見 `CLAUDE.md` §0–§1;
`docs/archive/` 為歷史檔勿引用。
