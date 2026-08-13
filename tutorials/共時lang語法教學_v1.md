# 共時 `.lang` 語法教學 v1

本章用「先做得到一件事，再解釋語法」的順序介紹共時系統。`.lang` 保存 deep grammar；一次推導的 context、derived token、surface、ChangeSet、歷史 State 與 LLM 對話都不寫回 `.lang`。

## 1. 最小 form–meaning pairing

`Symbol` 與 `Class` 建立音系 inventory。`trait` 是可繼承契約，`sign` 是可儲存或可套用的符號。四維固定為：`phon + syn` 是 form pole，`sem + prag` 是 meaning/function pole；沒有同義的 `form:` 或 `meaning:` 包裝。

```lang
Symbol d
Symbol o
Symbol g
Class vowel {o}

trait TutorialEntity:
    belongs Semantic

sign dog:
    belongs TutorialEntity
    phon:
        /dog/
    sem:
        kind = canine
```

`belongs` 指向同一棵 trait ontology。較近祖先勝；同距離衝突依 package priority、來源 `belongs` 順序穩定決勝；本地定義最後勝。已解決衝突是 warning，slot 契約衝突是 error。

## 2. enum feature 與規則

資料欄位必須先宣告：`number = enum(...)` 是 domain，`number = plural` 是值，`target => expression` 是規則。`$self` 唯讀；規則只能寫自己所在維度。執行順序是 Syn→Sem→Prag，所以 Sem 可讀 finalized Syn，Prag 可讀 finalized Syn/Sem。

宣告尾綴 `?`（如下例的 `case`）表示**這條 feature 可以沒有值**——和 `slot NAME [C]?`、`role NAME [C]?` 的 `?` 是同一個意思，一律寫在宣告處。沒有 `?` 的 feature 若在讀取時沒有值，是執行期錯誤而不是靜默跳過；下面的 `case` 之所以要 `?`，是因為它由外層構式在組合時填入（見 §5 的 `slot_features:`），詞條自己不一定有。

```lang
trait TutorialNominal:
    syn:
        feature:
            number = enum(singular, plural)
            case = enum(nominative, accusative)?

sign plural-dog:
    belongs TutorialNominal
    syn:
        feature:
            number = plural
    sem:
        count => $self.syn.number
```

guard 的 equality 使用 `==`：`$self.syn.number == plural`。分類判斷則用 `$self == [TutorialNominal]`。guard 有 Matched／Unmatched／Error；Error 不會落入 Else，Then 遇 Error 保留先前已提交步驟並中止分支。

trait 可以 `==` 分成多個、由 0 起算的 block；sign／trait body 內獨立一行的 `Parent[1]` 只 inline 指定 block，裸 `Parent` 或 `Parent[]` inline 全部 block。`belongs Parent` 則是 ontology 分類關係，不是 block 選擇。宣告衝突與一般 `=` 共用載入 precedence：dependency→std→natural→plugin→caller，package priority 先決；同 priority 再按繼承距離、`belongs` 書寫序與本地值決勝。slot／role 契約衝突不使用 priority 掩蓋，而是 error。

## 3. slots、格位分配與 occurrence

`slot` 表示構式論元；`?` 是 optional，`[*]` 接受任意 sign。格位由 governor construction 的 `slot_features` 分配給名詞 occurrence，而不是永久改寫 noun sign。

```lang
sign TutorialClause:
    syn:
        slots:
            subject [TutorialNominal]
            object [TutorialNominal]?
        slot_features:
            subject.case = nominative
            object.case = accusative
    phon:
        /{subject} {object}/
```

RHS 也可讀 `$slot.predicate.syn.assigned_case`。同批 RHS 全部讀 frozen probe，不受書寫順序影響；完整驗證後才原子提交。stored sign 從 effective base、derived token 從 `DeepTokenState` 重跑 Syn→Sem→Prag，再重選 filler realization。`SlotMap` 是 Rust API 的 preserve／rename／autofill／internalize／optional 操作，目前不新增 `.lang` 表面語法。

## 4. semantic frame 與 recursive roles

Frame 身分用 trait；結構論元用 `roles:`。宣告是 `role [Trait]?`，binding 是 `role = {slot}`。required role 在 saturated derive 前必須填滿，filler category 也必須符合。

```lang
trait TutorialTransferFrame:
    sem:
        roles:
            agent [TutorialEntity]
            theme [TutorialEntity]
            recipient [TutorialEntity]?

sign TutorialGiving:
    belongs TutorialTransferFrame
    syn:
        slots:
            giver [TutorialEntity]
            gift [TutorialEntity]
    sem:
        roles:
            agent = {giver}
            theme = {gift}
    phon:
        /{giver} {gift}/
```

`SemNode` 保存 types、typed features、recursive roles 與 provenance。`SemanticDocumentV1` 以 `conlang.semantic/v1` JSON 輸出／匯入 detached semantic value，未知 schema、trait、feature、role 或欄位均拒絕；它是未來 LLM 介面邊界，不是 provider API。

## 5. `$slot`／`$self`、Then 與 Else

規則左側永遠寫目前維度；RHS 與 guard 才能唯讀 `$self` 或 frozen `$slot`。`Else` 是 first-match fallback，各分支看同一輸入；`Then` 是 sequential feeding，後一步看得到前一步已提交的值。平坦規則不可混用兩者，也不支援巢狀 Then／Else。

```lang
sign TutorialRule:
    syn:
        status => ready / $self == [TutorialNominal]
            then checked => yes / status == ready
```

identity 轉換仍算 Matched，因此會阻擋 Else。未知 trait、畸形 path、`unify` 衝突等是 Error，不可偷走 fallback；Unmatched 才會試下一個 Else。`RuleRecord` 保存 RuleId、stage、dimension、branch、source line、`$self`／`$slot` reads 與 package provenance。

V2 expression 另有 `case:` 與 `when:`。`case:` 只取第一個 Matched fragment；`when:`
則累加所有 Matched fragment。要注意 `when` 不是 sequential feeding：它先凍結合併前的
Sign，所有 guard 都只讀這份 snapshot，之後才把命中的匿名 fragment 按來源序合併。
因此前一支新增的 feature 不會使後一支 guard 改為 Matched。`when` 可位於 Sign、
`syn:`、`sem:`、`prag:` context；phon／feature scalar／role scalar 的選擇仍使用 `case:`。
完整範例見 `docs/specifications/case_when與context_fragment_v2.md`。

## 6. 一個 deep sign，多個 surface realization

`phon:` 的 `/.../` 是 deep/default template；`realization:` 依 finalized token 第一匹配選完整模板。選定後展開 `{slot}`，確認只剩純 phon 字串，才交給 Tshiatūn 音變。詞界由 phon phrase 保存，`surface_phrase` 最後映射成空格；surface 永不寫回 sign。

```lang
sign TutorialNP:
    belongs TutorialNominal
    syn:
        slots:
            stem [TutorialNominal]
    phon:
        /{stem}/
        realization:
            case:
                $self.syn.number == plural:
                    /{stem}s/
                else:
                    /{stem}/
```

Rust 端以 `DerivationContext::new().feature(Dim::Syn, "number", "plural")` 約束同一個 `TutorialNP` deep SignId。這是 occurrence constraint，不是 priority override；與固定值或規則結果衝突時在 phon 前失敗。

## 7. 可執行完整範例

以下區塊也是 integration fixture；測試會抽取、parse、compile、先 derive plural NP，再把該 derived token 放入 clause，由 outer `slot_features` 注入 nominative 並從 deep baseline 重跑。結果同時檢查 surface、四維 token、recursive roles、occurrence trace 與兩次執行決定性。

<!-- conlang-test: tutorial-complete -->
```lang
Symbol d
Symbol o
Symbol g
Symbol r
Symbol u
Symbol n
Symbol s
Class vowel {o, u}

trait TutorialEntity:
    belongs Semantic

trait TutorialNominal:
    belongs TutorialEntity
    syn:
        feature:
            number = enum(singular, plural)
            case = enum(nominative, accusative)?

trait TutorialPredicate:
    belongs Semantic

trait TutorialClauseFrame:
    sem:
        roles:
            agent [TutorialEntity]
            predicate [TutorialPredicate]

sign dog:
    belongs TutorialNominal
    phon:
        /dog/

sign run:
    belongs TutorialPredicate
    phon:
        /run/

sign TutorialNP:
    belongs TutorialNominal
    syn:
        slots:
            stem [TutorialNominal]
    sem:
        feature:
            interpreted_case = enum(nominative, accusative)?
            interpreted_case => $self.syn.case
        roles:
            referent [TutorialEntity]
            referent = {stem}
    prag:
        feature:
            discourse_case = enum(nominative, accusative)
            discourse_case => $self.sem.interpreted_case
    phon:
        /{stem}/
        realization:
            case:
                $self.syn.number == plural:
                    /{stem}s/
                else:
                    /{stem}/

sign TutorialClause:
    belongs TutorialClauseFrame
    syn:
        slots:
            subject [TutorialNominal]
            predicate [TutorialPredicate]
        slot_features:
            subject.case = nominative
    sem:
        roles:
            agent = {subject}
            predicate = {predicate}
    phon:
        /{subject} {predicate}/
```

## 8. library、export 與 caller override

`compile_system` 自動載入 enabled std；`compile_with_libraries` 明選一個 natural package與 embedded plugins。`std/cxg` 提供 form–meaning schema、slots 與 rules；Grambank 保存 `0/1/?` 類型觀察；`natural:en-standard` 提供具體 English signs。package 的 `exports.tsv` 以 `stable_id<TAB>trait|sign<TAB>alias` 公開，rule ID 使用 `std:*`、`natural:*`、`plugin:*` namespace。effective language 依 dependency→std→natural→plugin→caller 疊加；`language()` 仍是 caller source，`effective_language()` 才供 runtime。

## 9. Rust 任務流程

```rust
let language = Language::parse(source)?;
let system = compile_system(language)?;
let np = system.derive_with_context(
    "TutorialNP",
    &[SlotFiller::sign("stem", "dog")],
    &SlotMap::identity(),
    DerivationContext::new().feature(Dim::Syn, "number", "plural"),
)?;
let clause = system.derive(
    "TutorialClause",
    &[
        SlotFiller::token("subject", &np.token),
        SlotFiller::sign("predicate", "run"),
    ],
    &SlotMap::identity(),
)?;
```

讀取 `SystemDerivation::{surface,token,rules,occurrences,realization,phon_steps,diagnostics}`；`OccurrenceRecord` 分開保存 probe 與 committed RuleRecords、constraints、是否重跑及 filler realization。

## 10. diagnostics、Semantic JSON 與 `.lang`／`.chg` 邊界

`.lang` 保存可編譯的共時來源；`<name>.lang.ids.json` v2 保存 stable NodeId。`.chg` 只對 caller `LanguageDocument` 執行 Insert／Delete／Update／Move，不改 effective libraries、CompiledSystem、DerivedToken 或 surface。ChangeSet resolve 後 selector 固化為 `node(kind,@namespace:ordinal)`，每個 statement 原子 commit；compile 是 dirty revision 的 lazy cache。

常見診斷：`FEATURE_UNDECLARED`、`FEATURE_VALUE_OUT_OF_DOMAIN`、`SLOT_FEATURE_DUPLICATE_TARGET`、`SLOT_FEATURE_DOMAIN_MISMATCH`、`ROLE_REQUIRED_MISSING`、`DERIVATION_FEATURE_CONFLICT`、`IDENTITY_SOURCE_MISMATCH`、`CHANGESET_BASE_SOURCE_MISMATCH`。

固定執行順序：evaluate fillers → frozen probe → atomic occurrence constraints → filler Syn/Sem/Prag re-evaluation → filler realization → compose deep outer token → outer context → outer Syn/Sem/Prag → pure phon realization → Tshiatūn → surface。

## 附錄 A：canonical grammar 導覽

| 層次 | canonical 寫法 | 用途 |
|---|---|---|
| inventory | `Symbol`、`Class`、`Prosody`／distribution declaration | 建立純 phon inventory 與環境 |
| ontology | `trait`、`global trait`、`belongs` | 分類、繼承與 frame identity |
| lexical/construction sign | `sign NAME:` | 同一 deep form–meaning pairing |
| 四維 block | `phon:`、`syn:`、`sem:`、`prag:` | 維度隔離的 Def、Rule 與契約 |
| typed feature | `feature: NAME = enum(...)`／`NAME = VALUE`／`NAME => EXPR` | 有限 domain、值與求值規則 |
| valence | `slots:`、`NAME [Trait]?`、`NAME [*]` | required／optional filler contract |
| occurrence forwarding | `slot_features: SLOT.FEATURE = RHS` | 將 governor constraint 給單次 filler occurrence |
| semantic arguments | `roles: NAME [Trait]?`／`NAME = {slot}` | typed recursive Sem |
| deep phon | `/.../` | default 完整 phon template與詞界 |
| conditional surface | `realization:` branch／`else` | 由 finalized token 選完整 template |
| rule control | `RULE`、縮排 `then` 或 `else` | sequential feeding 或 first-match fallback |
| metadata | `origin`、`provenance`、`lifecycle`、`entrenchment`、`lexicalized` | 共時來源資訊；不屬四維動力學 |

## 附錄 B：可編輯 Node 與不入 `.lang` 的資料

stable identity sidecar 為 Language root、inventory／distribution entry、Trait、Sign、Block、所有 SignItem、Rule／Then／Else branch 與 realization branch 保存 `NodeId`。`SignId`、`RuleId` 是 typed wrapper；rename 保 ID，Move 保整棵子樹 ID，Delete＋Insert 必換 ID。

`.lang` 不保存一次使用事件的 DerivationContext、FillerSnapshot、DerivedToken、OccurrenceRecord、surface、attestation、ChangeSet、History State、prompt/model/provider。Semantic JSON 是可驗證的 detached DTO；`.chg` 是 caller source edit；兩者都不是 `.lang` 新關鍵字。
