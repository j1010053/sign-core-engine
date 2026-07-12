# conlang-engine

> **開發者(含 Claude Code)請先讀 `CLAUDE.md`**:專案指引、設計不變式、決策制度、當前任務都在那裡。
> 規範文件依閱讀順序放在 `docs/01`–`12` + `docs/架構修補01`(01–05 規範/實作層、06–10+12 設計層、11 測試索引、修補01=P 系列決策權威);`docs/archive/` 為歷史檔勿引用。

Autosegmental 音變引擎(M0)。規範上游:《M0 實作參照 v1.0》→《執行語意規格 v0.1》→《語法規格 v0.3》。

## 目前進度:M0 步驟 5 完成 — 範例 8.1–8.5 全數端到端綠燈

```
cargo run -p conlang-cli --bin conlang -- examples/8_1_tonogenesis.dsl examples/words.txt
```

- **步驟 5**:I13 音段刪除跨層連鎖(SegDelete cascade:Span 平移、mora keep-empty、
  無核心音節清理、調浮游 D14);verbs 第二批 spread/shift/dominate_empty + rewrite 泛化
  (@class、coda、`*` 刪除、`.` 音節界);locality 可達上界;DSL 對應語法。
  範例 8.2(鼻化和諧)、8.3(聲調穩定)、8.4(補償性延長)、8.5(ATR 雙向)全綠
  (`examples/8_2`–`8_5` + `crates/dsl/tests/dsl_8_2_to_8_5.rs`)。

- `crates/dsl/`(步驟 4):logos lexer(NFC 入口,I6)+ chumsky parser + lowering + executor;
  宣告貼合 Lexurgy(`Feature`/`Symbol`/`Class`)+ 自有 `Melody`;音段規則 `A => B / C _ D`
  走 I12 通道(`SegRewrite` + `Inventory` 反查);`stage:` 標記已插入(P3/I14,預設 word)
- `crates/cli/`(步驟 4):規則檔+詞表 → 逐詞推導表(trace);暫定 CV 音節化
- `crates/core/src/strategy/`(步驟 3):統一候選解析器(D28)
- `crates/core/src/verbs/`(步驟 3–4):insert_floating_near、dock(I11)、fill(D22)、
  merge_adjacent_equal、rewrite(I12)——動詞=原語組合;rewrite=音段規則通道

### 步驟 1–2 基座

- `crates/core/src/repr/`(步驟 1):intern(SymId/ValId)、feature(FeatBits/Registry,含 [αF] 遮罩)、
  prosody(Level/Span/AnchorRef/StaleFlags,I8 拓撲)、melody(Autoseg/MelodyTier/policies)、
  word(Word 快照)、invariant(NCC/覆蓋/包含檢查,分級回報)、notation(H~μ0 / (H)@3 渲染)
- `crates/core/src/lifecycle/`(步驟 2):`Action` 六 variant、`commit`(凍結快照+一次寫入,I1/I2/I10)、
  `validate`(分級診斷)、`needs_reparse`(A3:repair 不觸發)、`run`(執行語意 §1 步驟 3–5 編排)
- `crates/core/src/primitives/`(步驟 2):associate/delink/insert/delete/dominate/release 建構器
  + proptest 不變量(純函數、守恆、逆元、單調、冪等)
- 依賴:smallvec、thiserror(dev:proptest、insta);CI 應掛 wasm32-unknown-unknown
- 整合測試 `tests/word_states.rs`:範例 8.1(tonogenesis)與 8.4(補償性延長)的狀態序列
  **已全為原語呼叫**(音段層改動除外,I9)

## 建置與測試

```
cargo test -p conlang-core
cargo build -p conlang-core --target wasm32-unknown-unknown   # 可移植性(I4)
```

## 新增決策(已回寫《M0 實作參照》§9)

- **I8**:支配拓撲——Syllable 與 Mora 皆直接以 Segment 為下層;音節層全覆蓋不重疊(D24),
  莫拉層部分覆蓋且允許重疊(長元音);空節點(lo==hi)為暫態病理結構,invariant 以 info 級回報。
- **I9**:六原語作用域定界——旋律 associate/delink/insert/delete + 韻律 dominate/release;
  音段層(骨架 Seg)改動不屬六原語,屬音段層規則(步驟 4+ 定案機制)。
- **I10**:步驟 2 commit 重編界定——僅 tier 內 delete → seq 收攏;dominate/release 不增刪節點;
  跨層連鎖重編(音段增刪→上層 Span+旋律 link)留步驟 5。
- **I11**:dock 浮游參考位置=原位投影——左鄰已聯結最大錨點+1 → 右鄰最小錨點−1 → seq 索引;
  各浮游者獨立求 nearest,共同著陸依 D27。
- **I12**:音段規則通道——`Action::SegRewrite`(替換,長度不變;非第七原語)+ Inventory 反查
  (無對應=error)。
- **I13**:音段刪除連鎖——`SegDelete` cascade(Span 平移、mora keep-empty、無核心音節清理、
  旋律重映射+浮游 D14、stale);I10 遞延解除。
- **P1–P4**(架構修補層,權威=`docs/架構修補01` §4):Word=臨時韻律域、Grammar Store、
  strata 層級錨定、cophonology 閂。

## 下一步:M0 步驟 6 — scan + spellout(M0 收工步)

Scan 三道鎖 + 序數 + 調簇不透明(D18);spell-out 純函數(C11)+ contour/D27 衝突檢查
+ phrase-level 掛鉤(P3);出口 = 8.6(Bantu)綠燈(見《M0 實作參照》§8)。
