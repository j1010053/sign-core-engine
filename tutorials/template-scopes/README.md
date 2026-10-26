# Sign 與模板的成員作用域

[example.lang](example.lang) 同時示範直接 Sign 與泛型 trait 的巢狀宣告。

```powershell
cargo test -p conlang-language --test template_contract tutorial_file_keeps_receiver_aliases_and_private_members --locked --offline
```

測試載入此檔，分別推導 `NP`、填入 noun 的 `NP.head` 與 `Container`，檢查 `a`、`a`、`ba`。將 noun 的 `/a/` 改為 `/u/`，前兩個推導都變為 `u`。

`NP` 套用 `Phrase<N>`，可在自己的 body 直接使用 `private-head`。外部呼叫 `NP.private-head` 會失敗；`NP.head` 因成員宣告有 export 而可呼叫，且與 `Phrase<N>.head` 指向同一具體成員。填入 `head=verb` 會因型別不符而失敗。

`Container` 直接宣告私有 `inner`，body 使用 `inner()`。宣告成員本身不等於填槽；範例中的 `map body autofill` 才是使用成員的地方。

相同作用域內的同名成員一律報錯，包含兩個模板的同名成員，以及本地成員與模板成員撞名。暫不支援覆寫。

歷時修改仍選來源，例如 `sign("Phrase.head")` 或 `sign("Container.inner")`；`NP.head` 是引用別名，沒有另一份來源節點。

完整語義與維護約束見 [I33](../../docs/implementation/trait-template-consistency.md)。
