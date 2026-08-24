# 共時 `.lang` V2：FP expression、typed case 與 constraint network

`<SynContext>`／`<SemContext>`／`<PragContext>` 與累加式 `when:` 的補充規格見
[`case_when與context_fragment_v2.md`](case_when與context_fragment_v2.md)。

## 1. Engine profile

本引擎是 identity-bearing construction grammar。詞彙 Sign、抽象 construction 與具體 construction 都是同一種四維 `Sign`；`phon+syn` 是 form pole，`sem+prag` 是 meaning/function pole。`slots:` 是 Sign 函數的 typed parameters，不是另外一種 implementation fragment。

```text
stored Sign / deep construction
→ bind named slot parameters
→ 同一 SignValue 的未飽和狀態（若仍有 required free variables）
→ validate equal/before/adjacent
→ Syn → Sem → Prag rules
→ typed case / phon realization
→ pure phon input → Tshiatūn phonology → surface
```

來源 Sign、已產生 token 與 library overlay 均不可變。每次 application 產生新的
provenance；部分套用不建立另一種實體，再次填參數時從同一 Sign value 保存的 baseline
重播 immutable fillers，原值保持不變。

## 2. V1/V2 邊界

V2 文件必須明示：

```lang
schema conlang.lang/v2
```

Step 13 期間只 frozen V1 `.lang` reader、printer 與 digest，供明示遷移；V1 不接受
application、typed `case` 或 `constraints:`。`LanguageDocument::migrate_to_v2()` 加入 schema、
保持既有 NodeId／address，並原子更新 source digest。後續新增的 Application、Case、
CaseBranch、Constraint 節點由 active identity namespace 配號。混合 package 可編譯；只要
任一 library 使用 V2，effective language 以 V2 canonical form 輸出，caller 原檔不會被
自動改寫。

這不是舊檔直接 replay 承諾：舊 `.chg`、舊 sidecar 或舊 source digest 不必直接套用到
遷移後文件。持久變更的 base 是明示遷移後重新釘住的 V2 source／identity snapshot；
Step 14 完成與相容性測試前，replay 介面都只視為 preview。

## 3. Slots 是 Sign parameters

```lang
sign Pairing:
    syn:
        slots:
            first [Piece]
            second [Piece]
    phon:
        /{$slot.first} {$slot.second}/
```

canonical application 使用具名參數；一個參數時可用位置 shorthand：

```lang
Pairing(first = {$self})
en_3sg({$self})
```

application 的值永遠是完整四維 `SignValue`。缺少 `second` 時，第一式仍回傳同一種
Sign value，只是 `has_free_variables() == true`；它保留 residual parameters、constraints、
categories、semantic roles、identity 與 provenance。Rust 端以
`CompiledSystem::apply_arguments(&value, arguments)` 填入剩餘參數；原 value 不變，結果的
`sign_id()` 仍指同一 deep construction。只有要求 concrete scalar、飽和 token 或 pure
phon input 時才拒絕尚未填入的自由變數。

## 4. Context-typed case

`case<TContext>` 是 expression，context type 由位置決定。它描述「該位置允許合併的
fragment」，不要求每個 branch 都重新回傳所在層的完整值。context type 是引擎封閉列舉：
`SignContext`、`PhonContext`、宣告過 enum 的 Feature context，以及 Role context；`.lang`
不能註冊新的 context type。無匹配且沒有 `else` 時，使用 case 外的 default：
Sign body 保留 `$self`，phon realization 保留 deep template。

```lang
sign en_3sg:
    belongs ThirdSingular
    syn:
        slots:
            stem [Verb]
    phon:
        /{$slot.stem}+s/
        realization:
            case stem.phon:
                == SibilantFinal:
                    /{$slot.stem}+es/
                else:
                    /{$slot.stem}+s/
```

Sign body 的 branch 有兩種合法結果：

1. Sign application／`$self`：產生完整 `SignValue`。
2. `SignContext` fragment：產生匿名 Sign body 片段，合併進目前正在建構的 Sign；不建立
   新 Sign、不配發新的 `SignId`。

完整 Sign application 後仍可增加 membership：

```lang
sign walk:
    belongs Verb
    phon:
        /walk/
    case:
        $self.syn.number == singular && $self.syn.person == third:
            en_3sg({$self})
            belongs FiniteVerb
```

`SignContext` fragment 可直接寫 `belongs`、四維內容、feature／role／rule／realization，
也可寫 trait 名。後者沿用正常 Sign 的 trait expansion 路徑，可視為匿名 trait 展開：

```lang
trait EnglishThirdPersonFragment:
    belongs ThirdSingular
    syn:
        feature:
            inflection = enum(base, third)
            inflection = third
    phon:
        /walks/

sign walk:
    phon:
        /walk/
    case:
        $self.syn.person == third:
            EnglishThirdPersonFragment
            prag:
                licensed = yes
```

這個 branch 的結果仍是 `walk` 的同一個 Sign；trait 內容與本地 `prag` fragment 合併後，
從未求值的 source/deep baseline 重跑 Syn→Sem→Prag。它不在已提交結果上疊加 Patch，
因此原 `SignId`、source provenance 與未命中的 `$self` 都保持不變。

trait 只能展開到 `SignContext`。`PhonContext` 只能回傳純 phon template 或完整 Sign 的
phon projection，不能寫 trait、`belongs`、Syn/Sem/Prag。需要在 phon 位置呼叫 Sign
函數時，必須明示完整求值後的 projection：

```lang
phon:
    realization:
        case:
            $self == [FiniteVerb]:
                /{en_3sg({$self}).phon.ret}/
```

branch 依來源序求值。Unmatched 繼續；硬 category/constraint blocking 記錄 `CASE_MORE_SPECIFIC_BLOCKED` 後繼續；Error 立即停止，不能落入 `else`。`Else:` 可讀，canonical 一律輸出 `else:`。

branch 結果也可再是相容 context type 的 `case`；parser／printer、validator、runtime、
identity sidecar 與 Primitive Edit 都遞迴處理。巢狀 branch 的 enum domain、guard path、
slot／trait reference 在 compile/edit commit 時驗證，不延到 derive 才失敗。Sign branch 的
`belongs` 會從未求值 baseline 重新物化完整 trait contract（Def、Feature、Slot、Rule、
Role、Realization 與 Sem types），不是只增加分類標籤。

## 5. Binary constraint network

```lang
constraints:
    equal(subject.syn.number, predicate.syn.number)
    before(subject, predicate)
    before(predicate, object)
    adjacent(subject, predicate)
```

- `equal` 要求兩端是相同 effective enum domain；值均 concrete 時必須相等。V1 `unify(a,b)` lowering 到相同 predicate。
- `before` 與 `adjacent` 只接受 construction form constituents，並檢查 phon template 的實際線性化。
- 未填 operand 的 constraint 保存在同一未飽和 Sign value；飽和或進入 pure phon
  boundary 前完成檢查。
- `before` cycle、unknown slot、domain mismatch、value conflict 與 template order conflict 都是 coded error。

package／inheritance priority 只解決 source declaration 衝突，不用來暗中阻擋 runtime construction。

## 6. Construction competition

`CompiledSystem::derive_candidates(category, fillers, mapping, context)` 以不產生 surface 的完整
context／rule／feature／role／Sign-case pipeline 驗證所有候選，回傳 category、slot signature
與 constraints 相容者並依 stable `SignId` 排序；grammar Error 不得被當成普通 miss 吞掉。
`derive_category` 遇零候選回 `NO_MATCHING_CONSTRUCTION`，遇多候選回
`AMBIGUOUS_CONSTRUCTION`。

選擇必須明示：deterministic selector 由 caller 提供 candidate `SignId`；`SampleEntrenchment { seed }` 才依非負有限 entrenchment 抽樣，0 不參與。trace 保存 seed、stable order、weights 與 selected ID；未指定 selector 時不以 priority 或 entrenchment 偷選。

## 7. Requirement-to-evidence

| Requirement | Runtime evidence |
|---|---|
| V1 source/dump 不變 | 全既有 round-trip、golden、identity tests |
| Sign application 回傳完整 Sign | `fp_v2::sign_application_returns_a_full_typed_sign` |
| 未飽和 Sign 可續填且 immutable | `fp_v2::unsaturated_sign_can_receive_arguments_without_becoming_another_entity` |
| typed case default／type boundary | V2 round-trip、V1 rejection、sign/phon runtime tests |
| SignContext fragment 合併且不產生新 Sign | `fp_v2::sign_context_case_merges_anonymous_trait_fragment_into_the_same_sign` |
| trait 只可展開到 SignContext | `fp_v2::trait_expansion_is_closed_to_sign_context` |
| feature／role typed case | `fp_v2.rs` 的 enum-domain、role-slot 與 default/error 反例 |
| 巢狀 typed case 與 trace | `fp_v2::nested_cases_round_trip_and_execute_in_feature_role_and_phon_positions`、`nested_sign_case_returns_the_inner_sign_expression` |
| branch `belongs` 完整 trait contract | `fp_v2::sign_case_membership_materializes_the_complete_trait_contract` |
| projection 先求完整 Sign | `fp_v2::phon_projection_evaluates_the_full_sign_before_extracting_phon` |
| 巢狀 application 參與解析與 cycle graph | `fp_v2::nested_applications_participate_in_static_resolution_and_cycle_checks` |
| equal/order constraints | `fp_v2::binary_constraints_execute_at_application` 與八個 `std/cxg` recipe E2E |
| competition 不暗選 | `fp_v2::competition_returns_all_candidates_and_samples_deterministically` |
| entrenchment 極大有限權重仍維持比例／決定性 | `fp_v2::entrenchment_sampling_normalizes_finite_weights_before_they_overflow` |
| candidate 完整求值／零候選／blocking fallback | `runtime_sealing.rs` 的 candidate 與 hard-constraint 反例 |
| Stored `$self` 不重播規則且保留 deep/source provenance | `runtime_sealing.rs` 的 inherited-rule 與 contextual source-baseline 反例 |
| identity-preserving migration | `fp_v2::explicit_document_migration_preserves_existing_node_ids` |
| expression identity 可由四原語定址 | `identity_sidecar.rs` 與 `primitive_edits.rs` 的 Application／Case／CaseBranch／Constraint 反例；匿名 fragment 的 items／Refs 另由 `anonymous_sign_context_fragment_items_keep_stable_identity_and_refs` 固定 |
| English abstract/concrete migration | English count、case、copula、do-support 與 12-construction E2E |

## 8. 鎖定邊界

V2 沒有 arbitrary cross-dimension assignment、conditional optional slot、可自行註冊的
context/fragment type、n-ary order primitive、automatic competition blocking 或隱式 stochastic choice。
feature RHS 與 role binding 的 `case` 已使用同一 expected-type checker：feature arm 必須回傳
該 enum domain 的 value，role arm 必須回傳符合 role contract 的 slot reference；Sign body
接受完整 Sign 或 SignContext fragment，phon realization 只接受 PhonContext。所有
expression position 共用同一 typed IR，
不得另加字串後門。
