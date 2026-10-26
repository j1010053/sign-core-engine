# 巢狀宣告與型別參數傳遞（I32）

狀態：已實作，2026-10-18。依使用者「先實作巢狀類型與類型參數傳遞」要求，修訂 P76 的型別引用與 marker 限制，擴充 I31 的成員可見性。舊[設計提案](../specifications/型別參數傳遞與具體化型別_v0.1_提案.md)中的其他語法不因此生效。

操作入口：[XP／NP／VP 範例](../../tutorials/nested-types/README.md)。證據：[驗證紀錄](../verification/nested-types_2026-10-18.md)。

2026-10-19 後續修訂：[I33](trait-template-consistency.md) 將作用域擴充至 Sign，接通接收者內部的模板成員與公開別名。以下作用域、身分與限制已同步；10-18 驗證紀錄仍保留當時範圍。

## 1. 宣告與作用域

`trait XP<T>:` 內可宣告 trait、marker trait、sign、paradigm。成員捕捉所有外層型別參數；內層 trait 可另外宣告參數，但不得重複或遮蔽外層參數、成員名稱。

```lang
export trait XP<T>:
    marker trait Bar:
    export sign leaf:
        belongs Bar
        syn:
            slots:
                head [T]
        phon:
            /{$slot.head}/
    syn:
        slots:
            head [Bar]
    phon:
        /{$slot.head}/
```

巢狀宣告是詞彙作用域，不是語法樹支配關係。它們存於 Sign／trait 共用的 `ScopedBody.members`，不佔 `==` 的 block 索引，canonical dump 仍保留巢狀來源。

未 export 的成員允許宣告者、模板接收者及其內層詞彙作用域引用。公開成員可使用完整實例路徑 `XP<N>.leaf`，或接收者別名 `NP.leaf`；同名成員衝突依 I33 報錯。普通呼叫不開啟私有作用域。跨套件同時套用 owner 的套件匯出規則及各層成員的 export 規則。

巢狀 sign／paradigm 捕捉外層參數，本次沒有自己的型別參數宣告。巢狀 global trait、巢狀 DSL／distribution 不合法。`__type_instance_`、`__formal_` 是編譯器保留前綴。

## 2. 型別引用與傳遞

各路徑段可帶遞迴實參：`Box<Pair<N, V>>`、`XP<N>.Bar<U>`、`P.XP<L.N>.leaf`。型別參數只在自己的宣告及後代可見；不同頂層宣告中的同名參數彼此獨立。

2026-10-25 依 [I36](named-type-arguments.md) 增加具名實參及先位置後具名的混合寫法；各段獨立按該段參數名稱綁定，整理後與位置寫法共用相同實例。

`belongs`、slot／role constraint、參數上界、分類 guard、掛載、SignApplication／autofill 使用同一個 structural TypeRef 解析與替換。`<…>` 是型別實參，`[0]` 仍是零起算的 block 掛載。

每次使用泛型都必須填滿實參，包含只寫 `belongs`、沒有顯式 mount 的情況。裸掛載可採用同一容器內明確且唯一的 `belongs` 綁定；有兩組不同綁定時拒絕猜測，必須明寫實參。索引掛載的完整 block 覆蓋規則不變。

上界可引用較早宣告的參數，如 `Pair<T, U: T>`；不能引用自身或稍後參數。依 [I35](sign-instance-types.md)，具體實參可為已知 trait 或 Sign 實例型別；上界仍是 trait，Sign 透過分類滿足上界。泛型 marker 現在合法，可用作 `X-bar<T>` 這類保留實參的分類。

構造型別採**不變性**：即使 `Child belongs N`，`Box<Child>` 也不因此成為 `Box<N>`。需要這種繼承關係時，必須在 trait 內明確宣告。`XP<N>.Bar` 與 `XP<V>.Bar` 身分不同，同一 `XP<N>` 中的成員共用同一 Bar 家族。

## 3. 編譯與執行

管線為 source → lexical binding → package binding → concrete specialization → 原有內容展開／ontology／驗證 → construction／derive／phon。未使用的泛型模板也以獨立的形式參數環境檢查，形式節點不進 runtime 語言。

引用 owner 會啟用其捕捉參數、但沒有自有參數的成員；引用公開成員也會啟用 owner。同一具體引用去重，遞迴槽位可回到同一型別；持續增長的實參循環會拒絕。實作另設深度 64、宣告實例總量 16384 的編譯上限。

完整入口 `check_language`、`compile_system`／`compile_with_packages` 與低階 `compile_with` 都會處理綁定與具體化。`expand_traits` 仍是個別 macro pass，不是完整的詞彙／套件解析入口。

執行時仍使用普通 SignApplication、填槽、DerivedToken 與既有規則／音韻管線，沒有新增 builder 或求值語言。本階段使用過 `conlang-compile/6`；後續型別契約依 [I35](sign-instance-types.md)，具名型別實參及現行快取版本依 [I36](named-type-arguments.md)。

## 4. 來源身分、修改與持久化

來源宣告只存在一次，巢狀 NodeId 的 parent 是宣告它的 trait 或 Sign。具體化不分配新的歷史來源節點；編譯 SignId 使用 `Instantiation { source, arguments }`，`CompiledSystem::source_sign_id` 可對回來源 SignId。分類與執行可分辨不同實例，來源 diff 不把實例算成新詞。

`.chg` 選擇來源時使用未實例化的詞彙路徑，如 `sign("XP.leaf")`。巢狀插入、刪除、更名、reconstruction、replay 會維持來源身分；型別參數與同名全域 trait 的引用分開處理。canonical 排序會同步更新地址。merge 仍以完整 owner 宣告為衝突單位，未承諾同一 owner 下不同成員可自動合併。

identity schema 仍為 v2；I32 引入可選的 `reference_semantics: 2`，標記型別實參、上界、詞彙 owner 的引用綁定。無此欄的舊 v2 使用凍結的舊 binder 嚴格比對，載入及重存保留既有 manifest 位元組與雜湊；需要新綁定的編輯會明確升級欄位，不重配 NodeId。沒有語意差異的平坦文件仍沿用原格式。I33 另外以 `reference_semantics: 3` 保存 Sign owner、接收者別名及新引用綁定，並保留舊版 2 的凍結 binder。[I35](sign-instance-types.md) 再以 `reference_semantics: 4` 接通 Sign 型別及規則 guard 引用，保留版本 3 的相容 binder。未知版本、損壞的引用與來源雜湊不符都會拒絕。舊版引擎不承諾讀取此新增欄位。

這是來源／引用格式的相容讀取，不會以舊編譯器語意執行舊快取。舊來源若省略泛型必要實參，現在必須補齊才可重新編譯。

## 5. 範圍與限制

本次只擴充共時型別與來源操作。歷時單位仍是 Primitive Edit／ChangeSet；時間切片、隨機抽樣、人口與接觸機制沒有新增模型。

尚未實作：builder／呼叫參數開啟私有作用域、一般型別推導、變異註記、sign／paradigm 自有泛型參數、執行時為從未在來源出現的實參即時產生新家族。不同功能不得以本次完成視為已裁定。
