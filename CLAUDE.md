# CLAUDE.md — conlang-engine 專案指引

你正在開發一個 **autosegmental(自體段)音變引擎**:conlang(人造語言)工作台的核心,
以 Rust 實作,支援多層音韻表徵(聲調、鼻化、和諧)、歷時音變規則 DSL、與未來的 WASM/Tauri 前端。
本專案的設計已經過完整的紙上收斂,**你的工作是實作既定規格,不是重新設計**。

## 0. 最重要的三件事

1. **規格已凍結,以編號決策為準。** 全部設計爭議都已裁決並編號:D1–D28(語法/本體論)、
   A1–A4 / B5–B9 / C10–C11(執行語意)、I1–I18(實作層,docs/05 §9)、
   **P1–P64、P69–P70(架構修補層;P1–P19 權威=《架構修補彙整 01–04》§1，
   P20–P64 權威=《架構修補彙整 05–11》§1，
   **P69–P70 權威=`specifications/function分支語意與選擇層_v1.0.md` §6，
   P71（含增修 A/B/C）權威=`specifications/Def路徑封閉清單與feature分工_v1.0.md` §5、§7–§9
   （已裁定，**Phase 1 已落地 2026-08-02**；Phase 2 待 M4），
   **P72–P80 權威=`architecture/架構修補13_引用與插值語法統一_v0.1.md` §3
   （引用與插值語法統一；已落地，行為量測見該檔 §6）**；
   個別修補文件保留詳細理由與落地紀錄，出入處一律以彙整為準;
   P7 已廢止→P14，P19 的 nativization 放置由 P56/P58/P64 局部覆寫，
   **P48 的 body 分支語意由 P69 修訂**；
   P65–P68 為《架構修補12》的**提案**，未定案，不得引用為決策)**。
   任何實作若與編號決策矛盾,**停下來明確指出衝突**,不要自行變通。規格未覆蓋的新問題:
   實作層提案編 I 系列入 docs/05 §9;架構層變更走 P 系列。
   **R 系列**(`architecture/演化專案結構與套件載入_v0.1.md`)是**已裁定但刻意不編 P 號**
   的第三類(R10,2026-08-04):套件載入那組(W/E/S、R7′、R9-a、R11–R14)已實作,
   權威掛在《修補06》§8 增修 A;專案結構那組(R1/R2/R4/R5/R6)尚未實作,僅本檔為準。
   **R15(2026-08-04)**:P29/P50 的「auto-discovery,**無顯式 import**」
   **適用範圍限 `.lang` / `.chg` 兩種資料層檔案**,不及於專案層宣告
   ——故 `project.toml` 的 import 表合法。仍禁:`.chg` prelude 的
   `import <ns> sha256:`、`.lang` 的 `import`/`use`、跨套件引用內部檔案路徑。
   界線判準與可重現性論證見《修補06》§8.5(權威)。
2. **每個開發階段必須以測試出口收尾。** 不存在「做完但沒有測試綠燈」的階段
   (M0 實作參照 §8:每步以哪個範例綠燈為出口)。
3. **哨兵規則:** 若你發現自己在逐檔/逐函式翻譯 Lexurgy(Kotlin, GPL-3.0)的原始碼,
   立即告知使用者,不需事先確認。參照它的 ANTLR 文法檔與行為文件是允許的;翻譯實作不是。

## 1. 文件鏈(規範層級,衝突時上位優先)

依序閱讀 `docs/`:

| 檔案 | 角色 |
|---|---|
| `architecture/架構書_v0.3.md` | 全局:產品定位、模組、路線圖(M0–M4) |
| `specifications/02_語法規格_v0.3.md` | DSL「能寫什麼」:selector、動詞、Scan、決策 D15–D28 |
| `specifications/03_語法規格_v0.2_凍結附錄.md` | **凍結但仍規範性**:D1–D14 與基礎定義的權威來源(v0.3 以「承 v0.2」引用) |
| `specifications/04_執行語意規格_v0.1.md` | 「怎麼跑」:Execution Model 生命週期、動詞語意表、locality、spell-out 純函數、A/B/C 決策 |
| `implementation/05_M0實作參照_v1.0.md` | 實作層:workspace、表徵方案、crate 清單、開發順序、I1–I8 |
| `specifications/演化圖本體論_v0.1.md` | 模組 D(容器層):節點/邊/來源統一、克里奧爾、互通度、replay。**設計層,非 M0 實作範圍** |
| `specifications/分層結構檔本體論_v0.1.md` | 模組 C(節點內部):sign 統一本體、四維特徵結構、組合=運算、固化=語法化、層作視圖投影(v0.1.1 修補)。**設計層,SYN 欄位待 A/B 驅動** |
| `architecture/AB模組需求分配_v0.2.md` | A=完整 sign 生產者(含組合造詞、借詞、entrenchment 初值);B=**原語集三層定稿**(L1 資料/L2 語言/L3 理論宏),已回填 06 的 ChangeEntry op=L1∪L2。命名分層規約見本檔 §6 |
| `specifications/Sign生成引擎本體論_v0.1.md` | 模組 A 核心:Sign 稀疏容器、四維、Need→Generator→Builder→Store 職責契約、五連接關係、兩種構詞、生命週期;附 18 案折磨測試(0 破壞)。**設計層** |
| `specifications/Def路徑封閉清單與feature分工_v1.0.md` | **P71 權威**:`Def` 路徑限封閉清單、自造欄位一律走 `feature:`、`sem.gloss` 併入 `senses`。增修 **A**=封閉清單同時約束 synchronic rule 目標(`gloss` 非法為規則目標)、**B**=`.chg` 新增 `feature[<dim>.<name>]` selector、**C**=`feature:` 開放至 `prag`(`phon` 仍不支援)。§7.5 為 A4 重新量測(§3 的數字是執行次數,已作廢)。**Phase 1 已落地;Phase 2 待 M4** |
| `specifications/function分支語意與選擇層_v1.0.md` | **P69–P70 權威**:歷時 function body 的四種形狀(序列/`case:`/`when:`/`choose:`)、分支條件三選一、frozen matching;選擇移出引擎層(零候選為合法結果)。與 `case_when與context_fragment_v2.md` 逐條對齊 |
| `specifications/統計先驗與抽樣引擎_v0.1.md` | 模組 E:唯讀先驗庫(PHOIBLE/Grambank/WALS/CLICS,附網址與授權)+ 無狀態抽樣;有效分佈=手動>導入 provider>投影>E1 先驗,覆寫層住節點。**設計層;01–10 設計鏈至此閉合** |
| `verification/測試案例集總索引_v0.1.md` | **全專案測試索引**:DSL 範例 8.1–8.6、18 案折磨測試、十實例、Rust 測試、Lexurgy 黑盒的統一映射與狀態;動工任一模組前先查其驗收案例 |
| `architecture/邏輯分層架構_v0.1.md` | 四層架構(展示/應用/引擎/資料);引擎分即時+批次兩子層,DSL core crate 跨兩子層共用;應用層(Command/Query API)為下一個設計空白。**設計層** |
| `architecture/架構修補01_共時規則系統與臨時韻律域_v0.1.md` | **P 系列決策權威(P1–P4)**:Word=臨時韻律域、Grammar Store、strata 層級錨定+循環套用、cophonology 閂;對 M0 步驟 2–3 零衝擊,插入點=步驟 4(`stage:` 標記,I14)與步驟 6(phrase-level 掛鉤);修補內容已回寫 docs/01–09、12 |
| `architecture/架構修補02_Trait機制_v0.1.md` | Trait = macro 展開模板(P5–P7;P7 廢止→P14) |
| `architecture/架構修補03_Trait與CompiledGrammar_v0.2.md` | Compiled Grammar 責任分離、Definition/Rule 二分(P8/P9) |
| `architecture/架構修補04_共時／歷時分離與歷時實作分層_v0.1.md` | 共時=編譯器/歷時=直譯器、歷時四層、State(P10–P13) |
| `architecture/架構修補彙整_01-04_v1.0.md` | **P1–P19 權威總表** + 裁決(P14–P19)+ 廢止/更名對照(全庫檢索用) |
| `architecture/架構修補彙整_05-11_v1.0.md` | **P20–P64 權威總表** + 覆寫／相容性清冊 + 契約到證據 + Step 17 缺口 |
| `architecture/架構2.0總鳥瞰_v1.0.md` | **2.0 單一入口**:Language/ChangeSet 雙軌全圖、四條資訊流、Debug 模塊化、**新實作順序(步驟 8–22,M1–M4)** |
| `architecture/架構修補05_Primitive與檔案格式_v0.1.md` | P20–P28 詳細來源:DSL 獨立性、IR dump/canonical printer、條件語法、四原語、Ref、檔案格式 |
| `architecture/架構修補06_插件服務與DSL_API_v0.1.md` | P29–P37 詳細來源:插件 code/data/config、服務生命週期、DSL API；完整插件仍是設計層。**§8 增修 A(2026-08-04)= 裁定 W/E/S 與 R7′/R9-a/R11–R15 的權威掛載點**:std 特權降為可覆寫預設、package 不必是編譯期常數；**§8.5 = P29/P50「無顯式 import」的適用範圍界線**(限 `.lang`／`.chg`) |
| `architecture/分層差異向量_v0.2_裁定.md` | **已裁定,未實作**(擁有者 2026-08-07)。現行 `diff_vector` **只走 `signs`**,故一條音變規則、或 `trait` 加一行 `belongs`(影響數百詞的閉包)**diff 皆為零**——實測一次音變後互通度 `1.0`,方言分群看不見它。這與《演化圖本體論》§6.1「規則性音變其次」「差異 = ChangeSet 距離的分層投影」不符,屬**實作未達規格**(§6.1 標【M】)。四條裁定:①**階層向量**(外層仍四維,內層分 signs/rules/…)②**不做正規化**,發 `both`/`changed`/`only_before`/`only_after` 四個原始計數,Jaccard 由呼叫端算(§6.4)③補**整個 `traits` 容器**④先忠實計數,權重歸 measure。⚠ **§3.1 待裁**:trait 規則傳播到 sign 要不要重複算。實作會 bump `UI_SCHEMA_V1` |
| `architecture/接觸痕跡與語言聯盟_v0.1.md` | **問題陳述,未裁定**(S-a/S-b/S-c)。接觸痕跡**只記在節點層**(`.chg` 的 `donor`、`Edge::reference`),沒記在內容層:`Adopt` 不設 `origin`(而 `validate_origin_graph` 的 `::` 豁免因此無生產者)、`SoundChange` 不帶 donor。**語言聯盟(Sprachbund)全 repo 未建模**;§6.2 已把趨同動力學推到【N】multi-agent。附語言聯盟定義與其與借入/克里奧爾的對照 |
| `architecture/資訊流D應用層框架_v0.1_提案.md` | **提案,未定案,不得引用為決策**(D-a–D-f 待裁)。應用層(步驟 21–22)參考框架:Query 純投影、Command 三分類(Language/View/ProjectData)、內容定址快取、互通度／分群只定接口。**Undo 按使用者活動分三條**:(A) 專案編輯=編輯一份寫到一半的 `.chg`(不落節點,**不需新格式或 `working/` 槽**)、(B) 演化 commit=app history stack(**非** graph parent 遍歷,children 在現行結構不可查)、views/data=文件編輯歷史。**§9 誌誤**記 v0.1 的四處錯 |
| `architecture/演化專案結構與套件載入_v0.1.md` | **R 系列裁定的詳細推導**(程式碼註解引用 `裁定 W/E/S`、`R7′`、`R9-a`、`R11`–`R14` 者查此檔)。**權威分兩半**:套件載入組(W/E/S、R7′、R9-a、R11–R15)以《修補06》§8 為準,**已實作**;專案結構組(R1–R6:`project.toml`／`packages.lock.json`／`views/`／`data/`／`packages/`)**僅本檔、尚未實作、未編 P 號**(R10 裁定 2026-08-04),隨 M4 落地 |
| `architecture/架構修補10_歷時function層與載入_v0.1.md` | P47–P55 詳細來源:function surface/load、接力展開、路徑庫、ServiceContext 接點、components、`.chg` canonical |
| `architecture/架構修補11_演化樹節點模型_v0.1.md` | P56–P64 詳細來源:immutable snapshot、typed rebase、node-v2、全 parent merge、donor、persistence |
| `architecture/架構修補12_授權面與封裝面分離_v0.1_提案.md` | P65–P68 **提案,未定案**:digest 移至邊、環境鎖分離、授權/封裝二分、bundle |
| `architecture/架構修補13_引用與插值語法統一_v0.1.md` | **P72–P80 權威**(P75 含**增修 A**=構式內部不回指構式本身;**P79** = function guard 主體改 `$<參數名>` + 環境求值,連言 `&&` 進文法):`$` 只建立引用不求值、`{…}` 求值後嵌入,兩者正交;主體一律顯式(裸寫法與首段猜測移除)、`{…}` 內容只有主體、範疇比對交還 `[Trait]` guard、Path 縮減為點分名段。§6 附行為量測(語料庫分佈、查找端是字串鍵、封閉清單可容納的形狀) |
| `architecture/架構修補09_phon命名block_v0.md` | P46 詳細來源:phon `name:`、結構化 block、propagate、grouped codegen 與 authoring |
| `verification/` | 測試索引與封板證據：M1++、Step 13、Step 14、Step 16；只宣告可觀測完成狀態，不取代規範權威 |
| `specifications/` | 規範性契約：DSL、Language、演化圖、Sign、統計與共時資料語意；02–04 的 D/A/B/C 決策編號保留於檔名。 |
| `implementation/` | 實作路徑與 authoring 方案；05 的 I 系列決策編號保留於檔名。 |
| `architecture/架構修補08_具名可定址節點_v0.1.md` | P45 詳細來源:可選標籤 + keyed 定址；標籤不取代 stable id |
| `architecture/架構修補07_共時四維系統_v0.1.md` | P38–P44 詳細來源:單一 ontology、四維內容/規則/patch、slots、construction-as-Sign、Else 三分 |
| `comparison/共時lang語法與資料貼合度_v0.1.md` | **共時 surface 實作對照**:巢狀 Path、`.lang` SlotMap、typed sign metadata，以及 Language 與 Evidence/Attestation 的 type/token 邊界。 |
| `comparison/std_Grambank預設traits_v0.1.md` | **stdlib 資料對照**:修補06 package 分層、Grambank v1.0 的 25 項 trait 子集、0/1/? 知識狀態、行為映射、限制與測試證據。 |
| `../tutorials/` | 可執行教學；規範仍以 `docs/` 與 P 系列彙整為準 |
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

### 4.1 本機怎麼跑測試(桌面 app 的系統依賴)

**完整閘門仍然是 `cargo test --workspace`**,CI 跑的就是它
(`windows-latest` + `ubuntu-latest` 兩個 OS,並額外跑
`cargo check -p langcraft-desktop`、前端 typecheck/lint/unit/build、
`xvfb-run pnpm e2e`)。

但 `apps/desktop/src-tauri`(`langcraft-desktop`)需要**該平台 webview 的
dev 套件**才編得起來——Tauri 不自帶瀏覽器引擎,借用 OS 的
(Linux→WebKitGTK、Windows→WebView2)。Linux 上還連帶要 D-Bus
(桌面通知/系統匣走它)。缺任何一個,`libdbus-sys` / `webkit2gtk-sys` 的
`build.rs` 會在 `pkg-config` 那一步 panic。

**沒裝那些套件的機器上,`--workspace` 會整組失敗**——包含九個本來編得過的
語意 crate。此時用:

```sh
cargo test --workspace --exclude langcraft-desktop   # 885 綠,完全不碰 Tauri
```

`--exclude` 是**單次指令的旗標**,不改 `Cargo.toml`、不改 CI、不從 workspace
移除任何東西。它買到的是「一個建不起來的 crate 不要把另外九個一起扣住」,
**不減少任何覆蓋率**。

代價要知道:這樣跑**驗不到** Tauri 那 39 個 command 的簽名是否還對得上
`conlang-app` 的公開 API(改 `conlang_app::ipc` 時尤其相關),要等 CI 才知道。
那個代價不是 `--exclude` 造成的——沒裝套件的機器本來就編不了它。

要在本機補齊(Linux):

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

**不要**改成把 `langcraft-desktop` 從 `[workspace] members` 移出去。那會讓它
掉出 CI 的 `--workspace`,得另立一條步驟接住——兩份設定各自要記得維護,
漏一邊就沒人知道。`tshiatun` 是真的排除,但理由不同:它**自有 workspace**
(P20 獨立產品,自帶測試與 insta 快照路徑)。

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
`path::parse_path`,Def 路徑驗證+步驟 13 定址複用;當時含 `.`欄位/`[key]`/`~tier`
三種段,**`[key]`/`~tier` 已由 P80 移除**,現為 `Name ('.' Name)*`)。
出口過:**round-trip 恆等**(canonical 輸入逐位元;id 依文件序決定性再生)、
非 canonical 輸入正規化為不動點、source→AST golden、錯誤定位(行號)。
規則 env/action 內部與守衛求值的結構化 = 步驟 10(compile pass 需求驅動)。

**引擎分離(2026-07-17,I7 v2)**:core/dsl/cli/examples/corpus 移至獨立 repo
**`../tshiatun`**(Tshiatūn/切韻;GPL-3.0-or-later;規則檔 `.qy`;bin `tshiatun`;
14 套件綠、Lexurgy 黃金 8/8、wasm 綠)——P20「獨立可分軟體」的實體化。
本 repo(工作台)含設計文件 + `crates/language`／`changeset`／`persistence`；
**音變主引擎以 git submodule 掛於 `tshiatun/`**（language 的 dep path =
`../../tshiatun/crates/dsl`）。`.gitmodules` 使用 GitHub 遠端 URL；引擎目錄為 ASCII
`tshiatun`。
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
(當時為 `.`/`[key]`/`~tier`;**後兩者已由 P80 移除**)；`syn:` 內平坦 `map SLOT OP [ARG]` 與 Rust 共用
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
`check_language` 已從 codegen 前抽出供 compile/edit 共用。詳見
`docs/verification/Step13_PrimitiveEdit與SourceIdentity_封板_v1.md`。

**步驟 13 語義／API 契約已再封板（2026-07-22）**：V2 Application／Case／CaseBranch／Constraint 亦納入
stable identity、typed resolver 與四原語；未飽和 application 仍是同一 `SignValue`，以
`apply_arguments` 補入自由參數；未飽和只是一個 `SignValue` 狀態，不是獨立實體。
slot／trait rename 會重寫 typed consumers，巢狀 typed case 可 round-trip、執行、定址與
Primitive Edit。功能回歸為根 workspace 251/251、Tshiatūn 157/157；完整本機工具閘門
仍為基礎設施 exit 2，未宣稱 release gate exit 0；證據見
`docs/verification/Step13_PrimitiveEdit與SourceIdentity_封板_v1.md`、docs/23。

**步驟 14 已封板（2026-07-24）**：Primitive-only `.chg` 的 parse／resolve／replay／
lazy compile 與 statement 交易定稿為 Step 14 completion。相容性測試補齊——replay 跨執行
決定性、`.chg` dump round-trip、**三道 digest**(base source／identity-manifest／library
lock)replay 前拒絕、交易回滾/部分保留、lazy compile cache(`step14_interpreter.rs`)。
契約見 `docs/verification/Step14_ChangeSetInterpreter_封板_v1.md`；
`cargo test --workspace` 全綠為證(本機無 pwsh,`.ps1` 閘門未實跑)。

**步驟 15 已封板（2026-07-30）**：義項／衍生邊成為 sem 一級節點；12 項
Atomic Rewrite 皆展開為四原語；層②③④統一 `name(args)` 呼叫；function 定義、
auto-discovery、data 路徑與 `ServiceContext` 接點依 P47–P55 落地。Step 17 才會把
Recipe／Goal 變成實際 runtime 層，本步不提前宣稱。

**步驟 16 已收官（2026-07-30）**：分層 diff、immutable EvolutionGraph／node-v2、
typed rebase、全 parent 3-way merge、donor／`adopt`、狀態→四原語 reconstruct 已接通。
收官補齊 identity reconciliation（exact `open` 仍 digest-strict）、同父 LCS Move、
persisted expression／realization typed reconstruct 與 phon source insert／顯式
`.phon_block:` bootstrap。P60／P64 亦由獨立 host persistence crate 收官：共享
content-addressed objects、node folder、hash-external annotation/config；細部拒絕邊界與
驗證矩陣見 `docs/verification/Step16_文件契約與驗收矩陣_v1.0.md`。
下一個歷時層是 Step 17。

**已完成(鳥瞰步驟 18,C1)= M3 開跑**:`crates/generate`——模組 A 的造詞流水線
`Need → Generator(唯讀)→ Vec<Proposal>(帶評分)→ 選擇 → Builder → Vec<PrimitiveEdit>`。
**C1 增修**(`Sign生成引擎本體論` §12):規格原文的 `Store` 在 2.0 不存在
(P2/P10 語言知識只住 Language、Grammar Store 計畫作廢),身分改由
`LanguageDocument`、fork 由 `EvolutionGraph`、持久化由 `conlang-persistence`;
**Builder 不改 Language**,構造 `AtomicRewrite::Create` 交既有
`rewrite::expand` 降階——造出來的詞因此自動可 replay、進得了演化圖、
受三道 digest 保護。選擇層**兩個模式對應干涉光譜**(架構書 §0;P12 明訂抽樣器只服務自動模式):
`ranked()`=手動/輔助(引擎只排序,選擇權交出去,全程不碰抽樣器)、
`sample_proposal(seed)`=自動(走與步驟 17 Goal 選擇**同一個**
`sample_weighted_index`,注入式 ChaCha20Rng,trace 記 algorithm/seed/ordered)。
兩者共通:列舉與選擇分離、零候選 `Ok(None)`(P70);有候選卻全零權重回 Err。
validate/blocking/resolve 三個 Strategy **委派不內建**(§0 紅線)。
出口 `generate/tests/coining.rs`(8 案):端到端造詞 → 四原語 → replay → `.lang`′;
折磨 **11**(thief 擋 stealer,且換掉策略即通過——證明未內建)、
**12**(20 候選、排序在提議側);Proposal 是幻影不改文件;origin/provenance 記錄。
突變 4/4 首輪全紅。**尚未做**:逆構詞 Generator(折磨 4)、真正的 donor 借入
(沿用既有 `Adopt`,需 `DonorScope`)、E1 抽樣接線(步驟 19)。

**已完成(鳥瞰步驟 19)**:`crates/stats`(模組 E)+ 流 C 接點。權威=
`統計先驗與抽樣引擎_v0.1` §1–§4 + **§6 增修 A**(擁有者 2026-08-04)。
**有效分佈是三層**(手動 > 導入 provider > E1),§2 原訂的第三層「統計投影」
**已移出抽樣棧**——投影照做但接 Query 當**唯讀報表**(`project_phoneme_freq`),
抽樣器不看。代價:分佈不隨演化自動更新,出口「造詞像這個語言」弱化為
「像**使用者宣告的分佈**」;可逆(日後在 provider 與 E1 間插回即可)。
鍵 = **IPA 字串**(不用 SymId,免得把 E 綁進引擎)。E1 自 package
`data/*/segments.tsv` 載入(裁定 W:先驗是 data;R9-a 後可外部注入)。
`EffectiveDistribution` 逐項覆寫 + `provenance()` 可審計每項來自哪層。
phonotactics 過濾 = **注入式 `PhonotacticFilter`**(§6.3),`generate` 零引擎依賴,
且**事後過濾**不藏進 Generator——「提了幾個、擋掉幾個」是自動模式要審計的數字。
`DistributionGenerator` 即流 C 圖上「Generator + E 抽樣」的最小實作,
複用既有 `sample_weighted_index`。評分合成公式依 §6.4 **永久擱置**。
投影切分依**給定音素清單**最長匹配(§6.6)——清單由呼叫端提供,因 dsl 域宣告
是不透明區塊(I15-a),`stats` 不得越界解析;清單外的音段仍計入使問題現形。
出口:`stats/tests/effective_distribution.rs`(13 案)、
`generate/tests/distribution_driven.rs`(6 案);突變 8/8 首輪全紅。
**尚未做**:E1 實際資料(PHOIBLE/Grambank 子集需離線匯入,§1 標明
Index Diachronica 授權不明、SSWL 覆蓋 2%–100% 極不均)、
`NaturalLanguage` provider、投影快取。

**已完成(鳥瞰步驟 20)= M3 收工**:`changeset::state::EvolutionState`
(time / region / society / contacts)。**定位收斂為 (A)**(修補04 增修 A):
State 是**撰寫時**的環境輸入,**replay 永不讀它**,故雜湊外
(`nodes/<id>/state`)。規格原句「使相同的 ChangeSet 在不同環境產生不同結果」
與 P26/三道 digest 衝突,已修訂為「影響**產生什麼** ChangeSet」。
`nativization` 不在 State——P19 已被 P56/P58/P64 覆寫為 immutable node content
(早已實作);`contact_history` 則仍屬 State。
接抽樣權重走**既有 provider 接點**(`generate::ContactInfluence`),不另立一層
——獨立一層等於宣稱它是抽樣棧常駐成員,會誘使日後在 replay 路徑上讀它。
出口 = **判別對**:正(記下接觸 → 鄰語音素進候選)+ 負(同一份 `.chg` 在不同
State 下 replay 產物逐位元相同);另有 persistence 往返 + 雜湊外驗證。
突變 3/3 全紅。**尚未做**:UI 顯示(M4 步驟 22)。

**v1 路徑已硬移除(2026-07-24)**:v2 為唯一模型。移除 `LanguageSchema` V1/V2 分野
(FP `case`/`when`/`constraints` 永遠可用,無需標頭;舊 `schema conlang.lang/v2` 行被
接受並忽略,printer 不再輸出)、identity manifest v1(`IdentityManifestV1`/
`IDENTITY_SCHEMA_V1`/`migrate_v1`/read 分支——`LanguageDocument::open` 只吃 v2 sidecar,
v1/未知 → `UnknownSchema`)。**舊 v1 檔不可載入**。
**`schema conlang.lang/v2` 行已於 2026-08-07 完全移除**:它在 v1 淘汰後對解析、
canonical dump 與 identity digest 皆零影響,parser 卻仍特別認得它——留著等於保留
一條「認得但無意義」的語法。現改為**顯式拒絕**並附「刪掉這一行」的訊息;
不能只把分支拿掉,因為 `.lang` 會把首個 language 構造前的內容 verbatim 交給 dsl 域
(裁決1),那行會變成難懂的 tshiatun token 錯誤。repo 內三個 `.lang` 已一併清乾淨。移除前已證 v1→v2 升級無損(Stage A,
git history)。12 base fixtures 於 v2 逐字不變(goldens 零 churn);263/263 綠、0 警告、
引擎零觸動。共用型別 `NodeEntryV1`/`RefTargetV1`/`RefBindingV1`(v2 沿用)保留。

