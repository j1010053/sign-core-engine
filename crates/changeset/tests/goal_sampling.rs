use conlang_changeset::__lock_content_for_tests;
use conlang_changeset::function::{
    evaluate_function_offline, functions_from_packages, load_functions, load_weight_db,
    select_goal_candidate, weight_db_from_packages, FunctionCall, FunctionCandidates,
    FunctionError, FunctionEvaluation, FunctionExecution, FunctionTable, GoalSelectionTrace,
    WeightDb,
};
use conlang_changeset::ReplayError;
use conlang_language::{
    LanguageDocument, LibraryCatalog, LibraryDataSource, LibraryExport, LibraryExportKind,
    LibraryId, LibraryKind, LibraryPackage, LibrarySpec, WEIGHTED_SAMPLER_ALGORITHM,
};

const SOURCE: &str = r#"trait MotionVerb:
    belongs Verb

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

const COMPETING_FUNCTIONS: &str = r#"package plugin:sampling:
    schema = conlang.functions/v1

function Light(x [Verb]):
    entrench(x, delta: 0.1)

function Heavy(x [Verb]):
    entrench(x, delta: 0.4)

function Choose(x [Verb]):
    choose:
        Light(x)
        Heavy(x)
"#;

const COMPETING_WEIGHTS: &str = "goal\trecipe\tweight\nChoose\tLight\t1\nChoose\tHeavy\t3\n";

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:sampling").unwrap()
}

fn invocation(name: &str, sign: &str) -> FunctionCall {
    FunctionCall {
        name: name.to_owned(),
        positional: Some(format!("sign(\"{sign}\")")),
        named: Vec::new(),
    }
}

fn package(
    name: &str,
    priority: i32,
    functions: &'static str,
    weights: &'static str,
    exported: &[&str],
) -> LibraryPackage {
    let id = LibraryId::new(LibraryKind::Plugin, name);
    LibraryPackage {
        name: id.name.clone(),
        rule_namespace: id.to_string(),
        version: "test".to_owned(),
        manifest_schema: 1,
        layer: conlang_language::PackageLayer::Overlay,
        capabilities: conlang_language::PackageCapabilities {
            functions: true,
            data: true,
            ..conlang_language::PackageCapabilities::default()
        },
        source: conlang_language::PackageSource::default(),
        enabled: true,
        priority,
        requires: Vec::new(),
        code_paths: Vec::new(),
        code_path: String::new(),
        data_path: "data/weights.tsv".to_owned(),
        data_paths: vec!["data/weights.tsv".to_owned()],
        data_sources: vec![LibraryDataSource {
            path: "data/weights.tsv".to_owned(),
            source: weights.to_owned(),
        }],
        function_paths: vec!["code/functions.chg".to_owned()],
        function_sources: Vec::new(),
        exports: exported
            .iter()
            .map(|alias| LibraryExport {
                package: id.name.clone(),
                package_id: id.clone(),
                stable_id: format!("plugin:{name}:{alias}"),
                kind: LibraryExportKind::Function,
                alias: (*alias).to_owned(),
            })
            .collect(),
        id,
        code: String::new(),
        functions: functions.to_owned(),
        data: weights.to_owned(),
    }
}

/// **明寫兩步**:引擎列候選(`when:`)、選擇是**分開的一步**。
///
/// 先前這裡有個 `evaluate_goal_offline` 包裝把三步黏成一次呼叫。它與 P12 衝突
/// (Goal 的型別到 `Vec<Recipe 候選>` 為止,抽樣器是下游),也讓「零候選」沒有地方
/// 表達——列舉與選擇綁死,唯一出口就只剩抽樣器的錯。選擇屬於應用層,不屬於引擎。
fn enumerate_then_select(
    table: &FunctionTable,
    call: &FunctionCall,
    document: &LanguageDocument,
    weights: &WeightDb,
    seed: u64,
) -> (GoalSelectionTrace, FunctionExecution) {
    let FunctionEvaluation::Candidates(candidates) =
        evaluate_function_offline(table, call, document, &LibrarySpec::default()).unwrap()
    else {
        panic!("{:?} 應該產出候選", call.name);
    };
    let selection = select_goal_candidate(&candidates, weights, seed)
        .unwrap()
        .expect("有候選才選得出來");
    let FunctionEvaluation::Executed(execution) = evaluate_function_offline(
        table,
        &selection.selected,
        document,
        &LibrarySpec::default(),
    )
    .unwrap() else {
        panic!("被選中的候選必須是可直接執行的 Recipe");
    };
    (selection, *execution)
}

#[test]
fn embedded_weight_db_uses_a_dedicated_data_source() {
    let catalog = LibraryCatalog::embedded().unwrap();
    let grammaticalization = catalog
        .packages()
        .iter()
        .find(|package| package.id.to_string() == "std:grammaticalization")
        .unwrap();
    assert_eq!(
        grammaticalization
            .data_sources
            .iter()
            .map(|source| source.path.as_str())
            .collect::<Vec<_>>(),
        ["data/paths.tsv", "data/weights.tsv"]
    );
    let lock = __lock_content_for_tests(grammaticalization);
    assert!(lock.contains("data-source data/paths.tsv\n"));
    assert!(lock.contains("data-source data/weights.tsv\n"));
    let weights = load_weight_db(&catalog, &LibrarySpec::default()).unwrap();
    assert_eq!(weights.weight("Future", "VerbToTense").unwrap(), 1.0);
    assert_eq!(weights.weight("Perfect", "VerbToTense").unwrap(), 1.0);
}

#[test]
fn goal_weight_sampling_is_reproducible_and_executes_only_the_selected_recipe() {
    let package = package(
        "sampling",
        0,
        COMPETING_FUNCTIONS,
        COMPETING_WEIGHTS,
        &["Light", "Heavy", "Choose"],
    );
    let table = functions_from_packages(&[&package]).unwrap();
    let weights = weight_db_from_packages(&[&package]).unwrap();
    let document = base();
    let original = document.source().to_owned();

    let FunctionEvaluation::Candidates(candidates) = evaluate_function_offline(
        &table,
        &invocation("Choose", "go"),
        &document,
        &LibrarySpec::default(),
    )
    .unwrap() else {
        panic!("Choose is a Goal");
    };
    assert_eq!(
        candidates
            .candidates
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>(),
        ["Light", "Heavy"]
    );
    let first = select_goal_candidate(&candidates, &weights, 42)
        .unwrap()
        .unwrap();
    let second = select_goal_candidate(&candidates, &weights, 42)
        .unwrap()
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.algorithm, WEIGHTED_SAMPLER_ALGORITHM);
    assert_eq!(first.ordered[0].1, 1.0);
    assert_eq!(first.ordered[1].1, 3.0);

    let light_seed = (0..100)
        .find(|seed| {
            select_goal_candidate(&candidates, &weights, *seed)
                .unwrap()
                .unwrap()
                .selected
                .name
                == "Light"
        })
        .unwrap();
    let heavy_seed = (0..100)
        .find(|seed| {
            select_goal_candidate(&candidates, &weights, *seed)
                .unwrap()
                .unwrap()
                .selected
                .name
                == "Heavy"
        })
        .unwrap();
    let (light_selection, light) = enumerate_then_select(
        &table,
        &invocation("Choose", "go"),
        &document,
        &weights,
        light_seed,
    );
    let (heavy_selection, heavy) = enumerate_then_select(
        &table,
        &invocation("Choose", "go"),
        &document,
        &weights,
        heavy_seed,
    );
    assert_eq!(light_selection.selected.name, "Light");
    assert_eq!(heavy_selection.selected.name, "Heavy");
    assert!(light.document.source().contains("entrenchment = 0.3"));
    assert!(heavy.document.source().contains("entrenchment = 0.6"));
    assert_eq!(document.source(), original, "authoring sampling is pure");
}

// ── 零候選:語言狀態決定「沒有路可走」,那不是失敗 ──────────────────────────

const GUARDED_FUNCTIONS: &str = r#"package plugin:sampling:
    schema = conlang.functions/v1

function Never(x):
    entrench(x, delta: 0.1)

function Always(x):
    entrench(x, delta: 0.2)

/* 對 verb 而言只有一個分支成立。 */
function Maybe(x):
    choose:
        Never(x) / x.syn.category == noun
        Always(x) / x.syn.category == verb

/* 對 verb 而言一個都不成立 —— 候選清單是空的。 */
function NoneApply(x):
    choose:
        Never(x) / x.syn.category == noun
"#;

const GUARDED_WEIGHTS: &str = "goal\trecipe\tweight\nMaybe\tAlways\t1\nNoneApply\tNever\t1\n";

fn guarded_candidates(goal: &str) -> (FunctionCandidates, WeightDb) {
    let package = package(
        "sampling",
        0,
        GUARDED_FUNCTIONS,
        GUARDED_WEIGHTS,
        &["Never", "Always", "Maybe", "NoneApply"],
    );
    let table = functions_from_packages(&[&package]).unwrap();
    let weights = weight_db_from_packages(&[&package]).unwrap();
    let FunctionEvaluation::Candidates(candidates) = evaluate_function_offline(
        &table,
        &invocation(goal, "go"),
        &base(),
        &LibrarySpec::default(),
    )
    .unwrap() else {
        panic!("{goal} 是 Goal");
    };
    (candidates, weights)
}

/// `when:` 的所有 guard 都不成立 ⇒ **空候選清單,不是錯誤**。
///
/// 「這個語言目前沒有任何適用的演化路徑」是語言狀態的事實(`go` 不是 noun),
/// 不是失敗。判別性靠 `Maybe`:同一套 guard 機制在 verb 上選得出東西,所以空清單
/// 不是「guard 全掛」的假象。
#[test]
fn a_goal_whose_guards_all_fail_yields_an_empty_candidate_list_not_an_error() {
    let (none, _) = guarded_candidates("NoneApply");
    assert!(none.candidates.is_empty(), "{:?}", none.candidates);

    let (some, _) = guarded_candidates("Maybe");
    assert_eq!(
        some.candidates
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>(),
        ["Always"],
        "同一套 guard 在 verb 上必須選得出東西"
    );
}

/// 從空清單選 ⇒ `Ok(None)`。
///
/// ## 誌誤
///
/// 先前這裡會落到 `sample_weighted_index` 的 `Empty`,被包成
/// `FunctionError::Sampling`,而 `Sampling` 分類為 **`Environment`**
/// 「套件/權重表換版了」——方向完全錯,那明明是語言狀態。
///
/// 改成 `Option` 而不是換一個錯誤變體:呼叫端**必須**分辨「沒有路可走」與
/// 「選出了一條」,回 `Ok(None)` 讓編譯器強迫它表態,回錯誤則可以被一句 `?` 靜默轉手。
#[test]
fn selecting_from_an_empty_candidate_list_is_a_legitimate_none() {
    let (none, weights) = guarded_candidates("NoneApply");
    assert!(select_goal_candidate(&none, &weights, 7)
        .expect("零候選不是錯誤")
        .is_none());

    // 判別性:有候選時照樣選得出來,故 `None` 不是「永遠回 None」。
    let (some, weights) = guarded_candidates("Maybe");
    let selection = select_goal_candidate(&some, &weights, 7)
        .expect("有候選")
        .expect("選得出來");
    assert_eq!(selection.selected.name, "Always");
}

#[test]
fn weight_db_rejects_missing_zero_invalid_and_ambiguous_weights() {
    let missing = package(
        "sampling",
        0,
        COMPETING_FUNCTIONS,
        "goal\trecipe\tweight\nChoose\tLight\t1\n",
        &["Light", "Heavy", "Choose"],
    );
    let table = functions_from_packages(&[&missing]).unwrap();
    let FunctionEvaluation::Candidates(candidates) = evaluate_function_offline(
        &table,
        &invocation("Choose", "go"),
        &base(),
        &LibrarySpec::default(),
    )
    .unwrap() else {
        panic!("Choose is a Goal");
    };
    let missing_db = weight_db_from_packages(&[&missing]).unwrap();
    let error = select_goal_candidate(&candidates, &missing_db, 0).unwrap_err();
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::WeightMissing { .. },
                ..
            }
        ),
        "{error:?}"
    );

    let zero = package(
        "sampling",
        0,
        COMPETING_FUNCTIONS,
        "goal\trecipe\tweight\nChoose\tLight\t0\nChoose\tHeavy\t0\n",
        &["Light", "Heavy", "Choose"],
    );
    let zero_db = weight_db_from_packages(&[&zero]).unwrap();
    let error = select_goal_candidate(&candidates, &zero_db, 0).unwrap_err();
    // **權重全零仍是錯**,而且仍屬 `Environment`(權重住套件的 data)。這是零候選改成
    // `Ok(None)` 之後的近似反例:證明我沒有把抽樣器的錯一併吞掉。
    assert!(
        matches!(
            &error,
            ReplayError::Function {
                source: FunctionError::Sampling { .. },
                ..
            }
        ),
        "{error:?}"
    );
    assert!(
        format!("{error}").contains("all candidate weights are zero"),
        "{error}"
    );

    let invalid = package(
        "invalid",
        0,
        COMPETING_FUNCTIONS,
        "goal\trecipe\tweight\nChoose\tLight\t-1\n",
        &["Light", "Heavy", "Choose"],
    );
    let error = weight_db_from_packages(&[&invalid]).unwrap_err();
    assert!(format!("{error}").contains("WEIGHT_DB_WEIGHT"), "{error}");

    let equal_a = package("equal-a", 5, "", COMPETING_WEIGHTS, &[]);
    let equal_b = package("equal-b", 5, "", COMPETING_WEIGHTS, &[]);
    let error = weight_db_from_packages(&[&equal_a, &equal_b]).unwrap_err();
    assert!(
        format!("{error}").contains("WEIGHT_DB_AMBIGUOUS"),
        "{error}"
    );
}

#[test]
fn higher_priority_package_overrides_a_weight_per_goal_and_recipe() {
    let lower = package(
        "lower",
        0,
        "",
        "goal\trecipe\tweight\nChoose\tLight\t1\n",
        &[],
    );
    let higher = package(
        "higher",
        10,
        "",
        "goal\trecipe\tweight\nChoose\tLight\t9\n",
        &[],
    );
    let weights = weight_db_from_packages(&[&higher, &lower]).unwrap();
    assert_eq!(weights.weight("Choose", "Light").unwrap(), 9.0);
}

#[test]
fn embedded_perfect_goal_runs_through_weight_db_and_sampler() {
    let catalog = LibraryCatalog::embedded().unwrap();
    let table = load_functions(&catalog, &LibrarySpec::default()).unwrap();
    let weights = load_weight_db(&catalog, &LibrarySpec::default()).unwrap();
    let mut call = invocation("Perfect", "finish");
    let document = base();

    let (selection, execution) = enumerate_then_select(&table, &call, &document, &weights, 7);
    assert_eq!(selection.selected.name, "VerbToTense");
    assert_eq!(selection.ordered[0].1, 1.0);
    assert!(execution.document.source().contains("core = PERFECT"));
    assert!(execution.document.source().contains("belongs Bound"));

    call.name = "VerbToTense".to_owned();
    call.named = vec![
        ("tense".to_owned(), "PERFECT".to_owned()),
        ("result_category".to_owned(), "Bound".to_owned()),
    ];
    // 對照:同一個表、把 Recipe 直接餵進去,回的是 `Executed` 而不是 `Candidates`。
    //
    // 先前這裡斷言的是 `FunctionError::NotGoal`——一個**只因為包裝函數存在而存在**的
    // 錯誤變體(全庫唯一建構點就在那個包裝裡)。包裝刪掉後,「這不是 Goal」不再是
    // 一種失敗,而是**回傳形狀本來就看得出來**的事實。斷言形狀比斷言防禦性錯誤誠實:
    // 前者是契約,後者只是某個呼叫者的期待落空。
    assert!(matches!(
        evaluate_function_offline(&table, &call, &document, &LibrarySpec::default()).unwrap(),
        FunctionEvaluation::Executed(_)
    ));
}
