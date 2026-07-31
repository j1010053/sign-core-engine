use conlang_changeset::__lock_content_for_tests;
use conlang_changeset::function::{
    evaluate_function_offline, evaluate_goal_offline, functions_from_packages, load_functions,
    load_weight_db, select_goal_candidate, weight_db_from_packages, FunctionCall,
    FunctionEvaluation,
};
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
"#;

const COMPETING_FUNCTIONS: &str = r#"package plugin:sampling:
    schema = conlang.functions/v1

function Light(x [Verb]):
    entrench(x, delta: 0.1)

function Heavy(x [Verb]):
    entrench(x, delta: 0.4)

function Choose(x [Verb]):
    when:
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
        enabled: true,
        priority,
        requires: Vec::new(),
        code_paths: Vec::new(),
        code_path: String::new(),
        data_path: "data/weights.tsv".to_owned(),
        data_paths: vec!["data/weights.tsv".to_owned()],
        data_sources: vec![LibraryDataSource {
            path: "data/weights.tsv".to_owned(),
            source: weights,
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
        code: "",
        functions,
        data: weights,
    }
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
    let first = select_goal_candidate(&candidates, &weights, 42).unwrap();
    let second = select_goal_candidate(&candidates, &weights, 42).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.algorithm, WEIGHTED_SAMPLER_ALGORITHM);
    assert_eq!(first.ordered[0].1, 1.0);
    assert_eq!(first.ordered[1].1, 3.0);

    let light_seed = (0..100)
        .find(|seed| {
            select_goal_candidate(&candidates, &weights, *seed)
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
                .selected
                .name
                == "Heavy"
        })
        .unwrap();
    let light = evaluate_goal_offline(
        &table,
        &invocation("Choose", "go"),
        &document,
        &LibrarySpec::default(),
        &weights,
        light_seed,
    )
    .unwrap();
    let heavy = evaluate_goal_offline(
        &table,
        &invocation("Choose", "go"),
        &document,
        &LibrarySpec::default(),
        &weights,
        heavy_seed,
    )
    .unwrap();
    assert_eq!(light.selection.selected.name, "Light");
    assert_eq!(heavy.selection.selected.name, "Heavy");
    assert!(light
        .execution
        .document
        .source()
        .contains("entrenchment = 0.3"));
    assert!(heavy
        .execution
        .document
        .source()
        .contains("entrenchment = 0.6"));
    assert_eq!(document.source(), original, "authoring sampling is pure");
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
    assert!(format!("{error}").contains("WEIGHT_DB_MISSING"), "{error}");

    let zero = package(
        "sampling",
        0,
        COMPETING_FUNCTIONS,
        "goal\trecipe\tweight\nChoose\tLight\t0\nChoose\tHeavy\t0\n",
        &["Light", "Heavy", "Choose"],
    );
    let zero_db = weight_db_from_packages(&[&zero]).unwrap();
    let error = select_goal_candidate(&candidates, &zero_db, 0).unwrap_err();
    assert!(format!("{error}").contains("all candidate weights are zero"));

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

    let execution = evaluate_goal_offline(
        &table,
        &call,
        &document,
        &LibrarySpec::default(),
        &weights,
        7,
    )
    .unwrap();
    assert_eq!(execution.selection.selected.name, "VerbToTense");
    assert_eq!(execution.selection.ordered[0].1, 1.0);
    assert!(execution
        .execution
        .document
        .source()
        .contains("core = PERFECT"));
    assert!(execution
        .execution
        .document
        .source()
        .contains("category = bound"));

    call.name = "VerbToTense".to_owned();
    call.named = vec![
        ("tense".to_owned(), "PERFECT".to_owned()),
        ("result_category".to_owned(), "bound".to_owned()),
    ];
    let error = evaluate_goal_offline(
        &table,
        &call,
        &document,
        &LibrarySpec::default(),
        &weights,
        7,
    )
    .unwrap_err();
    assert!(format!("{error}").contains("FUNCTION_NOT_GOAL"), "{error}");
}
