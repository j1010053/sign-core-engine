//! Step 17 缺口 1／2 —— **guarded Recipe／`case:` 求值** 與 **Goal 的 guarded 候選篩選**。
//!
//! 兩個缺口在實作上是同一件事:`FunctionBody` 的三形都卡在同一個「guard 不支援」的
//! 拒絕點,補上 guard 求值就一起通了。差別只在 `case:` 取第一個成立的、`choose:` 取
//! 全部成立的。
//!
//! ## 求值語意的來源
//!
//! guard 沿用 **`.lang` 規則既有的 `/ guard`**(修補10 §3),差別只在主體是**參數名**
//! 而非 `$self`。故實作是「代換成 `$self` 後交給既有求值器」,不是另寫一套述詞語言
//! ——兩套的走鐘是**無聲的**(同一句 guard 共時一個意思、歷時另一個意思)。
//!
//! ## 本檔特別要證明的一條
//!
//! 修補10 §11.2:「guard 到 **invoke 時才在實際 base 上求值**,定義檔即完全
//! base-independent」。故有一個測試把**同一個套件**餵給**兩份不同的文件**,
//! 斷言結果不同——那是「讀的是活文件」的唯一直接證據。

use conlang_changeset::function::{
    evaluate_function_offline, functions_from_packages, FunctionCall, FunctionError,
    FunctionEvaluation,
};
use conlang_changeset::ReplayError;
use conlang_language::{
    LanguageDocument, LibraryExport, LibraryExportKind, LibraryId, LibraryKind, LibraryPackage,
    LibrarySpec,
};

/// `go`/`finish` 是動詞、`stone` 是名詞;`telic` **只寫在 trait 上**,
/// 讓 guard 能證明它看得到繼承下來的內容。
const SOURCE: &str = r#"trait MotionVerb:
    belongs Verb
    syn:
        telic = no

sign go:
    belongs MotionVerb
    entrenchment = 0.2
    syn:
        category = verb

sign finish:
    belongs MotionVerb
    entrenchment = 0.2
    syn:
        category = verb

sign stone:
    belongs Noun
    entrenchment = 0.2
    syn:
        category = noun
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("fixture parses")
}

fn synthetic(functions: &'static str, exported: &[&str]) -> LibraryPackage {
    let id = LibraryId::new(LibraryKind::Plugin, "guards");
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
                stable_id: format!("plugin:guards:{alias}"),
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

fn call(name: &str, sign: &str) -> FunctionCall {
    FunctionCall {
        name: name.to_owned(),
        positional: Some(format!("sign(\"{sign}\")")),
        named: Vec::new(),
    }
}

fn run(
    functions: &'static str,
    exported: &[&str],
    invocation: &FunctionCall,
    document: &LanguageDocument,
) -> Result<FunctionEvaluation, conlang_changeset::ReplayError> {
    let package = synthetic(functions, exported);
    let table = functions_from_packages(&[&package]).expect("functions load");
    evaluate_function_offline(&table, invocation, document, &LibrarySpec::default())
}

fn executed(evaluation: FunctionEvaluation) -> (String, Vec<String>) {
    match evaluation {
        FunctionEvaluation::Executed(execution) => (
            execution.document.source(),
            execution
                .trace
                .iter()
                .map(|step| step.call.name.clone())
                .collect(),
        ),
        FunctionEvaluation::Candidates(candidates) => {
            panic!(
                "expected execution, got candidates from {:?}",
                candidates.source
            )
        }
    }
}

fn candidates(evaluation: FunctionEvaluation) -> Vec<String> {
    match evaluation {
        FunctionEvaluation::Candidates(candidates) => candidates
            .candidates
            .iter()
            .map(|call| call.name.clone())
            .collect(),
        FunctionEvaluation::Executed(_) => panic!("expected candidates, got an execution"),
    }
}

// ── A. header guard(缺口 1)────────────────────────────────────────────────

const HEADER_GUARD: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function OnlyVerbs(x) / x.syn.category == verb:
    entrench(x, delta: 0.3)
"#;

#[test]
fn a_header_guard_that_holds_lets_the_function_run() {
    let document = base();
    let (source, trace) = executed(
        run(
            HEADER_GUARD,
            &["OnlyVerbs"],
            &call("OnlyVerbs", "go"),
            &document,
        )
        .expect("guard holds"),
    );
    assert_eq!(trace, vec!["entrench"], "body 真的跑了");
    assert!(source.contains("entrenchment = 0.5"), "{source}");
}

#[test]
fn a_header_guard_that_fails_is_an_error_not_a_silent_no_op() {
    // **與姊妹機制一致**:修補10 §3 說參數約束「取代大部分 guard」,而約束不符時是
    // `Err`。若 guard 改成「不符就當沒呼叫過」,同一個條件寫成約束會擋、寫成 guard
    // 會靜默略過 —— 那是兩套行為。
    let document = base();
    let error = run(
        HEADER_GUARD,
        &["OnlyVerbs"],
        &call("OnlyVerbs", "stone"),
        &document,
    )
    .expect_err("stone 不是動詞");
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::GuardUnsatisfied { .. },
                ..
            }
        ),
        "{error}"
    );
}

// ── B. `case:` 取第一個成立的(缺口 1)──────────────────────────────────────

const CASE_ORDER: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Nudge(x):
    case:
        Small(x) / x.syn.category == verb
        Large(x) / x.syn.telic == no
        else Fallback(x)

function Small(x):
    entrench(x, delta: 0.1)

function Large(x):
    entrench(x, delta: 0.9)

function Fallback(x):
    attrit(x, delta: 0.1)
"#;

#[test]
fn case_takes_the_first_matching_branch_not_merely_some_matching_branch() {
    // **判別性**:`go` 讓**兩個** guard 都成立(它是動詞,且從 trait 繼承到 telic=no)。
    // 只驗「有跑到某個分支」分不出 first-match 與 last-match/任意;必須指名是第一個。
    let document = base();
    let (source, trace) =
        executed(run(CASE_ORDER, &["Nudge"], &call("Nudge", "go"), &document).expect("evaluates"));
    assert_eq!(trace, vec!["entrench"]);
    assert!(
        source.contains("entrenchment = 0.3"),
        "第一個分支 Small(+0.1),不是 Large(+0.9):{source}"
    );
}

#[test]
fn case_falls_through_to_else_when_no_guard_holds() {
    let document = base();
    let (source, trace) = executed(
        run(CASE_ORDER, &["Nudge"], &call("Nudge", "stone"), &document).expect("evaluates"),
    );
    assert_eq!(trace, vec!["attrit"], "走到 else");
    assert!(source.contains("entrenchment = 0.1"), "{source}");
}

#[test]
fn case_without_a_matching_branch_and_without_else_is_rejected() {
    const NO_ELSE: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Nudge(x):
    case:
        Small(x) / x.syn.category == verb

function Small(x):
    entrench(x, delta: 0.1)
"#;
    let document = base();
    let error = run(NO_ELSE, &["Nudge"], &call("Nudge", "stone"), &document)
        .expect_err("沒有分支成立且沒有兜底");
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::CaseNoBranch { .. },
                ..
            }
        ),
        "{error}"
    );
}

// ── C. `choose:` 篩選候選(缺口 2)──────────────────────────────────────────

const CHOOSE_FILTER: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Options(x):
    choose:
        Bleach(x) / x.syn.category == verb
        Harden(x) / x.syn.category == noun
        Always(x)

function Bleach(x):
    entrench(x, delta: 0.1)

function Harden(x):
    entrench(x, delta: 0.2)

function Always(x):
    entrench(x, delta: 0.3)
"#;

#[test]
fn choose_yields_only_the_candidates_whose_guard_holds() {
    // **這就是缺口 2**。三個候選,`go` 只讓第一個 guard 成立;無 guard 的恆成立。
    // 只驗「回傳非空」測不出篩選有沒有發生 —— 必須指名回傳的是哪些、順序為何。
    let document = base();
    let names = candidates(
        run(
            CHOOSE_FILTER,
            &["Options"],
            &call("Options", "go"),
            &document,
        )
        .expect("evaluates"),
    );
    assert_eq!(
        names,
        vec!["Bleach", "Always"],
        "只留 guard 成立者,且保持宣告順序"
    );
}

#[test]
fn choose_filters_differently_for_a_different_subject() {
    // 對稱案:換一個 sign,成立的是另一個 guard。同一份定義、不同輸入 → 不同候選集。
    let document = base();
    let names = candidates(
        run(
            CHOOSE_FILTER,
            &["Options"],
            &call("Options", "stone"),
            &document,
        )
        .expect("evaluates"),
    );
    assert_eq!(names, vec!["Harden", "Always"]);
}

#[test]
fn a_goal_never_executes_the_candidates_it_yields() {
    // 層級介入:`when:` **只列舉不執行**。若它偷跑了,文件會被改動。
    let document = base();
    let before = document.source();
    let evaluation = run(
        CHOOSE_FILTER,
        &["Options"],
        &call("Options", "go"),
        &document,
    )
    .expect("evaluates");
    assert!(matches!(evaluation, FunctionEvaluation::Candidates(_)));
    assert_eq!(document.source(), before, "候選列舉不得動到文件");
}

// ── D. guard 讀的是**實際 base**(修補10 §11.2)──────────────────────────────

#[test]
fn the_same_definition_evaluates_differently_against_a_different_document() {
    // **定義檔 base-independent 的唯一直接證據**。同一個套件、同一個呼叫,
    // 只換文件 —— 若 guard 是在定義時或載入時求值,兩邊會得到一樣的結果。
    const MOVED: &str = r#"trait MotionVerb:
    belongs Verb
    syn:
        telic = no

sign go:
    belongs Noun
    entrenchment = 0.2
    syn:
        category = noun
"#;
    let verbish = base();
    let nounish = LanguageDocument::import_new_root(MOVED, "evo:other").expect("parses");

    assert!(run(
        HEADER_GUARD,
        &["OnlyVerbs"],
        &call("OnlyVerbs", "go"),
        &verbish
    )
    .is_ok());
    assert!(
        run(
            HEADER_GUARD,
            &["OnlyVerbs"],
            &call("OnlyVerbs", "go"),
            &nounish
        )
        .is_err(),
        "同一個 go,在另一份文件裡是名詞 → guard 不成立"
    );
}

// ── E. guard 看得到繼承下來的內容 ─────────────────────────────────────────

#[test]
fn a_guard_reads_the_effective_sign_including_inherited_content() {
    // `telic` **只寫在 trait `MotionVerb` 上**,`go` 自己沒有這個 def。
    // 若 guard 只讀字面上的 sign,這個 guard 永遠不成立 —— 而參數約束(`[Verb]`)
    // 走的是 `belongs` 閉包、看得到繼承;兩者的可見範圍必須一致。
    const INHERITED: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Atelic(x) / x.syn.telic == no:
    entrench(x, delta: 0.3)
"#;
    let document = base();
    let (source, _) = executed(
        run(INHERITED, &["Atelic"], &call("Atelic", "go"), &document)
            .expect("繼承下來的 telic 要看得到"),
    );
    assert!(source.contains("entrenchment = 0.5"), "{source}");
}

// ── F. guard 主體的解析 ───────────────────────────────────────────────────

#[test]
fn a_guard_that_reads_no_parameter_is_rejected() {
    const NO_SUBJECT: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Odd(x) / [Verb]:
    entrench(x, delta: 0.3)
"#;
    let document = base();
    let error = run(NO_SUBJECT, &["Odd"], &call("Odd", "go"), &document).expect_err("沒有主體");
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::GuardNoSubject { .. },
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn a_guard_reading_two_parameters_is_rejected_not_guessed() {
    // `.lang` 的求值器只認**一個** `$self`。挑其中一個會靜默給出錯的答案,
    // 故明確拒絕。
    const MULTI: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Pair(x, y) / x.syn.category == y.syn.category:
    entrench(x, delta: 0.3)
"#;
    let document = base();
    let invocation = FunctionCall {
        name: "Pair".to_owned(),
        positional: Some("sign(\"go\")".to_owned()),
        named: vec![("y".to_owned(), "sign(\"finish\")".to_owned())],
    };
    let error = run(MULTI, &["Pair"], &invocation, &document).expect_err("兩個主體");
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::GuardMultiSubject { .. },
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn a_bare_parameter_name_on_the_right_hand_side_is_not_a_subject() {
    // **判別性**:`x.syn.category == y` 裡的 `y` 是**值**不是路徑頭。若主體掃描用
    // 「參數名有沒有出現」而不是「有沒有跟著 `.`」,這裡會被誤判成兩個主體而報錯。
    const RHS: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Compare(x, y) / x.syn.category == y:
    entrench(x, delta: 0.3)
"#;
    let document = base();
    let invocation = FunctionCall {
        name: "Compare".to_owned(),
        positional: Some("sign(\"go\")".to_owned()),
        named: vec![("y".to_owned(), "verb".to_owned())],
    };
    let (source, _) =
        executed(run(RHS, &["Compare"], &invocation, &document).expect("單一主體,右端是值"));
    assert!(source.contains("entrenchment = 0.5"), "{source}");
}

#[test]
fn a_guard_subject_that_is_not_a_sign_is_rejected() {
    const NOT_A_SIGN: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Odd(x) / x.syn.category == verb:
    entrench(x, delta: 0.3)
"#;
    let document = base();
    let invocation = FunctionCall {
        name: "Odd".to_owned(),
        positional: Some("trait(\"MotionVerb\")".to_owned()),
        named: Vec::new(),
    };
    let error = run(NOT_A_SIGN, &["Odd"], &invocation, &document).expect_err("不是 sign");
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::GuardSubjectNotASign { .. },
                ..
            }
        ),
        "{error}"
    );
}

// ── G. 套件私有 function call graph(缺口 1)────────────────────────────────

const WITH_PRIVATE: &str = r#"package plugin:guards:
    schema = conlang.functions/v1

function Public(x):
    Helper(x)

function Helper(x):
    entrench(x, delta: 0.4)
"#;

#[test]
fn an_exported_function_can_call_a_private_one_in_its_own_package() {
    // 先前的載入把未 export 的定義**整個丟掉**,所以套件內的呼叫圖不存在
    // ——一個 Recipe 無法把步驟拆成同套件的小函式。
    let document = base();
    let (source, trace) = executed(
        run(WITH_PRIVATE, &["Public"], &call("Public", "go"), &document).expect("私有可內部呼叫"),
    );
    assert_eq!(trace, vec!["entrench"]);
    assert!(source.contains("entrenchment = 0.6"), "{source}");
}

#[test]
fn a_private_function_is_not_reachable_from_outside() {
    // **「私有」的意義全在這一條**。若少了它,上一個測試只證明「進得了表」,
    // 而「進表但誰都看得到」不是私有,是把 export 表變成裝飾。
    let document = base();
    let error = run(WITH_PRIVATE, &["Public"], &call("Helper", "go"), &document)
        .expect_err("Helper 未 export");
    assert!(format!("{error}").contains("unknown function"), "{error}");
}

#[test]
fn one_package_cannot_call_another_packages_private_function() {
    // 跨套件:B 的私有 function 對 A 不可見,即使名字知道。
    let owner = synthetic(WITH_PRIVATE, &["Public"]);
    const CALLER: &str = r#"package plugin:caller:
    schema = conlang.functions/v1

function Outside(x):
    Helper(x)
"#;
    let caller_id = LibraryId::new(LibraryKind::Plugin, "caller");
    let mut caller = synthetic(CALLER, &["Outside"]);
    caller.name = caller_id.name.clone();
    caller.rule_namespace = caller_id.to_string();
    caller.exports = vec![LibraryExport {
        package: caller_id.name.clone(),
        package_id: caller_id.clone(),
        stable_id: "plugin:caller:Outside".to_owned(),
        kind: LibraryExportKind::Function,
        alias: "Outside".to_owned(),
    }];
    caller.id = caller_id;

    let table = functions_from_packages(&[&owner, &caller]).expect("both load");
    let error = evaluate_function_offline(
        &table,
        &call("Outside", "go"),
        &base(),
        &LibrarySpec::default(),
    )
    .expect_err("Helper 是 guards 套件的私有");
    // 訊息必須說「有定義但不可見」,不是「不是內建 rewrite」—— 後者會把人導向
    // 完全錯的方向(去查 12 個原子改寫,而問題其實在 export 表)。
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::NotVisible { .. },
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn a_cross_package_call_to_an_exported_function_works() {
    // 判別性:與上一個只差 **被呼叫者有沒有 export**。若解析根本沒在分可見性,
    // 兩個測試會同時過或同時紅。
    let owner = synthetic(WITH_PRIVATE, &["Public", "Helper"]);
    const CALLER: &str = r#"package plugin:caller:
    schema = conlang.functions/v1

function Outside(x):
    Helper(x)
"#;
    let caller_id = LibraryId::new(LibraryKind::Plugin, "caller");
    let mut caller = synthetic(CALLER, &["Outside"]);
    caller.name = caller_id.name.clone();
    caller.rule_namespace = caller_id.to_string();
    caller.exports = vec![LibraryExport {
        package: caller_id.name.clone(),
        package_id: caller_id.clone(),
        stable_id: "plugin:caller:Outside".to_owned(),
        kind: LibraryExportKind::Function,
        alias: "Outside".to_owned(),
    }];
    caller.id = caller_id;

    let table = functions_from_packages(&[&owner, &caller]).expect("both load");
    let evaluation = evaluate_function_offline(
        &table,
        &call("Outside", "go"),
        &base(),
        &LibrarySpec::default(),
    )
    .expect("Helper 已 export");
    let (source, _) = executed(evaluation);
    assert!(source.contains("entrenchment = 0.6"), "{source}");
}

#[test]
fn a_packages_own_private_function_wins_over_a_foreign_export_of_the_same_name() {
    // **判別性**:兩個套件都定義 `Helper` —— 自己的是私有(+0.4),外來的有 export(+0.1)。
    // 呼叫端寫的是自己套件裡的名字,被外來同名 export 搶走會是最難查的一種錯:
    // 程式照跑、結果不同、沒有任何訊息。
    //
    // 只驗「有跑到某個 Helper」分不出是哪一個 —— 兩者的 delta 必須不同才有判別力。
    let owner = synthetic(WITH_PRIVATE, &["Public"]);
    const FOREIGN: &str = r#"package plugin:foreign:
    schema = conlang.functions/v1

function Helper(x):
    entrench(x, delta: 0.1)
"#;
    let foreign_id = LibraryId::new(LibraryKind::Plugin, "foreign");
    let mut foreign = synthetic(FOREIGN, &["Helper"]);
    foreign.name = foreign_id.name.clone();
    foreign.rule_namespace = foreign_id.to_string();
    foreign.exports = vec![LibraryExport {
        package: foreign_id.name.clone(),
        package_id: foreign_id.clone(),
        stable_id: "plugin:foreign:Helper".to_owned(),
        kind: LibraryExportKind::Function,
        alias: "Helper".to_owned(),
    }];
    foreign.id = foreign_id;

    let table = functions_from_packages(&[&owner, &foreign]).expect("both load");
    let evaluation = evaluate_function_offline(
        &table,
        &call("Public", "go"),
        &base(),
        &LibrarySpec::default(),
    )
    .expect("resolves to its own Helper");
    let (source, _) = executed(evaluation);
    assert!(
        source.contains("entrenchment = 0.6"),
        "應走自己套件的 Helper(+0.4 → 0.6),不是外來的(+0.1 → 0.3):{source}"
    );
}
