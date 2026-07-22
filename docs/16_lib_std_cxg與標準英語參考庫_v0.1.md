# `lib` 統一語法庫、std/cxg 與標準英語參考庫（v0.1）

## 1. 契約與載入順序

所有可編譯語法資源位於 `crates/language/lib`，套件共同使用
`code/`、`data/`、`config/`。`compile_system(Language)` 只自動載入 enabled std；
`compile_with_libraries(Language, LibrarySpec)` 才會依序合併 dependency、std、選取的
natural、選取的 embedded plugin、caller language。一次最多選一個 natural package。

目前內嵌：

| ID | 用途 | 自動載入 |
|---|---|---|
| `std:core` | 單一分類樹根與通用範疇 | 是 |
| `std:grambank` | 25 項 `0/1/?` 類型學事實 | 是 |
| `std:cxg` | form–meaning schema、Slot 與八個可選 realization trait；只依賴 `std:core` | 是 |
| `natural:en-standard` | 英美共同正式核心的可執行參考文法 | 否 |

外部 plugin 掃描與 WASM host bridge 延後；`lib/plugin` 本階段只定義相同 package
契約。duplicate package/export/alias/rule namespace、未知或停用 dependency、循環與
錯誤 kind 均由 loader 拒絕，並可經 `LibraryLoadError::code()` 取得穩定的 `LIBRARY_*`
機器代碼。同層載入按 priority、package name 排序。

## 2. 語法面與執行語意

form pole 仍是 `phon+syn`，meaning/function pole 是 `sem+prag`。不增加 `form:` 或
`meaning:` 同義包裝。construction 缺 phon template 是 error；缺 sem/prag 先報
`CONSTRUCTION_MEANING_MISSING` warning；未被模板、語意 role 或 Rule 引用的 slot 報
`CONSTRUCTION_SLOT_UNUSED` warning。

Slot 支援 category constraint 與 any-sign：

```lang
syn:
    slots:
        controller [Nominal]
        target [*]?
    agreement.number => unify(
        $slot.controller.syn.number,
        $slot.target.syn.number
    )
```

實際 parser 的 Rule 一行一分支，因此 canonical 形式將 `unify(...)` 印在同一行。
`$slot.NAME.DIM.PATH` 讀 construction 套用前已完成 filler rules 的 frozen snapshot。
slot 不存在或 path 畸形為 compile error；optional slot 未填或 filler 缺該純量為
Unmatched，可進 Else；`unify` 值衝突為 runtime Error。Then 讀前一步 token patch，
但 filler snapshot 永遠不變。

`RuleRecord` 除原 RuleId、三分狀態、branch 與來源行號外，保存 package namespace及
每次 `$slot`／`$self` read 的維度、path、值。`DerivedToken.fillers` 保存 phon/syn/sem/prag、
categories與 provenance，可供下一層 derived token 遞迴使用。
遞迴 filler 重用會先保留 context-free frozen probe；外層 construction 的
slot-specific feature constraint 驗證完成後，可向下套到 stored sign 或既有 derived
token 的單次 occurrence。stored sign 從 effective base、derived token 從私有 deep
baseline 重跑 Syn→Sem→Prag，再重選 `$self` realization；不改 lexicon 或原 token。

`syn: slot_features:` 的 literal 與 `$slot.SOURCE.syn.FEATURE` RHS 全部只讀同批 frozen
probe。整批 binding 先檢查 domain、固定值衝突與重複 target，再原子提交；因此 binding
書寫順序不會污染 RHS。`OccurrenceRecord` 保存 probe 與 committed trace。

`recipe`／`goal` 是歷時 function 的分層名稱，不是共時 `.lang` 關鍵字，也不是
`std/cxg` export kind。共時庫固定分成：

```text
code/schema.lang          抽象 form–meaning 範疇、Slot 與限制
code/realizations.lang    可選的語序／形式實現 trait
data/realizations.tsv     實現索引與查表 metadata
```

manifest 的 `code` 接受有序逗號列表；宣告與 Rule 順序依該列表固定。對外可見性只由
`config/exports.tsv` 決定，`.lang` 不增加 `export` 前綴：表內 `kind=trait|sign` 是實際
AST kind，stable ID 的 `realization:` 只是契約路徑，不是語法關鍵字。
`std:cxg` 不依賴 `std:grambank`；反而是 natural-language construction 同時引用
Grambank observation 與 cxg schema，避免類型學資料反向決定唯一實現。

## 3. 抽象／具象對照

| 要求 | std/cxg 抽象 | English 具象 | 分類 | runtime 證據 |
|---|---|---|---|---|
| 定／不定 NP | DeterminationConstruction | the/a + nominal | Exact | `the dog`, `a book` 與 recursive reference roles |
| 複數 | NumberMarkingConstruction | 單一 `EnglishCountNounForm` + `DerivationContext`／`realization:` | Exact（規則 `-s`） | 同一 SignId 產生 `dog`／`dogs`；不規則仍 lexicalized |
| 所有格 | PossessionConstruction | possessor + literal s + possessed | Workaround | `johns book` 與雙 role |
| 屬性修飾 | AttributionConstruction | adjective–nominal | Exact | `big dog` 與 property/referent |
| 介詞片語 | PrepositionalPhrase | in + NP | Exact | `in house` 與 relation/ground |
| 不及物子句 | IntransitiveClauseConstruction | subject–predicate | Exact/詞形 workaround | `john runs`；unify 驗證一致 |
| 及物語序 | SVOTransitiveClause | subject–verb–object | Exact | `john sees mary` 與 agent/event/patient |
| occurrence-local 格位 | slot-specific syn constraint + baseline re-evaluation + realization guard | `see` 分配 nominative／accusative；stored 或 derived nominal 依 occurrence 實現格位 | Exact | `she sees her`、derived-token forwarding、occurrence trace、source/token immutability與 conflict 拒絕 |
| 繫詞述謂 | CopularPredicationConstruction | subject–be–predicate | Workaround | `john is big`；be 詞形明列 |
| 否定 | NegationConstruction | subject–do–not–predicate | Workaround | `john does not run`；do-support 明列 |
| 極性問句 | PolarQuestionConstruction | auxiliary inversion | Workaround | `does john run` 與 prag illocution |
| 被動 | PassiveConstruction | patient–be–participle–by–agent | Workaround | `mary is seen by john`；participle 明列 |
| Grambank GB132 | verb-medial typology only | English SVO sign另行繼承 realization trait | Exact boundary | GB132 本身不產生 SVO/OVS slot/order |
| Grambank GB117 | 語言中有 copular predicate-nominal 證據 | English construction 選 required `copula [Copula]` | Exact boundary | profile value 不向所有 construction 注入 slot；optional 用 `[Copula]?`、zero 不宣告 slot |

`natural:en-standard/data/grambank-v1.0.3.tsv` 是官方 CLDF `stan1293` 的 25 列縮錄，
保存 Parameter、Value、Code_ID、Language_ID 與來源鍵。特別保留容易誤判的
GB082=0、GB107=1、GB118=1；不得由 English construction surface 反推或改寫。

## 4. 缺口判定與關鍵字結論

| 能力 | 目前判定 | 原因／最小反例 | 本階段處理 |
|---|---|---|---|
| filler feature selection | Supported | `syn: slot_features:` 先從 frozen probe 解析整批 enum constraint，再原子套到單次 occurrence；固定值衝突拒絕且 source 不變 | 維持窄 API；不加 `where` 或任意跨維寫入 |
| derived-token downward forwarding | Supported | 外層 construction 可把 slot-specific feature constraint 傳給既有 derived-token filler；每次 occurrence 從私有 deep baseline clone，不改原 token | 由 `slot_feature_bindings.rs` 的 nested derived-token 反例驗證 |
| contextual filler-rule 重跑 | Supported | occurrence constraint 注入後從 effective/deep base 重跑 Syn→Sem→Prag，再重新選 filler realization | `OccurrenceRecord` 同時保存 probe 與 committed RuleRecords，避免把 Patch 疊加誤當重跑 |
| prag 條件 optionality | Missing primitive | `?` 是靜態 optional，不能依 discourse context 改變 | 報告；不加 `when` |
| 抽象 constituent order | Missing primitive | phon template 可實現順序，但不能獨立聲明偏序約束 | 報告；不加 `order` 關鍵字 |
| productive inflection/allomorphy | Partial | regular `-s` 可由 typed context 選 realization；不規則 see/sees、do/does 等仍須明列 | 保留 lexicalized 標記；不新增形態 edit DSL |
| construction competition | Missing primitive | 無 do-support、已有 auxiliary、copula之間的候選阻擋模型 | 報告；不新增 competition 語法 |
| 語用授權 | Partial | prag 可存結果、Rule 可讀 slot prag，但無外部 discourse context | 報告 |

結論：Grambank 是類型學觀察層，但它可以關聯到 `std/cxg` 的可執行抽象行為；不應
為個別參數新增 `copula`、`GBxxx` 等專屬 DSL 關鍵字。跨語言共用的缺口應提升為通用
Slot／Rule／construction-applicability 原語。原始 value、抽象 schema、可選 realization
與 natural-language construction 必須分層保存。

## 5. Requirement-to-evidence

| 鏈節 | 實作 | 反例／斷言 |
|---|---|---|
| source/config | `lib/*/*/{code,data,config}` | config kind/ID/dependency/export 驗證 |
| parser/IR | `SlotConstraint`、typed `feature:`、`roles:`、`realization:`、`$self` | enum/role/realization round-trip；unknown slot/path 拒絕 |
| selection | `LibraryCatalog::select` | 未選 English 不可見；選後有效語言可見；循環/停用/未知拒絕 |
| transition | frozen probe + deep-baseline occurrence rebuild + sequential token Patch + `DerivationContext` | Then feeding、optional Else、unify conflict、filler immutability、`she`／`her` case realization、context conflict；stored 與 derived-token forwarding 都從 baseline 重跑 filler rules／typed cases，再選 realization |
| observation | `DerivedToken`、`RuleRecord`、`SystemDerivation`、`PhonRealization` | 四維狀態、self/slot reads、package、line、provenance、pure phon input、surface 同時斷言 |
| public API | `compile_with_libraries` | 12 English constructions與八 realization trait 均走真實 phon runtime |
| determinism | catalog/order/derive repeat | effective dump、token、RuleRecord與surface兩次完全相同 |

主要證據位於 `tests/library_cxg_english.rs`、`tests/slot_feature_bindings.rs` 與
`tests/sem_roles_realization.rs`；2026-07-22 Step 13 語義／API 封板為根 workspace **251/251**（language 220、changeset 31）、Tshiatūn
**157/157** 通過。既有 M1++、Grambank、construction與 Tshiatūn 雙軌測試仍為回歸
護欄。這些證據只覆蓋已列出的共時範圍，不是 M1+ 完整封板。完整 Sem/phon runtime
契約見 `docs/18_sem_roles_self_phon_realization_v1.md`。歷時入口、caller-only source
邊界與英語 occurrence-context 格位契約見
`docs/19_M1plus_共時完整性_英語格位與歷時入口_v0.1.md`；該文件是設計清單，不是封板聲明。
