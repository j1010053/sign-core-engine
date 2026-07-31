use conlang_changeset::function::{
    evaluate_function_offline, functions_from_packages, load_functions, FunctionBody, FunctionCall,
    FunctionEvaluation,
};
use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
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
        FunctionBody::When(_)
    ));
    assert!(matches!(
        table.get("Perfect").unwrap().body,
        FunctionBody::When(_)
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
    when:
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
        FunctionBody::When(_)
    ));
}

#[test]
fn std_recipe_executes_in_order_on_a_temporary_language() {
    let catalog = LibraryCatalog::embedded().unwrap();
    let table = load_functions(&catalog, &LibrarySpec::default()).unwrap();
    let mut call = invocation("VerbToTense", "go");
    call.named.push(("tense".to_owned(), "FUTURE".to_owned()));
    call.named
        .push(("result_category".to_owned(), "aux".to_owned()));

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
    assert!(source.contains("category = aux"), "{source}");
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
            ("result_category".to_owned(), "aux".to_owned())
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
            ("result_category".to_owned(), "bound".to_owned())
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
    assert!(evolved.items.iter().any(|item| matches!(
        item,
        SignItem::Def(definition)
            if definition.path == "syn.category" && definition.value == "bound"
    )));
    assert!(evolved.items.iter().any(|item| matches!(
        item,
        SignItem::Def(definition)
            if definition.path == "entrenchment" && definition.value == "0.5"
    )));
    let source = execution.document.source();
    assert!(source.contains("core = PERFECT"), "{source}");
    assert!(source.contains("category = bound"), "{source}");
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
    reanalyze(sign("missing"), target: category, to: aux)
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

#[test]
fn changeset_dispatches_recipe_and_stops_at_goal_candidates() {
    let document = base();
    let libraries = LibrarySpec::default();
    let mut recipe_source = change_set_prelude(&document, &libraries, "evo:std-recipe").unwrap();
    recipe_source.push_str(
        "\n    #0:\n        VerbToTense(sign(\"go\"), tense: FUTURE, result_category: aux)\n",
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
    assert!(outcome.document.source().contains("category = aux"));

    let mut goal_source = change_set_prelude(&document, &libraries, "evo:std-goal").unwrap();
    goal_source.push_str("\n    #0:\n        Future(sign(\"go\"))\n");
    let error = UnresolvedChangeSet::parse(&goal_source)
        .unwrap()
        .resolve(&document, &libraries)
        .expect_err("Goal selection belongs to the caller, not implicit resolution");
    assert!(
        format!("{error}").contains("FUNCTION_CANDIDATES_REQUIRE_SELECTION"),
        "{error}"
    );
}
