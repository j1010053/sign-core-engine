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
        /{$slot.subject} {$slot.object}/
```

RHS 也可讀 `$slot.predicate.syn.assigned_case`。同批 RHS 全部讀 frozen probe，不受書寫順序影響；完整驗證後才原子提交。stored sign 從 effective base、derived token 從 `DeepTokenState` 重跑 Syn→Sem→Prag，再重選 filler realization。`SlotMap` 的 preserve／rename／autofill／internalize／optional 同時有 Rust API 與平坦 `.lang` 表面，例如 `map head rename nucleus`、`map head optional false`；V3 的 `autofill` 另可接受 `Paradigm()` 這類 callable application。

## 4. semantic frame 與 recursive roles

Frame 身分用 trait；結構論元用 `roles:`。宣告是 `role [Trait]?`，binding 是 `role = {$slot.NAME}`。required role 在 saturated derive 前必須填滿，filler category 也必須符合。

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
            agent = {$slot.giver}
            theme = {$slot.gift}
    phon:
        /{$slot.giver} {$slot.gift}/
```

`SemNode` 保存 types、typed features、recursive roles 與 provenance。`SemanticDocumentV1` 以 `conlang.semantic/v1` JSON 輸出／匯入 detached semantic value，未知 schema、trait、feature、role 或欄位均拒絕；它是未來 LLM 介面邊界，不是 provider API。

## 5. 參數化 trait

前面的 slot 與 role 都把 constraint 寫死（`[TutorialNominal]`、`[TutorialEntity]`）。如果多個 trait 只差在 constraint 類別，可以用 **type parameter** 把類別變成參數，由 `belongs` 提供具體值。

```lang
trait TutorialAgreement<C, T>:
    syn:
        slots:
            controller [C]
    sem:
        roles:
            target [T]?
```

`<C, T>` 是宣告；使用時在 `belongs` 提供具體類別：

```lang
sign TutorialSVA:
    belongs TutorialAgreement<TutorialNominal, TutorialPredicate>
    TutorialAgreement
```

展開後 slot `controller` 的 constraint 變成 `[TutorialNominal]`，role `target` 變成
`[TutorialPredicate]`——slot 與 role 使用同一套型別參數規則，和手寫一模一樣，
但只定義一次。

**trait 之間的傳播**：一個 trait 可以只填部分參數，剩下的向上傳播給使用者：

```lang
trait TutorialSubjectAgreement<T>:
    belongs TutorialAgreement<TutorialNominal, T>
    TutorialAgreement
```

`TutorialSubjectAgreement` 把 `C` 固定為 `TutorialNominal`，但 `T` 仍是參數。sign 使用時填入最後一個：

```lang
sign TutorialSVA2:
    belongs TutorialSubjectAgreement<TutorialPredicate>
    TutorialSubjectAgreement
```

parameter 也可以加上 bound：`<C: TutorialEntity>` 表示 `C` 只接受
`TutorialEntity` 或其子類型。參數只在**宣告它的同一個 trait** 裡有效：大寫開頭
不會自動變成參數，另一個 trait 恰好宣告同名參數也不算；拼錯的具體 category
仍會得到 `SLOT_UNKNOWN_CATEGORY` 或 `ROLE_UNKNOWN_CONSTRAINT`。

trait 把自己的自由參數傳給另一個泛型 trait 時，外層 bound 必須等於或窄於內層
要求。換句話說，`T: TutorialNominal` 可以傳給要求 `C: TutorialEntity` 的位置，
但無 bound 的 `T` 或更寬的 `T: Semantic` 不行，會得到
`TYPE_PARAM_BOUND_VIOLATION`。

下面是可直接驗證的完整正例；`T: Predicate` 比內層要求的 `Entity` 更窄：

<!-- conlang-test: parameterized-trait-scope -->
```lang
trait TutorialGenericEntity:
    pass

trait TutorialGenericNominal:
    belongs TutorialGenericEntity
    TutorialGenericEntity

trait TutorialGenericPredicate:
    belongs TutorialGenericEntity
    TutorialGenericEntity

trait TutorialAgreement<C: TutorialGenericEntity, T: TutorialGenericEntity>:
    syn:
        slots:
            controller [C]
    sem:
        roles:
            target [T]?

trait TutorialSubjectAgreement<T: TutorialGenericPredicate>:
    belongs TutorialAgreement<TutorialGenericNominal, T>
    TutorialAgreement

sign TutorialSVA:
    belongs TutorialSubjectAgreement<TutorialGenericPredicate>
    TutorialSubjectAgreement
    phon:
        /{$slot.controller}/
```

反例中，未宣告的 `MisspelledCategory` 不會因為大寫就被放行：

<!-- conlang-test: parameterized-trait-invalid-scope -->
```lang
trait Schema<C>:
    syn:
        slots:
            head [MisspelledCategory]
    sem:
        roles:
            referent [OtherParam]
```

而無 bound 的外層參數不能保證符合內層要求：

<!-- conlang-test: parameterized-trait-invalid-bound -->
```lang
trait TutorialBoundEntity:
    pass

trait Inner<C: TutorialBoundEntity>:
    syn:
        slots:
            head [C]

trait Outer<T>:
    belongs Inner<T>
    Inner
```

`marker trait` 是沒有內容的分類契約，現在可以帶型別參數，例如 `marker trait Bar<T>:`；
具體類型會保留實參身分。巢狀宣告與傳遞見 [XP 範例](nested-types/README.md) 和 [I32](../docs/implementation/nested-types-and-specialization.md)。
`global trait` 不能有 type parameter。`marker` 與 `global` 互斥，同一個 trait
不能同時具有兩種旗標，違反時是 `TRAIT_GLOBAL_MARKER_CONFLICT`。

`global trait` 是唯一會自動套用的模板，一份有效語言最多只能選一個。選擇是顯式的：
把要自動套用者寫成 `global trait`，其他可選模板仍寫成普通 `trait`，再由個別 Sign
以 `belongs`／mount 使用。若兩個宣告都寫成 `global trait`，不是依名稱或書寫順序猜測，
而是回報 `GLOBAL_TRAIT_SELECTION_REQUIRED`；不需要全域模板時則可以完全不宣告。
自動套用不只指音變：global 裡的 slot、feature／role 宣告、預設值與
syn／sem／prag 規則都會進每個 Sign 的有效內容，因此普通 trait 的規則也能直接讀取
global 宣告的 slot 或寫入其 feature。值衝突時是 global < 個別 `belongs` trait < Sign
本地值。global 及其祖先的 phon 規則則集中進全域 grammar 一次，不會再對每個 Sign
當局部音系重跑。

## 6. `$slot`／`$self`、Then 與 Else

規則左側永遠寫目前維度；RHS 與 guard 才能唯讀 `$self` 或 frozen `$slot`。`Else` 是 first-match fallback，各分支看同一輸入；`Then` 是 sequential feeding，後一步看得到前一步已提交的值。平坦規則不可混用兩者，也不支援巢狀 Then／Else。

```lang
sign TutorialRule:
    syn:
        status => ready / $self == [TutorialNominal]
            then checked => yes / status == ready
```

identity 轉換仍算 Matched，因此會阻擋 Else。未知 trait、畸形 path、`unify` 衝突等是 Error，不可偷走 fallback；Unmatched 才會試下一個 Else。`RuleRecord` 保存 RuleId、stage、dimension、branch、source line、`$self`／`$slot` reads 與 package provenance。

V3 expression 另有 `case:` 與 `when:`。`case:` 只取第一個 Matched fragment，fallback
拼作 `base:`；`when:`
則累加所有 Matched fragment。要注意 `when` 不是 sequential feeding：它先凍結合併前的
Sign，所有 guard 都只讀這份 snapshot，之後才把命中的匿名 fragment 按來源序合併。
因此前一支新增的 feature 不會使後一支 guard 改為 Matched。`when` 可位於 Sign、
`syn:`、`sem:`、`prag:` context；phon／feature scalar／role scalar 的選擇仍使用 `case:`。
`when` 的 fallback 仍拼作 `else:`。兩者的 fallback 都必須放最後；只有 `case` 的
`base:` 可以留白，表示沿用目前 context 的基底。

多個 branch 都檢查同一組 feature 時，可在 `case [dim.feature, ...]:` 先宣告欄位，
列中只寫值。`_` 不檢查該欄；其他欄位全部以 AND 匹配，仍依來源序 first-match：

<!-- conlang-test: bundle-match -->
```lang
sign TutorialCopulaChoice:
    syn:
        feature:
            tense = enum(present, past)
            person = enum(first, second, third)
            number = enum(singular, plural)
            form = enum(am, is, are, be)
            form = be
            form =
                case [syn.tense, syn.person, syn.number]:
                    [present, first, singular]: am
                    [present, third, singular]: is
                    [present, _, _]: are
                    base:
    phon:
        /be/
```

欄位須為 effective Sign 已宣告的 typed feature；欄數不符、未知欄位與 enum 值域外
的值都會報錯。缺失或未決定的值不匹配常量；`_` 則完全不讀該欄。
完整範例見 `docs/specifications/case_when與context_fragment_v2.md`。

## 7. SignApplication 與 Paradigm

### 7.1 Sign 是可 partial apply 的函式值

每個 Sign 都可用 `name(...)` 引用，slot 就是它的參數。應用不複製 stored Sign，也不
產生新的 SignId：

```lang
atom()                              /* 沒有必填 slot：回傳 atom 自身 */
wrapper()                           /* 尚缺必填 value：回傳 partial wrapper */
wrapper(value: {$slot.item})        /* 具名綁定，推薦寫法 */
wrapper({$slot.item})               /* 只有一個參數時的位置 shorthand */
outer(value: wrapper(value: $self)) /* 參數可以是 $self、slot 或巢狀 application */
```

多參數一律具名；參數名必須對應 callable 的 residual slot。partial value 可繼續傳遞或由
Rust `apply_arguments` 續填，但需要完整 Sign 的 consumer 會拒絕尚有必填 residual slot
的結果。這使「引用同一個 Sign」和「以某組 filler 具體化該 Sign」保持同一身分鏈。

### 7.2 Paradigm：依 bundle 選一個真正的 Sign

Paradigm 是水平連結，不是 Sign 的別名。它有自己的 ParadigmId，但執行結果保留被選
成員的 SignId。分支必須是顯式 `name(...)`，最後一定要有非空 `base`：

<!-- conlang-test: paradigm-deferred-autofill -->
```lang
Symbol a
Symbol b
Symbol c
Symbol e
Symbol i
Symbol j
Symbol m
Symbol r
Symbol s
Symbol t
Symbol u
Class vowel {a, e, i, u}

marker trait TutorialCopula:

paradigm TutorialBE:
    case [syn.tense, syn.person, syn.number]:
        [present, first, singular]: am()
        [present, third, singular]: is()
        [present, _, _]: are()
        base: be()

sign am:
    belongs TutorialCopula
    syn:
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person = first
            number = enum(singular, plural)
            number = singular
    phon:
        /am/

sign are:
    belongs TutorialCopula
    syn:
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person = first | second | third
            number = enum(singular, plural)
            number = singular | plural
    phon:
        /are/

sign be:
    belongs TutorialCopula
    syn:
        feature:
            tense = enum(present, past)
            tense = present | past
            person = enum(first, second, third)
            person = first | second | third
            number = enum(singular, plural)
            number = singular | plural
    phon:
        /be/

sign is:
    belongs TutorialCopula
    syn:
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person = third
            number = enum(singular, plural)
            number = singular
    phon:
        /is/

sign TutorialThirdSubject:
    syn:
        feature:
            person = enum(first, second, third)
            person = third
            number = enum(singular, plural)
            number = singular
    phon:
        /subject/

sign TutorialCopularClause:
    syn:
        slots:
            subject [*]
            copula [TutorialCopula]
        map copula autofill TutorialBE()
        feature:
            tense = enum(present, past)
            tense = present
            person = enum(first, second, third)
            person => $slot.subject.syn.person
            number = enum(singular, plural)
            number => $slot.subject.syn.number
    phon:
        /{$slot.subject} {$slot.copula}/
```

`autofill TutorialBE()` 會等 consumer 從 `subject` 算完 person／number，再用同一份 frozen
bundle 選 `is()`；所以結果是 `subject is`。若明確提供 `copula` slot，顯式 filler 會覆蓋
autofill。各成員若有相同 slot 介面，`TutorialBE()` 也可先回傳 partial member，再由
外層以 `TutorialBE(value: {$slot.value})` 轉送參數。

完整本體、錯誤契約、Rust／Query 消費者與 `.chg` 欄位見
`docs/specifications/Paradigm與SignApplication_V3.md`。

## 8. 一個 deep sign，多個 surface realization

`phon:` 的 `/.../` 是 deep/default template；`realization:` 依 finalized token 第一匹配選完整模板。選定後展開 `{$slot.NAME}`，確認只剩純 phon 字串，才交給 Tshiatūn 音變。詞界由 phon phrase 保存，`surface_phrase` 最後映射成空格；surface 永不寫回 sign。

```lang
sign TutorialNP:
    belongs TutorialNominal
    syn:
        slots:
            stem [TutorialNominal]
    phon:
        /{$slot.stem}/
        realization:
            case:
                $self.syn.number == plural:
                    /{$slot.stem}s/
                base:
                    /{$slot.stem}/
```

Rust 端以 `DerivationContext::new().feature(Dim::Syn, "number", "plural")` 約束同一個 `TutorialNP` deep SignId。這是 occurrence constraint，不是 priority override；與固定值或規則結果衝突時在 phon 前失敗。

## 9. 可執行完整範例

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
            referent = {$slot.stem}
    prag:
        feature:
            discourse_case = enum(nominative, accusative)
            discourse_case => $self.sem.interpreted_case
    phon:
        /{$slot.stem}/
        realization:
            case:
                $self.syn.number == plural:
                    /{$slot.stem}s/
                base:
                    /{$slot.stem}/

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
            agent = {$slot.subject}
            predicate = {$slot.predicate}
    phon:
        /{$slot.subject} {$slot.predicate}/
```

## 10. library、export 與 caller override

`compile_system` 自動載入 enabled std；`compile_with_libraries` 明選一個 natural package 與 embedded plugins。v2 專案則先解析 `project.toml`，將同一份 `ResolvedPackages` 傳給 `compile_document_with_packages`。`std/cxg` 提供 form–meaning schema、slots 與 rules；Grambank 保存 `0/1/?` 類型觀察；`natural:en-standard` 提供具體 English signs 與 Paradigms。

套件中的 trait、paradigm、sign 必須在宣告最前面加 `export`，外部才能直接引用或覆寫，例如 `export trait Noun:`。schema 2 的 `exports.tsv` 是選用的穩定 ID 表，不能替沒有 `export` 的定義開放權限。套件內部仍能使用自己的私有定義。

以 [English 套件教學](package-exports/README.md)的最小套件為例，專案可在 `[packages.aliases]` 設定 `English = "natural:english"`，然後以 `English.Noun`、`English.cat()` 引用其公開定義。同名匯出無法唯一解析時，裸名會報錯。在本地宣告 `trait English.Noun:` 會整個替換此專案使用的公開定義，未寫出的舊內容不會自動保留；原套件檔案不會被修改。

rule ID 繼續使用套件 namespace。`language()` 保存 caller source，`effective_language()` 則是供 runtime 使用、已解析作用域的完整語法，包含公開定義所需的私有實作。

## 11. Rust 任務流程

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

## 12. diagnostics、Semantic JSON 與 `.lang`／`.chg` 邊界

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

stable identity sidecar 為 Language root、inventory／distribution entry、Trait、Paradigm、Sign、Block、所有 SignItem、Case／CaseBranch／Application、Rule／Then／Else branch 與 realization branch 保存 `NodeId`。`ParadigmId`、`SignId`、`RuleId` 是 typed wrapper；rename 保 ID，Move 保整棵子樹 ID，Delete＋Insert 必換 ID。

`.lang` 不保存一次使用事件的 DerivationContext、FillerSnapshot、DerivedToken、OccurrenceRecord、surface、attestation、ChangeSet、History State、prompt/model/provider。Semantic JSON 是可驗證的 detached DTO；`.chg` 是 caller source edit；兩者都不是 `.lang` 新關鍵字。
