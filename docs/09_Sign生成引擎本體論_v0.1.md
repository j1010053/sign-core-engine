# Sign 生成引擎本體論(v0.1)— 模組 A 核心

C(07)定了「底層只有 sign」;本檔定「sign 如何被生成、儲存、引用、組合」,以**可直接落成 Rust 型別**為目標。經 18 案架構折磨測試(§10)驗證:0 案破壞設計理念。

> 梯次:【M】MVP /【M+】後 /【A/B驅動】待案例 /【N】v-next。
> 上游:C(07,sign 四維)、M0 repr(interning/快照/單一資訊源/無 petgraph)。
> **先引擎、後投影**:本檔只定引擎(sign 的生成與結構);辭典/語意場/覆蓋率是其產物的唯讀投影(§9),不反向定義引擎。

---

## 0. 職責契約(每模組唯一變更理由;防 God Object)

單一職責的操作判準:**一個模組只有一個變更的理由**。裁決「要不要復用既有機制」需**兩條並用**:(a) 現有機制能表達嗎?(防重複)(b) 這責任屬於那機制的單一職責嗎?(防 God Object)。兩條都過才復用;僅 (a) 過,拆出新的小職責,不硬塞。

| 模組 | 唯一變更理由 | 明確**不**負責 |
|---|---|---|
| **Need** | 造詞意圖如何表達 | 不決定形式、不碰 store |
| **Generator** | 如何從 Need+Store 提出候選(唯讀) | 不寫 store、不分配 id、不做 blocking/validation |
| **Builder** | 如何把選定 proposal 寫進 store(協調) | 不內建語言學;validate/block/resolve 是**委派的 Strategy** |
| **Store** | 語言單位的身分、共享引用、fork | 不記錄歷史(那是 D);不做寫入決策(那是 Builder) |
| **Sign** | 一個語言單位由哪些維度**組裝** | 各維複雜性住各維型別,不由 Sign 代管 |
| **維度型別(Phon/Sem/Syn/Prag)** | 該維的內容、diff、B 原語掛鉤 | 不知道彼此(跨維整合在末端,如 C 的 spell-out) |
| **D(外部)** | 歷史演化如何記錄(version 序列) | **不**負責共時的 fork 能力(那是 Store);不當所有版本的 God Module |

紅線(審查者三觀察):D 開始管非歷史事、Sign 長成所有單位超類(爆 Option)、Builder 開始懂語言學——任一發生,拆出去,不續塞。

---

## 1. Sign:維度的稀疏容器【M 骨架 / A/B驅動 各維細節】

```rust
struct Sign {
    id: SignId,                       // interned;唯一有 id、進 store、被 D 快照者
    dims: Dimensions,                 // 稀疏:一 sign 只持有它實際有的維度(Case 8)
    slots: SmallVec<[Slot; 0]>,       // 非空 = 抽象 sign(construction);空 = 詞彙 sign
    entrenchment: Entrenchment,       // 固著度;token→type 固化門檻(C §4)
    origin: Option<SignId>,           // 借詞/衍生的輕量共時指標(詞源鏈全史在 D)
    provenance: Provenance,           // Native | Loan | Grammaticalized | Suppletive | ...
    lifecycle: Lifecycle,             // Active | Obsolete(tombstone,不真刪;§8)
}

struct Dimensions {                   // 稀疏容器:維度種類封閉、持有可選
    phon: Option<Phon>,
    sem:  Option<Sem>,
    syn:  Option<Syn>,
    prag: Option<Prag>,
}
```

- **construction 與 lexeme 是同一型別 `Sign`**,差別只在持有的維度子集與有無 slots(Case 5/8)。加新單位種類**不改 Sign**——防 Sign 變超類。
- 維度種類**封閉**(就這四個),但**持有可選**——避免「一堆 None 表示 layer 沒固定」的批評:topic construction 只持 syn+prag,無 phon(Case 8),合法且零成本。
- morpheme/word/construction 抽象度不同、組合尺度不同,但同型別、同組合運算(§5)。

## 2. 四維定義(各維:內容 / partial / diff / B 掛鉤)【M 骨架 / A/B驅動 細節】

每維是自足型別,遵守同一契約:有 partial 形式(供補全)、diff 函數(供 D §6)、B 原語掛鉤。維間互不知曉。

**PHON —— 即 M0 的 `Word`(autosegmental)。**
```rust
struct Phon { form: PhonForm }        // PhonForm 包裹/引用 M0 的 Word;可部分(模板)
```
補全=phonotactics+頻率抽樣(有效分佈與 seeded 抽樣見 docs/10);diff=autosegmental 編輯距離;B 掛鉤=`sound-change`(跑 DSL 規則)。**construction 的 phon 可為部分 tier 指定**(模板 CVCVC=骨架+元音 tier),詞根填輔音 tier=M0 association——**非串接構詞天生支援**(Case 17/18)。

- **phon 存底層形 UR(《架構修補01》P1)**:表層形永不儲存,按需導出。
- **construction 的 phon 增 cophonology 槽(可選;P4 雙來源閂)**:`cophonology: Option<RuleSetRef>`——構式專屬小規則組,僅使用者手動或 `Morphologization` 宏可建,自動演化不得憑空為任意子類建 cophonology(文獻警告:可切分的子模式數量無限)。

**SEM —— sense 集合,指向節點內演化的概念網絡。**
```rust
struct Sem { senses: SmallVec<[SenseId; 1]> }
struct Sense { concept: ConceptId,            // 指本語言節點的概念網絡節點(非普世)
               derivations: Vec<DerivEdge> }  // 出邊掛來源 sense(單一資訊源)
struct DerivEdge { to: SenseId, kind: DerivKind, transparency: Transparency }
```
- **Concept 是本語言節點概念網絡的節點,自身會演化**(答審查者第三點):概念 split/merge = sem 網絡上的 split/merge,與 sign 的同機制(Case 1/13)。`ConceptId` 指向該網絡,不指普世清單——語意密度是語言性質。CLICS 是對各語言概念網絡的投影統計,非共用底層。
- polysemy vs homonymy = sense 間有無透明衍生邊連通;透明度漂到不透明 → B `split`,兩新 sign 各留 origin。
- **最小演化單位在 sem 是 concept/sense,非整 sign**(Case 1)。

**SYN —— usage-based 輕量 trait【欄位 A/B驅動】。**
```rust
struct Syn { provides: CatSet, requires: SmallVec<[Slot; 0]> }  // 範疇取自 §9 受控本體
```
單步匹配(=M0 `FeatBits::contains` 超集測試)、無約束求解;複雜巢狀交上層 sign。**productivity 住在 construction sign 上**(其 slots 約束 + entrenchment),非 Builder、非全域(Case 2)。

**PRAG —— 規約化語用【骨架 M / 欄位 A/B驅動;語言學定位與五類/可程序化分級見 07 §5b】。**
```rust
struct Prag { frame: Option<FrameId>, discourse: SmallVec<[DiscourseTag; 0]>,
              register: Register, speech_act: Option<SpeechActId> }
```
不只 register(答審查者第四點):資訊結構(topic/focus)、示證、言者意圖需 frame/discourse/speech_act。**Prag→Morph→Syn 的跨層演化**(敬語,Case 10)以 prag 標記為起點,B 原語逐維遷移。標籤取自受控本體的 prag 分支。

## 3. 生成流水線:Need → Generator → Builder → Store【M】

分層(採審查者第五點),**唯讀提議與唯寫修改分離**:

```
Need ──▶ Generator(唯讀:讀 Need+Store)──▶ Vec<Proposal> ──▶ Builder(唯一寫入)──▶ Store
                                              (帶評分,排序在此側)      │委派
                                                              validate / block / resolve = Strategy
```

- **Generator 唯讀、可多實作**(rule / AI / LLM / 借詞),共用同一 Builder。**雙向**:正向填槽提議(組合造詞)+ **逆向拆解提議**(逆構詞 editor→edit,Case 4)。
- **Proposal 帶評分**(音韻/文化/詞頻分量),**排序是提議側的事**;Builder 只對選定者寫入(Case 12)。
- **Builder 純協調**:assign SignId、intern、寫入;validate(良構)/ blocking(同義阻擋 thief 擋 stealer,Case 11)/ conflict resolve 一律**委派給可插拔 Strategy**(仿 DSL D28),不是 Builder 內臟——防 God Builder。
- **對齊 M0 執行語意**:Generator=Parallel Match(唯讀)、Builder=Commit(唯一寫入點)。

候選(Proposal)= UI 幻影,不進 store、無 id(C 的候選非真實);seeded 生成保證可重現,拒絕不留痕(頂多工具層 reject-cache,不入專案資料)。

## 4. 連接關係:五種,各單一資訊源,無統一圖【M】

**沒有 `Graph<Sign>` 物件**(採審查者第一點)。sign 間有五種不同型別/拓撲的關係,各有唯一的家;「圖」是查詢時組裝的視圖(同 C 層是投影、同 M0 無 petgraph)。

| 關係 | 拓撲 | 唯一存放處 | 案例 |
|---|---|---|---|
| **component**(成分) | DAG | 複合 Sign 持 `Vec<Component>`;每個帶 transparency + 是否 fusion | 火車(6)、hamburger(3)、portmanteau(15) |
| **origin**(來源) | 鏈 | `Sign.origin` | 借詞、逆構詞 |
| **derivation**(sense→sense) | 網絡 | `Sense.derivations` | 隱喻固化(7) |
| **slot-fill**(填槽) | 組合時暫態 | `Token.fillers`(不進 store) | 組合造詞 |
| **paradigm**(範式) | 表 | construction sign 的 slots + `Sign.origin` 標 suppletive | go/went(14) |

```rust
struct Component { child: SignId, role: Role,
                   transparency: Transparency,   // 透明→D replay 傳播成分音變;不透明→凍結(Case 6)
                   fusion: bool }                 // true=多成分不可線性切分(portmanteau,Case 15)
```

- **paradigm**:範式是 construction sign(範式模板)+ 其填充 sign。**suppletion** = 範式某槽由獨立 sign 填充,`provenance: Suppletive` + 化石標記(不由共時規則推導),不經構詞(Case 14)。
- **不變量**(仿 M0 `check_word`,回報為分級資料非 panic):component DAG 無環、origin 鏈無環——`check_store()`。

## 5. 組合層級:兩種「構詞」必須分清【M 骨架 / A/B驅動 細節】

**關鍵區分(18 案最重要的產出):「詞由語素組成」有兩種完全不同的機制。**

1. **串接/複合 = sign 間的 component 組合**:火+車→火車。抽象 construction sign 的 slot 收填充 sign,單步匹配(syn.provides ⊇ slot.constraint),產 Token,fillers 記 component 連結(初始透明)。遞迴 → DAG。複雜句式(倒裝)= 高層抽象 sign,非 SYN 內遞迴。
2. **非串接/模板 = 單一 sign 的 phon 維內 tier 聯結**:k-t-b 填 CVCVC。**不經 component**,而是 phon 維(M0 `Word`)的 association——詞根是部分 tier 指定的 sign,模板是骨架+元音 tier 的 construction,組合=把詞根輔音聯結到模板骨架(Case 17/18)。

> **實作紅線**:模板構詞**不可**做成串接 component,否則 Semitic 詞法錯誤。component 在 sign **之間**;tier association 在 sign **之內**(phon 維)。你所謂「取模板填詞」精確地說是後者(phon 維 association);複合是前者。

**臨時 Word 的建構(《架構修補01》P1/P3)**:component 組合樹的實體化 = 建構**臨時韻律域**(M0 `Word`,預設 ω)——這正是層疊套用的驅動:逐組合環跑 stem-level(含觸發構式的 cophonology)→ 整詞 word-level → spell-out 跑 phrase-level(循環套用語意見執行語意規格 §2.5)。導出的表層形是 token,僅跨 entrenchment 閾值者固化回 store。

Slot:
```rust
struct Slot { role: Role, constraint: CatSet }   // 配價;約束取自受控範疇
```

## 6. 生命週期:三態型別 + 四結構轉換【M】

三態是**三個型別**(採審查者第二點,非 enum State 一直 match):

```
PartialSign(候選/輸入,無 id,不進 store)
   │ Builder 採納+補全
   ▼
Sign(進 store,得 id,被 D 快照)  ──split(1→2)/ merge(2→1)/ lose(tombstone)──▶
   ▲
   │ entrench 跨閾值 lexicalize
Token(組合暫態,不進 store)
```

- 只有 `Sign` 進 store/被快照/有 id。候選與 token 在 store 外,生滅不留痕(除非 token 固化升格)。
- **lose = tombstone**(標 Obsolete,id 保留供 D diff 與 origin 鏈;化石接口),不真刪——同 M0「空節點合法暫態」的克制。
- split/merge 生新 id + origin 指回舊 id。
- **這四轉換 = B 的結構原語(08)= D 的 ChangeEntry**:生命週期、B 原語、D 條目三位一體,同一組操作三個名字。

## 7. 共享與 fork:Store 負責,D 只記錄何時【M 共享 / M+ 結構共享】

答審查者第六點 + 防 God-D(職責裁決):

- **共享** = 兩 sign 的某維指向同一 interned 內容(如共用 PhonForm P1),Store 的 interning 提供(同 M0 SymId 模式)。
- **fork** = 其一演化時,對該維斷開共享、生新內容;**fork 的能力屬 Store**。
- **version**(歷史上多版本序列)屬 **D**;D 記錄「此 fork 發生在歷史哪一點」,但不提供 fork 能力。
- **不引入 feature 版本化子系統**(拒絕審查者處方):否則 feature 級 + 節點級兩層版本控制職責重疊。最小演化單位 = **sign 的某維**(B 分維原語 target 之),已足(Case 7)。

## 8. 與 D 的接口【M】

- Sign store 是 D 節點的內容;D 快照/replay 復用 M0 snapshot-and-actions(MVP:每節點完整 store,clone;結構共享列 M+)。
- ChangeEntry 作用於「某 SignId 的某維」或結構轉換(§6);replay 重建 store。
- diff 以 SignId 對齊(C v0.1.1);sem diff 深入 concept/sense。

## 9. 投影層(唯讀,引擎定義後才登場)【M】

辭典=已固化 sign 的表格投影;語意場/覆蓋率=sem 對參考地圖(Swadesh/CLICS)的投影;派生家族=沿 origin+derivation 的 DAG 投影(對齊 DeriNet);匯出=投影序列化。投影不回寫引擎。

## 10. 架構折磨測試(附錄:18 案判決)

七問固定:Need 是什麼 / Generator 誰負責 / Builder 要不要知道 / Store 存什麼 / D 如何 replay / 有無新機制 / 有無 God Object。判決 ✅ 直接吸收 /⚠️ 補既有原則內定義 /❌ 破壞原則。

| # | 案例 | 判決 | 落點 |
|---|---|---|---|
| 1 | 青→青/緑 語義分裂 | ⚠️ | sem concept split;最小單位=concept(§2) |
| 2 | happiness✓/smartth✗ 生產力 | ⚠️ | productivity 在 construction sign(§2 SYN) |
| 3 | hamburger 重分析 | ⚠️ | reanalyze=改 component 切分+create(§4) |
| 4 | editor→edit 逆構詞 | ⚠️ | Generator 逆向提議(§3) |
| 5 | be going to→gonna | ✅ | construction=sign(§1) |
| 6 | 火車 不透明 | ✅ | component transparency(§4) |
| 7 | 共享成分 fork | ✅ | Store fork + D version(§7) |
| 8 | dative 無 form | ✅ | Sign 稀疏維度(§1) |
| 9 | V不V 同形異構 | ✅ | 引擎不做剖析消歧(職責邊界) |
| 10 | 敬語 Prag→Morph→Syn | ⚠️ | Prag 厚骨架(§2 PRAG) |
| 11 | thief 擋 stealer | ✅ | blocking=委派 Strategy(§3) |
| 12 | 河:20 候選 | ⚠️ | Proposal 帶評分,排序在提議側(§3) |
| 13 | 手/臂 colexification 分裂 | ✅ | =Case 1 同構(§2) |
| 14 | go/went suppletion | ⚠️ | paradigm 槽 + Suppletive 化石(§4) |
| 15 | au=à+le portmanteau | ⚠️ | component fusion 標記(§4) |
| 16 | sheep 零標記 | ✅ | Ø 已對齊 Leipzig(§2/C) |
| 17 | k-t-b 非串接 | ⚠️ | phon 維 tier 聯結,非 component(§5) |
| 18 | CVCVC 模板 | ✅ | construction phon 模板+填根(§5) |

**結論:0 案破壞理念。** ⚠️ 九案的定義已全部寫入 §1–§5;❌ 為空。本體通過折磨測試。

## 11. 實現順序

| 梯次 | 項目 |
|---|---|
| 【M】 | Sign 稀疏容器(§1)、四維骨架(§2)、Need→Gen→Builder→Store 分層+職責契約(§0/§3)、五連接關係+check_store(§4)、兩種構詞(§5)、三態生命週期(§6)、Store 共享/fork(§7)、投影(§9) |
| 【A/B驅動】 | 各維欄位細節(SYN slots 種類、PRAG frame/speech_act、SEM derivkind 全集)——待 A2 組合造詞 + B 原語案例回填 C §3.3 |
| 【M+】 | 結構共享最佳化(§7/§8)、Generator 的 AI/LLM 實作、blocking/scoring Strategy 進階 |
| 【N】 | (無 A 特有;跨模組留白見 D/B) |

**依賴**:各維欄位由 A2(組合造詞)+ B(原語)案例逼出,回填 C §3.3。下一步:細化 B 的原語集(定死 D 的 ChangeEntry op),或開始 A 的 Generator/Builder 實作骨架。
