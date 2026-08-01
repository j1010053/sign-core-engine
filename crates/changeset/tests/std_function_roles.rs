use conlang_changeset::function::{
    evaluate_function_offline, functions_from_packages, load_functions, FunctionBody, FunctionCall,
    FunctionError, FunctionErrorClass, FunctionEvaluation,
};
use conlang_changeset::{change_set_prelude, ChangeInterpreter, ReplayError, UnresolvedChangeSet};
use conlang_language::{
    LanguageDocument, LibraryCatalog, LibraryExport, LibraryExportKind, LibraryFunctionSource,
    LibraryId, LibraryKind, LibraryPackage, LibrarySpec, SignItem,
};

const SOURCE: &str = r#"trait MotionVerb:
    belongs Verb

sign go:
    belongs MotionVerb
    entrenchment = 0.2
    syn:
        category = verb
    sem:
        senses:
            core = GO

sign finish:
    belongs MotionVerb
    entrenchment = 0.2
    syn:
        category = verb
    sem:
        senses:
            core = FINISH

sign stone:
    belongs Noun
    entrenchment = 0.2
    syn:
        category = noun
    sem:
        senses:
            core = STONE
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("fixture parses")
}

fn invocation(name: &str, sign: &str) -> FunctionCall {
    FunctionCall {
        name: name.to_owned(),
        positional: Some(format!("sign(\"{sign}\")")),
        named: Vec::new(),
    }
}

#[test]
fn std_recipe_and_goal_are_independent_definition_documents() {
    let catalog = LibraryCatalog::embedded().expect("embedded catalog");
    let package = catalog
        .packages()
        .iter()
        .find(|package| package.id.to_string() == "std:grammaticalization")
        .expect("std function package");
    assert_eq!(
        package.function_paths,
        ["code/recipes.chg", "code/goals.chg"]
    );
    assert_eq!(
        package
            .function_sources
            .iter()
            .map(|source| source.path.as_str())
            .collect::<Vec<_>>(),
        ["code/recipes.chg", "code/goals.chg"]
    );

    let table = load_functions(&catalog, &LibrarySpec::default()).expect("loads both documents");
    assert!(matches!(
        table.get("VerbToTense").unwrap().body,
        FunctionBody::Sequence(_)
    ));
    assert!(matches!(
        table.get("Future").unwrap().body,
        FunctionBody::Choose(_)
    ));
    assert!(matches!(
        table.get("Perfect").unwrap().body,
        FunctionBody::Choose(_)
    ));
}

#[test]
fn function_body_not_file_name_determines_the_evaluation_boundary() {
    const SEQUENCE: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function SequenceInGoals(x):
    entrench(x, delta: 0.1)
"#;
    const CANDIDATES: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function CandidatesInRecipes(x):
    choose:
        SequenceInGoals(x)
"#;
    let mut package = synthetic("", &["SequenceInGoals", "CandidatesInRecipes"]);
    package.function_paths = vec!["code/goals.chg".to_owned(), "code/recipes.chg".to_owned()];
    package.function_sources = vec![
        LibraryFunctionSource {
            path: "code/goals.chg".to_owned(),
            source: SEQUENCE,
        },
        LibraryFunctionSource {
            path: "code/recipes.chg".to_owned(),
            source: CANDIDATES,
        },
    ];
    let table = functions_from_packages(&[&package]).unwrap();
    assert!(matches!(
        table.get("SequenceInGoals").unwrap().body,
        FunctionBody::Sequence(_)
    ));
    assert!(matches!(
        table.get("CandidatesInRecipes").unwrap().body,
        FunctionBody::Choose(_)
    ));
}

#[test]
fn std_recipe_executes_in_order_on_a_temporary_language() {
    let catalog = LibraryCatalog::embedded().unwrap();
    let table = load_functions(&catalog, &LibrarySpec::default()).unwrap();
    let mut call = invocation("VerbToTense", "go");
    call.named.push(("tense".to_owned(), "FUTURE".to_owned()));
    call.named
        .push(("result_category".to_owned(), "Aux".to_owned()));

    let FunctionEvaluation::Executed(execution) =
        evaluate_function_offline(&table, &call, &base(), &LibrarySpec::default()).unwrap()
    else {
        panic!("a sequence must execute rather than return candidates");
    };
    assert_eq!(
        execution
            .trace
            .iter()
            .map(|step| step.call.name.as_str())
            .collect::<Vec<_>>(),
        ["drift", "reanalyze", "entrench"]
    );
    let source = execution.document.source();
    assert!(source.contains("core = FUTURE"), "{source}");
    assert!(source.contains("belongs Aux"), "{source}");
    assert!(source.contains("entrenchment = 0.5"), "{source}");
    assert!(!execution.edits.is_empty());
}

#[test]
fn std_goal_returns_candidates_without_executing_them() {
    let catalog = LibraryCatalog::embedded().unwrap();
    let table = load_functions(&catalog, &LibrarySpec::default()).unwrap();
    let document = base();
    let original = document.source().to_owned();

    let FunctionEvaluation::Candidates(candidates) = evaluate_function_offline(
        &table,
        &invocation("Future", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .unwrap() else {
        panic!("`when:` must stop at the candidate boundary");
    };
    assert_eq!(candidates.source, "Future");
    assert_eq!(candidates.candidates.len(), 1);
    assert_eq!(candidates.candidates[0].name, "VerbToTense");
    assert_eq!(
        candidates.candidates[0].positional.as_deref(),
        Some("sign(\"go\")")
    );
    assert_eq!(
        candidates.candidates[0].named,
        [
            ("tense".to_owned(), "FUTURE".to_owned()),
            ("result_category".to_owned(), "Aux".to_owned())
        ]
    );
    assert_eq!(document.source(), original);

    let error = evaluate_function_offline(
        &table,
        &invocation("Future", "stone"),
        &document,
        &LibrarySpec::default(),
    )
    .expect_err("a non-Verb is a near miss");
    assert!(format!("{error}").contains("requires [Verb]"), "{error}");
}

#[test]
fn perfect_goal_can_select_verb_to_bound_tense_marker() {
    let catalog = LibraryCatalog::embedded().unwrap();
    let table = load_functions(&catalog, &LibrarySpec::default()).unwrap();
    let document = base();
    let original = document.source().to_owned();
    let original_id = document
        .language()
        .signs
        .iter()
        .find(|sign| sign.name == "finish")
        .unwrap()
        .id
        .clone();

    let FunctionEvaluation::Candidates(candidates) = evaluate_function_offline(
        &table,
        &invocation("Perfect", "finish"),
        &document,
        &LibrarySpec::default(),
    )
    .unwrap() else {
        panic!("a Goal must stop before selecting its Recipe");
    };
    assert_eq!(candidates.source, "Perfect");
    assert_eq!(candidates.candidates.len(), 1);
    let candidate = &candidates.candidates[0];
    assert_eq!(candidate.name, "VerbToTense");
    assert_eq!(candidate.positional.as_deref(), Some("sign(\"finish\")"));
    assert_eq!(
        candidate.named,
        [
            ("tense".to_owned(), "PERFECT".to_owned()),
            ("result_category".to_owned(), "Bound".to_owned())
        ]
    );
    assert_eq!(document.source(), original, "a Goal cannot mutate state");

    let FunctionEvaluation::Executed(execution) =
        evaluate_function_offline(&table, candidate, &document, &LibrarySpec::default()).unwrap()
    else {
        panic!("the explicitly selected Recipe must execute");
    };
    assert_eq!(
        execution
            .trace
            .iter()
            .map(|step| step.call.name.as_str())
            .collect::<Vec<_>>(),
        ["drift", "reanalyze", "entrench"]
    );
    let evolved = execution
        .document
        .language()
        .signs
        .iter()
        .find(|sign| sign.name == "finish")
        .unwrap();
    assert_eq!(
        evolved.id, original_id,
        "reanalysis preserves sign identity"
    );
    assert!(evolved.items.iter().any(|item| matches!(
        item,
        SignItem::Sense(sense) if sense.name == "core" && sense.gloss == "PERFECT"
    )));
    // 重分析搬的是 `belongs`(範疇 = 本體樹成員關係),不是一個裸 `syn.category` def。
    assert!(evolved
        .items
        .iter()
        .any(|item| matches!(item, SignItem::Belongs(name) if name == "Bound")));
    assert!(
        !evolved
            .items
            .iter()
            .any(|item| matches!(item, SignItem::Belongs(name) if name == "MotionVerb")),
        "舊範疇必須被取代,不是並存"
    );
    assert!(evolved.items.iter().any(|item| matches!(
        item,
        SignItem::Def(definition)
            if definition.path == "entrenchment" && definition.value == "0.5"
    )));
    let source = execution.document.source();
    assert!(source.contains("core = PERFECT"), "{source}");
    assert!(source.contains("belongs Bound"), "{source}");
    assert!(source.contains("entrenchment = 0.5"), "{source}");
}

fn synthetic(functions: &'static str, exported: &[&str]) -> LibraryPackage {
    let id = LibraryId::new(LibraryKind::Plugin, "fixture");
    LibraryPackage {
        name: id.name.clone(),
        rule_namespace: id.to_string(),
        version: "test".to_owned(),
        enabled: true,
        priority: 0,
        requires: Vec::new(),
        code_paths: Vec::new(),
        code_path: String::new(),
        data_path: "data/none.tsv".to_owned(),
        data_paths: vec!["data/none.tsv".to_owned()],
        data_sources: Vec::new(),
        function_paths: vec!["code/functions.chg".to_owned()],
        function_sources: Vec::new(),
        exports: exported
            .iter()
            .map(|alias| LibraryExport {
                package: id.name.clone(),
                package_id: id.clone(),
                stable_id: format!("plugin:fixture:{alias}"),
                kind: LibraryExportKind::Function,
                alias: (*alias).to_owned(),
            })
            .collect(),
        id,
        code: "",
        functions,
        data: "",
    }
}

#[test]
fn sequential_handoff_is_observable_and_failure_is_atomic() {
    const FUNCTIONS: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function Reinforce(x [Verb]):
    entrench(x, delta: 0.2)
    entrench(x, delta: 0.2)

function Broken(x [Verb]):
    entrench(x, delta: 0.2)
    reanalyze(sign("missing"), target: category, to: Aux)
"#;
    let package = synthetic(FUNCTIONS, &["Reinforce", "Broken"]);
    let table = functions_from_packages(&[&package]).unwrap();
    let document = base();

    let FunctionEvaluation::Executed(execution) = evaluate_function_offline(
        &table,
        &invocation("Reinforce", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .unwrap() else {
        panic!("sequence executes");
    };
    assert!(
        execution.document.source().contains("entrenchment = 0.6"),
        "the second call must read the first call's 0.4 result:\n{}",
        execution.document.source()
    );

    let original = document.source().to_owned();
    assert!(evaluate_function_offline(
        &table,
        &invocation("Broken", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .is_err());
    assert_eq!(
        document.source(),
        original,
        "failure cannot commit partially"
    );
}

/// **靜態形狀 ⟺ 求值結果形狀**——只有 `choose:` 回候選,其餘三種都回已執行。
///
/// ## 為什麼要單獨釘住這條
///
/// 「Recipe 與 Goal 的差別」在本系統裡表達了兩次,都不靠關鍵字(P48):
///
/// | | 讀得到的地方 | 用途 |
/// |---|---|---|
/// | 靜態 | `table.get(name)?.body` 是不是 `Choose` | 引擎在**求值前**判定;應用層據此決定要不要準備選擇介面 |
/// | 動態 | 求值回 `Candidates` 還是 `Executed` | 型別強迫呼叫端處理,忘不掉 |
///
/// 引擎的三個 dispatch 點(`.chg` 語句層、序列內的一行、`choose:` 的候選層級檢查)
/// **一律用靜態那一欄**,所以「靜態說得準」是整個機制的前提。這兩欄各自有斷言,
/// 對應本身卻沒有——哪天 `when:` 又開始回候選,靜態判定就會靜默說謊。
///
/// 判別性:四種 body 各一個,而且**兩種結局都必須出現**。只放 `choose:` 的話,
/// 「一律回 Candidates」會通過。
#[test]
fn the_body_shape_predicts_the_evaluation_shape() {
    const ALL_FOUR: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function Runs(x [Verb]):
    entrench(x, delta: 0.1)

function Picks(x [Verb]):
    case:
        entrench(x, delta: 0.2) / x.syn.category == verb
        else entrench(x, delta: 0.3)

function Accumulates(x [Verb]):
    when:
        entrench(x, delta: 0.1) / x.syn.category == verb

function Offers(x [Verb]):
    choose:
        Runs(x)
"#;
    let package = synthetic(ALL_FOUR, &["Runs", "Picks", "Accumulates", "Offers"]);
    let table = functions_from_packages(&[&package]).expect("loads");
    let document = base();

    let mut saw_candidates = false;
    let mut saw_executed = false;
    for name in ["Runs", "Picks", "Accumulates", "Offers"] {
        let is_choose = matches!(table.get(name).unwrap().body, FunctionBody::Choose(_));
        let evaluation = evaluate_function_offline(
            &table,
            &invocation(name, "go"),
            &document,
            &LibrarySpec::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        match evaluation {
            FunctionEvaluation::Candidates(_) => {
                assert!(is_choose, "{name}: 回候選的 body 必須是 `choose:`");
                saw_candidates = true;
            }
            FunctionEvaluation::Executed(_) => {
                assert!(!is_choose, "{name}: `choose:` 不得直接執行");
                saw_executed = true;
            }
        }
    }
    // 前提:兩種結局都真的出現過,否則上面的迴圈是空轉。
    assert!(saw_candidates && saw_executed);
}

/// `when:` 回歸 `.lang` 的 `CaseSelection::Accumulate`——**所有成立的分支都執行**。
///
/// 這一格先前是空的:純序列不能帶 guard、`case:` 只取一個、`when:` 被候選列舉徵用,
/// 於是「**有條件的全跑**」在 function 層寫不出來,而那正是 `when` 在 `.lang` 裡的本義
/// (`system.rs` 把每個成立分支 `merged_items.extend` 進結果)。
///
/// 判別性:三個分支,兩個成立一個不成立,而且三者的 delta 互不相同——
/// 「只跑第一個」「全部都跑」「一個都不跑」會落在三個不同的數字上。
///
/// **三個 guard 都走同一條可讀路徑**(`syn.category`,由 `reanalyze` 原子改寫寫入)。
/// 中間那條先前寫成 `x.sem.senses[core].concept == NOPE`——那條路徑 guard **根本
/// 讀不到**(義項是一級節點不是 `Def`),於是它的「不成立」與值無關,把中間值改成
/// 真值也照樣不成立。現在改成同一條路徑的不同值,**改值就會改結果**。
#[test]
fn when_runs_every_branch_whose_guard_holds() {
    const ACCUMULATE: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function Both(x [Verb]):
    when:
        entrench(x, delta: 0.1) / x.syn.category == verb
        entrench(x, delta: 0.2) / x.syn.category == noun
        entrench(x, delta: 0.4) / x.syn.category == verb
"#;
    let package = synthetic(ACCUMULATE, &["Both"]);
    let table = functions_from_packages(&[&package]).expect("loads");
    let document = base();

    let FunctionEvaluation::Executed(execution) = evaluate_function_offline(
        &table,
        &invocation("Both", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .expect("evaluates") else {
        panic!("`when:` 執行,不列舉");
    };
    // go 的起始 entrenchment 是 0.2。第一、三個 guard 成立(各 +0.1、+0.4),
    // 第二個不成立 ⇒ 0.2 + 0.1 + 0.4 = 0.7。
    // 只跑第一個 ⇒ 0.3;全跑不篩 ⇒ 0.9;一個都不跑 ⇒ 0.2。三者都會紅。
    assert!(
        execution.document.source().contains("entrenchment = 0.7"),
        "{}",
        execution.document.source()
    );
}

/// `when:` 一個分支都不成立 ⇒ **無操作,不報錯**。
///
/// 比照 `.lang` 的 `!any_matched => return Ok(current)`。與 `case:` 的
/// `CaseNoBranch` 刻意不同:`case:` 的契約是「選一個」,選不出來是作者漏了兜底;
/// `when:` 的契約是「成立的都做」,一個都不成立就是「這次沒事要做」。
#[test]
fn when_with_no_matching_branch_is_a_no_op_not_an_error() {
    const NONE_MATCH: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function Quiet(x [Verb]):
    when:
        entrench(x, delta: 0.5) / x.syn.category == noun
"#;
    let package = synthetic(NONE_MATCH, &["Quiet"]);
    let table = functions_from_packages(&[&package]).expect("loads");
    let document = base();

    let FunctionEvaluation::Executed(execution) = evaluate_function_offline(
        &table,
        &invocation("Quiet", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .expect("一個都不成立不是錯誤") else {
        panic!("`when:` 執行");
    };
    assert!(execution.edits.is_empty(), "{:?}", execution.edits);
    assert_eq!(execution.document.source(), document.source());
}

/// `else` = **只在前面的分支都不成立時**生效(`.lang` 的 `CaseCondition::Else =>
/// !any_matched`)。
///
/// ## 誌誤
///
/// 先前 `else X` 與裸 `X` 都記成 `guard: None` 且**都恆成立**,`else` 只是裝飾。
/// 在 `case:`(取第一個)下兩種讀法結果相同,所以一直沒被發現;在 `when:`/`choose:`
/// (全取)下**不同**——舊行為會讓 else 跟著一起跑。
///
/// 判別性:同一個 fixture 的兩半,差別只在第一個 guard 成不成立,而三種讀法落在
/// 三個不同的數字上(見各自的註解)。
/// **Frozen matching**——所有 guard 只讀同一份比對前的文件。
///
/// 規格《`case`、`when` 與 Context Fragment(V2)》§`when:` 第 2 條:
///
/// > 所有非 `else` guard 都只讀同一份 snapshot;先前命中的 fragment 對後續 guard
/// > 不可見。
///
/// ## 誌誤
///
/// 把 `when:` 改回 accumulate 時,第一版是**邊比對邊執行**,於是後面的 guard 讀得到
/// 前面分支的結果。這條 fixture 就是那個洩漏的最短形:第一個分支把 `go` 從 verb
/// reanalyze 成 aux,第二個分支的 guard 問「是不是 aux」。
///
/// 判別性:凍結 ⇒ 第二個 guard 讀的是 verb,不成立,entrenchment 維持 0.2;
/// 洩漏 ⇒ 讀到 aux,成立,0.2 + 0.5 = 0.7。兩者差一個數字。
#[test]
fn when_guards_all_read_the_document_as_it_was_before_any_branch_ran() {
    const LEAKY: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function Chain(x [Verb]):
    when:
        reanalyze(x, target: category, to: Aux) / x.syn.category == verb
        entrench(x, delta: 0.5) / x.syn.category == aux
"#;
    let package = synthetic(LEAKY, &["Chain"]);
    let table = functions_from_packages(&[&package]).expect("loads");
    let document = base();
    let FunctionEvaluation::Executed(execution) = evaluate_function_offline(
        &table,
        &invocation("Chain", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .expect("evaluates") else {
        panic!("`when:` 執行");
    };
    let rendered = execution.document.source();
    // 第一個分支確實跑了(category 變成 aux),證明 fixture 不是空轉。
    assert!(rendered.contains("belongs Aux"), "{rendered}");
    // 第二個分支不得跑:它的 guard 讀的是比對前的 verb。
    assert!(
        rendered.contains("entrenchment = 0.2"),
        "guard 讀到了前一個分支的結果:\n{rendered}"
    );
    assert_eq!(execution.edits.len(), 1, "{:?}", execution.edits);
}

#[test]
fn else_only_fires_when_no_earlier_branch_matched() {
    const FALLBACK: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function Matched(x [Verb]):
    when:
        entrench(x, delta: 0.1) / x.syn.category == verb
        else entrench(x, delta: 0.5)

function Unmatched(x [Verb]):
    when:
        entrench(x, delta: 0.1) / x.syn.category == noun
        else entrench(x, delta: 0.5)
"#;
    let package = synthetic(FALLBACK, &["Matched", "Unmatched"]);
    let table = functions_from_packages(&[&package]).expect("loads");
    let document = base();
    let run = |name: &str| {
        let FunctionEvaluation::Executed(execution) = evaluate_function_offline(
            &table,
            &invocation(name, "go"),
            &document,
            &LibrarySpec::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}")) else {
            panic!("`when:` 執行");
        };
        execution.document.source().to_owned()
    };

    // 第一個成立 ⇒ else **不補位**:0.2 + 0.1 = 0.3。
    // 舊行為(else 恆成立)會是 0.2 + 0.1 + 0.5 = 0.8。
    assert!(
        run("Matched").contains("entrenchment = 0.3"),
        "{}",
        run("Matched")
    );

    // 第一個不成立 ⇒ else 補位:0.2 + 0.5 = 0.7。
    // 若 else 改成「永不成立」則是 0.2 —— 兩個方向的退化都抓得到。
    assert!(
        run("Unmatched").contains("entrenchment = 0.7"),
        "{}",
        run("Unmatched")
    );
}

/// 裸呼叫是 `Always`,**計入 `any_matched`** —— 它成立了,後面的 `else` 就不該補位。
///
/// 這條把 `Always` 與 `Else` 的差別釘死:若兩者又被塌成同一個,不論塌成哪一邊,
/// 數字都會變。
#[test]
fn an_unconditional_branch_suppresses_a_later_else() {
    const BARE_THEN_ELSE: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function BareFirst(x [Verb]):
    when:
        entrench(x, delta: 0.1) / x.syn.category == noun
        entrench(x, delta: 0.2)
        else entrench(x, delta: 0.5)
"#;
    let package = synthetic(BARE_THEN_ELSE, &["BareFirst"]);
    let table = functions_from_packages(&[&package]).expect("loads");
    let document = base();
    let FunctionEvaluation::Executed(execution) = evaluate_function_offline(
        &table,
        &invocation("BareFirst", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .expect("evaluates") else {
        panic!("`when:` 執行");
    };
    // guard 不成立(跳過)+ 裸呼叫成立(+0.2)⇒ else 不補位:0.2 + 0.2 = 0.4。
    // 若裸呼叫不計入 any_matched,else 會跟著跑 ⇒ 0.9。
    assert!(
        execution.document.source().contains("entrenchment = 0.4"),
        "{}",
        execution.document.source()
    );
}

/// 對照:`case:` 的 `else` 行為**不變**。取第一個成立者,走到 else 時前面必然都沒
/// 成立,故 `!any_matched` 恆為真——與 `.lang` 的 `Else => Ok(true)` 自然一致。
/// 沒有這條,上面的改動可能悄悄動到 `case:`。
#[test]
fn case_else_still_fires_as_the_fallback() {
    const CASE_ELSE: &str = r#"package plugin:fixture:
    schema = conlang.functions/v1

function Pick(x [Verb]):
    case:
        entrench(x, delta: 0.1) / x.syn.category == noun
        else entrench(x, delta: 0.5)
"#;
    let package = synthetic(CASE_ELSE, &["Pick"]);
    let table = functions_from_packages(&[&package]).expect("loads");
    let document = base();
    let FunctionEvaluation::Executed(execution) = evaluate_function_offline(
        &table,
        &invocation("Pick", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .expect("evaluates") else {
        panic!("`case:` 執行");
    };
    assert!(
        execution.document.source().contains("entrenchment = 0.7"),
        "{}",
        execution.document.source()
    );
}

#[test]
fn calling_a_goal_from_a_changeset_is_broken_input_not_a_conflict() {
    let document = base();
    let libraries = LibrarySpec::default();
    for argument in ["go", "stone"] {
        let mut source =
            change_set_prelude(&document, &libraries, "evo:goal-in-chg").expect("prelude");
        source.push_str(&format!("\n    #0:\n        Future(sign({argument:?}))\n"));
        let error = UnresolvedChangeSet::parse(&source)
            .expect("parses")
            .resolve(&document, &libraries)
            .expect_err("Goal 不能寫在 .chg 裡");
        assert!(
            matches!(
                &error,
                ReplayError::Function {
                    source: FunctionError::CandidatesRequireSelection { .. },
                    ..
                }
            ),
            "{argument}: {error:?}"
        );
        assert_eq!(
            FunctionError::CandidatesRequireSelection {
                function: "Future".to_owned()
            }
            .class(),
            FunctionErrorClass::Broken
        );
    }

    // 近似反例:非 Goal 的**真正**引數錯誤不得被靜態檢查吞掉,仍須報約束不符。
    let mut source = change_set_prelude(&document, &libraries, "evo:recipe-bad-arg").unwrap();
    source.push_str(
        "\n    #0:\n        VerbToTense(sign(\"stone\"), tense: FUTURE, result_category: Aux)\n",
    );
    let error = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&document, &libraries)
        .expect_err("stone 不是 Verb");
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
fn changeset_dispatches_recipe_and_stops_at_goal_candidates() {
    let document = base();
    let libraries = LibrarySpec::default();
    let mut recipe_source = change_set_prelude(&document, &libraries, "evo:std-recipe").unwrap();
    recipe_source.push_str(
        "\n    #0:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: Aux)\n",
    );
    let recipe = UnresolvedChangeSet::parse(&recipe_source)
        .unwrap()
        .resolve(&document, &libraries)
        .expect("std recipe resolves through the public changeset path");
    let outcome = ChangeInterpreter::new(document.clone(), libraries.clone(), "evo:std-recipe")
        .unwrap()
        .run(&recipe)
        .unwrap();
    assert!(outcome.document.source().contains("core = FUTURE"));
    assert!(outcome.document.source().contains("belongs Aux"));

    let mut goal_source = change_set_prelude(&document, &libraries, "evo:std-goal").unwrap();
    goal_source.push_str("\n    #0:\n        Future(sign(\"go\"))\n");
    let error = UnresolvedChangeSet::parse(&goal_source)
        .unwrap()
        .resolve(&document, &libraries)
        .expect_err("Goal selection belongs to the caller, not implicit resolution");
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::CandidatesRequireSelection { .. },
                ..
            }
        ),
        "{error}"
    );
}
