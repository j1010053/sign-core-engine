# 寫 English 套件：export 與限定引用

這個小範例展示套件語法，尚不是完整的標準英語文法。
先看 [grammar.lang](packages/natural/english/code/grammar.lang)，再看引用它的
[language.lang](language.lang)。[project.toml](project.toml) 已設定 `English` 別名。

## 1. 用 export 決定公開介面

```lang
export trait Noun:
    syn:
        feature:
            number = enum(singular, plural)
```

`export` 放在宣告最前面。也支援 `export marker trait`、`export global trait`、
`export paradigm` 與 `export sign`。同一套件的所有 code 檔案共用作用域。
沒有 `export` 的定義仍可在套件內使用。

範例公開 `Noun`、`CatForms`、`cat`，把 `cats` 留在套件內部。
所以外部可以呼叫 `English.CatForms()` 選出複數成員，但不能直接呼叫 `English.cats()`。
這樣以後更換複數的內部實作時，不必更改外部引用。

schema 2 不必另寫 `exports.tsv`。如果需要自訂穩定匯出 ID，可以提供該表；
表中的 trait／paradigm／sign 必須已在來源標為 `export`。
`function` 的匯出仍依原本的 `.chg`／export table 契約。

## 2. 設定並使用套件別名

```toml
[packages]
roots = ["natural:english@1.0.0"]

[packages.aliases]
English = "natural:english"
```

`natural:english` 是套件 ID，`English` 是這個專案用來引用它的別名。
在 `language.lang` 可以寫：

```lang
sign NP:
    syn:
        slots:
            head [English.Noun]
    phon:
        /{$slot.head}/
```

`belongs English.Noun`、`English.Noun[0]`、`English.cat()`、
`map head autofill English.CatForms()` 都使用相同規則。
泛型實參寫在 `belongs Wrapper<English.Noun>`，後續掛載仍寫 `Wrapper[0]`。
slot、feature、role 的名稱維持各自原本的語法；`.` 不代表把 feature 路徑當成套件名稱。

裸名依序找本地定義、選取套件的公開定義；同名公開定義超過一個就會報錯，需寫限定名。
未設定別名時，也可用唯一的套件 name，例如 `english.Noun`。

套件本身的相依另在 `package.toml` 指定，別名不會繼承自引用它的專案：

```toml
requires = ["catalog:categories@1.0.0"]

[aliases]
Categories = "catalog:categories"
```

接著包內即可寫 `belongs Categories.Noun`。只有自己的宣告與已宣告依賴的公開內容可用。

專案別名會進入編譯快取鍵與 `.chg` 的 `package_aliases` 鎖。更換別名指向後，
舊 ChangeSet 會拒絕直接重播；要以原本的套件與別名設定還原，或明確重新建立修改歷程。

## 3. 在專案中修改公開定義

在本地 `.lang` 宣告同一個限定名稱，即建立這個專案的覆寫：

```lang
trait English.Noun:
    syn:
        feature:
            number = enum(singular, plural, dual)
```

這是整個定義的替換，未寫出的舊內容不會自動保留。套件原檔與鎖定的 bytes 不會被修改。
`trait English.Secret:` 若指向未匯出的 trait，就會被拒絕。

本地覆寫建立後，`.chg` 可用一般 selector 修改它，例如
`update trait("English.Noun").exported = true`。`exported` 是本地宣告的可見性欄位，
不會改變原套件的匯出設定。尚未建立本地覆寫的套件節點不屬於目前文件，
不能直接用文件的 Primitive Edit 修改；可以先用 `insert into language at end:` 插入限定名稱的覆寫。

## 4. 預期結果與驗證入口

把 `English.cat` 填進 `NP.head` 得到 `kat`；推導 `plural_cat` 得到 `kats`。
直接引用 `English.cats` 失敗。套件內的私有成員仍保有自己的穩定 Sign 身分。

這些檔案由引擎整合測試實際載入，從 repository 根目錄執行：

```sh
cargo test -p conlang-language --test package_exports tutorial_package_is_executable --locked --offline
```

使用專案的套件 resolver 或以上測試驗證本範例。完整契約見 [I31](../../docs/implementation/package-exports-and-qualified-names.md)。
