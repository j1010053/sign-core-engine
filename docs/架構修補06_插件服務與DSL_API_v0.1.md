# 架構修補 06:插件、服務與 DSL API(v0.1)

> **依賴(一律以此二者為準)**:《架構 2.0 總鳥瞰 v1.0》、《架構修補彙整 01–04 v1.0》(P1–P19 權威表);另依賴《架構修補 05》(P20–P28)。
> **本修補新增**:P29–P37(見 §8)。
> **範圍**:三個彼此扣合的擴充面——(一)插件系統(資料層與程式碼層),(二)外部服務的生命週期(ServiceRef → resolve → 執行 → 驗證 → History),(三)音變 DSL 的最小對外 API(經 Lexurgy 對照修訂)。

---

## 0. 修正目的

回答三個問題:

1. **官方標準庫與未來第三方插件**如何提供共用的 trait / sign、歷時 function 與服務?
2. **外部服務**(SemanticBackend / DistributionProvider / Strategy)如何被引用、執行、驗證、重播?
3. **音變 DSL 作為獨立軟體**(P20),最小要開出什麼 API?

三者共用一條貫穿的界線:**資料層(宣告式,.lang/.chg 可表達)與程式碼層(真正運算,Rust 實作)嚴格分離**——延續 P5(trait 不存計算)、P22(條件語法三道界)的既有鐵律。

---

## 0b. P20 補述(兩條;非新決策,是 P20 未答完的具體問題)

### 補述一:feature 語法不遷移,留在 dsl 域

**問題**:dsl 的 `Feature voice (+voice, -voice)`(Lexurgy 形)是否遷至 Language 的 `feature voice = (+, -)`(Definition 形)?

**裁決:不遷移。** 判準即 P20 邊界測試——dsl crate 認不認識這個語法?`Feature`/`Symbol`/`Class`(音段集合義)宣告的是音系原子系統(M0 `FeatBits` 的特徵位);DSL 獨立發布(路徑 A)**必須能自己宣告 feature**,否則規則檔寫不出 `[+voice]`。遷去 Definition 形 = 強迫獨立模式依賴 Language 語法,違反 P20 依賴方向。Language 的 `=` 只管 dsl 不認識的概念(prosody/受控範疇/trait/global/sign)。

**連帶訂正修補 05 §10.1**:檔案格式定義區拆兩段——
```
# dsl 域(Lexurgy 形;dsl parser 認得)
Feature voice (+voice, -voice)
Symbol  m [+sonorant, +nasal]
Class   vowel {a, e, i, o, u}

# language 域(Definition 形 `=`;language parser 專屬)
prosody = μ σ Ft ω φ ι U
category VERB ⊂ SYN_ONTOLOGY
```
**邊界情況**:`class` 一名兩域(音段集合=dsl;受控範疇=language)——**靠檔案區域區分,非關鍵字**;步驟 9 明確測試。

### 補述二:crate 佈局——build_word / executor 歸屬

**判準**:由「誰認識 Sign」決定。

| 元件 | 認識什麼 | crate |
|---|---|---|
| `build_word`(組合樹→臨時 Word)、循環套用迴圈 | Sign(必須) | **language** |
| `executor` / lifecycle / primitives / verbs | 只有 Word | **dsl**(不變) |
| `compile_phon_rules`(Compiled Grammar phon 側) | Sign+規則格式 | **language**,輸出 `dsl::Program` |

```rust
// language(依賴 dsl 型別;單向)
fn build_word(sign: SignId, ctx: &Language) -> dsl::Word
fn compile_phon_rules(&Language) -> dsl::Program
```
executor 對「第幾環、為何跑 stem 三次」一無所知——每環由 language 呼叫 `build_word` + `dsl::run`。

---

## 1. 插件目錄架構【P29】

### 1.1 三層目錄

```
/std/<套件名>/            ← 官方標準庫(隨軟體發布)
/plugin/<套件名>/         ← 第三方插件(使用者安裝)
    code/     *.lang + *.chg     ← trait / sign 與具 layer metadata 的歷時 function
    data/     語言知識、機率權重   ← E1 先驗庫快照、Weight DB、分佈表
    config/   啟用開關、export 表  ← 中介資訊(見 §1.2)
```

**code/ 與 data/ 的分界判準**:**會被 compile(進 AST)的東西在 `code/`;只被查表讀值的東西在 `data/`。** trait/sign 與歷時 function 定義在 code;先驗數值、權重表在 data。Goal/Recipe 是 function 的分層 code 名稱，不是 `.lang` 或 `.chg` 關鍵字。

### 1.2 config/ 與 export 機制

`config/` 包含:

- **啟用開關**:本套件是否參與 compile。
- **export 表**:**只有列在 `exports.tsv` 的符號對外可見**;compile 時自動掃描所有啟用套件的 export 表,**無需顯式 import**(無 `use` 語句)。`.lang` 不使用 `export trait`／`export sign` 前綴；visibility 與 stable ID 只由 config 表負責。
- **priority 權重**(選填):本套件在跨套件衝突時的優先序。
- **Registry 實作清單**(若本套件附帶程式碼層服務,列出提供的服務名,§3)。

**export 的關鍵作用——隔離內部檔案結構**:Evolution_node 的 ChangeSet **永遠不引用套件內部檔案路徑**;export 表是唯一穩定契約。套件作者重組內部檔案不會斷掉任何既有 replay。

**export 帶穩定 ID**:每個 export 項有穩定識別(`套件:版本:符號` 形式或等價 UUID);**名字只是人類可讀別名,底層引用走 ID**——與 P26 決定性 ID 同一精神(穩定身分優先於可讀名字)。套件作者改名不斷引用。

### 1.3 解析與衝突

- **就近解析(auto-discovery)**:`VerbCommon[1]` 依 priority 由高到低找同名 export;找到即綁定。
- **Priority 四層**(P6/P14 的跨檔案擴展,同構於居所階梯):

```
未啟用(不參與)< std(最低)< 已啟用 plugin(依 config 權重或載入序)< 專案本地定義(最高)
```

- **同名衝突且同 priority**:compile **warn + 強制消歧**(不靜默選一)——此時才需要寫全名 `套件名::符號`(`::` 與 Path 的 `.` 視覺區分,防與欄位存取混淆);平常靠 auto-discovery,不寫前綴。
- **語法化路徑標準庫的落點**:Heine & Kuteva 路徑庫 = `std::grammaticalization` 套件的 `code/*.chg`(一組 Recipe-layer 具名 function),docs/08 懸置的「路徑資料庫」就此落地——**官方加新路徑 = 加 .chg 檔,不改引擎**。

---

## 2. 程式碼層插件:哪些可外部檔案、哪些必須後端【P30】

**判準**:核心動作能否用 .lang/.chg 的宣告式語法表達?能 → 外部檔案;不能(真正計算 / 外部 I/O)→ 後端 Rust。

| 服務 | 判定 | 落點 |
|---|---|---|
| **Strategy(簡單條件式)**:blocking(「已有同義高固著 sign 則擋」)、基本 validate | ✓ 條件判斷,P22 語法可表達 | `code/`(宣告式規則) |
| **Strategy(複雜算法式)**:語音相似度加權排序等 | ✗ 具體算法(編輯距離) | Registry(Rust trait) |
| **SemanticBackend**:drift 量化(向量/LLM) | ✗ 數值運算/外部服務;P22 明文禁計算 | Registry(Rust trait) |
| **DistributionProvider 的執行**:讀 PHOIBLE 統計、跨節點投影 | ✗ 外部 I/O、跨節點查詢 | Registry(Rust trait) |
| **DistributionProvider 的產物**:某次統計出的權重表快照 | ✓ 純數值表格 | `data/`(快照表) |

宣告式 strategy 範例(住 `code/`):

```
strategy blocking BlockBySynonym {
    reject / exists(sign) & sign.sem.concept == proposal.sem.concept
                          & sign.entrenchment > 0.7
}
```

**Registry 統一形狀**:

```rust
struct PluginRegistry {
    semantic_backends:      HashMap<Name, Box<dyn SemanticBackend>>,
    distribution_providers: HashMap<Name, Box<dyn DistributionProvider>>,
    strategies:             HashMap<Name, Box<dyn Strategy>>,
}
```

`.lang`/`.chg` 內**只存名字引用**(如 `drift(backend: "llm-v2")`),啟動時查表解析——資料層永遠不含邏輯。

**MVP 限制【P31】**:受 WASM-safe 規範(禁 dlopen/執行緒),MVP 的程式碼層插件 = **編譯期靜態註冊**(feature flag / 顯式註冊清單);**執行時動態載入為 N 級**,屆時走 **host 端橋接**(插件邏輯跑 host shell,經既定介面呼叫核心),非動態連結。

---

## 3. 服務生命週期【P32–P34】

六項確認,構成完整鏈:**身分(ServiceRef)→ 綁定(resolve)→ 執行(request–response)→ 安全(core 驗證)→ 決定性(History)**。

### 3.1 ServiceRef:具型別、namespace、版本【P32】

```
ServiceRef = { kind:      SemanticBackend | DistributionProvider | Strategy,
               namespace: std | <套件名>,
               name:      "llm-drift",
               version:   SemVer }
```

- **kind** → resolve 時型別檢查(Strategy 名字填錯進 backend 欄位 = 編譯期錯,非執行期爆)。
- **namespace** → 服務也是套件的 export 之一(與 trait/歷時 function 同一張 export 表)。
- **version** → replay 可追溯「當年用哪版」;實際角色見 §3.4(c)。

### 3.2 resolve 與 execution 分離【P32】

與 compile/execute 分離(P10)同構。三個收益:

1. **啟動時全量 resolve**:載入 ChangeSet 即檢查所有 ServiceRef 可解析——缺服務立即報錯,不是跑到第 47 條語句才炸。
2. **fixture 注入點**:測試時 resolve 到 stub(鳥瞰 §3.2「SemanticBackend 必須可 stub」的機制落點:換 resolve 表,不改 ChangeSet)。
3. **綁定表可 dump**:「每個名字綁到哪個實作哪個版本」是一份可序列化的表,debug 與重現靠它。

### 3.3 core / host 同一邏輯名稱系統 + request–response 模型【P33】

- **呼叫端只看邏輯名**,不知道服務住 core(編譯進 WASM)或 host(Tauri shell 側的 LLM/檔案 I/O);Registry 在 resolve 時綁 in-core 實作或 host-bridge proxy。**部署差異是 resolve 的細節,不是呼叫端的語意**——同一份 ChangeSet 在「純離線」與「有 LLM host」環境都合法,只是綁定表不同。
- **request–response / 暫停恢復**是三條既有約束的交集唯一解(core 禁執行緒 + host 服務慢且非同步 + 直譯器要長時間批次跑):

```
直譯器遇 service 呼叫 → 發 Request、序列化暫停點 → 讓出控制權
host 完成 → 帶 Response 恢復 → 續跑
```

本質為 coroutine/effect-handler,core 全程單執行緒。**紅利**:暫停點可序列化 = 長演化可存檔續跑、進度條、可取消——批次子層(docs/12)要的非同步排程,不用執行緒就拿到。

- **裁決 (a) 暫停粒度**:允許**語句中途**暫停(一條 rewrite 展開到一半等 LLM),但**交易邊界不變**——暫停序列化「已展開部分 + 待決 Request」,恢復續展開,**commit 仍整句原子**(P26 語句級交易不破)。

### 3.4 core 驗證 + History record–replay【P34】

- **外部結果經 core 驗證才能改 Language**:host 回傳 = 不受信任輸入。Response 先過 core 的型別/schema 檢查,再由 core 轉成 Primitive Edit,走**正常的語句級交易 + `check_language()`**(P26)。**外部服務永遠沒有直接寫 Language 的路徑**——只能「提供資料給 core 的正常寫入流程」。安全邊界 = 信任邊界 = crate 邊界,三線合一。
- **History record–replay**(解決 P26「逐位元可重現」與 LLM 非確定性的正面矛盾):

```
首次執行:呼叫服務 → 結果記入 History(以呼叫點為鍵)
Replay:  不再呼叫服務 → 直接讀 History 記錄
```

非確定性被隔離在「首次執行」那一刻,**replay 永遠確定**;連帶「離線 replay」成立(當年用了 LLM,今天沒網路照樣 replay)。

- **裁決 (b) History 存哪**:存 **Evolution_node 的側表**(service-result log,鍵 = 語句序號+呼叫序);**不進 Language**(非語言知識)、**不進 ChangeSet 本文**(ChangeSet = 使用者意圖,記錄 = 執行事實;使用者要新結果時清記錄重錄,本文不動)。與 View Config 不進 Language 同一原則(意圖/事實分離)。
- **裁決 (c) 版本釘選**:**replay 一律走 History,不重呼叫服務**(版本不匹配不影響 replay);版本檢查只在使用者主動 **re-roll** 時生效——版本不同即警告「結果可能與當年不同」。History 是版本問題的主要解,version 欄位降為 re-roll 提示。

---

## 4. DSL 最小 API(經 Lexurgy 對照修訂)【P35–P37】

### 4.1 呼叫點倒推(需求來源)

| 呼叫者 | 要 dsl 做什麼 |
|---|---|
| `language::compile_phon_rules`(步驟 11) | 解析 phon 規則 → 規則集 |
| 循環套用(步驟 12,流 A) | 建 Word、跑某 stage、拿回 Word |
| spell-out(流 A 末端) | phrase 規則 + 轉寫 → 表層字串 |
| 直譯器 `sound_change`(流 B) | 對 UR 跑規則(restructuring) |
| Generator/Builder validate(流 C) | phonotactics 合法性 |
| UI 詞源回放(流 D) | 逐步 trace |

### 4.2 Lexurgy 對照結論

Lexurgy 的公開 API:字串進出(`change(words, startAt?, stopBefore?, debugWords)`),Word 型別不外露;`Declarations` 為獨立物件貫穿傳遞;trace 具名記錄;CLI 內建 `--compare-versions` 迴歸。

| 對照結果 | 內容 |
|---|---|
| **驗證** | `Program` 自足編譯單位(= 其 SoundChanger);Decls 獨立物件、建構在外(其 Declarations 貫穿);trace 不存全量快照、深查用重跑(其 startAt 錨點哲學) |
| **缺口一** | 我方缺**字串門面層**——獨立產品(P20)需要 Lexurgy 同形的傻瓜介面 |
| **缺口二** | 我方缺**部分執行錨點**(startAt/stopBefore)——其最受歡迎的 debug 功能 |
| **教訓** | Unicode 規範化曾放錯位置出 bug → **規範化是 parse 的內部職責**,呼叫者不做(防雙重轉換) |
| **刻意差異** | Word 層外露(我方有 sign 組詞需求,它無)、stage 參數(P3 分層,它平坦)、建構門面(串接/聯結分立,docs/09 §5 紅線)、RuleId 錨點(P25 穩定身分——它用規則名當錨,改名即斷) |

**多出的介面無一多餘,皆對應 2.0 的增量功能——對照本身即 API 邊界劃對的證據。**

### 4.3 兩層 API 定稿【P35】

```rust
// ═══ 門面層(路徑 A:獨立產品;Lexurgy 對等)═══
pub fn change(src: &str, words: &[String],
              start_at: Option<&str>, stop_before: Option<&str>)
    -> Result<Vec<String>, Diags>;
// 字串進出;內部自理 parse + run + surface;CLI 另提供 --compare-versions、選定詞 trace

// ═══ Word 層(路徑 B:主引擎)═══
// 0. 核心型別(可序列化、Result+thiserror、WASM-safe)
pub struct Program;                    // 自足編譯單位(宣告+規則)
pub struct Word;                       // 臨時韻律域(M0 repr)
pub enum  Stage { Stem, Word, Phrase } // 惰性標籤:dsl 不解讀語意,語意在 language(P3/P18)
pub struct Diags;                      // error/warn/info/trace 分級
pub struct RunSpan { start_at: Option<RuleRef>, stop_before: Option<RuleRef> }  // 部分執行

// 1. 編譯
pub fn parse_program(src: &str) -> Result<Program>;   // 規範化(NFC)在內部做
pub fn parse_word(text: &str, decls: &Decls) -> Result<Word>;
//    Decls 建構 API 開放;多來源 merge(含 priority)在 language 側做完再交付

// 2. Word 建構門面(language::build_word 的積木)
pub fn word_empty(level: ProsodyLevel) -> Word;
pub fn word_concat(parts: &[Word], brackets: &[Bracket]) -> Word;      // 串接構詞
pub fn word_associate(template: &Word, melody: &Word) -> Result<Word>; // 非串接(tier 聯結)
pub fn word_wrap(level: ProsodyLevel, words: &[Word]) -> Word;         // 包 φ/ι/U(sandhi 域)

// 3. 執行(單 stage 單次;循環由 language 驅動)
pub fn run(word: &Word, prog: &Program, stage: Stage, span: RunSpan) -> (Word, Diags);
pub fn run_traced(...) -> (Word, Diags, Trace);   // Trace 存 notation 字串(輕);深查=重跑到錨點

// 4. 輸出
pub fn surface(word: &Word, prog: &Program) -> Result<String>;  // 純函數(C11)
pub fn render(word: &Word) -> String;                           // notation,round-trip

// 5. 驗證
pub fn check_word(word: &Word) -> Diags;
pub fn check_phonotactics(word: &Word, prog: &Program) -> Diags;

// 6. 邊界
pub fn brackets(word: &Word) -> &[Bracket];
```

**M+ 延後**:`distance(a,b)`(D 的 phon diff 分量)、反向套用(N)。

### 4.4 三條契約【P36–P37】

- **括號穿越保證【P36】**:規則執行(含 commit 錨點重編)後 morph 括號位置正確維護,`brackets()` 隨時可查——**sandhi 拆回 sign 的唯一依據**。M0 的 I2 已隱含在做,現升格為對外契約,配專測:拼接→跑規則→拆回,sign 邊界不錯位。
- **run 單 stage 單次【P35 內】**:executor 對「第幾環、為何跑 stem 三次」一無所知;循環套用的迴圈在 language(crate 佈局裁決的兌現)。
- **雙軌迴歸的精確對象【P37】**:同一 `(Word, Program, Stage)` 三元組,路徑 A(門面)與路徑 B(language 經 Word 層)必須產出相同 `(Word′, Diags)`;加上 P20 §1.4 的表層逐字比對,共三個比對面。

---

## 5. 白話摘要(供快速理解)

- **插件就是資料夾**:官方標準庫和第三方插件都是「code(定義)+ data(數值)+ config(開關與公開清單)」三個資料夾;寫新的語法化路徑 = 加一個 .chg 檔,不改引擎。引用不用 import,編譯時自動找;撞名才要求寫全名。
- **需要真正運算的東西**(LLM 語意漂移、讀外部資料庫、複雜算法)不能塞進資料檔,住在 Rust 側的註冊表;資料檔裡只寫名字,啟動時查表。
- **外部服務可能很慢、可能亂回**:引擎呼叫時可以暫停存檔等結果;結果回來先驗證再走正常寫入流程;**第一次的結果會被記下來,之後重播直接讀記錄**——所以就算用了 LLM,歷史重播永遠一致、離線也能跑。
- **音變引擎的介面學 Lexurgy**:外層傻瓜(丟規則丟詞拿結果,還能「跑到第幾條規則停下來看」),內層精細(組詞、分階段、查結構)——多出來的每一個介面都對應我們比 Lexurgy 多的功能(詞典、演化、分層音系),不多不少。

---

## 6. P 系列決策(本文件新增)

| 編號 | 決策 |
|---|---|
| **P29** | **插件 = code/data/config 三層目錄 + export 表**:std 與 plugin 同構;code=會 compile 的宣告、data=查表數值、config=啟用+export+priority+Registry 清單;Goal/Recipe 只是歷時 function layer code,非關鍵字;**export 為唯一穩定契約**(帶穩定 ID,名字僅別名;隔離內部檔案結構,ChangeSet 永不引用內部路徑);auto-discovery 無顯式 import;Priority 四層(未啟用<std<plugin<本地);同名同級衝突 warn+強制消歧(此時才用 `套件::符號`);H&K 路徑庫 = `std::grammaticalization` 的 .chg |
| **P30** | **程式碼層服務入統一 PluginRegistry**:SemanticBackend(必後端)/ DistributionProvider(執行必後端,產物快照可入 data)/ Strategy(簡單條件式可入 code 宣告,複雜算法式後端);`.lang`/`.chg` 只存名字引用,啟動查表——資料層永不含邏輯 |
| **P31** | **MVP 插件為編譯期靜態註冊**:受 WASM-safe 約束;執行時動態載入為 N 級,走 host 端橋接(邏輯跑 host、經介面呼叫核心),非 dlopen |
| **P32** | **ServiceRef{kind, namespace, name, version} + resolve/execution 分離**:kind 供編譯期型別檢查;服務與 trait 同走 export 表;啟動全量 resolve(缺服務立即報錯);resolve 表 = fixture 注入點 + 可 dump 綁定表 |
| **P33** | **core/host 同一邏輯名稱系統 + request–response 暫停恢復**:部署位置(core/host)是 resolve 細節非呼叫端語意;暫停點可序列化(存檔續跑/進度/取消);**允許語句中途暫停,commit 仍整句原子**(P26 不破) |
| **P34** | **外部結果經 core 驗證 + History record–replay**:host 回傳為不受信任輸入,過 schema 檢查→轉 Primitive Edit→正常交易+check_language,外部服務無直寫 Language 路徑;History 存 **Evolution_node 側表**(鍵=語句序號+呼叫序;不進 Language、不進 ChangeSet 本文——意圖/事實分離);**replay 一律讀 History 不重呼叫**(非確定性隔離於首次執行;離線 replay 成立);版本檢查僅於 re-roll 時警告 |
| **P35** | **DSL 兩層 API**:門面層 `change(src, words, start_at?, stop_before?)` 字串進出(Lexurgy 對等,獨立產品用);Word 層(parse/Decls 建構/word_empty·concat·associate·wrap/run(stage, span)/surface/render/check/brackets)供主引擎;**RunSpan 部分執行錨點用 RuleId**(改名不斷);Trace 存 notation、深查重跑;**規範化為 parse 內部職責**(呼叫者不做,防雙重轉換);Stage 為惰性標籤(語意在 language);Decls 多來源 merge 在 language 側 |
| **P36** | **括號穿越契約**:規則執行後 morph 括號位置正確維護、`brackets()` 可查——sandhi 拆回 sign 的唯一依據;I2 升格為對外契約,配拼接→執行→拆回專測 |
| **P37** | **雙軌迴歸三比對面**:同 `(Word, Program, Stage)` 下路徑 A/B 的 `(Word′, Diags)` 相同 + P20 §1.4 表層逐字相同;列步驟 11 出口 |

---

## 7. 逐文件修補清單

### CLAUDE.md
- §2 不變式追加:「外部服務結果必經 core 驗證,無直寫 Language 路徑(P34)」「replay 讀 History 不重呼叫服務(P34)」「資料層(.lang/.chg/data)永不含可執行邏輯,只存名字引用(P30)」。

### docs/05 M0 實作參照
- §8 步驟 11 出口:雙軌迴歸擴為三比對面(P37)。
- 新增節:DSL 兩層 API 面(P35)為 dsl crate 的對外契約;括號穿越契約(P36)及其專測。

### docs/06 演化圖(D)
- Evolution_node 增**側表:service-result History**(P34);replay 語意補「服務呼叫一律讀 History」。

### docs/08 A/B 分配
- 「路徑資料庫」落地為 `std::grammaticalization` 套件(P29);SemanticBackend 經 ServiceRef 引用(P32)。

### docs/10 統計引擎(E)
- DistributionProvider 拆分:執行(Registry)/產物快照(plugin 的 data/)(P30);Weight DB 可由 plugin 的 data/ 提供。

### docs/11 測試索引
- 新增:resolve 表 stub 注入(P32)、History replay 決定性測試(P34)、括號穿越專測(P36)、雙軌三比對面(P37)。

### docs/12 邏輯分層
- 應用層補:PluginRegistry 與 resolve 階段;批次子層補:request–response 暫停恢復與序列化暫停點(P33)。

### 修補 05
- §10.2 ChangeSet 格式:服務引用寫法 `drift(backend: "llm-v2")` 的 resolve 語意指向本檔 §3。
- §13 實作插入點追加:步驟 8 同時定 export 表格式與穩定 ID(P29);步驟 14 直譯器含暫停恢復骨架(P33)與 History 側表(P34)。
