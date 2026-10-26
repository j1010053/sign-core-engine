# 用 Sign 實例型別指定 DP／VP

[phrases.lang](phrases.lang) 是可直接執行的完整範例。`a / u / b` 是用來辨認推導的測試音形，不是英語的音韻分析。

```lang
export sign TP:
    belongs XP<TenseHead, DP, VP>
    XP[0]
```

XP 的宣告是 `trait XP<H, S, C>`。這裡 `H` 綁到 trait `TenseHead`，`S` 綁到 Sign `DP`，`C` 綁到 Sign `VP`；內層 `bar-comp` 的 `comp [C]?` 因而成為 `comp [VP]?`。

填入時仍使用現有的 SignApplication：

```lang
map phrase autofill TP(spec: DP(), head: TP.bar-comp(head: tense(), comp: VP(head: verb())))
```

`VP(head: verb())` 推導出的值具有 VP 的實例型別，因此符合 `[VP]`。另一個同樣屬於 V 或輸出相同字串的 Sign 不會因此符合它。若希望接受整個分類，slot 應寫 trait 名稱，例如 `[Verbal]`，並讓可接受的 Sign `belongs Verbal`。

從 repository 根目錄執行：

```sh
cargo test -p conlang-language --test instance_types three_parameter_xp_tutorial_derives_applied_vp_and_nullary_dp --locked --offline
```

此測試載入範例，檢查表層 `a u b` 及型別不符的反例。把 `comp: VP(head: verb())` 改成 `comp: DP()` 會失敗，因為 DP 的身分不符合 VP。

`TP.bar-comp` 與 `XP<TenseHead, DP, VP>.bar-comp` 指向同一成員。private 的 Bar 仍只在模板及接收者內使用。套件中的公開 Sign 可寫成 `English.VP`，也可作為 `XP<TenseHead, DP, English.VP>` 的實參。

`belongs VP` 不會套用 VP 的內容；模板仍由 trait 宣告。本例的 `XP<H,S,C>` 也可寫成 `XP<H=TenseHead, S=DP, C=VP>`；具名寫法見[下一篇](../named-type-arguments/README.md)。完整型別、可見性與保存規則見 [I35](../../docs/implementation/sign-instance-types.md)。
