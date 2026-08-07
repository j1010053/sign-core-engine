//! Step 17 缺口 4 —— **resolved call trace、錯誤分型、golden 與端到端驗收**
//! (《彙整05-11》§5.1 第 4 項)。
//!
//! 三件事各自的判別點:
//!
//! 1. **resolved call trace**:`.chg` 寫的是 `VerbToTense(sign("go"), …)` 一行,
//!    展開後是好幾筆原語。trace 要說得出「哪一句、哪個函數、代換後的引數是什麼」
//!    ——只斷言原語數量證明不了這件事。
//! 2. **錯誤分型**(P57 鐵律):比對**變體**,不比對訊息字串。本檔刻意不寫
//!    `format!("{error}").contains(...)`——那種斷言在錯誤全部塞進 `Parse` 時
//!    照樣會綠,正是它讓底下那個誤判活了下來。
//! 3. **端到端**:`.chg` 原文 → resolve → 直譯 → `.lang`,再走演化圖 rebase。
//!
//! ## 本檔誕生的原因(誌誤)
//!
//! 分型之前,function runtime 的每一種失敗都是 `ReplayError::Parse("TAG: …")`,
//! 而 `RebaseOutcome::classify` 把 `Parse` 判成 **`Broken`**「changeset 本身壞了」。
//! 於是「祖先把 `go` 的歸屬挪出 Verb,後代那句 `VerbToTense(sign("go"), …)` 的 `[Verb]` 約束套不上」
//! ——**最典型的一種衝突**——會叫使用者去修一個沒壞的檔案,而且連是哪一句都說不出。
//! `a_constraint_that_stopped_holding_is_a_conflict_not_broken_input` 就是那個反例。

use conlang_changeset::evolution::{Edge, EvolutionGraph, Nativization, NodeId, RebaseOutcome};
use conlang_changeset::function::{FunctionError, FunctionErrorClass};
use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, PrimitiveEdit, ReplayError, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};

/// 固定 fixture。`go` 屬 `MotionVerb`,而 `MotionVerb belongs Verb`——std 的
/// `VerbToTense(verb [Verb], …)` 靠 `belongs` 閉包才收得下它。
const SOURCE: &str = r#"trait MotionVerb:
    belongs Verb

trait Inert:

sign go:
    belongs MotionVerb
    entrenchment = 0.2
    syn:
        feature:
            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)
            category = verb
    sem:
        senses:
            core = GO

sign finish:
    belongs MotionVerb
    entrenchment = 0.2
    syn:
        feature:
            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)
            category = verb
    sem:
        senses:
            core = FINISH
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("fixture parses")
}

fn changeset_for(document: &LanguageDocument, namespace: &str, body: &str) -> String {
    let mut source =
        change_set_prelude(document, &LibrarySpec::default(), namespace).expect("prelude");
    source.push_str(body);
    source
}

fn resolve(document: &LanguageDocument, namespace: &str, body: &str) -> UnresolvedChangeSet {
    UnresolvedChangeSet::parse(&changeset_for(document, namespace, body)).expect("parses")
}

// ── ① resolved call trace ───────────────────────────────────────────────────

#[test]
fn the_resolved_trace_names_the_statement_the_stack_and_the_substituted_arguments() {
    // **判別性**:兩句、兩個不同的 sign、兩種不同的 tense。任何一項寫死
    // (句號恆 0、只記第一句、記的是形式參數而非實值)都會在此顯形。
    let document = base();
    let libraries = LibrarySpec::default();
    let resolved = resolve(
        &document,
        "evo:trace",
        "\n    #0:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: Aux)\
         \n\n    #1:\n        VerbToTense(sign(\"finish\"), tense: PERFECT, result_category: Bound)\n",
    )
    .resolve(&document, &libraries)
    .expect("std recipe resolves");

    // 兩句各展開三個原子改寫(std 的 VerbToTense body 是 drift/reanalyze/entrench)。
    assert_eq!(resolved.calls.len(), 6, "{:?}", resolved.calls);

    // 句號歸屬:前三筆屬第 0 句,後三筆屬第 1 句。
    assert_eq!(
        resolved
            .calls
            .iter()
            .map(|call| call.statement)
            .collect::<Vec<_>>(),
        [0, 0, 0, 1, 1, 1]
    );

    // 堆疊:每一筆都由 `VerbToTense` 擁有,而不是憑空出現在語句層。
    for call in &resolved.calls {
        assert_eq!(call.stack, ["VerbToTense"], "{call:?}");
    }

    // **代換後**的引數:記的必須是 `sign("go")`/`FUTURE`,不是形式參數 `verb`/`tense`。
    let first = &resolved.calls[0];
    assert_eq!(first.call.name, "drift");
    assert_eq!(first.call.positional.as_deref(), Some("sign(\"go\")"));
    assert_eq!(
        first.call.named,
        [
            ("sense".to_owned(), "core".to_owned()),
            ("gloss".to_owned(), "FUTURE".to_owned())
        ]
    );
    // 第二句換了 sign 與 tense——證明不是把第一句抄了兩遍。
    let fourth = &resolved.calls[3];
    assert_eq!(fourth.call.positional.as_deref(), Some("sign(\"finish\")"));
    assert_eq!(
        fourth.call.named,
        [
            ("sense".to_owned(), "core".to_owned()),
            ("gloss".to_owned(), "PERFECT".to_owned())
        ]
    );
}

#[test]
fn the_trace_is_observation_only_and_never_reaches_the_dump() {
    // 步驟 14 封板契約:`dump()` 只排原語。trace 若滲進 dump,round-trip 會把
    // 「觀察」重新當成「操作」replay。
    let document = base();
    let libraries = LibrarySpec::default();
    let resolved = resolve(
        &document,
        "evo:dump",
        "\n    #0:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: Aux)\n",
    )
    .resolve(&document, &libraries)
    .expect("resolves");

    assert!(!resolved.calls.is_empty());
    let dump = resolved.dump();
    assert!(
        !dump.contains("VerbToTense"),
        "trace 不得留痕於 dump:\n{dump}"
    );
    assert!(!dump.contains("drift"), "trace 不得留痕於 dump:\n{dump}");

    // dump 回讀後只剩原語 → trace 為空,而 edits 逐字相同。
    let round = UnresolvedChangeSet::parse(&dump)
        .expect("dump 可回讀")
        .resolve(&document, &libraries)
        .expect("回讀可解析");
    assert!(round.calls.is_empty(), "{:?}", round.calls);
    assert_eq!(round.dump(), dump);
}

#[test]
fn a_statement_without_any_function_call_records_no_trace() {
    // 近似反例:trace 不是「每句都塞一筆」。純原語語句必須留空,否則上面那些
    // 句號歸屬的斷言就只是在數數。
    let document = base();
    let resolved = resolve(
        &document,
        "evo:plain",
        "\n    #0:\n        update sign(\"go\").feature[syn.category].value = aux\n",
    )
    .resolve(&document, &LibrarySpec::default())
    .expect("resolves");
    assert_eq!(resolved.statements.len(), 1);
    assert!(resolved.calls.is_empty());
}

// ── ② golden ────────────────────────────────────────────────────────────────

#[test]
fn resolved_trace_and_primitive_expansion_golden() {
    // 行為測試抓「結果錯」;golden 抓「展開的**形狀**變了」——例如哪天 VerbToTense
    // 的第三步從 entrench 悄悄變成別的東西,或原語數量改變。
    let document = base();
    let resolved = resolve(
        &document,
        "evo:golden",
        "\n    #0:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: Aux)\n",
    )
    .resolve(&document, &LibrarySpec::default())
    .expect("resolves");

    let trace = resolved
        .calls
        .iter()
        .map(|call| {
            let named = call
                .call
                .named
                .iter()
                .map(|(name, value)| format!("{name}: {value}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "#{} [{}] {}({}{}{})",
                call.statement,
                call.stack.join(" > "),
                call.call.name,
                call.call.positional.as_deref().unwrap_or(""),
                if call.call.positional.is_some() && !named.is_empty() {
                    ", "
                } else {
                    ""
                },
                named
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("verb_to_tense_trace", trace);

    let kinds = resolved.statements[0]
        .edits
        .iter()
        .map(|edit| match edit {
            PrimitiveEdit::Insert { .. } => "insert",
            PrimitiveEdit::Delete { .. } => "delete",
            PrimitiveEdit::Update { .. } => "update",
            PrimitiveEdit::Move { .. } => "move",
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("verb_to_tense_primitives", kinds);
}

// ── ③ 錯誤分型 ──────────────────────────────────────────────────────────────

#[test]
fn every_function_error_declares_its_rebase_class() {
    // 分類表本身可測。`class()` 是窮盡 match,故新增變體時編譯器會擋;這裡釘的是
    // **已定案的歸屬**,改動任何一項都要說得出理由。
    let cases: &[(FunctionError, FunctionErrorClass)] = &[
        // 讀 Language 當前狀態 ⇒ 換 base 就會變 ⇒ 衝突
        (
            FunctionError::GuardUnsatisfied {
                function: "R".to_owned(),
                guard: "g".to_owned(),
            },
            FunctionErrorClass::Conflict,
        ),
        (
            FunctionError::CaseNoBranch {
                function: "R".to_owned(),
            },
            FunctionErrorClass::Conflict,
        ),
        (
            FunctionError::ConstraintUnsatisfied {
                function: "R".to_owned(),
                parameter: "verb".to_owned(),
                constraint: "Verb".to_owned(),
                sign: "go".to_owned(),
            },
            FunctionErrorClass::Conflict,
        ),
        // 住在套件裡 ⇒ 換版就沒了 ⇒ 環境變動,不該要人改 changeset
        (
            FunctionError::Unknown {
                name: "R".to_owned(),
            },
            FunctionErrorClass::Environment,
        ),
        (
            FunctionError::WeightMissing {
                goal: "G".to_owned(),
                recipe: "R".to_owned(),
            },
            FunctionErrorClass::Environment,
        ),
        // 呼叫端自己寫錯 ⇒ 換幾個 base 都一樣錯
        (
            FunctionError::CandidateLayer {
                function: "G".to_owned(),
                candidate: "R".to_owned(),
            },
            FunctionErrorClass::Broken,
        ),
        (
            FunctionError::GuardMultiSubject {
                function: "R".to_owned(),
                guard: "g".to_owned(),
            },
            FunctionErrorClass::Broken,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(error.class(), *expected, "{error:?}");
    }
}

#[test]
fn the_three_classes_reach_three_different_rebase_outcomes() {
    // 判別性:三桶各取一個,結果必須**互不相同**。若 `classify` 的新分支被寫成
    // 一律 `Broken`(也就是修好之前的行為),Conflict 與 Environment 兩支會紅。
    let conflict = RebaseOutcome::classify(ReplayError::Function {
        ordinal: Some(3),
        source: FunctionError::GuardUnsatisfied {
            function: "R".to_owned(),
            guard: "g".to_owned(),
        },
    });
    // 句號必須原封傳出來——rebase 靠它定位是哪一句。
    assert!(
        matches!(
            conflict,
            RebaseOutcome::Conflict {
                statement: Some(3),
                ..
            }
        ),
        "{conflict:?}"
    );
    assert!(matches!(
        RebaseOutcome::classify(ReplayError::Function {
            ordinal: None,
            source: FunctionError::WeightMissing {
                goal: "G".to_owned(),
                recipe: "R".to_owned(),
            },
        }),
        RebaseOutcome::Environment(_)
    ));
    assert!(matches!(
        RebaseOutcome::classify(ReplayError::Function {
            ordinal: None,
            source: FunctionError::CandidateLayer {
                function: "G".to_owned(),
                candidate: "R".to_owned(),
            },
        }),
        RebaseOutcome::Broken(_)
    ));
}

#[test]
fn a_function_failure_inside_a_statement_carries_that_statement_number() {
    // **判別性**:失敗的是第 1 句而非第 0 句,故「句號寫死成 0」或「根本沒補句號」
    // 都會被抓到。第 0 句刻意成功,證明句號不是靠「第一個失敗的一定是 0」蒙對的。
    let document = base();
    let error = resolve(
        &document,
        "evo:ordinal",
        "\n    #0:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: Aux)\
         \n\n    #1:\n        VerbToTense(sign(\"nowhere\"), tense: PERFECT, result_category: Bound)\n",
    )
    .resolve(&document, &LibrarySpec::default())
    .expect_err("nowhere 不存在");
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                ordinal: Some(1),
                source: FunctionError::ConstraintUnknownSign { .. },
            }
        ),
        "{error:?}"
    );
}

// ── ④ 端到端 ────────────────────────────────────────────────────────────────

#[test]
fn a_changeset_calling_a_std_recipe_runs_end_to_end_and_replays_deterministically() {
    let document = base();
    let libraries = LibrarySpec::default();
    let resolved = resolve(
        &document,
        "evo:e2e",
        "\n    #0:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: Aux)\n",
    )
    .resolve(&document, &libraries)
    .expect("resolves");

    let interpreter = ChangeInterpreter::new(document.clone(), libraries.clone(), "evo:e2e")
        .expect("interpreter");
    let first = interpreter.run(&resolved).expect("runs");
    let second = interpreter.run(&resolved).expect("runs again");
    assert_eq!(
        first.document.dump_pair().unwrap(),
        second.document.dump_pair().unwrap(),
        "同一份 resolved 兩次執行必須逐位元相同"
    );

    let rendered = first.document.source();
    // 三步都真的落到 `.lang` 上,而不是只在 trace 裡好看。
    assert!(rendered.contains("core = FUTURE"), "{rendered}");
    assert!(rendered.contains("belongs Aux"), "{rendered}");
    assert!(rendered.contains("entrenchment = 0.5"), "{rendered}");
    // `finish` 沒被點名,不得受影響。
    assert!(rendered.contains("core = FINISH"), "{rendered}");
}

#[test]
fn a_constraint_that_stopped_holding_is_a_conflict_not_broken_input() {
    // ── 本檔的核心反例(見檔頭誌誤)──
    //
    // n2 呼叫 `VerbToTense(sign("go"), …)`,其參數約束是 `[Verb]`。把 n2 rebase 到
    // 「祖先把 `go` 的歸屬挪出 Verb」的 n1' 上:sign 還在、名字沒變,但**約束不再
    // 成立**。這是語言狀態變了導致編輯套不上,也就是 P57 的「衝突」;分型之前它是
    // `Parse` → `Broken`「你的 changeset 壞了」——反過來要人去修一個沒壞的檔案。
    //
    // 這個構造依賴 `belongs["名字"]` 路徑選擇器(見 `belongs_addressing.rs`)。
    // 在補上那個選擇器之前,`.chg` 寫不出「改掉歸屬」的祖先,這條路徑就只證得到
    // 單元層。
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    let root = graph.add_root(base()).expect("root");
    let root_doc = graph.snapshot(&root).expect("snapshot").clone();

    let n1 = graph
        .commit(
            vec![Edge::trunk(
                root.clone(),
                changeset_for(
                    &root_doc,
                    "evo:n1",
                    "\n    #0:\n        update sign(\"go\").def[entrenchment].value = 0.3\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .expect("n1");
    let n1_doc = graph.snapshot(&n1).expect("snapshot").clone();

    let n2 = graph
        .commit(
            vec![Edge::trunk(
                n1,
                changeset_for(
                    &n1_doc,
                    "evo:n2",
                    // 第 0 句刻意只碰 `finish`——在新 base 上照樣成立,故句號
                    // 若被寫死成 0 或漏補,下面的 `Some(1)` 會紅。
                    "\n    #0:\n        update sign(\"finish\").def[entrenchment].value = 0.35\
                     \n\n    #1:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: Aux)\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .expect("n2");

    // n1':祖先把 `go` 的歸屬從 MotionVerb(→ Verb)挪到 Inert(不是 Verb)
    // → sign 還在、名字沒變,但 `[Verb]` 約束不再成立。
    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root,
                changeset_for(
                    &root_doc,
                    "evo:n1",
                    "\n    #0:\n        update sign(\"go\").belongs[\"MotionVerb\"].target = Inert\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .expect("n1'");

    let outcome = graph.rebase(&n2, &n1_prime).expect("rebase 可執行");
    let RebaseOutcome::Conflict { statement, error } = outcome else {
        panic!("約束不再成立是**衝突**,不是壞輸入,得到 {outcome:?}");
    };
    // **判別性**:衝突在第 1 句(第 0 句在新 base 上照樣成立)。
    assert_eq!(statement, Some(1), "{error}");
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::ConstraintUnsatisfied { .. },
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn a_recipe_call_that_still_holds_rebases_cleanly() {
    // 對照組:同樣的 rebase,但新 base 沒動到 `go` 的歸屬 → 乾淨。沒有這一條,
    // 上面那個測試無法區分「正確判成衝突」與「這條路徑一律失敗」。
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    let root = graph.add_root(base()).expect("root");
    let root_doc = graph.snapshot(&root).expect("snapshot").clone();

    let n1 = graph
        .commit(
            vec![Edge::trunk(
                root.clone(),
                changeset_for(
                    &root_doc,
                    "evo:n1",
                    "\n    #0:\n        update sign(\"go\").def[entrenchment].value = 0.3\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .expect("n1");
    let n1_doc = graph.snapshot(&n1).expect("snapshot").clone();
    let n2 = graph
        .commit(
            vec![Edge::trunk(
                n1,
                changeset_for(
                    &n1_doc,
                    "evo:n2",
                    "\n    #0:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: Aux)\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .expect("n2");

    // n1'' 只碰 `finish`,`go` 的 [Verb] 約束照樣成立。
    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root,
                changeset_for(
                    &root_doc,
                    "evo:n1",
                    "\n    #0:\n        update sign(\"finish\").def[entrenchment].value = 0.9\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .expect("n1''");

    let outcome = graph.rebase(&n2, &n1_prime).expect("rebase 可執行");
    let RebaseOutcome::Clean(rebased) = outcome else {
        panic!("約束仍成立 → 應乾淨,得到 {outcome:?}");
    };
    graph.verify_all().expect("rebase 產物必須通過 fsck");
    let rendered = graph.snapshot(&rebased).expect("snapshot").source();
    assert!(rendered.contains("belongs Aux"), "{rendered}");
    assert!(rendered.contains("entrenchment = 0.9"), "{rendered}");
    let _: &NodeId = &rebased;
}
