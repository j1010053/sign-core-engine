# 具名型別實參

[phrases.lang](phrases.lang) 使用 `XP<Head, Spec, Comp>`，並把型別傳入內層 `bar-comp`。
從 repository 根目錄執行：

```sh
cargo test -p conlang-language --test named_type_arguments reordered_named_and_mixed_runtime_references_share_nested_instance_identity --locked --offline
```

此測試載入範例並檢查推導結果、等價拼寫及成員身分；預期表層為 `a u b`。這是型別與推導的診斷材料；`a`、`u`、`b` 都宣告為 vowel 以通過最小音韻管線，不是英語語料。

```lang
export sign TP:
    belongs XP<TenseHead, Comp=VP, Spec=DP>
    XP[0]
```

也可以寫 `XP<Head=TenseHead, Spec=DP, Comp=VP>` 或 `XP<TenseHead, DP, VP>`。
名稱取自模板的型別參數宣告，區分大小寫。若模板宣告為 `XP<H,S,C>`，對應寫法是 `XP<H=TenseHead, S=DP, C=VP>`。

`XP[0]` 會沿用同一容器 `belongs` 的完整綁定。也可直接指定：

```lang
XP<Comp=VP, Spec=DP, Head=TenseHead>[0]
```

巢狀成員可以使用具名型別路徑，或透過接收者別名呼叫：

```lang
XP<Comp=VP, Head=TenseHead, Spec=DP>.bar-comp(head: tense(), comp: VP(head: verb()))
TP.bar-comp(head: tense(), comp: VP(head: verb()))
```

`Comp=VP` 綁定型別，`comp: VP(head: verb())` 填入值。具名型別實參可任意排序，與位置實參混用時須先位置、後具名；值呼叫仍沿用原有 SignApplication 規則。
同一組型別綁定共用巢狀成員身分。`[VP]` 會接受 VP 的應用結果，其他 Sign 即使發音相同也不因此符合。

可手動改成下列寫法檢查診斷：

| 寫法 | 結果 |
|---|---|
| `XP<Head=TenseHead, Spec=DP, Comp=VP>` | 成功 |
| `XP<TenseHead, Comp=VP, Spec=DP>` | 成功 |
| `XP<Comp=VP>` | 缺少 `Head`、`Spec` |
| `XP<TenseHead, Head=TenseHead, Spec=DP, Comp=VP>` | 重複指定 `Head` |
| `XP<Head=TenseHead, DP, Comp=VP>` | 具名後出現位置實參 |
| `XP<head=TenseHead, Spec=DP, Comp=VP>` | 未知參數 `head` |

可選 slot 的 `?` 不會讓其型別參數變成可省略。沒有新增預設型別實參、型別推導、Sign 內容掛載或執行時新家族。
完整規則見 [I36](../../docs/implementation/named-type-arguments.md)，實例型別見 [I35](../../docs/implementation/sign-instance-types.md)。
