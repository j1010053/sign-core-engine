# CLAUDE.md — conlang-engine 專案指引

你正在開發一個 **autosegmental(自體段)音變引擎**:conlang(人造語言)工作台的核心,
以 Rust 實作,支援多層音韻表徵(聲調、鼻化、和諧)、歷時音變規則 DSL、與未來的 WASM/Tauri 前端。
本專案的設計已經過完整的紙上收斂,**你的工作是實作既定規格,不是重新設計**。

## 0. 最重要的三件事

1. **規格已凍結,以編號決策為準。** 全部設計爭議都已裁決並編號:D1–D28(語法/本體論)、
   A1–A4 / B5–B9 / C10–C11(執行語意)、I1–I10(實作層)、P1–P4(架構修補層)。
   任何實作若與編號決策矛盾,**停下來明確指出衝突**,不要自行變通。若遇到規格未覆蓋的
   新問題,提出方案並建議編為新的 I 系列決策(I11、I12…),寫進
   `docs/05_M0實作參照_v1.0.md` §9 的表格後再實作;架構層變更走 P 系列(權威=《架構修補01》§4)。
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
| `架構修補01_共時規則系統與臨時韻律域_v0.1.md` | **P 系列決策權威(P1–P4)**:Word=臨時韻律域、Grammar Store、strata 層級錨定+循環套用、cophonology 閂;對 M0 步驟 2–3 零衝擊,插入點=步驟 4(`level:` 標記)與步驟 6(phrase-level 掛鉤);修補內容已回寫 docs/01–09、12 |
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
- **歷時演化編輯有限儲存(P2)**:演化 = 編輯 Grammar Store(AddRule/LoseRule/Reorder/Invert)與 UR,**永不枚舉無限的組合空間**;「套規則改詞表」僅為 AddRule+Lexicalization 相容模式。

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
插入點在步驟 4(規則檔 `level: stem|word|phrase` 標記,預設 word)與步驟 6(spellout 的
phrase-level 空掛鉤);Grammar Store 本體為 M0 後的步驟 8。

**已完成(M0 步驟 1,commit `bccc837`)**:`crates/core/src/repr/` 表徵模組——intern、feature、
prosody(I8 拓撲)、melody、word、invariant、notation。本機工具鏈首跑即全綠。

**已完成(M0 步驟 2,commit `2966df0`)**:
- `lifecycle/`:`Action` 六 variant、`commit`(凍結快照+一次寫入;I10 收攏)、`validate`、
  `needs_reparse`(A3)、`run`(執行語意 §1 步驟 3–5 編排)。
- `primitives/`:六原語建構器 + proptest 不變量。新決策 I9/I10(docs/05 §9)。

**已完成(M0 步驟 3)**:
- `strategy/`:統一解析器 `resolve(candidates, reference, strategy)`(D28);內建
  nearest(+prefer-left/right tie-break,D17)/leftmost/rightmost;自定義註冊留步驟 4+。
- `verbs/` 第一批:`insert_floating_near`(onset 特徵環境)、`dock`(條件 associate,
  浮游參考=原位投影 **I11**)、`fill`(逐 Ø insert+associate,D22)、`merge_adjacent_equal`
  (delete+associate)。全組合原語;動詞做執行語意 §1 步驟 1–2,產 `Vec<Action>` 交 `run`。
- 出口已過:`tests/tonogenesis_8_1.rs`——8.1 全規則序列 × 四詞(*pa/*ba/*baba/*a),
  每 commit 一 insta 快照;對立轉移/三分 H/M/Ø/OCP 合併皆有硬斷言。devoicing 為手動
  音段操作(I9,音段層規則機制隨步驟 4+ 引入)。
- 新決策:I11(dock 原位投影),見 docs/05 §9。

**下一個任務(M0 步驟 4,見 M0 實作參照 §8)**:
- `crates/dsl/`:logos lexer(入口 unicode-normalization 正規化,I6)+ chumsky parser
  → 型別化 AST(引用 core 型別);能解析 8.1 規則檔全文。
- **P3 插入點**:規則檔語法加 `level: stem|word|phrase` 標記(預設 word;φ/ι/U 為合法層名無行為)。
- 音段層規則機制(devoicing 類 rewrite)需在此步或步驟 5 定案 commit 通道——屆時提案新 I 決策。
- `crates/cli/`:讀規則檔+詞表 → 跑 → 輸出(含 trace),串 end-to-end。
- 之後:步驟 5(spread/shift/locality/lazy-reparse → 8.2–8.5)→ 步驟 6(scan/spellout + phrase-level 掛鉤 → 8.6)。

## 6. 命名原則(全專案實作規範,凌駕任何單篇審查建議)

**核心規約:名字服務讀者;操作屬哪一層由「原子性/正交性」決定,不由「是不是語言學術語」決定。**

演化操作分三層,命名各異:

- **L1 純資料原語**(讀者=機器/引擎內部):用**工程名**——`Create` / `Delete` / `Modify` / `Split` / `Merge` / `Reference` / `SetTransparency`。術語在此只添亂。
- **L2 語言原語**(讀者=懂語言學的規則作者、演化樹 UI):**凡對應一個穩定、正交、單步的語言學慣例,就用慣例名**——`SoundChange` / `Reanalyze` / `Drift` / `DeriveSense` / `Fuse` / `Adopt` / `Entrench` / `Lexicalize`。自造工程名反而逼作者心譯,增加混淆。無對應慣例時才用通用工程動詞(如 `Fuse`)。
- **L3 理論宏**(讀者=語言學家):用**完整理論術語**——`Grammaticalization` / `Bleaching` / `Univerbation` / `Decategorialization` / `Subjectification`。L3 是資料表(展開成 L1/L2 序列),非 enum variant。

**判定原則**:一個操作進 L2 還是 L3,看它是否「單步且正交」——`Reanalyze` 是(單步結構操作)→ L2 用慣例名;`Bleaching` 不是(= 有方向的 `Drift`,切法因理論而異)→ L3 macro。**術語與否只決定它在所屬層的名字,不決定它屬哪一層。**

推論:`enum ChangeOp` = L1 + L2(小而穩定、正交、原子;L2 多為語言學家一眼認得的慣例名,免心譯);理論標籤全在 L3 資料表,**加新理論只加資料、不改 enum**。macro↔primitive 的雙向映射是儲存的一等資料(演化樹回放要能對使用者顯示術語、對引擎跑資料)。

這條凌駕個別審查的「一律工程名」或「一律術語名」——兩者都錯在忽略讀者分層。同一原則已體現於既有設計:notation 是共同語言、Lexurgy 匯入器做術語轉換、DSL 的 associate(L1 式)vs spread(具名)。

## 7. 工作方式

- **先讀規格再寫碼**:動到某模組前,重讀對應規格章節(§5 的模組↔章節對照在 M0 實作參照 §5)。
- **動工先查 docs/11**:任一模組動工前,先在測試案例總索引查它的驗收案例(什麼算做對了)。
- **測試命名對齊規格**:測試名引用範例編號或決策編號(如 `tonogenesis_8_1`、`ncc_soft_constraint_d7`),
  讓紅燈能直接回溯到規格條文。
- **notation 是共同語言**:任何表徵狀態的斷言優先用 `notation::render_*` 的字串比對
  (`"H~μ0~μ1 (L)@2"`),與規格記法一致、與未來 insta 快照銜接。
- **commit message 前綴**:`M0 stepN:`;若引入新決策:`I9: <一句話> + M0 stepN: …`,
  並同步更新 `docs/05` 的決策表。
- **與使用者的語言**:繁體中文(台灣);程式碼註解可中英混用,識別字用英文。
- **不確定時**:先查決策編號索引(D→docs/02+03、A/B/C→docs/04 §0、I→docs/05 §9、
  P→架構修補01 §4),查無才發問;發問時附上你建議的方案與其對既有決策的影響。
