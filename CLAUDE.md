# CLAUDE.md — conlang-engine 專案指引

你正在開發一個 **autosegmental(自體段)音變引擎**:conlang(人造語言)工作台的核心,
以 Rust 實作,支援多層音韻表徵(聲調、鼻化、和諧)、歷時音變規則 DSL、與未來的 WASM/Tauri 前端。
本專案的設計已經過完整的紙上收斂,**你的工作是實作既定規格,不是重新設計**。

## 0. 最重要的三件事

1. **規格已凍結,以編號決策為準。** 全部設計爭議都已裁決並編號:D1–D28(語法/本體論)、
   A1–A4 / B5–B9 / C10–C11(執行語意)、I1–I18(實作層,docs/05 §9)、
   **P1–P55(架構修補層;P1–P19 權威=《架構修補彙整 01–04》§1 總表、P20–P28 權威=《修補05》§11、
   P29–P37 權威=《修補06 插件服務與DSL API》§8、P38–P44 權威=《修補07 共時四維系統》§9、
   P45 權威=《修補08 具名可定址節點》、P46 權威=《修補09 phon 命名 block》、
   **P47–P55 權威=《修補10 歷時 function 層與載入》**;
   個別修補文件與彙整出入處以彙整/最新修補為準;P7 已廢止→P14)**。
   任何實作若與編號決策矛盾,**停下來明確指出衝突**,不要自行變通。規格未覆蓋的新問題:
   實作層提案編 I 系列入 docs/05 §9;架構層變更走 P 系列。
2. **每個開發階段必須以測試出口收尾。** 不存在「做完但沒有測試綠燈」的階段
   (M0 實作參照 §8:每步以哪個範例綠燈為出口)。
3. **哨兵規則:** 若你發現自己在逐檔/逐函式翻譯 Lexurgy(Kotlin, GPL-3.0)的原始碼,
   立即告知使用者,不需事先確認。參照它的 ANTLR 文法檔與行為文件是允許的;翻譯實作不是。

## 1. 文件鏈(規範層級,衝突時上位優先)

依序閱讀 `docs/`:

| 檔案 | 角色 |
|---|---|
| `01_架構書_v0.3.md` | 全局:產品定位、模組、路線圖(M0–M4) |
| `02_語法規格_v0.3.md` | DSL「能寫什麼」:selector、動詞、Scan、決策 D15–D28 |
| `03_語法規格_v0.2_凍結附錄.md` | **凍結但仍規範性**:D1–D14 與基礎定義的權威來源(v0.3 以「承 v0.2」引用) |
| `04_執行語意規格_v0.1.md` | 「怎麼跑」:Execution Model 生命週期、動詞語意表、locality、spell-out 純函數、A/B/C 決策 |
| `05_M0實作參照_v1.0.md` | 實作層:workspace、表徵方案、crate 清單、開發順序、I1–I8 |
| `06_演化圖本體論_v0.1.md` | 模組 D(容器層):節點/邊/來源統一、克里奧爾、互通度、replay。**設計層,非 M0 實作範圍** |
| `07_分層結構檔本體論_v0.1.md` | 模組 C(節點內部):sign 統一本體、四維特徵結構、組合=運算、固化=語法化、層作視圖投影(v0.1.1 修補)。**設計層,SYN 欄位待 A/B 驅動** |
| `08_AB模組需求分配_v0.2.md` | A=完整 sign 生產者(含組合造詞、借詞、entrenchment 初值);B=**原語集三層定稿**(L1 資料/L2 語言/L3 理論宏),已回填 06 的 ChangeEntry op=L1∪L2。命名分層規約見本檔 §6 |
| `09_Sign生成引擎本體論_v0.1.md` | 模組 A 核心:Sign 稀疏容器、四維、Need→Generator→Builder→Store 職責契約、五連接關係、兩種構詞、生命週期;附 18 案折磨測試(0 破壞)。**設計層** |
| `10_統計先驗與抽樣引擎_v0.1.md` | 模組 E:唯讀先驗庫(PHOIBLE/Grambank/WALS/CLICS,附網址與授權)+ 無狀態抽樣;有效分佈=手動>導入 provider>投影>E1 先驗,覆寫層住節點。**設計層;01–10 設計鏈至此閉合** |
| `11_測試案例集總索引_v0.1.md` | **全專案測試索引**:DSL 範例 8.1–8.6、18 案折磨測試、十實例、Rust 測試、Lexurgy 黑盒的統一映射與狀態;動工任一模組前先查其驗收案例 |
| `12_邏輯分層架構_v0.1.md` | 四層架構(展示/應用/引擎/資料);引擎分即時+批次兩子層,DSL core crate 跨兩子層共用;應用層(Command/Query API)為下一個設計空白。**設計層** |
| `架構修補01_共時規則系統與臨時韻律域_v0.1.md` | **P 系列決策權威(P1–P4)**:Word=臨時韻律域、Grammar Store、strata 層級錨定+循環套用、cophonology 閂;對 M0 步驟 2–3 零衝擊,插入點=步驟 4(`stage:` 標記,I14)與步驟 6(phrase-level 掛鉤);修補內容已回寫 docs/01–09、12 |
| `架構修補02_Trait機制_v0.1.md` | Trait = macro 展開模板(P5–P7;P7 廢止→P14) |
| `架構修補03_Trait與CompiledGrammar_v0.2.md` | Compiled Grammar 責任分離、Definition/Rule 二分(P8/P9) |
| `架構修補04_共時／歷時分離與歷時實作分層_v0.1.md` | 共時=編譯器/歷時=直譯器、歷時四層、State(P10–P13) |
| `架構修補彙整_01-04_v1.0.md` | **P1–P19 權威總表** + 裁決(P14–P19)+ 廢止/更名對照(全庫檢索用) |
| `架構2.0總鳥瞰_v1.0.md` | **2.0 單一入口**:Language/ChangeSet 雙軌全圖、四條資訊流、Debug 模塊化、**新實作順序(步驟 8–22,M1–M4)** |
| `架構修補05_Primitive與檔案格式_v0.1.md` | **P20–P28 權威**:DSL 獨立性、IR dump/canonical printer、條件語法(else/Path/tier-adjacency)、四原語、Ref 模型、.lang/.chg 檔案格式 |
| `架構修補06_插件服務與DSL_API_v0.1.md` | **P29–P37 權威**:插件系統(資料層/程式碼層分離)、外部服務生命週期(ServiceRef→resolve→執行→驗證→History)、音變 DSL 最小對外 API(承 P5/P22 鐵律)。完整插件仍是設計層；embedded std 已先實作 package code/data/config 子集。 |
| `架構修補10_歷時function層與載入_v0.1.md` | **P47–P55 權威**:層①=語句/層②③④=函數呼叫 `name(args)`(層級由名字解析,非關鍵字);Recipe/Goal 是 `code/` 檔案分工,body 語意由既有 case/when 承載;定義住套件 `code/*.chg`(`function Name(參數 [約束]):`);**載入沿用 P29 auto-discovery,否決顯式 import**(可重現性靠既有 library lock);Recipe **接力**展開;路徑庫 = 參數化 function(code)+ 路徑表(data);`expand` 先開好 `ServiceContext` 接點;**P54** sign 增 `components` metadata(兌現 §4.3 對 fuse 的 component 引用;與單一來源的 `origin` 職責不同);**P55** `.chg` 語句標記 `#N:` + 註解統一 `/* … */`(三格式一致;`#` 在 `.qy` 為詞界 D19) |
| `架構修補09_phon命名block_v0.md` | **P46 權威**:phon 規則命名/塊對齊 Lexurgy `.qy`(`name:` 前綴 + `Then:`/`Else:` 塊);取徑 A。slice 1 已落地(`name:` 前綴 inline + codegen 標籤);S2–S4(巢狀 block IR/語句定址/propagate)staged |
| `架構修補08_具名可定址節點_v0.1.md` | **P45 權威**:Rule/TypedCase/CaseBranch 可選 `@name` 標籤 + keyed 定址(`rule["x"]`/`case["x"]`/`branch["x"]`);承 P24/P25/P26,標籤不取代穩定 id。取徑 B(不動 sign 扁平結構) |
| `架構修補07_共時四維系統_v0.1.md` | **P38–P44 權威**:phon/syn/sem/prag 四維獨立(OntologyRegistry)、Defs+typed projection/patch、`belongs`(取代 provides)、valence=slots、construction-as-Sign+slot mapping、Lexurgy 式 Else 三分、四維同步規則;路線圖插入步驟 12a–12e(M1++,M2 前) |
| `14_共時lang語法與資料貼合度_v0.1.md` | **共時 surface 實作對照**:巢狀 Path、`.lang` SlotMap、typed sign metadata，以及 Language 與 Evidence/Attestation 的 type/token 邊界。 |
| `15_std_Grambank預設traits_v0.1.md` | **stdlib 資料對照**:修補06 package 分層、Grambank v1.0 的 25 項 trait 子集、0/1/? 知識狀態、行為映射、限制與測試證據。 |
| `archive/` | 已取代的歷史文件,勿引用(各檔頭有橫幅說明) |

## 2. 不可違反的設計不變式(內化這些,寫每一行程式碼時對照)

- **純度不變式(D1)**:規則的條件可沿連結(聯結+支配)**唯讀**跨層;改動只能寫自己那層。
  跨層整合只發生一次:末端 spell-out。
- **兩套語彙(D2)**:韻律層(時間軌道)用支配動詞 dominate/release/parse;
  旋律層(時間乘客)用聯結動詞 associate/delink/…。唯二共用名 merge/delete(D26,受詞型別消歧)。
- **Ø 零特徵(D12)**:旋律層預設狀態是「未指定」= 錨點無邊,不是一個符號。
- **位置定址只在 Scan(D3)**:「第幾個」的能力只存在於 Scan 塊;tier 內規則是內容定址,
  受局部可達上界(執行語意 §5:可達閉包,成分邊界 ∪ 錨定層相鄰一格,取聯集——是可達性邊界,**不是距離度量**)。
- **Spell-out 是純函數(C11)**:Representation → Surface,僅 lookup/flatten/render;
  禁 rewrite/search/spread/Scan。表層音變寫成拼讀前的最後一條普通規則。
- **snapshot-and-actions(I1/I2)**:凍結快照上匹配 → Action 清單 → commit 產生新 Word。
  快照 = clone;韻律 = Span 序列非一般圖;AnchorRef = 層級+索引,commit 時重編。
- **單一資訊源(實作原則 3)**:每個 identity 恰一個權威存放處
  (邊只在 `Autoseg::links`、序列位置只是 `seq` 索引、韻律只在 Span、stale 只在 `StaleFlags`)。
- **旁註層正交於本體(07 §5c)**:文化說明/隱喻傾向/使用者語料是唯讀旁註,不參與 replay、不被 diff、不約束生成(軟提示);已固化的俗語是 idiomatic sign(本體內),未固化傾向才進旁註層。
- **prag 維開放非封閉(07 §5b)**:語用五類是預設標記集非固定入口;語法化的語用入口是 pragmatic strengthening 機制,B 只設一個 `pragmaticalize` 原語。
- **NCC 是軟約束(D7)**:偵測但預設 warn;`strict-ncc` 可升級。中間態放任合法,出口把關(D27 同哲學)。
- **sign.phon = 底層形 UR(P1)**:表層形**永不儲存**,由 Grammar Store 共時規則按需導出;`Word` 是臨時韻律域(sign 組合按需建構,預設 ω),非儲存單位。
- **歷時演化編輯有限儲存(P2/P10)**:語言知識只住 **Language**(Global/Trait/Sign);演化 = 直譯器改寫 Language 後重 compile,**永不枚舉無限的組合空間**;歷時不得直接碰 Compiled Grammar/Sign。
- **共時=編譯器、歷時=直譯器(P10)**:Compile 僅存在於共時側;Compiled Grammar 是可丟棄重算的編譯產物(P8)。
- **`dsl` crate 不得 import 任何 Language/Sign 型別(P20)**:依賴方向 `changeset → language → dsl`,CI 檢查;dsl 只知 Word。
- **ID 配發必須決定性(P26)**:純序列性,replay 逐位元可重現;禁隨機/時間戳。
- **引用是 Ref 屬性值,非圖邊(P24)**:四原語(insert/delete/update/move)在樹上封閉的前提。

## 3. 實作原則(執行語意 §9,每次 PR 自問)

1. **不重複造輪子**:需要新能力先查 crate(已核定清單見 M0 實作參照 §6;刻意不用 petgraph/slotmap/rayon)。
2. **基礎先行**:具名動詞一律由原語組合(spread=迭代 associate、shift=delink+associate、
   fill=逐Ø insert+associate…),不得另闢狀態。
3. **單一資訊源**:同上;文件層也是——概念定義唯一,其餘引用。
4. **功能完整下精簡**:以測試界定「完整」;優先刪抽象層而非加。

## 4. 可移植性規範(core crate,違者 CI 紅燈)

- 禁多執行緒、禁 `std::fs`、禁 `std::time::Instant`/`SystemTime`、禁環境隨機源(用注入的 seeded `rand_chacha`)
- 禁 `panic!` / `println!` / `unwrap()`(測試碼除外):錯誤走 `Result<_, EngineError>`,
  診斷(error/warn/info/trace)是**回傳資料**(見 `repr::invariant` 的模式)
- CI 必掛:`cargo build -p conlang-core --target wasm32-unknown-unknown`
- `#![forbid(unsafe_code)]` 已設,維持

## 5. 目前狀態與下一個任務

**設計鏈狀態**:docs/01–12 + 架構修補01 全部到位並已納入 repo(repo 版為權威;根目錄
`../docs/` 為使用者設計工作區的歷史快照)。修補的四個決策已另立 **P 系列**(P1–P4,
權威=修補01 §4;原建議編號 I9–I12 因與實作層撞號而重編)。修補對 M0 步驟 2–3 **零衝擊**,
插入點在步驟 4(規則檔 `stage: stem|word|phrase` 標記,預設 word;關鍵詞依 I14)與步驟 6(spellout 的
phrase-level 空掛鉤);Grammar Store 本體為 M0 後的步驟 8。

**已完成(M0 步驟 1,commit `bccc837`)**:`crates/core/src/repr/` 表徵模組——intern、feature、
prosody(I8 拓撲)、melody、word、invariant、notation。本機工具鏈首跑即全綠。

**已完成(M0 步驟 2,commit `2966df0`)**:
- `lifecycle/`:`Action` 六 variant、`commit`(凍結快照+一次寫入;I10 收攏)、`validate`、
  `needs_reparse`(A3)、`run`(執行語意 §1 步驟 3–5 編排)。
- `primitives/`:六原語建構器 + proptest 不變量。新決策 I9/I10(docs/05 §9)。

**已完成(M0 步驟 3,commit `0809f7a`)**:`strategy/`(D28 統一解析器)+ `verbs/` 第一批
(insert_floating_near / dock / fill / merge_adjacent_equal,全組合原語)。新決策 I11(dock 原位投影)。

**已完成(M0 步驟 4)**:
- **I12(音段規則通道)**:`Action::SegRewrite{idx,sym,feats}`(整段替換、長度不變;非第七原語,
  不供動詞組合)+ `repr::Inventory`(SymId↔FeatBits,住 `Env`)+ `verbs::rewrite`
  (特徵矩陣匹配、onset 述語、逐欄位 set_field、Inventory 反查,無對應=error)。
- `crates/dsl/`:logos lexer(入口 NFC,I6;**註解 `/*…*/`**,擁有者定案,記法表見 docs/02 §2)
  + chumsky 行導向 parser → 型別化 AST → lowering(名稱→id/bits;超出 8.1 範圍者明確報
  Unsupported)→ executor(每規則一 commit;B5 同規則語句共享凍結 match)。
  宣告貼合 Lexurgy(`Feature`/`Symbol`/`Class` + 自有 `Melody`);音段規則 `A => B / C _ D`。
  **P3 已插入**:`stage: stem|word|phrase` 標記(預設 word,僅記錄無行為;I14 改名)。
- `crates/cli/`:`conlang <rules.dsl> <words.txt>` → 逐詞推導表(trace);
  造詞為**暫定 CV 音節化**(`Class vowel` 判核心;`Parse` 宣告於步驟 5+ 取代)。
- 出口已過:`examples/8_1_tonogenesis.dsl` 全文經 DSL 管線與 CLI 實跑,四詞推導
  與步驟 3 語意一致(`dsl_8_1.rs` insta 快照 + 硬斷言);devoicing 已是 DSL 規則。

**已完成(M0 步驟 5)**:
- **I13(音段刪除連鎖,I10 解除)**:`Action::SegDelete` + commit cascade——Span 平移、
  mora keep-empty、**無核心音節清理**(統一 8.3 調浮游與 8.4 空莫拉修復)、旋律 links
  重映射 + on-anchor-loss float(D14)+ 原位(D6)、stale 標記;殘餘節外音段待重剖(步驟 6+)。
- `verbs/` 第二批:`spread`(iterative 展開、blocked-by、within pword|stem、through、
  bidirectional 同拍衝突 D11 + on-conflict stop|val)、`shift`(邊出界→浮游)、
  `dominate_empty`(repair,A3)、`rewrite` 泛化(@class、coda 述語、`*` 刪除、`.` 音節界)。
- `locality/`:可達上界計算(執行語意 §5);對一般 selector 的強制執行隨步驟 6 接上。
- DSL:spread/shift/dominate 語句、`@class`/`<level>`/`*`/`.` element、`+nasal` 型值名、
  `anchor segment`;註解 `/*…*/`。
- 出口已過:**8.2–8.5 全數端到端綠燈**(`examples/8_2`–`8_5` + `dsl_8_2_to_8_5.rs`)。
  詞彙給定旋律/型態括號/WBP 莫拉由測試注入(詞條載入層職責)。

**已完成(M0 步驟 6,M0 收工)**:
- `scan/`:三道鎖枚舉(linked-only D4;`over all` 留步驟 7+)、調簇不透明(D18,粗掃描
  停靠刻度首錨)、序數 `[n]`/`[first]`(D16)、值改寫沿掃描軸(D5/D20,= delete+insert
  同位同聯結)。DSL:Scan 塊頭 + 塊內 `associate <值> -> <目標>[序數]` 與值改寫;
  塊內每語句各為一條規則(各自 commit)。
- `spellout/`:純函數(C11)——order/empty/floating/contour 宣告、D27 多承載無對應=error、
  長元音 `ː` 投影(8.4 收尾)、phrase-level 掛鉤(P3,M0 空集)。**Spell-out 的 DSL 宣告
  區塊(C10)未接 parser**——API 層已測,語法接入留步驟 7。
- 出口已過:**8.6 綠燈(`examples/8_6` + `dsl_8_6.rs`)= 範例集 8.1–8.6 全數端到端,
  M0 驗收達成**(六原語+生命週期+CLI 批次導出皆綠)。

**已完成(M0 步驟 7 / 尾聲)**:
- corpus/lexurgy submodule + 白名單(`corpus/whitelist.md`)+ harness 骨架(分類器)。
- 收尾補強:`diag/`(B9 分級資料;executor 附 B8 noop/reparse info)、Spell-out DSL
  宣告區塊(C10;CLI 印表層)、scan `over all`(D4 浮游按 origin 入列)、**Parse 宣告**
  (D24 子集:`Parse mora: @V | @V :: @C` WBP、`Parse syllable: @C? :: @V :: @C?`;
  8.4 全 DSL 化)、字面符號改寫(`x => h`,SegOut::Symbol)。
- **Lexurgy 黃金測試(執行級)**:自 core spec 測試萃取三元組,M0 子集 8/8 通過
  (含 compound=B5、parallel 凍結兩個語意對齊點);17 案 → M2。`…` 任意距離未做(步驟 8+)。

**架構 2.0 已定(2026-07-13 讀入)**:修補02–05 + 彙整 + 總鳥瞰納入 repo。
「Grammar Store 容器」計畫**作廢**——新路線圖 = 《架構2.0總鳥瞰》§4(步驟 8–22):
- **M1 共時側**:步驟 8(Language 資料結構 + canonical printer/empty root + IR dump,P21/P28)
  → 9(Language parser:trait/==block/[n]/Definition=/Rule=>/@stage/Path/else)
  → 10(Compile 五 pass,每 pass dump golden)→ 11(Compiled Grammar;🔑 **雙軌迴歸**:
  8.1–8.6 兩路徑表層逐字相同,P20)→ 12(臨時 Word 建構 + 循環套用)。
- **M2 歷時側**:13(Primitive Edit 四原語 P23)→ 14(ChangeSet Interpreter,P26;🔑 歷時貫通)
  → 15(Atomic Rewrite 12 項展開 golden)→ 16(Evolution_node + Replay)→ 17(Recipe/Goal/Weight DB;🔑 層級介入)。
- **M3**:18(Need→Generator→Builder)→ 19(E1 + Weight DB)→ 20(State)。**M4**:應用層 + UI。
- 現有 dsl/core = **P20 的獨立音變 DSL(路徑 A)**,原樣保留為可交付產品;
  Lexurgy 完整匯入器與 Latin→Romance 黃金鏈仍屬其後續。
- **三項裁決(2026-07-13,docs/13 §4)**:(1) 語法邊界——feature/symbol/class 留 dsl 域
  (Lexurgy 形),Language 的 `=` Definition 只管 dsl 不認識的概念,同名靠檔案區域區分;
  (2) crate 佈局——build_word/循環套用協調在 language crate,engine 側不動,接口
  `language::build_word/compile_phon_rules → dsl::Word/RuleSet → dsl::run` 單向依賴;
  (3) docs 舊文件不逐檔回填,以最新檔案為基準。
- **步驟 8 前置(擁有者指示)**:先確保 dsl 音變演化引擎可作為**獨立軟體發布**(P20)——已完成
  (CLI 詞表′契約、依賴守衛、產品 README,commit `2a7d530`)。

**已完成(鳥瞰步驟 8,M1 開跑)**:`crates/language`——五組 AST 節點(修補05 §10.3:
Def/Rule(RuleId)/TraitDef(Block 節點,P27)/SignDef/Ref 型別/distribution)、
canonical empty root(P28)、canonical printer(P21 確定性;IR dump = canonical form)、
P26 序列性 id(不入印出格式,I15);dsl 域宣告以不透明區塊承載(裁決1);
`language → dsl` 依賴已掛(P20 方向)。出口過:單元 5 + dump golden(修補05 §10.1 樣例)。

**已完成(鳥瞰步驟 9)**:Language Parser(行導向遞降;chumsky 屬 I6 的 DSL 規則語言範圍,
不及於行/括號結構的 .lang)——dsl 域區依**檔案位置**判定(裁決1:首個 language 構造前
verbatim)、`==` 切 Block(P27)、`Name[n]` 引用、`=`/`=>` 二分、`@stage`(省略=word)、
**`else` 鏈**(P22,入 `Rule.else_chain`,printer 同步輸出)、**Path 文法**(修補05 §3.5,
`path::parse_path`:`.`欄位/`[key]`/`~tier`,Def 路徑驗證+步驟 13 定址複用)。
出口過:**round-trip 恆等**(canonical 輸入逐位元;id 依文件序決定性再生)、
非 canonical 輸入正規化為不動點、source→AST golden、錯誤定位(行號)。
規則 env/action 內部與守衛求值的結構化 = 步驟 10(compile pass 需求驅動)。

**引擎分離(2026-07-17,I7 v2)**:core/dsl/cli/examples/corpus 移至獨立 repo
**`../tshiatun`**(Tshiatūn/切韻;GPL-3.0-or-later;規則檔 `.qy`;bin `tshiatun`;
14 套件綠、Lexurgy 黃金 8/8、wasm 綠)——P20「獨立可分軟體」的實體化,待 push GitHub。
本 repo(工作台)自此只含設計文件 + `crates/language`;**主引擎以 git submodule 掛於 `tshiatun/`**
(language 的 dep path = `../../tshiatun/crates/dsl`)。目前 .gitmodules URL 為本地絕對路徑,
**push GitHub 後須改為遠端 URL 並 `git submodule sync`**。引擎目錄已更名 ASCII `tshiatun`。
引擎相關測試/開發改在 tshiatun repo 進行。

**已完成(鳥瞰步驟 10,I16)**:`language::compile`——①Source→②Expanded
(Trait Expansion:非 global trait 與 TraitUse 消去、引用位置 inline;global trait
存續為合法 Language 載體;P5 全 block 完整性 = 編譯錯誤)→③Resolved(同 path Def
文件序**後者勝**=P6 欄位級 priority 的位置語意實現;Rule 不合併不去重)→④Ordered
(P18:各容器 Rule 槽位間 stem→word→phrase 穩定排序,Def 原地不動;global blocks
展平單 block)。每 pass 純函數 `Language → Language`(P21 無隱藏狀態,冪等)+
trait 索引(Compile Artifact,P8)。出口過:**每 pass 一份 dump golden** 且每份產物
re-parse round-trip 恆等、結構斷言、四類 CompileError、決定性。workbench workspace
`exclude = ["tshiatun"]`(submodule 自有 workspace,避免 --workspace 外跑其測試)。

**已完成(鳥瞰步驟 11,I17)**:`language::codegen`——⑤Codegen。
`CompiledGrammar { phon_source, program }`:phon 側 = dsl 可直接吃的規則集原文
(P20 §1.3;dsl 域宣告 verbatim + global trait 規則,canonical 名稱序 × ④ 序;
`;` 多語句塊展開為合成標籤 rN: 保 B5;`stage:` 僅 ≠word 時輸出),Program 由
`tshiatun_dsl::compile` 產出;P8 無 trait/priority 痕跡。`CompiledSign`:③ 後者勝
欄位 + sign 局部規則(消費者 = 步驟 12)。parser 擴充(I17-a):容器內非 Def 非
`=>` 行 = 原文 dsl 動詞語句 Rule。顯式拒絕:else 鏈/Scan+stage/dsl 拒收(I17-d)。
出口過:🔑 **雙軌迴歸 8.1–8.6 全綠**(P20 §1.4,表層逐字 + 逐步 Word 全狀態 +
步數;`tests/dual_track.rs` + `tests/fixtures/8_*.lang`)= **共時側 2.0 化完成**;
codegen 語意測試 10 案(golden/P8/決定性/顯式拒絕);CLI 功能回測 11 案新編
(`tests/cli_functional.rs`,子程序呼叫 tshiatun 二進位,只讀不寫)。
(零規則丟失輸入詞缺陷已由上游 `520e0c8` 修復;submodule bump 2026-07-20。)

**引擎同步(2026-07-20)**:tshiatun submodule bump `51f1a30` → `520e0c8`
(上游 wuc-codex:stage=規則可見域、build_phrase `+`/空白縫、convert 子命令、
Lexurgy 匯入器、phrase 支援;零規則缺陷已修)。工作台側:CLI 測試契約更新
(`tshiatun` 版本字串、trace `output` 行、零規則=詞表′恆等);examples 經
`-w` diff 證實內容零變,.lang fixtures 免重移植。

**已完成(鳥瞰步驟 12,I18)**:`language::word`——臨時 Word 建構 + 循環套用。
`Component::Sign|Ring` 組合樹 → cophonology 前趟(sign 局部 stem 規則於自己
的葉上跑;M1 子集=音段效果,旋律殘留/非 stem 層顯式拒絕)→ UR 文字
(`phon = /…/`)以 `+`/空白拼縫 → `dsl::build_phrase`(韻律域+括號)→
**P3 驅動:Program stage 切片 stem→word→phrase 串跑**(呼叫端驅動,協作規範
§3-1;展平組合下 ≡ ④ 排序單趟,metamorphic 釘住)→ `surface_phrase`。
出口過:🔑 **詞根+詞綴組合 → 循環 → 表層 + surface sandhi 首測**
(`tests/word_compose.rs` 8 案:組合鏈 `pa+ap pa → bx+ab ba`、無組合負例、
cophonology 僅及自身 sign、驅動等價、決定性、三類定位錯誤/顯式拒絕;
規則語意錨定上游 stages.rs 已驗證模式)。**M1 共時側閉環達成**。

**M1++ 插入(擁有者定案 2026-07-20,修補07 立 P38–P44)**:進 M2 前先把**共時
四維系統做穩**(construction grammar 核心 = meaning-form pair,需 syn/sem 落地)。
路線圖插入步驟 **12a–12e**(修補07 §8):12a 四維 ontology + `belongs` + typed
projection → 12b construction-as-Sign + slots(🔑 組合造詞)→ 12c construction
semantics(form-meaning pair)→ 12d 四維同步規則 + Lexurgy 式 Else → 12e typed
patch **僅介面/欄位**(entrenchment/lexicalization 行為留 M2 後)。關鍵決策:四維
獨立不共享分類樹(P38)、`Defs` 唯一源 + typed projection/patch(P39)、`belongs`
取代 provides(P40)、valence=slots(P41)、construction=Sign(P42)、Else 三分
Matched/Unmatched/Error(P43)、每維規則只改自己那維(P44)。

**已完成(鳥瞰步驟 12a,I19/I20)**:單一維度中立 ontology + `belongs` + typed
projection。`crates/language/ontology.rs` 的 `OntologyRegistry` 自一組 Language 建成
一棵分類樹；phon/syn/sem/prag 是正交內容投影，不各建同名分類樹。**最小本體 =
額外引用的 stdlib `.lang`**；`std_ontology()`/`with_std()` 保持相容。`belongs` 閉包
菱形去重、循環安全，有效內容依遠祖→近祖、同距離後寫 `belongs`、本地最後決議；
與 `Name[n]` macro 並存分工。建構期診斷涵蓋未知目標、循環、重名、Def winner
provenance 與 slot conflict。出口見 `tests/ontology.rs` 與 M1++ 封板矩陣。

**已完成(鳥瞰步驟 12b,I21)**:Construction 與 slots。`construction.rs`:
construction-as-Sign(P42,帶 ≥1 slot 的 sign)、具名 slots(`slot NAME [Filler]`,
**optional 尾綴 `?`**,P41 valence=slots)、filler 授權 = filler 的 syn `belongs`
閉包含 slot 約束(P40 複用 12a)、`apply` → derived token(暫態不進庫;殘餘 slots
= 剩餘 valence;飽和=無必填未填)、phon 模板(`{slot}` + 字面素材如環綴 `ge{stem}t`)、
表層經引擎(build_phrase→run→spell-out)、**不就地改來源 sign**(P42)。負例:
CategoryMismatch/UnknownSlot/DuplicateFill/Unsaturated/NotAConstruction/
TemplateSlotUnknown(不默默近似)。AST `SignItem::Slot`;parser/printer 擴充
(`slot`/`?` round-trip)。🔑 出口過:**德語現在式變位系統**
(`tests/construction_german.rs` 10 案:sage/sagst/sagt/sagen/sagt/sagen 全變位、
optional 分離前綴、環綴分詞 gesagt、範疇授權/殘餘 valence/無就地改/round-trip)。
workbench 73 測試綠;引擎零觸動、wasm 綠。

**語法重設計 + 單一分類樹(2026-07-21,I22;修訂 P38→v0.2、I19/I20 語法面)**:
owner 裁決把 Language 表面語法改為 **colon+縮排**(取代 `{ }`,貼合 tshiatun/Lexurgy):
容器頭 `sign/trait/global trait Name:`(**無 `syn trait`**);**統一 body 語法**
(trait body ≡ sign body,`Item` enum 廢除,Block 持 `Vec<SignItem>`):belongs /
`Name[n]` / `==` / 頂層 Def / **維度區塊** `syn:`/`phon:`/`sem:`/`prag:`(內
`field = value` → `dim.field` Def;`phon:` 下 `/…/` = UR/模板、其餘 = phon 規則;
`syn:` 下 `slots:` → slot 行,`?`=optional)。**分類樹改為單一維度中立樹**
(P38 v0.2:`trait` 是分類節點,`OntologyRegistry` 單樹,syn/sem/phon/prag 退為
內容面向);projection 分類閉包中立、Def 按維過濾;construction phon 模板包於
`/…/`。所有 trait 存續 compile。全回歸綠(雙軌 8.1–8.6、德語變位、compile/codegen/
roundtrip golden 重生);Flow A(共時導出)端到端不變。

**已完成(鳥瞰步驟 12c,I23)**:Construction semantics(form-meaning pair)。
`sem.rs::SemNode`(遞迴 feature-structure:`fields` 純量 + `roles` role→子節點)=
**可擴充語意接口**,容納未來複雜語意模型(義項網絡/衍生邊/frame/多義,以新欄位
擴充不破壞 API)。construction 的 `sem:` 值 `{slot}` → **role 綁 filler 的語意節點**
(`SemNode::of_sign`,非字串替換),否則純量欄位;derived token 增 `sem` 欄(meaning
極)。polysemy/synonymy 合法;`SemRefUnknown`(引用非 slot);部分套用 role 暫略;
不就地改、決定性。出口過:`tests/construction_semantics.rs` 8 案(frame 綁節點、
form+meaning 同時導出、多義/同義、診斷、部分套用)。

**已完成(鳥瞰步驟 12d,I25)**:四維同步規則 + Lexurgy 式 Else 三分。
`Rule.dim` 維度標記(parser 依區塊;printer/codegen 用之,P44 維度隔離:phon 側
只收 phon 規則)。`synchronic.rs`:syn/sem/prag 規則求值於 Sign projection——
`field => value [/ guard]`(guard `[Category]`/`field == value`),寫入自帶維度前綴
(結構隔離);typed `DimPatch`(`Sign × Patch → Sign'`,`apply_patch` 保留原 Sign)。
**Else 三分(P43)**:Matched(含 identity 值未變仍 Matched、阻 else)/Unmatched
(matched==0→else)/Error(畸形→診斷不進 else);逐 sign 判定、順序求值(後見前
patch);`RuleRecord` 保 status/changed/branch/diag/RuleId。出口過:
`tests/synchronic_rules.rs` 10 案。延後:完整 P22 守衛文法、跨維協調、phon else。

**已完成(鳥瞰步驟 12e,I27)= M1++ 閉環**:typed patch 接口 + entrenchment 資料
欄位(僅介面/欄位,無動力學)。`patch.rs`:`Patch{dim, ops}`(Set upsert/Unset 移除
本地 Def)+ 具名建構 `Patch::syn()/…` + builder(自動維度前綴 → 型別層隔離)+
`apply`(Sign×Patch→Sign',保留原 Sign)+ `render`/`parse` 序列化 round-trip。
`SignDef::entrenchment()`/`with_entrenchment()`(跨維 meta 欄位,無固化動力學,
留 M2/B)。**共時語法功能總檢查**:`tests/synchronic_system.rs` 整合一份 mini-grammar
串 12a–12e 全層(分類樹→投影→construction form-meaning→同步規則→patch→Flow A)。
workbench M1++ 封板回歸綠；引擎零觸動。

**共時 `.lang` surface 補齊(2026-07-21)**:維度 Def lhs 接完整 Path
(`.`/`[key]`/`~tier`)；`syn:` 內平坦 `map SLOT OP [ARG]` 與 Rust 共用
`SlotMapOp`，source mapping 經 compile 驗證後進 construction runtime；sign 頂層
`origin/provenance/lifecycle` typed 化。歷史 attestation 年代/文本/可信度仍不進
Language，對照見 docs/14 與 `tests/lang_surface.rs`。

**統一 library + std/cxg + English(2026-07-21)**:依修補06建立
`crates/language/lib/{std,plugin,natural}`，由 `LibraryCatalog` 驗證 kind、dependency、
priority、stable exports 與 rule namespace；`stdlib` 與舊 ontology API 為相容 facade。
std 含 core/grambank/cxg；釘選 Grambank v1.0.3 `stan1293` 的 25
項二元參數，每項有未知根、`_Absent`(0)、`_Present`(1)與保守的 syn/sem/prag
行為 Def。cxg 提供 typed Slot(`[*]`)與 slot-aware Rule；`natural:en-standard` 由
`compile_with_libraries` 顯式選取並提供 12 類核心構式。出口
`tests/library_cxg_english.rs` 與 `tests/stdlib_grambank.rs`，詳見 docs/15–16。
外部 plugin discovery、英語完整文法與缺口新關鍵字仍延後。

**步驟 13 已完成**：`LanguageDocument` 以 versioned sidecar 保存 caller source 的
stable NodeId/Ref binding，`conlang-changeset` 實作 immutable checked
insert/delete/update/move、stable Anchor、LanguageDiff 與 PrimitiveRecord；
`check_language` 已從 codegen 前抽出供 compile/edit 共用。詳見 docs/20。

**步驟 13 語義／API 契約已再封板（2026-07-22）**：V2 Application／Case／CaseBranch／Constraint 亦納入
stable identity、typed resolver 與四原語；未飽和 application 仍是同一 `SignValue`，以
`apply_arguments` 補入自由參數；未飽和只是一個 `SignValue` 狀態，不是獨立實體。
slot／trait rename 會重寫 typed consumers，巢狀 typed case 可 round-trip、執行、定址與
Primitive Edit。功能回歸為根 workspace 251/251、Tshiatūn 157/157；完整本機工具閘門
仍為基礎設施 exit 2，未宣稱 release gate exit 0；證據見 docs/20、docs/23。

**步驟 14 已封板（2026-07-24）**：Primitive-only `.chg` 的 parse／resolve／replay／
lazy compile 與 statement 交易定稿為 Step 14 completion。相容性測試補齊——replay 跨執行
決定性、`.chg` dump round-trip、**三道 digest**(base source／identity-manifest／library
lock)replay 前拒絕、交易回滾/部分保留、lazy compile cache(`step14_interpreter.rs`)。
契約見 docs/22;`cargo test --workspace` 全綠為證(本機無 pwsh,`.ps1` 閘門未實跑)。

**v1 路徑已硬移除(2026-07-24)**:v2 為唯一模型。移除 `LanguageSchema` V1/V2 分野
(FP `case`/`when`/`constraints` 永遠可用,無需標頭;舊 `schema conlang.lang/v2` 行被
接受並忽略,printer 不再輸出)、identity manifest v1(`IdentityManifestV1`/
`IDENTITY_SCHEMA_V1`/`migrate_v1`/read 分支——`LanguageDocument::open` 只吃 v2 sidecar,
v1/未知 → `UnknownSchema`)。**舊 v1 檔不可載入**。移除前已證 v1→v2 升級無損(Stage A,
git history)。12 base fixtures 於 v2 逐字不變(goldens 零 churn);263/263 綠、0 警告、
引擎零觸動。共用型別 `NodeEntryV1`/`RefTargetV1`/`RefBindingV1`(v2 沿用)保留。

