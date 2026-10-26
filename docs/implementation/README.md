# Implementation

> **類型**：實作架構、落地路線與 authoring 介面
> **權威邊界**：本目錄說明目前如何實作規範；規範語意以 specifications 與 architecture
> 的 P 系列彙整為準，完成狀態以 verification 的可觀測證據為準。

- 05 保留編號，因其承載 I 系列實作決策。
- 其餘文件以階段或能力命名，記錄 M1+ 入口與 ChangeSet authoring 的實作結構。
- `lang_chg_weightDB_std_package_MVP_v0.1.md`：`.lang`／`.chg`／兩類 weight data 的
  std package 邊界、補齊順序與驗收矩陣。

- [套件匯出與限定名稱](package-exports-and-qualified-names.md)
- [巢狀宣告與型別參數傳遞（I32）](nested-types-and-specialization.md)
- [Sign／trait 共用內容與模板作用域（I33）](trait-template-consistency.md)
- [case 共用求值與命中後不回退（I34）](case-evaluation-unification.md)
- [Sign 實例型別（I35）](sign-instance-types.md)
- [具名型別實參（I36）](named-type-arguments.md)
