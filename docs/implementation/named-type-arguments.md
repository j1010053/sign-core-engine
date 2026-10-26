# 具名型別實參（I36）

日期：2026-10-25。依使用者確認的行為及實作授權，承接 [I32](nested-types-and-specialization.md)、[I33](trait-template-consistency.md) 與 [I35](sign-instance-types.md)。

## 契約

`trait XP<Head, Spec, Comp>` 的 `XP<TenseHead, DP, VP>`、
`XP<Head=TenseHead, Spec=DP, Comp=VP>`、`XP<Comp=VP, Head=TenseHead, Spec=DP>`
及 `XP<TenseHead, Comp=VP, Spec=DP>` 綁定相同的參數，並共用具體型別、巢狀成員及編譯 SignId。
位置實參須在具名實參之前。名稱區分大小寫，只能是該段宣告的型別參數名稱；slot 名稱不自動成為參數名稱。

每個必要參數須恰好指定一次。未知名稱、重複指定（含位置與具名重複）、缺漏、過多實參及具名後接位置實參皆報錯。
具名語法不新增預設值、部分特化或推導；slot 的 `?` 不允許省略型別實參。

每一段 `<...>` 都獨立綁定該段的參數。實參值可為 trait、Sign 名義型別、作用域內的型別參數，或遞迴的限定型別引用。
具名參數只影響綁定，其他作用域、export/private、型別約束及不變性規則與位置寫法相同。
依宣告順序整理後才檢查 bound，因此 `Pair<T, U: T>` 的結果不受具名實參書寫順序影響。

`Comp=VP` 是編譯時型別綁定；`VP(head: verb())` 是值的套用。前者傳遞至內層 `[Comp]` 後，後者依 I35 的 producer identity 滿足 `[VP]`。
型別參數名稱因此成為公開 API；修改參數宣告不會自動猜測舊名稱與新名稱的對應。

## 實作與稽核範圍

引擎使用 `.lang` 的結構化型別引用、Sign／trait body、編譯時實例索引及具 producer identity 的 token。
共時推導是確定性的；本次不改 token rules、case、realization、音變、歷史時間或抽樣政策。
來源 NodeId 與編譯實例 SignId 分開；來源保留具名拼寫，實例鍵使用依參數宣告順序排列的具體位置實參。

驗證路徑為 parser／printer → 共用參數綁定 → 套件與成員作用域 → 特化／bound → 完整 derive → 語意及 token identity；
另檢查來源引用追蹤、改名／reconstruct／replay、GraphStore 重開及編譯 API 的診斷。
沒有新增 converter 或隨機機制。未擴充一般型別推導、預設型別實參、Sign／Paradigm 自有泛型或執行時建立新家族。

## 共用實作與保存

`TypeRef` 的每段持有 `TypeArgument { name, value }`，只有 `value` 參與型別查找、替換、改名與引用追蹤。
`TypeSegment::ordered_arguments` 是參數綁定的唯一規則，供特化、成員組合、外部既有實例查找、低階 trait 展開及 trait view 使用。
`MemberScopes` 在比較 owner、合併重複 mount 及取得接收者成員前整理實參。只有書寫順序不同的同一綁定不形成成員衝突。
每個 bound 在既有特化路徑依宣告順序處理，token 的名義型別判定沿用 I35。

role 宣告及 Sign application 的分隔符只在巢狀深度零處切分，避免把 `<Comp=VP>` 的等號當成外層賦值。
編譯 API 的 ValidationReport 保留診斷碼和訊息，包含未知名稱、重複指定及錯誤實參順序。

`.lang`／`TraitMount.args` 保留具名來源順序；等價語法可有不同來源文字與內容雜湊，但共用相同具體型別綁定。
來源 identity schema 維持 v2，引用欄位仍按來源位置追蹤實參值；本次不新增 `reference_semantics` 版本。
I35 的 v4 及舊 sidecar 相容性維持，改名只重寫型別值，與參數同名的全域宣告不因此成為引用。
Rust 結構化型別 API 的 `TypeSegment.arguments` 改為 `Vec<TypeArgument>`；來源 AST 及持久化格式不變。
編譯快取語意版本更新為 `conlang-compile/10`。

操作見[範例](../../tutorials/named-type-arguments/README.md)，測試與平台邊界見[驗證紀錄](../verification/named-type-arguments_2026-10-25.md)。
