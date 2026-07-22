# 共時 `.lang` V2：FP expression、typed case 與 constraint network

## 1. Engine profile

本引擎是 identity-bearing construction grammar。詞彙 Sign、抽象 construction 與具體 construction 都是同一種四維 `Sign`；`phon+syn` 是 form pole，`sem+prag` 是 meaning/function pole。`slots:` 是 Sign 函數的 typed parameters，不是另外一種 implementation fragment。

```text
stored Sign / deep construction
→ bind named slot parameters
→ PartialSign（若仍有 required free variables）
→ validate equal/before/adjacent
→ Syn → Sem → Prag rules
→ typed case / phon realization
→ pure phon input → Tshiatūn phonology → surface
```

來源 Sign、已產生 token 與 library overlay 均不可變。每次 application 產生新的 provenance；部分套用再次填參數時重播已保存的 immutable fillers，不修改原 `PartialSign`。

## 2. V1/V2 邊界

V2 文件必須明示：

```lang
schema conlang.lang/v2
```

V1 reader、printer 與 digest 保留；V1 不接受 application、typed `case` 或 `constraints:`。`LanguageDocument::migrate_to_v2()` 只加入 schema，保持既有 NodeId／address，並原子更新 source digest。後續新增的 Application、Case、CaseBranch、Constraint 節點才由 active identity namespace 配號。混合 package 可編譯；只要任一 library 使用 V2，effective language 以 V2 canonical form 輸出，caller 原檔不會被自動改寫。

## 3. Slots 是 Sign parameters

```lang
sign Pairing:
    syn:
        slots:
            first [Piece]
            second [Piece]
    phon:
        /{first} {second}/
```

canonical application 使用具名參數；一個參數時可用位置 shorthand：

```lang
Pairing(first = {$self})
en_3sg({$self})
```

application 的值永遠是完整四維 Sign。缺少 `second` 時，第一式回傳 `PartialSign`，保留 free variable、residual parameter、constraints、categories、semantic roles 與 provenance。Rust 端以 `SignValue::partial()` 取得它，再用 `CompiledSystem::resume_partial` 填入剩餘參數。未飽和值可以作符號式中間 Sign；只有要求 concrete scalar、飽和 token 或 pure phon input 時才拒絕自由變數。

## 4. Context-typed case

`case<T>` 是 expression，`T` 由位置決定。無匹配且沒有 `else` 時，使用 case 外的 default：Sign body 回到 `$self`，phon realization 回到 deep template。

```lang
sign en_3sg:
    belongs ThirdSingular
    syn:
        slots:
            stem [Verb]
    phon:
        /{stem}+s/
        realization:
            case stem.phon:
                == SibilantFinal:
                    /{stem}+es/
                else:
                    /{stem}+s/
```

Sign body的 branch 回傳 Sign，所以其後可增加 membership：

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

phon case 只能回傳 `Phon`，不能 `belongs` 或寫 Syn/Sem/Prag。需要在 phon 位置呼叫 Sign 函數時，必須明示完整求值後的 projection：

```lang
phon:
    realization:
        case:
            $self == [FiniteVerb]:
                /{en_3sg({$self}).phon.ret}/
```

branch 依來源序求值。Unmatched 繼續；硬 category/constraint blocking 記錄 `CASE_MORE_SPECIFIC_BLOCKED` 後繼續；Error 立即停止，不能落入 `else`。`Else:` 可讀，canonical 一律輸出 `else:`。

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
- 未填 operand 的 constraint 保存在 `PartialSign`；飽和或進入 pure phon boundary 前完成檢查。
- `before` cycle、unknown slot、domain mismatch、value conflict 與 template order conflict 都是 coded error。

package／inheritance priority 只解決 source declaration 衝突，不用來暗中阻擋 runtime construction。

## 6. Construction competition

`CompiledSystem::derive_candidates(category, fillers, mapping, context)` 回傳所有 category、slot signature 與 constraints 相容的候選，依 stable `SignId` 排序。候選多於一個時 `derive_category` 回報 `AMBIGUOUS_CONSTRUCTION`。

選擇必須明示：deterministic selector 由 caller 提供 candidate `SignId`；`SampleEntrenchment { seed }` 才依非負有限 entrenchment 抽樣，0 不參與。trace 保存 seed、stable order、weights 與 selected ID；未指定 selector 時不以 priority 或 entrenchment 偷選。

## 7. Requirement-to-evidence

| Requirement | Runtime evidence |
|---|---|
| V1 source/dump 不變 | 全既有 round-trip、golden、identity tests |
| Sign application 回傳完整 Sign | `fp_v2::sign_application_returns_a_full_typed_sign` |
| PartialSign 可續填且 immutable | `fp_v2::partial_sign_can_be_resumed_without_mutating_the_original` |
| typed case default／type boundary | V2 round-trip、V1 rejection、sign/phon runtime tests |
| projection 先求完整 Sign | `fp_v2::phon_projection_evaluates_the_full_sign_before_extracting_phon` |
| 巢狀 application 參與解析與 cycle graph | `fp_v2::nested_applications_participate_in_static_resolution_and_cycle_checks` |
| equal/order constraints | `fp_v2::binary_constraints_execute_at_application` 與八個 `std/cxg` recipe E2E |
| competition 不暗選 | `fp_v2::competition_returns_all_candidates_and_samples_deterministically` |
| identity-preserving migration | `fp_v2::explicit_document_migration_preserves_existing_node_ids` |
| English abstract/concrete migration | English count、case、copula、do-support 與 12-construction E2E |

## 8. 鎖定邊界

V2 沒有 arbitrary cross-dimension assignment、conditional optional slot、implementation fragment、n-ary order primitive、automatic competition blocking 或隱式 stochastic choice。feature/role expression 的 generic AST type已保留，但本版公開 parser 的 executable vertical slice 是 Sign body 與 phon realization；新增 expression position 必須沿用同一 expected-type checker，不得另加字串後門。
