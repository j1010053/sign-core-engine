# Standard English grammar reconstruction

這組材料證明庫內的 Standard English 語法範疇可以經由持久 `.chg`
重建，而不只是在 Rust helper 內套用原語。

| 檔案 | 角色 |
|---|---|
| `base.lang` | 最小復原基底；只含 schema，canonical Language 為空 |
| `restore.chg` | 單一原子 statement，插入 17 個 Symbol、1 個 Class、37 個 trait 與 30 個 sign |
| `../../crates/language/lib/natural/en-standard/code/grammar.lang` | 目標 Standard English 快照 |

`restore.chg` 包含 base source、identity manifest 與四個 stdlib package lock
的 digest。任一基底或套件內容不符時，resolve 會在 replay 前拒絕。

## digest **不得手改**

`restore.chg` 的 prelude 含三道 digest(base source、identity manifest、
四個 stdlib package 的內容 lock),它們全是**衍生值**。std 套件內容一變,
這份 `.chg` 就真的不該再被 replay——鎖是名實相符的(`grammar.lang` 確實
`belongs Adposition`(`std:core`)、`belongs AttributionConstruction`(`std:cxg`))。

**要更新只能重生:**

```sh
cargo run -p conlang-changeset --example bless_en_standard_restore
```

工具會逐行印出改了什麼,並區分「只有 prelude 的衍生值變動」與「statements
真的變了」——後者代表 `reconstruct` 的產物變了,那不是例行漂移,要先確認。

> **誌誤(2026-08-07)**:有人把兩行 sha256 從引擎算得出來的值手改成算不出來的
> 值,這份教學材料因此一度變成 `resolve` 不過的死檔。當時測試把 prelude 與
> statements 綁在同一條 `assert_eq!` 裡,2 行的差異被呈現成 50 KB 的字串 diff,
> 看不出重點。現已拆開:statements 逐字元比,prelude 交給 `resolve` 驗。

驗證入口：

```powershell
cargo test -p conlang-changeset --test english_grammar_reconstruct
```

測試執行完整的 `base.lang → parse .chg → resolve → replay → canonical
Language source`，並驗證 `.chg` dump round-trip、決定性 identity 配號、基底 identity
保留，以及新節點歸屬 `evo:en-standard-restore`。原始 `grammar.lang` 的註解與空白
不屬於 Language 狀態，因此不會由 `.chg` 復原。
