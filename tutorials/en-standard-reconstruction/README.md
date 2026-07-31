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

驗證入口：

```powershell
cargo test -p conlang-changeset --test english_grammar_reconstruct
```

測試執行完整的 `base.lang → parse .chg → resolve → replay → canonical
Language source`，並驗證 `.chg` dump round-trip、決定性 identity 配號、基底 identity
保留，以及新節點歸屬 `evo:en-standard-restore`。原始 `grammar.lang` 的註解與空白
不屬於 Language 狀態，因此不會由 `.chg` 復原。
