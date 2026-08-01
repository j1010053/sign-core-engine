//! P49/P50 ④ —— `.chg` **定義文件模式**。
//!
//! 定義是函數、不綁 Language,故無 base digest;以 `package` 頭與可 replay 的
//! `changeset` 頭區分。**Recipe/Goal 非關鍵字**(P48):body 語意由既有的
//! `case`(選一)/`when`(收集)/純序列(全跑)承載。

use conlang_changeset::function::{
    parse_functions, BranchCondition, FunctionBody, FUNCTIONS_SCHEMA_V1,
};

fn package(body: &str) -> String {
    format!("package std:grammaticalization:\n    schema = {FUNCTIONS_SCHEMA_V1}\n\n{body}")
}

// ── 定義文件的形狀 ────────────────────────────────────────────────────────

#[test]
fn a_definition_document_has_no_base_digest() {
    let parsed = parse_functions(&package(
        "function VerbToTense(verb [Verb], tense):\n    drift(verb, sense: core, gloss: tense)\n",
    ))
    .expect("parses");
    assert_eq!(parsed.namespace, "std:grammaticalization");
    assert_eq!(parsed.schema, FUNCTIONS_SCHEMA_V1);
    assert_eq!(parsed.functions.len(), 1);
}

#[test]
fn a_changeset_header_is_not_a_definition_document() {
    let err = parse_functions("changeset evo:x:\n    schema = conlang.changeset/v1\n")
        .expect_err("must be rejected");
    assert!(format!("{err}").contains("package"), "{err}");
}

#[test]
fn statements_are_rejected_in_a_definition_document() {
    // 定義文件只收 function;可 replay 的編輯屬 changeset。
    let err = parse_functions(&package("    statement 0:\n        delete sign(\"x\")\n"))
        .expect_err("must be rejected");
    assert!(format!("{err}").contains("functions only"), "{err}");
}

// ── 參數 ──────────────────────────────────────────────────────────────────

#[test]
fn parameters_carry_slot_style_constraints() {
    let parsed = parse_functions(&package(
        "function VerbToTense(verb [Verb], tense):\n    entrench(verb, delta: 0.3)\n",
    ))
    .unwrap();
    let params = &parsed.functions[0].params;
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "verb");
    assert_eq!(params[0].constraint.as_deref(), Some("Verb"));
    assert_eq!(params[1].name, "tense");
    assert_eq!(params[1].constraint, None, "約束是選填");
}

#[test]
fn a_header_guard_is_split_outside_the_parentheses() {
    let parsed = parse_functions(&package(
        "function F(x) / x.sem.concept == GO:\n    entrench(x, delta: 0.1)\n",
    ))
    .unwrap();
    assert_eq!(
        parsed.functions[0].guard.as_deref(),
        Some("x.sem.concept == GO")
    );
}

// ── body 的三種語意(P48:不新增 layer 標記)────────────────────────────────

#[test]
fn a_plain_sequence_means_run_all_in_order() {
    // 慣稱 Recipe。
    let parsed = parse_functions(&package(
        "function VerbToTense(verb [Verb], tense):\n    drift(verb, sense: core, gloss: tense)\n    reanalyze(verb, target: category, to: Aux)\n    entrench(verb, delta: 0.3)\n",
    ))
    .unwrap();
    match &parsed.functions[0].body {
        FunctionBody::Sequence(calls) => {
            assert_eq!(calls.len(), 3);
            assert_eq!(calls[0].name, "drift");
            assert_eq!(calls[1].name, "reanalyze");
            assert_eq!(calls[0].positional.as_deref(), Some("verb"));
            assert_eq!(
                calls[0].named,
                vec![
                    ("sense".to_owned(), "core".to_owned()),
                    ("gloss".to_owned(), "tense".to_owned())
                ]
            );
        }
        other => panic!("expected a sequence, got {other:?}"),
    }
}

#[test]
fn branch_forms_parse_into_the_three_conditions() {
    // 三種條件必須**分得開**:`/ guard`、`else`、裸呼叫。先前 `else` 與裸呼叫都記成
    // `guard: None`,`else` 因此只是裝飾——在 `when:`/`choose:` 下兩者語意不同
    // (`else` 是 `!any_matched`),塌成同一個就錯了。
    let when = parse_functions(&package(
        "function Future(target):\n    when:\n        VerbToTense(target, tense: FUTURE) / target.sem.concept == GO\n        else Auxiliary(target)\n",
    ))
    .unwrap();
    match &when.functions[0].body {
        FunctionBody::When(branches) => {
            assert_eq!(branches.len(), 2);
            assert_eq!(
                branches[0].condition,
                BranchCondition::Guard("target.sem.concept == GO".to_owned())
            );
            assert_eq!(
                branches[1].condition,
                BranchCondition::Else,
                "`else` 必須與裸呼叫分得開"
            );
            assert_eq!(branches[1].call.name, "Auxiliary");
        }
        other => panic!("expected `when`, got {other:?}"),
    }

    // 裸呼叫 = `Always`(恆成立),與 `else` 是不同的東西。
    let bare = parse_functions(&package(
        "function Any(target):\n    when:\n        A(target)\n",
    ))
    .unwrap();
    match &bare.functions[0].body {
        FunctionBody::When(branches) => {
            assert_eq!(branches[0].condition, BranchCondition::Always);
        }
        other => panic!("expected `when`, got {other:?}"),
    }

    let case = parse_functions(&package(
        "function Pick(target):\n    case:\n        A(target) / target.sem.concept == GO\n        else B(target)\n",
    ))
    .unwrap();
    assert!(matches!(case.functions[0].body, FunctionBody::Case(_)));
}

#[test]
fn an_else_branch_may_not_carry_a_guard() {
    let err = parse_functions(&package(
        "function F(x):\n    when:\n        else A(x) / x.sem.concept == GO\n",
    ))
    .expect_err("must be rejected");
    assert!(format!("{err}").contains("else"), "{err}");
}

// ── P48:循環偵測(終止性)──────────────────────────────────────────────────

#[test]
fn a_call_cycle_is_rejected() {
    let err = parse_functions(&package(
        "function A(x):\n    B(x)\n\nfunction B(x):\n    A(x)\n",
    ))
    .expect_err("A→B→A must be rejected");
    assert!(format!("{err}").contains("cycle"), "{err}");
}

#[test]
fn a_self_call_is_rejected() {
    let err = parse_functions(&package("function A(x):\n    A(x)\n"))
        .expect_err("self recursion must be rejected");
    assert!(format!("{err}").contains("cycle"), "{err}");
}

#[test]
fn functions_may_call_each_other_when_acyclic() {
    // P48:允許互相呼叫,只禁循環。
    let parsed = parse_functions(&package(
        "function Outer(x):\n    Inner(x)\n    entrench(x, delta: 0.1)\n\nfunction Inner(x):\n    attrit(x, delta: 0.1)\n",
    ))
    .expect("acyclic composition is fine");
    assert_eq!(parsed.functions.len(), 2);
}

// ── 其餘負例 ──────────────────────────────────────────────────────────────

#[test]
fn malformed_documents_are_rejected() {
    for (source, why) in [
        (package("function F(x):\n"), "empty body"),
        (package("function F(x)\n    entrench(x, delta: 1)\n"), "header missing colon"),
        // 識別字規則刻意**對齊 `.lang` 的 `ident_ok`**(允許數字開頭、禁空白),
        // 不另立更嚴的規則。
        (
            package("function F(bad name):\n    entrench(x, delta: 1)\n"),
            "parameter name contains whitespace",
        ),
        (package("function F(x [Verb):\n    entrench(x, delta: 1)\n"), "unclosed constraint"),
        (package("function F(x):\n    not a call\n"), "body line is not a call"),
        (package("function F(x):\n    when:\n"), "no branches"),
        (package("function A(x):\n    B(x)\n\nfunction A(y):\n    C(y)\n"), "duplicate name"),
        (
            "package std:g:\n    schema = conlang.functions/v2\n\nfunction F(x):\n    entrench(x, delta: 1)\n".to_owned(),
            "unsupported schema",
        ),
    ] {
        assert!(
            parse_functions(&source).is_err(),
            "expected rejection ({why})"
        );
    }
}
