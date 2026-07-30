# conlang-engine — 共時工作台

> **開發者(含 Claude Code)請先讀 `CLAUDE.md`**:專案指引、設計不變式、決策制度、當前任務都在那裡。

conlang(人造語言)工作台的**共時／歷時核心(架構 2.0)**。本 repo 含**設計文件**、
`crates/language`（Language IR、編譯管線、construction-grammar 四維共時系統）、
`crates/changeset`（四原語、EvolutionGraph、reconstruct）與 `crates/persistence`
（P60/P64 host filesystem boundary）。

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

## 目前進度：Step 16 已收官；Step 17 為下一個歷時層

`crates/language`(步驟 8–12e)。以 `.lang` 檔承載語言知識,經 ①–⑤ compile pipeline
產出 Compiled Grammar/Sign；M1++ 公共入口 `compile_system(Language)` 再串起 ontology、
有效 Sign、construction/token 規則與 Tshiatūn phon runtime 導出表層。

- **步驟 8–9**:Language IR(五組 AST 節點、canonical printer P21、決定性 id P26)+ parser。
- **步驟 10**:compile ①Source→②Expanded→③Resolved→④Ordered(trait 展開、後者勝
  解析、stage 排序;每 pass dump golden)。
- **步驟 11**:⑤Codegen — Compiled Grammar(phon 側 = dsl 可食規則集)+ Compiled Sign;
  🔑 雙軌迴歸 8.1–8.6。
- **步驟 12**:臨時 Word 建構 + 循環套用(stem→word→phrase);詞根+詞綴組合 → 表層。
- **步驟 12a–12e(M1++,共時四維系統;修補07 P38v2–P44)**:
  - **12a** 單一維度中立分類樹(`belongs`)+ 四維 typed projection;最小本體為額外引用的
    stdlib `.lang`。資源統一置於 `lib/{std,plugin,natural}`；std 的
    `core|grambank|cxg` 自動載入，Grambank v1.0.3 提供 25 個三態(`?`/`0`/`1`)
    語法特徵，`std/cxg` 提供 form–meaning schema 與八個 recipe。
  - **12b** construction-as-Sign + slots(valence=slots,`?`=optional)+ typed Rust
    `SlotMap`(preserve/rename/autofill/internalize/optional override)；同一組操作亦可由
    `.lang` 的平坦 `map` 語句宣告。
  - **12c** 完整四維 derived token(form-meaning pair、遞迴 filler、frozen filler
    snapshots、provenance)；Slot 支援 category 與 `[*]` any-sign。
  - **12d** Sign/token 同步規則 + Lexurgy 式 **`Else`/`Then`**;phon 分支直接排放
    Tshiatūn；trace 保存實體 `.lang` 行號。
  - **12e** 私有且驗證的 typed `Patch` + per-dimension diff + entrenchment/
    lexicalized 資料欄位(僅介面/欄位，無動力學)。

M1+ 的 `syn: slot_features:` 現同時支援 stored sign 與 derived-token filler：所有 RHS
讀 frozen probe，完整驗證後才原子注入 occurrence constraints；stored sign 從 effective
base、derived token 從 deep baseline 重跑 Syn→Sem→Prag，再重選 realization，且不改寫
來源 sign/token。stable source identity／typed resolver 與 V2 expression node 身分由
Step 13 提供；Primitive-only ChangeSet replay 已於 Step 14 封板。完整稽核與邊界見
docs/19–20、docs/23。

V2 typed `case` 以封閉 context type 檢查 branch。Sign body 可回傳完整 Sign application，
也可回傳匿名 `SignContext` fragment：fragment 可沿用 trait expansion 並合併回目前 Sign，
不建立新 SignId；`PhonContext` 只接受純 phon template／projection，trait 不可展開到 phon。
`SynContext`、`SemContext`、`PragContext` 則限制匿名 fragment 只能包含各自維度的內容。
`when:` 累加所有命中 fragment，但所有 guard 一律讀同一份合併前 frozen snapshot；
確定 match 結果後才按來源序 merge，前一命中不能影響後一 guard。完整契約與反例見
[docs/24_case_when與context_fragment_v2.md](docs/24_case_when與context_fragment_v2.md)。

**Step 13 source interface 與 Primitive Edit 的語義／API 契約已封板**：caller `.lang` 搭配版本化
identity sidecar，為 V1 節點及 V2 Application／Case／CaseBranch／Constraint 提供 stable
`NodeId`／typed resolver；四原語、`check_language`、Language diff 與 trace 均只修改
caller source。slot／trait rename 會重寫 typed consumers，巢狀 case 亦可 round-trip、定址與
移動。2026-07-22 根 workspace 251/251、Tshiatūn 157/157 通過；完整工具閘門仍因
本機工具鏈與既存 dirty submodule 回傳 exit 2，未宣稱 release gate exit 0；詳見 docs/20。

**Step 14 已封板（2026-07-24）**：Primitive-only `.chg` 的 parse／resolve／replay／
lazy compile 與 statement 交易定稿。相容性測試涵蓋 replay 跨執行決定性、`.chg` dump
round-trip、三道 digest(base source／identity-manifest／library lock)replay 前拒絕、
交易回滾/部分保留、lazy compile cache。`cargo test --workspace` 全綠為證(本機無
PowerShell,`.ps1` 閘門未實跑)。契約見 docs/22。

**Step 15／16 已收官（2026-07-30）**：12 項 Atomic Rewrite 全部降階為四原語；
歷時 function 載入與呼叫、分層 diff、不可變 EvolutionGraph、node-v2 內容定址、
rebase、全 parent 3-way merge、donor／`adopt` 與狀態→四原語重建均已接通。
收官補齊項亦已落地：手改 `.lang` 可走顯式 identity reconciliation（exact `open`
仍嚴格驗 digest）、同父有序序列以決定性 LCS 重建 Move、persisted expression／
realization 節點具明確 typed reconstruct capability，structured phon 可用 `.phon_block:`
顯式 bootstrap 並從 `.chg` 插入 leaf／Then／Else／propagate sub-block。P60／P64
亦由 `crates/persistence` 收官：canonical sign／trait／non-sign fragment、changeset 與
identity sidecar 進共享內容定址物件庫，節點落在 `nodes/<id>/`，annotation／config
留在 node-v2 雜湊外。下一個歷時層為 Step 17；刻意不支援的細界線見 docs/26。
收官矩陣見 [docs/26_Step16收官_文件契約與驗收矩陣_v1.0.md](docs/26_Step16收官_文件契約與驗收矩陣_v1.0.md)。

**v1 路徑已硬移除（2026-07-24）**：v2 為唯一模型。移除 `LanguageSchema` V1/V2 分野
(FP `case`/`when`/`constraints` 為預設,無需標頭;舊 `schema conlang.lang/v2` 行被接受
並忽略、不再輸出)與 identity manifest v1(`open` 只吃 v2 sidecar,v1→`UnknownSchema`)。
**舊 v1 檔不可載入**;移除前已證 v1→v2 升級無損。base fixtures 於 v2 逐字不變。

## `.lang` 語法一瞥(colon+縮排,I22)

```
sign give:
    belongs Transitive          /* 分類:單一維度中立 belongs 樹 */
    origin = sign(ancestor::proto_give) /* 跨節點輕量指標；完整歷史在 ChangeSet/Evidence */
    provenance = grammaticalized
    lifecycle = active
    syn:
        slots:
            agent [Nominal]
            theme [Nominal]     /* [Filler]? = optional */
        map agent rename actor  /* preserve/rename/autofill/internalize/optional */
        class => transitive / [Verb]
            else class => other /* Lexurgy Else:第一匹配 */
    phon:
        /geben/                 /* UR;construction 模板可含 {slot} + 字面素材 */
    sem:
        senses[core].concept = GIVE /* `.`/`[key]` 共用 Path 文法 */
    entrenchment = 0.5          /* 跨維 meta 欄位(無動力學) */
```

容器頭 `sign`/`trait`/`global trait Name:`;body 統一(trait ≡ sign);維度區塊
`syn:`/`phon:`/`sem:`/`prag:`;`/* … */` 區塊註解可置任意位置。
`.lang` 只記 construction type 與共時狀態；年代、文本出處、原文/正規化與可信度
屬 Evidence/Attestation，不混入 Language。完整貼合矩陣見
`docs/14_共時lang語法與資料貼合度_v0.1.md`。

stdlib trait 以 `belongs` 引用，例如 `belongs GB107_Present`。Grambank 的「未掛
trait」表示未陳述，不等於 code `0`；需要明確負值時使用 `GB107_Absent`，資料不足則
使用參數根 trait `GB107_BoundVerbalNegation`（投影為 `?`）。完整選項與證據見
`docs/15_std_Grambank預設traits_v0.1.md`。

顯式選取自然語言庫使用 `compile_with_libraries(Language, LibrarySpec)`；內建
`natural:en-standard` 提供 12 類可執行 Standard English 核心 construction。抽象／
具象對照、lexicalized workaround 與不新增 Grambank 關鍵字的結論見 `docs/16`。

## 建置與測試

```
cargo test -p conlang-language                              # 工作台(共時側)
cargo test --manifest-path tshiatun/Cargo.toml             # Tshiatūn 引擎
cargo build --manifest-path tshiatun/Cargo.toml \
    -p tshiatun-core --target wasm32-unknown-unknown       # 可移植性(I4)

# P38–P44 本機一鍵封板(先做工具鏈/submodule preflight)
powershell -ExecutionPolicy Bypass -File scripts/verify-m1pp.ps1
```

首次取得須初始化 submodule:`git submodule update --init`。

## 文件

規範文件依閱讀順序放 `docs/`。P1–P19 以架構修補彙整 01–04 為準，P20–P64
依架構修補 05–11；Step 13–16 的實作封板契約分別見 docs/20、docs/22、docs/26。
完整決策索引見 `CLAUDE.md` §0–§1；`docs/archive/` 為歷史檔勿引用。
