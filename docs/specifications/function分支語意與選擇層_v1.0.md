# function 分支語意與選擇層（P69–P70）

> **狀態:已定案**(擁有者 2026-08-01 裁定)。**修訂 P48**;承 P12(Goal/Recipe 皆為
> 函數,Weight DB 只存權重)、P26(逐位元可重現)、P29/P50(auto-discovery)、
> P51(逐步接力展開)、P55(`.chg` 表面收斂)、P57(typed 錯誤分型)。
> 與《修補12》(P65–P68,提案中)無交集,故編號自 **P69** 起。
>
> **本檔住 `specifications/` 而非 `architecture/`**:它規定的是**語言寫什麼、
> 怎麼跑**(body 形狀、分支條件、比對與執行的分期),屬規範性契約;
> 姊妹檔為《`case`、`when` 與 Context Fragment(V2)》,兩者對同一組關鍵字
> 分別規範共時側與歷時 function 側,**必須逐條對齊**。

---

## 0. 修正目的

`.chg` function 層有**兩處同名反義**——同一個關鍵字在 `.lang` 與 function 層意思相反。
兩者都不是實作 bug,是規格層把兩件事混為一談。

| | 關鍵字 | `.lang` 的意思 | function 層原本的意思 |
|---|---|---|---|
| ① | `when:` | 所有 Matched **依序合併**(全都生效) | 列舉候選(**全都不生效**) |
| ② | `else` | `!any_matched`(前面都不成立才補位) | 恆成立(與裸呼叫無異) |

`.qy`/`.lang`/`.chg` 是同一批人寫、常常並排閱讀的三份語言。P55 剛為了「三者註解形式不
一致」把 `.chg` 的 `#` 改成 `/* */`;同一個標準下,**同名反義比不同名更糟**——不同名
只是要多記一個詞,同名反義會讓讀者把已經理解的東西套錯。

---

## 1. `when:` 的語意被借走【P69 ①】

### 1.1 證據

`.lang`(`crates/language/src/system.rs`,`CaseSelection::Accumulate` 分支):

```rust
for branch in &case.branches {
    if !matches { continue; }
    merged_items.extend(items.iter().cloned());   // ← 每個成立的分支都生效
}
```

function 層(修正前的 `crates/changeset/src/function.rs`):

```rust
FunctionBody::When(branches) => {
    for branch in branches {
        if matched { candidates.push(...); }      // ← 收集,一個都不執行
    }
    Some(FunctionCandidates { ... })
}
```

**「取全部」相同,「然後呢」相反**:一個是全都生效,一個是全都不生效。

### 1.2 《修補10》§2 的論證跳了一步

原文的理由是:

> `when:` 對 Goal 特別貼合:Goal 的純函數半契約是 `Goal.candidates(…) → Vec<(Recipe,
> weight)>`——回傳**所有**符合的候選,不是第一個。

這只證明了「**取全部**而非取第一個」,**沒有證明「取全部之後不執行」**。論證把
*which*(選哪些)與 *then what*(選完做什麼)混為一談,而 `when` 的既有語意管的是前者、
Goal 需要的差異在後者。

### 1.3 可驗證的後果:有一格是空的

| body 形狀 | 取哪些 | 然後 |
|---|---|---|
| 純序列 | 全部(**無條件**) | 全跑 |
| `case:` | 第一個成立 | 跑它 |
| `when:`(原) | 全部成立 | 一個都不跑,交出去等選 |
| **(空缺)** | **全部成立** | **全跑** ← `.lang` 的 `when` 本義 |

序列的每一行不能帶 guard(`parse_call` 沒有 guard 位),所以**「有條件的全跑」在
function 層寫不出來**。這不是理論缺憾:「是動詞就做 A、有 telic 就做 B、兩個都成立
就都做」是語法化路徑最常見的形狀之一。

### 1.4 裁定

`when:` **回歸 `CaseSelection::Accumulate`**——所有成立的分支依序**執行**。
細節逐項照抄 `.lang`,不自創:

- **一個分支都不成立 ⇒ 無操作、不報錯**,對應 `.lang` 的 `!any_matched => return
  Ok(current)`。與 `case:` 的 `CaseNoBranch` 硬錯**刻意不同**:`case:` 的契約是
  「選一個」,選不出來是作者漏了兜底;`when:` 的契約是「成立的都做」,一個都不成立
  就是「這次沒事要做」,那是合法結果。

---

## 2. `choose:` 承接候選列舉【P69 ②】

候選列舉獨立為第四種 body 形狀 `choose:`(擁有者選定的關鍵字)。

### 2.1 這不違反 P48「不新增 layer 標記」

P48 反對的是 `@layer goal` 這類**宣告式標記**(原文:「日後若需顯式層標記,`@layer`
為選填、預設 recipe」)。**body 形狀本來就是語意載體**——P48 自己就是這樣安排的。
差別在於:

- **宣告**可以與行為不符(寫 `@layer goal` 卻放一個序列 body);
- **結構**不可能與行為不符(寫了 `choose:`,它就是列舉)。

所以補一種 body 形狀是在 P48 的機制**之內**修正它選錯的關鍵字,不是推翻它。

### 2.2 引擎一律以靜態形狀判定

引擎有三個地方會遇到「這個呼叫需要選擇」,修正後**一律查靜態 body 形狀**:

| 位置 | 判定 |
|---|---|
| `.chg` 語句層 | `functions.get(name)?.body` 是不是 `Choose` |
| 序列/分支內的一行 | `entry.definition.body` 是不是 `Choose` |
| `choose:` 的候選層級檢查 | 候選自己是不是 `Choose`(候選不得又是候選函數) |

**先求值再看結果是錯的**:`choose:` 出現在不該出現的位置是**與語言狀態無關**的事實,
而求值會先炸在 guard 或參數約束上——那些錯依 P57 分類為 `Conflict`,於是 rebase 會叫
使用者去解一個「換什麼 base 都解不掉」的衝突。實測:

```
.chg 寫 Future(sign("stone"))       ← stone 是 Noun,而 Future(target [Verb])
  舊: ConstraintUnsatisfied      → Conflict → 「去解衝突」
  新: CandidatesRequireSelection → Broken   → 「這行不能寫在 .chg 裡」
```

連帶:`CandidatesRequireSelection` 的 `candidates: usize` 欄位**刪除**。那個數字對
`.chg` 作者毫無用處(不管幾個候選,這行都得刪掉),卻**正是它逼出了「先求值」的順序**
——不求值就數不出來。

---

## 3. `else` = `!any_matched`,與裸呼叫分開【P69 ③】

### 3.1 原本 `else` 只是裝飾

parser 分得出 `else X` 與裸 `X`,但兩者都記成 `guard: None` 且**都恆成立**。
在 `case:`(取第一個)下兩種讀法結果相同,所以一直沒有顯形;在 `when:`/`choose:`
(全取)下**不同**。

`.lang` 是分選擇模式處理的(`system.rs`):

| 選擇模式 | `Else` 的處理 |
|---|---|
| `FirstMatch`(`case`) | `Ok(true)` |
| `Accumulate`(`when`) | `!any_matched` |

### 3.2 裁定

分支條件由 `Option<String>` 改為三選一 `BranchCondition`:

| 寫法 | 條件 | 對應 `.lang` |
|---|---|---|
| `<call> / <guard>` | `Guard(g)` | `CaseCondition::Guard` |
| `else <call>` | `Else` → **`!any_matched`** | `CaseCondition::Else` |
| 裸 `<call>` | `Always` → 恆成立,且**計入 `any_matched`** | (無,見 §3.3) |

`case:` 的行為**零改變**:取第一個成立者,走到 `else` 時前面必然都沒成立,`!any_matched`
恆為真,與 `.lang` 的 `Else => Ok(true)` 自然一致。實際改掉的只有 `when:`/`choose:`。

### 3.3 誠實標記:`Always` 是推導,不是規格

`.lang` 沒有「裸分支」——它的 `CaseCondition` 只有 `Equals`/`Guard`/`Else`。
function 層保留裸呼叫的理由是**結構差異而非語意分歧**:

- `.lang` 的 `when:` 區塊住在更大的 sign body 裡,無條件的項目寫在**區塊外**就好;
- function 的 body **整個就是一個區塊**(`parse_body` 一旦看到 `when:` 就把其餘全部
  當分支),沒有「區塊外」可以放。

裸呼叫因此是「這個區塊裡的無條件成員」的唯一寫法。此推導記於 `BranchCondition` 的
doc comment;若擁有者認為應改為「強制寫 `else` 或 guard」,是可以再裁的一點。

---

## 3.4 比對與執行分期(frozen matching)

《`case`、`when` 與 Context Fragment(V2)》§`when:` 第 2 條:

> 所有非 `else` guard 都**只讀同一份 snapshot**;先前命中的 fragment 對後續 guard
> 不可見。

歷時側逐條對齊:

| 共時規格 §`when:` | 歷時 function 層 |
|---|---|
| 1. 建立一次合併前的 frozen snapshot | 比對階段取 `&self`,`self.document` 全程不變——**凍結由型別保證** |
| 2. 所有非 `else` guard 只讀該 snapshot | `match_branches` 一次算完命中表,之後才執行 |
| 3. guard 為 Error 時在 merge 前中止,無部分提交 | 比對階段出錯時**尚未執行任何分支** |
| 5. `else` 只在沒有普通 branch Matched 時命中 | `BranchCondition::Else => !any_matched` |

**誌誤**:把 `when:` 改回 accumulate 的第一版是**邊比對邊執行**,於是後面的 guard
讀得到前面分支的結果。最短反例:

```
function Chain(x [Verb]):
    when:
        reanalyze(x, target: category, to: aux) / x.syn.category == verb
        entrench(x, delta: 0.5) / x.syn.category == aux
```

凍結 ⇒ 第二個 guard 讀到的仍是 `verb`,不成立,entrenchment 維持 `0.2`;
洩漏 ⇒ 讀到 `aux`,成立,`0.2 + 0.5 = 0.7`。實測當時得到 0.7。

**與 P51 不衝突**:「逐步接力展開」是**執行**階段的性質(第 n 步讀第 n−1 步的結果),
比對是另一個階段。分開之後兩者同時成立,並順帶得到第 3 條的原子性。

共時規格第 4 條(merge 依來源序、同 path stable later-wins)在歷時側由**執行順序**
承擔:命中的分支依來源序執行,後者覆寫前者,語意等價。

---

## 4. 選擇不屬於引擎層【P70】

### 4.1 P12 已經把線畫好了

> `Goal(target, Language, State) → **Vec<Recipe 候選>**`(決定可能性)…
> **Weight DB 決定機率**(**自動模式**的抽樣權重)→ 抽樣器選擇

Goal 的型別**到候選清單為止**。抽樣器是消費那份清單的**下游、另一個東西**;而且
括號裡寫明它只服務**自動模式**——互動模式是使用者從候選面板挑,全程不碰抽樣器。

流 C 的結構同構,而且已經分開了:

```
Generator ──▶ Vec<Proposal>(帶評分) ──▶ [使用者挑選 / 自動採納] ──▶ Builder
```

準確的說法是:**`choose:` 產出候選;候選需要一個「選擇者」,抽樣器只是其中一種。**

### 4.2 移除 `evaluate_goal_offline`

該函數是三步合成的包裝(求值 → 選 → 再求值),與 P12 衝突,且**零生產呼叫端**。
它把一個應用層的動作硬塞進引擎,連帶三個後果:

| 移除 | 理由 |
|---|---|
| `evaluate_goal_offline` | 應用層的組合,不是引擎能力 |
| `GoalExecution` | 只為承載那個包裝的回傳值 |
| `FunctionError::NotGoal` | **全庫唯一建構點就在包裝裡**。包裝一走,「這不是 Goal」不再是失敗,而是回傳形狀本來就看得出來的事實 |

`offline` 這個字用在 Goal 上另有一層類別錯誤:`_offline` 的語意來自 P34「replay 不打
live service」,但 **Goal 永遠不會被 replay**——persist 進 `.chg` 的是**被選中的具體
Recipe 呼叫**。Goal 只在授權時跑一次,而授權時服務恰恰是 live 的。

### 4.3 零候選是合法結果,不是錯誤

`select_goal_candidate` 改回 `Result<Option<GoalSelectionTrace>, _>`,空候選回 `Ok(None)`。

「`choose:` 的所有 guard 都不成立」代表**這個語言目前沒有任何適用的演化路徑**
(例如對一個沒有動詞的語言跑「動詞語法化」)。那是語言狀態的事實,不是失敗。
先前它落到抽樣器的 `Empty`,被包成 `FunctionError::Sampling`,而 `Sampling` 依
P57 分類為 **`Environment`**「套件/權重表換版了」——方向完全錯。

**用 `Option` 而不是換一個錯誤變體**:呼叫端*必須*分辨「沒有路可走」與「選出了一條」;
回 `Ok(None)` 讓編譯器強迫它表態,回錯誤則可以被一句 `?` 靜默轉手。

連帶效果:擋掉空清單之後,`Sampling` 只承載 `InvalidWeight`/`AllZero` 兩種**權重資料**
問題,`Environment` 這個分類才名副其實。

### 4.4 否決:把抽樣寫成關鍵字

曾考慮在 `.chg` 或 body 層加一個 `sample` 關鍵字。**否決**,四個理由:

1. 與 P12 衝突(見 §4.1)、與 P48「不新增 layer 標記」衝突。
2. **致命**:`.chg` 內含隨機操作 ⇒ replay 不可重現,而且**把 seed 烘進檔案也救不了**
   ——同一個 seed 只在候選清單相同時才給同一個結果,而候選清單取決於(a)當下語言
   狀態(b)載入套件的 WeightDB。rebase 一次或套件升版一次,同一個 seed 就選到不同的
   Recipe,**檔案一字未改**。fsck 會發現 snapshot 對不上卻說不出原因,連 `Conflict`
   都歸不了類。現行設計相反:把**選中的具體 Recipe 呼叫**烘進 `.chg`,rebase 重跑一個
   具名 Recipe,套不上就是 P57 的 typed `Conflict`,指得出哪一句。
3. P30:資料層永不含邏輯,只存名字引用。抽樣是邏輯。
4. 流 C 已經給了同構的答案(§4.1)。

若痛點是「授權時想快速丟骰子」,那是 **UI affordance**(一顆按鈕),不是語言特性。

---

## 5. 已知界線

**`choose:` 不能出現在序列或分支中途**——引擎直接拒絕,不是暫停等待。
中途交棒需要「暫停 → 問外面 → 帶答案恢復」,那是 P33 的 pause/resume,亦即
《彙整05-11》§5.1 缺口 3,目前刻意延後。語言**禁止它支援不了的事**:若允許中途出現
`choose:` 而引擎偷選第一個,那是永遠查不出來的錯。

實務上「語法化走到一半讓使用者挑分支」today 要拆成三次獨立呼叫,由應用層串接;
選擇發生在**呼叫之間**而不是呼叫**之中**。缺口 3 完成後,同一個靜態判定可以從
「硬錯」改為「暫停點」,語法不必再動。

---

## 6. P 系列決策

| # | 內容 |
|---|---|
| **P69** | **function body 分支語意與 `.lang` 逐項對齊**(**修訂 P48**)。①`when:` 回歸 `CaseSelection::Accumulate`——所有 Matched **依序執行**;一個都不成立為**無操作不報錯**(照抄 `.lang` 的 `!any_matched => return Ok(current)`),與 `case:` 的 `CaseNoBranch` 硬錯刻意不同。②候選列舉獨立為 **`choose:`**;body 形狀是**結構**而非宣告,故不屬 P48 所否決的 layer 標記。引擎三個 dispatch 點**一律以靜態 body 形狀判定**,不先求值——先求值會讓與狀態無關的錯被與狀態有關的錯蓋掉,rebase 因而誤報 `Conflict`;連帶刪除 `CandidatesRequireSelection.candidates` 欄位(它正是逼出錯誤順序的原因)。③分支條件三選一 `Guard`/`Else`/`Always`:`else` = **`!any_matched`**(對齊 `.lang`),裸呼叫 = 恆成立且計入 `any_matched`。`case:` 行為零改變。④**比對與執行分期**(frozen matching,對齊《case_when 與 context fragment V2》§`when:` 第 2/3 條):所有 guard 只讀比對前的文件,一次算完命中表再依序執行;與 P51 逐步接力不衝突——接力是執行階段的性質。`Always` 為**推導**(function body 整個就是一個區塊,沒有「區塊外」可放無條件項目),非 `.lang` 既有 |
| **P70** | **選擇不屬於引擎層**(落實 P12)。Goal 的型別到 `Vec<Recipe 候選>` 為止,抽樣器是下游、且**只服務自動模式**(互動模式由使用者挑,同流 C 的 `[使用者挑選 / 自動採納]`)。移除 `evaluate_goal_offline`/`GoalExecution`/`FunctionError::NotGoal`(該變體的唯一建構點即在該包裝內)。**零候選為合法結果**:`select_goal_candidate` 回 `Result<Option<_>>`,空清單 `Ok(None)`——「沒有適用路徑」是語言狀態的事實,先前被誤分類為 `Environment`;用 `Option` 而非新錯誤變體,是為了讓編譯器強迫呼叫端表態。連帶 `Sampling` 只剩權重資料問題,`Environment` 名實相符。**否決**把抽樣寫成關鍵字:`.chg` 內含隨機操作則 replay 不可重現,且烘 seed 無效(候選清單隨 base 與套件版本改變) |

---

## 7. 落地狀態

已實作並驗證(2026-08-01)。

| 面 | 證據 |
|---|---|
| `when:` accumulate | `std_function_roles.rs::when_runs_every_branch_whose_guard_holds`(三分支、兩成立一不成立、delta 互異,三種退化落在三個不同數字)、`when_with_no_matching_branch_is_a_no_op_not_an_error` |
| `choose:` 與靜態判定 | `the_body_shape_predicts_the_evaluation_shape`(四種 body 形狀,要求兩種結局都出現)、`calling_a_goal_from_a_changeset_is_broken_input_not_a_conflict`(兩個引數必須得到同一答案) |
| frozen matching | `std_function_roles.rs::when_guards_all_read_the_document_as_it_was_before_any_branch_ran`(reanalyze→aux 後,下一個 guard 仍須讀到 verb;凍結 0.2 vs 洩漏 0.7) |
| `else` / `Always` | `else_only_fires_when_no_earlier_branch_matched`、`an_unconditional_branch_suppresses_a_later_else`、`case_else_still_fires_as_the_fallback`(對照組)、`function_definitions.rs::branch_forms_parse_into_the_three_conditions` |
| 零候選 | `goal_sampling.rs::a_goal_whose_guards_all_fail_yields_an_empty_candidate_list_not_an_error`、`selecting_from_an_empty_candidate_list_is_a_legitimate_none` |
| 兩步走取代包裝 | `goal_sampling.rs` / `flow_b_end_to_end.rs` 的 `enumerate_then_select` helper(明寫列候選 → 選 → 執行) |

**突變測試**(各被對應測試抓到):`when:` 跑完第一個就 break;`when:` 不篩選全跑;
`else` 恆成立;`else` 恆不成立;`Always` 不計入 `any_matched`;**退回邊比對邊執行**;`.chg` 退回先求值;
零候選退回抽樣器錯;`select` 永遠回 `None`。

**Golden churn(逐條理由)**:

1. `crates/language/lib/std/grammaticalization/code/goals.chg`:兩個 Goal 的 `when:`
   → `choose:`。
2. `tutorials/en-standard-reconstruction/restore.chg` 的 `std:grammaticalization`
   lock digest:`e5bad04…` → `e4bf2c2…`。**套件內容變了,鎖必須跟著**——這正是
   library lock 應有的行為,不是測試在配合實作。

`function_definitions.rs` 的分支解析測試**刻意保留 `when:`**:它們驗的是 guard/else/
空 body 的解析,對兩種形狀都適用,留著正好保住 `when:` 的解析覆蓋。

---

## 8. 逐文件修補清單

### docs/architecture/架構修補10 §2

body 形狀表擴為四列(見本檔 §1.3 修正後的表),`when:` 一列的「慣稱」由
「Goal 的候選列舉」改為「有條件的全跑」,並加註指向本檔。

### docs/architecture/架構修補彙整_05-11 §1

P48 一列加註:「body 分支語意由 **P69** 修訂——`when:` 為 accumulate,候選列舉為
`choose:`,`else` 為 `!any_matched`。」

### docs/specifications/case_when與context_fragment_v2.md

加註:歷時 function 層的 `when:`/`choose:`/`else` 由本檔(P69)規範,兩側逐條對齊表
見本檔 §3.4。

### docs/architecture/架構修補彙整_05-11 §5.1

缺口 4(resolved call trace、錯誤分型、golden 與端到端驗收)已完成,見
`step17_call_trace.rs` 與 `flow_b_end_to_end.rs`。
