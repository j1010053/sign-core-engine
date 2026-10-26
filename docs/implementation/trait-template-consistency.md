# Sign／trait 共用內容與模板作用域（I33）

日期：2026-10-19。依使用者授權實作；同名成員採「報錯、暫不支援覆寫」的明確裁定。本文件承接 [Trait 機制](../architecture/架構修補02_Trait機制_v0.1.md)的 Sign 內容模板原則、I22 的共用 body 語法及 [I32](nested-types-and-specialization.md)。驗收結果見[本次紀錄](../verification/template-consistency_2026-10-19.md)。

## 1. 語言契約

trait 的內容能力來自 Sign 的內容模型；trait 另外提供型別參數、分塊、分類與重用。Sign 與 trait 都可擁有巢狀 trait、sign、paradigm；Sign／paradigm 仍沒有自己的泛型參數宣告。成員捕捉所有外層參數，巢狀宣告不佔 macro block 索引，也不自動建立語法成分。

`belongs Template<N>`、完整 mount 與完整的 block-index mount 都讓接收者及其內層作用域能使用模板成員；同一實例掛載多個 block 只引入一組成員。內部可直接寫 `helper()`；外部需使用 `Receiver.helper()`，且每層成員都必須 export。普通呼叫的引數仍在呼叫者作用域解析；`Receiver(head: Receiver.private())` 不取得私有權限。跨套件另檢查最外層宣告的 export。

公開的接收者成員路徑是別名。例如 `NP.head` 與 `Phrase<N>.head` 指向同一個具體成員；兩個接收者套用同一實參時共享該模板家族。`Phrase<N>` 與 `Phrase<V>` 仍是不同家族，不因接收者別名而混同。

模板內已綁定的私有 helper 不受使用處的同名全域宣告影響。兩個模板提供同名成員，或接收者自己的成員與模板成員同名，回報 `TEMPLATE_MEMBER_CONFLICT`；不依遍歷順序覆寫。多層成員引用與繼承循環須有界處理。

`belongs` 的分類身分及既有內容繼承契約保留；完整 mount／block-index mount 仍決定顯式展開的位置，indexed mount 必須涵蓋全部 block。本次不把 trait 消除成沒有分類身分的純文字巨集，也不裁定共時清冊中的其他內容通道提案。

## 2. 共用實作

`ScopedBody<C>` 統一擁有內容及 `members` 作用域。Sign 的內容是 `Block`（一串 `SignItem`），trait 的內容是 `TraitBlocks`。兩者沒有各自獨立的 members 存放契約；`member_scopes`／`members_at` 提供共同的遞迴與地址解析入口。

parser 使用同一個作用域 body 解析函式；printer 使用同一個成員輸出函式。套件綁定、具體化、型別替換與來源 rename 共用結構化引用 visitor，並對 `SignItem` 完整列舉，新增項目時須重新檢視引用語義。普通 feature 值不當作型別名稱替換。Sign 的 `origin`／`components` 引用也經同一個 visitor，可指向成員與帶實參的路徑；既有跨語言 `::` 詞源寫法保留。這些 metadata 維持既有 Sign 專用宣告位置，未開放為 trait 自身的 metadata。

`MemberScopes` 是成員組合與別名綁定的共同入口，供套件綁定、編譯、執行時既有實例查找及來源引用追蹤使用。成員由來源宣告衍生別名，不複製歷史節點。接收者私有權限同時檢查其實參環境。

顯式掛載曾使 autofill／規則內容再從 ontology 繼承一次。現在展開記錄已實體化的 trait，effective Sign 據此避免重複套用；分類邊保留。此記錄屬編譯產物的 provenance，不列入 `.lang` 來源或來源內容比較。編譯中間 AST 的 dump 是診斷視圖，不能當成完整編譯產物的持久化格式。本階段快取語意版本為 `conlang-compile/7`；目前版本依 [I36](named-type-arguments.md)。

## 3. 身分與相容性

巢狀來源 NodeId 的 parent 可以是 Sign 或 trait，地址沿用 `Signs`／`Traits`／`Paradigms` 段。來源 lookup、canonical 排序、插入、刪除、更名、diff、reconstruct、replay 及 GraphStore 讀寫共同使用這個作用域模型。

接收者別名沒有新的來源 NodeId。編譯的 SignId 仍以來源及具體實參區分；`source_sign_id` 指回原宣告。`.chg` 修改來源時使用 `sign("Template.leaf")`，不把 `sign("NP.leaf")` 當成一份可獨立修改的來源。

identity schema 維持 v2；本階段有新綁定的文件使用 `reference_semantics: 3`，涵蓋 Sign owner、接收者別名、autofill 巢狀引數及 metadata 引用。無欄位／值為 2 的舊 sidecar 使用各自凍結的 binder 嚴格校驗，開啟重存保持原 bytes 與雜湊；實際重建或修改才升級，不重配 NodeId。相容模組中的重複程式碼刻意隔離舊格式語義，不作為新增能力的另一條實作入口。

[I35](sign-instance-types.md) 增加 Sign 名義實例型別及 `reference_semantics: 4`。Sign 仍不是內容模板；slots／roles／guards 共用型別判定，語意節點額外保留產生者身分。新型別能力須遵守下列直接內容／模板等價性要求。

## 4. 持續維護的契約

在相同型別綁定、插入位置、合併條件與外部引用環境下，直接 Sign 內容與抽成 trait 再套入的內容，應有相同的內容行為。

比較內容須包含 slots／optional、features、roles、規則結果、必要錯誤、可見性及來源映射；不能只比較 surface。`belongs` 增加的分類關係要明列預期，不能為了相等而一律刪掉。不同來源的 NodeId 不要求逐值相等；巢狀名義型別須明列來源與實例對應。

維護規則：

- 新增共享內容能力時，同時核對直接 Sign、trait 套用、至少一個作用域或錯誤反例。
- 參考結果使用人工核對的直接 Sign 或獨立不變量，不用待測的展開函式產生 expected result。
- 成功案例走完整 derive；規則使用能區分執行一次／兩次的輸入。修改反例驗證失敗不改原文件。
- `template_contract` 在一般 workspace 測試路徑執行；不以 ignored 測試或接受錯誤快照充當防線。
- 修改 source／runtime 身分、綁定或編譯語義時，同步檢查儲存格式與 cache version。

既有契約測試位於 language／changeset 的 `tests/template_contract.rs`，推導與持久化使用[可操作範例](../../tutorials/template-scopes/README.md)。測試覆蓋與未執行平台只在驗收紀錄維護；這組防線不宣稱自動證明所有未來語法擴充。

builder、呼叫引數開啟私有作用域、一般型別推導、執行時新實參家族、同名覆寫及細粒度成員 merge 均未增加。
