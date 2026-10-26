# 巢狀宣告與型別參數傳遞驗證

> 本報告保留當日工作樹的驗證結果；僅提交 I31–I36 的隔離版本另見[2026-10-26 提交驗證](discussion-scope_2026-10-26.md)。

日期：2026-10-18。環境：Windows／PowerShell，既有離線 Cargo／npm 依賴。本次驗證工作目錄含先前未提交內容；未排除 desktop crate，未修改相鄰 `lang-preview`。

契約與限制集中於 [I32](../implementation/nested-types-and-specialization.md)。可操作的來源見 [phrases.lang](../../tutorials/nested-types/phrases.lang)。

## 引擎輪廓與範圍

| 面向 | 本次採用的契約 |
|---|---|
| 表徵 | Language／TraitDef.members 為來源樹，ontology 與 effective Sign 為編譯產物，DerivedToken 為推導結果 |
| 修改單位 | Primitive Edit／ChangeSet 修改來源 NodeId；具體化不新增歷史來源節點 |
| 作用域 | 套件 export、trait 詞彙作用域、外層捕捉與內層自有參數 |
| 執行／衝突 | 決定性的名稱綁定與具體化；不變型別比對、明確實參、歧義／上界／循環拒絕；沿用原有 derive 與 phon 規則 |
| 時間與隨機性 | 編譯期實例建立與推導步驟分開；不新增歷時時間軸、人口、接觸或 stochastic 機制 |
| 身分與 trace | source NodeId 保留；runtime Instantiation 可分族並映回來源；規則保留原有來源位置 |
| 輸入／持久化 | `.lang`／套件 → parser；canonical source／identity sidecar v2 → ChangeSet replay／GraphStore 重開 |
| 公共入口 | check／compile、derive／evaluate_application、候選名稱、CLI preview；VS Code TextMate 語法 |
| 已保留與擴充 | I19／I24 的 belongs／block 掛載分工保留；本次新增 nested capture 與 constructed types；僅編譯 IR 展平，來源 ownership 不展平 |
| 延後 | builder、`NP.member` 轉送、呼叫開啟私有作用域、執行時新型別家族；merge 仍採整個 trait 衝突粒度 |

## 需求至證據

| 需求／最小反例 | 實作 | 可觀察斷言與測試 |
|---|---|---|
| 巢狀宣告與 block 分開 | parser／printer／TraitDef.members | [language nested_types](../../crates/language/tests/nested_types.rs)：XP 仍一個 block，成員有獨立來源 parent，dump→parse 相同 |
| 遞迴型別與多層捕捉 | [TypeRef](../../crates/language/src/type_ref.rs)、[specialization](../../crates/language/src/specialization.rs) | `Outer<Box<N>>.Inner<N>`、`Pair<N,T>` 不捕獲同名全域 T；明確 mount 實參經 dump 保留 |
| 參數真的到達葉節點 | specialization／ontology／construction | leaf→bar-adjunct→NP 完整 derive 表層 `a`；同 token 放入 VP 失敗；leaf head 放入 V 失敗；N/V 實例 ID 不同但 source ID 相同 |
| 私有與套件作用域 | [package_scope](../../crates/language/src/package_scope.rs)、specialization | 兩套件 `P.XP<L.N>.leaf` 推導成功；私有 Hidden／inner 拒絕外部使用；公開 paradigm 可在詞彙作用域使用私有 sign |
| 泛型 guard 不抹除實參 | 共享型別 visitor／runtime | Box<N> 與 Box<V> 表層同為 `a`，但 selected feature 分別 yes／no；case belongs 實參保留 |
| 不完整／歧義／上界／循環 | formal validation＋concrete validation | 少／多實參、未知實參、重複／遮蔽參數、自身／前向 bound、違反 constructed bound、增長循環、歧義裸 mount、缺 block 全拒絕 |
| 來源編輯與歷史身分 | [identity](../../crates/language/src/identity.rs)、changeset edit／reconstruct／diff | [changeset nested_types](../../crates/changeset/tests/nested_types.rs)：來源 path selector、更名、插入／刪除、reconstruct、replay、重開穩定；來源 diff 不重複計數；新增參數不保留同名全域錯誤綁定 |
| 舊 sidecar 相容而不容忍損壞 | reference_semantics 版本與凍結 binder | [identity_sidecar](../../crates/language/tests/identity_sidecar.rs)：舊泛型／同名參數綁定的 canonical manifest 位元組不變，編輯升級時 NodeId 不變；損壞與未知版本拒絕 |
| 實際儲存／重開 | GraphStore | [store_roundtrip](../../crates/persistence/tests/store_roundtrip.rs)：source／sidecar 相同，重開後 compile→derive `a`，實例來源 ID 仍等於存檔的 leaf ID |
| 使用者實際操作 | CLI／preview | preview（當時本機工作樹，未包含於本次範圍提交）：讀取教學檔推導 `ba`；公開成員 `a`；錯誤 head 非零 exit；改來源後得到 `bu`。涵蓋巢狀 private Bar、slot／role 捕捉與普通巢狀呼叫 |
| 編輯器 | TextMate grammar | grammar.test.cjs（當時本機工作樹，未包含於本次範圍提交）：巢狀宣告、遞迴實參、限定成員名稱；教學 corpus tokenization |

## 執行結果

從 repository 根目錄執行：

```powershell
cargo test --workspace --locked --offline --no-fail-fast
```

結果：**1,342 passed，0 failed，1 ignored，151 個 test／doc-test targets**。忽略項為原有 `crates/app/src/ipc.rs` 第 13 行 doc-test。包含 language、changeset、CLI、persistence、query、app、desktop 與其餘 workspace crates；未代跑獨立 Tshiatūn workspace、前端 GUI E2E 或跨平台驗收。

```powershell
cd editors/vscode
npm test
```

結果：**12 passed，0 failed**。只驗語法 tokenization，不等同 VS Code 擴充功能的 GUI 安裝驗收。

本次異動的 Rust 檔案以 `rustfmt --edition 2021 --config skip_children=true` 格式化；完整 suite 已涵蓋先前分別執行的 nested_types、P76、package_exports、identity、diff、reconstruct、tutorial、update_fields 與 CLI 測試。

## 有理由的既有測試調整

1. `roundtrip__ast_debug.snap` 只新增 `TraitDef.members` 的空 Language 欄位。這反映巢狀宣告明確獨立存放，不改 fixture 的語義或 canonical 來源。
2. 原有部分 `.chg` fixture 把已被使用的零參數 trait 改為有參數，卻保留缺少實參的 belongs。完整 arity 檢查現在會拒絕此狀態。header update／reconstruct 教學改用未被引用的 Schema／Agreement；diff reach 測試改為有效泛型的 bound 變更，保留原本的斷言目的。
3. P76 的 generic marker 由負例改為正例；未知具體實參在型別綁定階段提早報錯。這兩項都是 I32 的明確語義修訂。

編譯 cache version 為 `conlang-compile/6`。舊來源讀取相容性與舊編譯語義分開；未宣稱可執行省略必要實參的舊來源，也未宣稱舊引擎可讀新增的 reference_semantics 欄位。
