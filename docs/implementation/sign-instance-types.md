# Sign 實例型別（I35）

日期：2026-10-23。依使用者「開始實作實例型別」授權，擴充 [I32](nested-types-and-specialization.md) 的實參種類，保留 [I33](trait-template-consistency.md) 的模板哲學及 [I34](case-evaluation-unification.md) 的求值與 base 政策。操作見[範例](../../tutorials/instance-types/README.md)，測試與平台範圍見[驗證紀錄](../verification/instance-types_2026-10-23.md)。

## 語言契約

型別位置接受 trait 分類或 Sign 的名義實例型別。`comp [VP]` 若解析到 `sign VP`，表示 filler 的產生者必須是該具體 Sign。`VP(head: verb())`、同一 Sign 的其他參數組合，以及該 Sign 的未飽和應用都保留此身分；填槽是否完整另由既有飽和度檢查處理。無槽的詞彙 Sign 也可作實例型別。

```lang
export sign TP:
    belongs XP<TenseHead, DP, VP>
    XP[0]
```

此處 XP 的三個位置實參可以分別是 trait、Sign、Sign。實參沿原有結構化 TypeRef 替換至內層的 slots、roles、guards 及其他型別位置。`<…>` 傳型別，`(…)` 傳 Sign 應用的值；兩者不混用。

| 位置 | 接受內容 |
|---|---|
| slot／role `[Type]`、分類 guard | trait 分類或 Sign 實例型別 |
| `Template<Type>` 的實參 | 已知 trait、Sign 或作用域中的型別參數 |
| `belongs`、完整／block mount | trait 模板；即使經參數替換，也不能掛載 Sign |
| 參數上界 `C: Verbal` | 仍為 trait；Sign 實參可用既有分類滿足上界 |
| `VP(...)` | Sign／Paradigm 的既有應用規則；trait 不可呼叫 |

Paradigm 不是實例型別。它選出的 Sign 值保留實際成員身分；`Choose()` 選出 `VP(...)` 後能符合 `[VP]`，但不能寫 `[Choose]`。這不增加 Paradigm 的分類 selector 語法。

名義型別不以名稱字串、表層或結構相似判定。另一個同樣屬於 Verbal、同樣輸出 `/a/` 的 Sign 仍不能填 `[VP]`。外層 `Wrapper(inner: VP(...))` 的型別是 Wrapper；內層 VP 身分保留在 filler／語意角色中。不新增不同 Sign 之間的繼承關係。

## 共用實作與可見性

引用 visitor 區別 `Type` 與指定 declaration kind；套件綁定、具體化及 rename 共用此分類。trait／Sign 名稱在型別位置有歧義時報錯；跨套件以限定名稱分辨，公開符號的內部鍵也必須分開。

`OntologyRegistry` 保存從編譯來源推得的 Sign 名稱 → 身分／分類索引；Sign 不加入 trait 繼承樹。slots、roles、autofill 驗證、guards 與候選搜尋共用 `type_satisfies`。它在 trait 約束下檢查分類，在 Sign 約束下比較具體 `SignId`。`type_is_within` 用於上界及 slot 收窄，不複製 Sign 內容。

export、套件及詞彙作用域規則維持有效。合法傳入的 private 型別可作為已捕捉的實參繼續使用，這不授權存取該型別的其他私有成員。外部引用仍檢查每個實參，不能藉已產生的 `Box<Owner.Hidden>` 實例繞過 private 限制。接收者別名與完整成員引用作為實參時，先正規化到同一具體型別。

`DerivedToken.construction_id` 是 token 產生者的權威。填槽時據此建立語意身分快照；`SemNode.construction_id` 讓角色及巢狀 filler 保留該身分。共用 body、trait 內容套用、同名成員報錯政策不變。

## 身分、相容性與歷時操作

來源 `NodeId` 與編譯的實例 `SignId` 分開。巢狀實例沿用 `Instantiation { source, arguments }`，其中 arguments 為正規化的具體引用；`source_sign_id` 指回來源。改名保留來源 NodeId 並更新所有引用；重新編譯可能改變含名稱的 specialization 身分，不承諾舊編譯 token 跨修改後的系統直接續用。相同來源與 sidecar 重開後，則須得到相同 token 身分與語意。

source identity schema 保持 v2；需要新綁定時使用 `reference_semantics: 4`。新增 Sign 型別引用、型別位置的 `ExternalType`、規則 guard 及其遞迴實參追蹤，避免把同名全域 Sign 當成捕捉參數。無欄位及版本 2／3 均依相容 binder 校驗並原樣保存 bytes；實際編輯才升級。改名後的 reconstruction 先套用會連動引用的宣告更名，再比較其餘欄位，避免產生指向未改名型別的中間編輯。

`conlang.semantic/v1` 增加可選的 `construction_id`，遞迴保存角色的實例身分。舊 JSON 缺少此欄位仍可載入、按 trait schema 驗證，但不能宣稱符合 Sign 名義型別。提供身分時必須對應當前編譯系統中的 Sign，並用該 Sign 的語意 schema 驗證；來源顯示名稱不是身分依據。舊版引擎不保證接受此新欄位。要跨重開保留身分，使用來源與 identity sidecar 配對，不能只 dump 後重新 import 裸來源。

本階段編譯快取語意版本為 `conlang-compile/9`，後續版本依 [I36](named-type-arguments.md)。此變更是共時型別擴充與來源操作接線；沒有新增歷時原語、時間軸或隨機機制。

## 保留範圍

具名型別實參已由 [I36](named-type-arguments.md) 接通；模板宣告為 `XP<Head, Spec, Comp>` 時，可寫 `XP<Head=TenseHead, Spec=DP, Comp=VP>`，與位置寫法等價。
尚未實作預設型別實參、型別推導、Sign／Paradigm 自有泛型、執行時新實參家族，以及以 Sign 作內容模板或上界。
