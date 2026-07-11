# conlang-engine

> **開發者(含 Claude Code)請先讀 `CLAUDE.md`**:專案指引、設計不變式、決策制度、當前任務都在那裡。
> 規範文件依閱讀順序放在 `docs/01`–`10`(01–05 規範/實作層、06–10 設計層);`docs/archive/` 為歷史檔勿引用。

Autosegmental 音變引擎(M0)。規範上游:《M0 實作參照 v1.0》→《執行語意規格 v0.1》→《語法規格 v0.3》。

## 目前進度:M0 步驟 1 — `repr` 表徵模組

- `crates/core/src/repr/`:intern(SymId/ValId)、feature(FeatBits/Registry,含 [αF] 遮罩)、
  prosody(Level/Span/AnchorRef/StaleFlags,I8 拓撲)、melody(Autoseg/MelodyTier/policies)、
  word(Word 快照)、invariant(NCC/覆蓋/包含檢查,分級回報)、notation(H~μ0 / (H)@3 渲染)
- 零外部依賴(smallvec/thiserror 於步驟 2 引入);`cargo test` 應可離線直跑
- 整合測試 `tests/word_states.rs`:範例 8.1(tonogenesis)與 8.4(補償性延長)的表徵狀態序列

## 建置與測試

```
cargo test -p conlang-core
```

注意:本程式碼在無 Rust 工具鏈的沙箱中撰寫——演算法核心(NCC、覆蓋)已以 Python 鏡像驗證,
Rust 語法為桌面檢查。首次 `cargo test` 若有編譯錯誤屬預期內小修。

## 決策 I8(本步驟新增,已回寫《M0 實作參照》)

支配拓撲:Syllable 與 Mora 皆直接以 Segment 為下層;音節層全覆蓋不重疊(D24),
莫拉層部分覆蓋(onset 不入莫拉)且允許重疊(長元音=兩莫拉共享音段);
空節點(lo==hi)為暫態病理結構,invariant 以 info 級回報。
