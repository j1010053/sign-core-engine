# Step 16 收官：文件契約與驗收矩陣 v1.0

> 狀態：**2026-07-30 收官**。本檔是 Step 16 的現行實作對照；P56–P64 的架構決策
> 以《架構修補彙整 05–11》§1 為權威，《架構修補11》保留完整推導。P60/P64 亦已由
> host persistence crate 落地；本檔只以
> 可觀測測試宣告完成，並保留刻意拒絕的邊界。

## 1. 封板範圍

Step 16 接通下列路徑：

1. `LanguageDocument` 兩狀態可產生分層 diff vector。
2. EvolutionGraph 的節點保存 immutable snapshot，主幹 edge 保存 `.chg`；commit 時
   replay 一次，讀節點狀態為 O(1)，`verify` 可重算內容 id 與 replay 不變式。
3. rebase 只依 `ReplayError` 型別化變體分類，不比對錯誤訊息。
4. 多 parent 先做 3-way merge，再套主幹 changeset。`signs`／`traits`／
   `distribution` 逐項合併；`dsl_decls` 整塊比較；衝突即不建節點。
5. `.chg` prelude 可宣告 `donor <alias> <node-id>`；`DonorSpec` 注入內容並限制可見
   範圍，`adopt` 於 resolve 階段降階為自足的 Insert 原語。
6. 同一血脈的 before／after 文件可由 `reconstruct` 還原 Update／Move／Insert／Delete，
   再套回 before 得到 after。
7. `.lang` structured phon 可輸出 `.qy` `{ }` grouped block，由 Tshiatūn 解析與執行；
   phon statement update、block-level `propagate` 可被 reconstruct 往返。
8. 外部手改 `.lang` 可先呼叫 `reconcile_edited_source`：hint 優先，其次只採父範圍內
   mutual-unique 的名稱／匿名子樹／完整子樹語意指紋；無法證明時回 ambiguity，不按序數猜。
9. 同父有序序列由 deterministic LCS 還原最少保留集合，非 LCS 節點以 stable anchor
   產生 Move；Insert／Delete 混合時仍先 replay 自我驗證 canonical source。
10. persisted expression／realization 節點有 exhaustive capability map：可型別化更新者
    必須產生 Update，結構容器或廢止種類必須明確拒絕，不可靜默回空。
11. phon authoring 可用 `.phon_block:` block update 將 flat rule 顯式轉成 structured，
    並從 `.chg` source 插入 leaf／Then／Else／propagate sub-block。
12. P60/P64 由獨立 host crate 實作：snapshot 拆成 canonical global／trait／sign 與
    identity 物件，changeset 亦依內容雜湊共用；node folder 保存 manifest／edges，
    annotation／config 位於 node-v2 雜湊外。load 後必重跑 node-v2 與 replay/fsck。

Step 17 才負責 Recipe／Goal runtime；Step 16 不以 function 定義或展開接點冒充上層
runtime 已完成。

## 2. NodeId v2 契約

NodeId 的 canonical payload 為：

```text
conlang-node-v2
snapshot <sha256(canonical .lang source)>
identities <sha256(identity manifest JSON)>
parent <from NodeId> <has_changeset> <sha256(changeset or empty)>
...（依 edge 順序）
nativization <canonical value>
```

因此：

- 同原文、不同 identity namespace 會得到不同 NodeId，可作為不同 root 共存。
- 同 snapshot、不同 parent 或不同 changeset 是不同歷史節點，不會把一條歷史靜默摺掉。
- `label`、annotation 與 host config 不進內容雜湊。
- root namespace 仍須唯一；跨家族穩定 id 碰撞由 merge 分析擋下。

## 3. Reconstruct 支援矩陣

| 類別 | 現行行為 |
|---|---|
| Language 一級資料 | DSL declaration、distribution、trait／sign rename |
| Sign／trait item | belongs、trait use、def、rule／feature rule、sense／edge、slot、feature、role、slot map、constraint |
| 有序子節點 | rule then／else branch、case branch、phon statement |
| Expression／realization | Case header（selection／expected／scrutinee／name）、CaseBranch、SignApplication、projection/interpolation owner、Realization |
| Structured phon | leaf body → `RuleBranchBody`；block wrapper → `Propagate(bool)`；rule root → `PhonBlockRoot`；statement/block insert/delete/move |
| 同父重排 | items／branches／phon elements 等有序 logical sequence 以 deterministic LCS + stable anchor 還原 |
| 生滅 | 只為最上層新增／刪除各發一筆，子樹隨 payload／父節點處理 |
| capability boundary | Language immutable、Block 僅結構容器、廢止的 RealizationBranch 明確 unsupported；不回空序列 |

Flat rule → structured root 與 structured root kind 替換可由 `PhonBlockRoot` 明確表達；
structured → flat、巢狀 `PhonBlockNode` 在同一 stable id 下改變 Leaf／Then／Else 種類仍
明確拒絕。`Propagate` 是透明 wrapper，不占 address segment，切換時子 statement id 不變。
Application arguments 是 expression 的固定位置，不冒充可 Move 的序列；canonical-unordered
Sign／Trait／Distribution 與 singleton 亦不進同父 LCS。

## 4. 主引擎 ↔ Tshiatūn 契約

| 工作台 `.lang` | Tshiatūn `.qy` | 身分／相容性 |
|---|---|---|
| `PhonBlock::Leaf` | 同 leaf 的裸 statement | simultaneous |
| `Then`／`Else` 的 compound element | 行級 `{ ... }` group | 巢狀靠 braces，不靠縮排 |
| flat 單層 block | 無 braces | 舊輸出逐字相容 |
| rule／element `propagate` | 對應 header／boundary modifier | wrapper 透明，不造成 child id churn |
| 同層混寫未分組 Then／Else | `LscMixedBlock` 類錯誤 | 明確拒絕 |
| leading compound element | 開頭 `{ ... }` | 已支援 |
| leading `Propagate` element | 無可掛 modifier 的 `.qy` 位置 | 明確 `LeadingPropagateUnsupported` |

Tshiatūn 的 grouped-block parser、workbench codegen、propagate 組合均已有正反例。CLI
文件中的「括號 group 延後」只指 Lexurgy 來源括號的自動轉換，不再指 `.qy` 引擎能力。

## 5. 收官後仍明確保留的邊界

- **identity 的安全線**：`LanguageDocument::open` 仍只接受 digest 完全相符的 sidecar；
  reconciliation 是另一個顯式 API。相同 sibling 或整體內容與名稱都改變時必須給 hint，
  不以位置「猜認親」。
- **reconstruct 結構邊界**：Application arguments 是固定位置；廢止的
  `RealizationBranch` 不重新開公共能力；structured phon → flat 與巢狀 block kind
  in-place replacement 明確拒絕。
- **phon surface 邊界**：flat rule 不會因 insert 靜默 bootstrap，必須先
  `update <rule>.phon_block:`；leading `Then/Else/Propagate` root 因無合法 `.qy` 掛點而拒絕。
- **條件式 donor 集合選擇**：指名借入與清單形語法已完成；會隱藏實際借入集合的條件式
  選取不在封板內。
- **Step 17**：Recipe／Goal runtime、History 與外部服務實際執行。

## 6. 驗收矩陣

| 契約 | 正例 | 反例／近似防線 |
|---|---|---|
| state → primitives → state | `reconstruct_roundtrip` | unsupported node／phon shape 必須報錯 |
| node-v2 決定性 | `evolution_replay` | corrupt id、snapshot／identity mismatch |
| typed rebase | `evolution_replay` | conflict／broken input／environment 分型 |
| 全 parent merge | `merge_plan` + EvolutionGraph commit | LCA 不唯一、雙邊異值、id／name collision |
| donor／adopt | `donor_declaration`、`atomic_rewrite*` | 未宣告、未 materialize、越界 donor |
| identity reconciliation | `identity_reconcile` | identical siblings／全改名內容須 hint；exact open 不放寬 |
| expression + reorder reconstruct | `reconstruct_roundtrip` | capability unsupported／fixed-position arguments |
| phon authoring | `phon_authoring` | flat insert 不偷 bootstrap、leading boundary root 拒絕 |
| structured phon | `phon_grouped_codegen`、`phon_propagate*`、Tshiatūn `block_ir` | 未閉 brace、同層 mixed block、leading propagate |
| P60/P64 persistence | `conlang-persistence/store_roundtrip` | object corruption、immutable manifest、path traversal；config 不得改 state |
| workspace 相容性 | `cargo test --workspace` | formatter／clippy／submodule targeted tests |

建議封板指令：

```powershell
cargo fmt --all -- --check
cargo test -p conlang-changeset --test reconstruct_roundtrip
cargo test -p conlang-changeset --test identity_reconcile
cargo test -p conlang-changeset --test phon_authoring
cargo test -p conlang-persistence
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --manifest-path tshiatun/Cargo.toml -p tshiatun-dsl --test block_ir
```

若 Windows 預設 target 缺 MSVC linker，應改用已安裝的
`stable-x86_64-pc-windows-gnu` 與專案的 `rust-lld`／`dlltool` 配置；這是工具鏈前置，
不可把 `link.exe not found` 記成語意測試失敗。
