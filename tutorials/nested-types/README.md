# 用一份 XP 模板建立 NP、VP

直接編輯 [phrases.lang](phrases.lang)。它是可執行的最小例子，`a`、`b`、`u` 是測試詞，尚未建立標準英語的詞序、補語選擇或一致規則。

`XP<T>` 內的 `Bar`、`bar-comp`、`bar-adjunct` 都捕捉外層的 `T`。所以 `belongs XP<N>` 後，`bar-comp` 的 `head [T]` 會成為 `head [N]`，其 semantic role 也接受同一類型。NP 和 VP 共用來源模板，保留不同的具體分類。

```lang
sign NP:
    belongs XP<N>
    XP[0]
```

`<N>` 是型別實參；`[0]` 是第零個內容區塊。此處 `XP[0]` 採用同一容器唯一的 `belongs XP<N>` 綁定，也可以明寫 `XP<N>[0]`。只有 `belongs` 時仍會繼承有效內容；顯式掛載用來安排 macro 內容位置。

## 親手檢查

從 repository 根目錄執行：

```powershell
cargo test -p conlang-language --test nested_types tutorial_file_derives_and_enforces_captured_types --locked --offline
```

測試載入此檔：`demo` 產生 `ba`，`XP<N>.bar-comp` 填入 noun 產生 `a`；`XP<V>.bar-comp` 填入 noun 則失敗，因為 `noun` 不屬於 `V`。把 `noun` 的 `/a/` 改成 `/u/` 再跑 `demo`，結果會變成 `bu`。

完整組合已寫在 `demo` 的 autofill 中，沿用普通 SignApplication：

```lang
NP(head: XP<N>.bar-adjunct(adjunct: adj(), head: XP<N>.bar-comp(head: noun())))
```

需要巢狀 application 時可直接寫進 `.lang`，如範例的 `map phrase autofill ...`。這些呼叫會實際組出短語樹；宣告放在 XP 裡只是決定名稱與參數的作用域。

## 哪些名稱能在外面使用

`Bar` 沒有 `export`，可在 XP、接收模板的 NP／VP 及其內層作用域引用。外部不能直接引用 `XP<N>.Bar` 或 `NP.Bar`；普通呼叫引數仍在呼叫者作用域解析。詳見[共用作用域教學](../template-scopes/README.md)。

`bar-comp`、`bar-adjunct` 是 **sign**，因為它們是可呼叫的構式；trait 本身不能當函式呼叫。兩者加上 `export`，因此可用 `XP<N>.bar-comp(...)` 或 `NP.bar-comp(...)`。若拿掉 `export`，須由 XP 或接收者的內部使用它，外部呼叫 `NP(...)` 不會自動開放私有名稱。

放入套件後，公開 owner 可加套件限定名，例如 `Phrases.XP<English.N>.bar-comp(...)`。owner 與成員都須符合各自的匯出規則。套件設定見[套件教學](../package-exports/README.md)。

`NP.bar-comp(...)` 已可作為 `XP<N>.bar-comp(...)` 的別名；builder 語法未增加。執行時能使用的是來源已啟用的具體型別家族；範例中的 `NP`、`VP` 分別啟用 `XP<N>`、`XP<V>`。

完整契約見 [I32](../../docs/implementation/nested-types-and-specialization.md)，驗證見[測試報告](../../docs/verification/nested-types_2026-10-18.md)。
