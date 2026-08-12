//! P52:語法化路徑庫 = 一個參數化 function(code)+ 一張路徑表(data)。
//!
//! 「GO→未來、WANT→未來、COME→未來…」機制完全相同,只有兩端的概念不同,
//! 所以它們**不是 30–50 個 function**。修補10 §6 的驗收句是:
//!
//! > 官方加新路徑 = 加 data 一行
//!
//! 這份測試的重心就是那一句話能不能兌現——尤其
//! [`a_third_party_adds_a_path_with_one_data_row_and_no_code`]:一個**只有
//! 一個 data 檔、沒有任何 `.chg`**的套件,讓 std 的 `Future` 多認一條來源。
//! 若哪天有人把來源概念搬回 `.chg`(多一個 `choose:` 分支、多一個 function),
//! 那條會紅。

use conlang_changeset::function::{
    evaluate_function_offline, evaluate_function_with_packages, functions_from_packages,
    load_path_db, path_db_from_packages, FunctionCall, FunctionErrorClass, FunctionEvaluation,
};
use conlang_changeset::ReplayError;
use conlang_language::{
    LanguageDocument, LibraryCatalog, LibrarySpec, PackageId, PackageRequirement, PackageResolver,
    PackageSource, PackageSources, PackageSpec, ResolvedPackages,
};

/// `go` = GO(在 std 表上,通往 FUTURE)、`finish` = FINISH(通往 PERFECT)、
/// `take` = TAKE(**不在** std 表上)、`stone` 不是動詞。
const SOURCE: &str = r#"trait MotionVerb:
    belongs Verb

sign go:
    belongs MotionVerb
    entrenchment = 0.2
    sem:
        senses:
            core = GO

sign finish:
    belongs MotionVerb
    entrenchment = 0.2
    sem:
        senses:
            core = FINISH

sign take:
    belongs MotionVerb
    entrenchment = 0.2
    sem:
        senses:
            core = TAKE

sign stone:
    belongs Noun
    entrenchment = 0.2
    sem:
        senses:
            core = STONE
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("fixture parses")
}

fn goal(name: &str, sign: &str) -> FunctionCall {
    FunctionCall {
        name: name.to_owned(),
        positional: Some(format!("sign(\"{sign}\")")),
        named: Vec::new(),
    }
}

fn candidates(evaluation: FunctionEvaluation) -> Vec<FunctionCall> {
    match evaluation {
        FunctionEvaluation::Candidates(found) => found.candidates,
        FunctionEvaluation::Executed(_) => panic!("a goal must stop at its candidate list"),
    }
}

fn named<'a>(call: &'a FunctionCall, key: &str) -> &'a str {
    call.named
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("candidate has no {key:?} argument: {call:?}"))
}

// ── 表就是路徑清單 ────────────────────────────────────────────────────

/// std 出貨的四條路徑全部讀得到,且 δ 是表上的值。
#[test]
fn the_std_path_table_is_loaded_from_the_declared_table_type() {
    let catalog = LibraryCatalog::embedded().expect("embedded catalog");
    let paths = load_path_db(&catalog, &LibrarySpec::default()).expect("path db");

    assert_eq!(paths.len(), 4);
    for source in ["GO", "WANT", "COME"] {
        assert!(paths.contains(source, "FUTURE"), "{source} -> FUTURE");
        assert_eq!(paths.delta(source, "FUTURE").unwrap(), 0.3);
    }
    assert!(paths.contains("FINISH", "PERFECT"));
    assert!(!paths.contains("GO", "PERFECT"), "沒宣告的組合不存在");

    let mut sources = paths.sources_for("FUTURE");
    sources.sort_unstable();
    assert_eq!(sources, ["COME", "GO", "WANT"]);
}

// ── 🔑 加一行 data = 多一條路徑 ───────────────────────────────────────

/// 只有一個 data 檔的套件:沒有 `code/`、沒有 `.chg`、沒有 export。
fn path_only_package(rows: &str) -> PackageSources {
    PackageSources {
        config: r#"schema = 2
id = "plugin:areal-paths"
version = "1.0.0"
layer = "data"
capabilities = ["data"]
data = ["data/areal.tsv"]
"#
        .to_owned(),
        // 檔名不叫 `paths.tsv` 也照樣被認出來——選表認表型不認路徑(P29)。
        tables: "path\ttype\ndata/areal.tsv\tengine:GrammaticalizationPathTable\n".to_owned(),
        data: rows.to_owned(),
        data_files: vec![conlang_language::PackageFile {
            path: "data/areal.tsv".to_owned(),
            source: rows.to_owned(),
        }],
        source: PackageSource::Injected("p52-test".to_owned()),
        ..PackageSources::default()
    }
}

/// std 的預設那組 + 指名的外掛。std 仍在,所以用的是 std 的
/// `Future`/`VerbToTense`——外掛只多帶一張表。
fn spec_with(plugin: &str) -> PackageSpec {
    PackageSpec::from_legacy(&LibrarySpec::default()).with_root(PackageRequirement::exact(
        PackageId::new("plugin", plugin),
        "1.0.0",
    ))
}

fn with_extra_paths(rows: &str) -> ResolvedPackages {
    LibraryCatalog::with_packages([path_only_package(rows)])
        .expect("catalog")
        .resolve(&spec_with("areal-paths"))
        .expect("resolved")
}

fn std_only() -> ResolvedPackages {
    LibraryCatalog::embedded()
        .expect("embedded catalog")
        .resolve_legacy(&LibrarySpec::default())
        .expect("resolved")
}

/// 🔑 P52 的驗收句。第三方**加一行 data、零行 code**,std 的 `Future` 就多認
/// 一條來源概念,連 δ 都照那一行走。
#[test]
fn a_third_party_adds_a_path_with_one_data_row_and_no_code() {
    let document = base();

    // 前:TAKE 不在任何表上 ⇒ 零候選(不是錯誤,P70)。
    let before = candidates(
        evaluate_function_with_packages(
            &conlang_changeset::function::load_functions_from_resolved(&std_only())
                .expect("functions"),
            &goal("Future", "take"),
            &document,
            &std_only(),
        )
        .expect("零候選是合法結果,不該報錯"),
    );
    assert!(before.is_empty(), "TAKE 還不是 FUTURE 的已知來源");

    // 後:一行 data。`.chg` 一個字都沒動——用的還是 std 的 Future/VerbToTense。
    let packages = with_extra_paths("source\ttarget\tdelta\nTAKE\tFUTURE\t0.45\n");
    let table =
        conlang_changeset::function::load_functions_from_resolved(&packages).expect("functions");
    let after = candidates(
        evaluate_function_with_packages(&table, &goal("Future", "take"), &document, &packages)
            .expect("evaluates"),
    );

    assert_eq!(after.len(), 1, "多了一條路徑就多一個候選");
    assert_eq!(after[0].name, "VerbToTense");
    assert_eq!(after[0].positional.as_deref(), Some("sign(\"take\")"));
    // δ 來自新加的那一行,不是 recipe 裡寫死的數字。
    assert_eq!(named(&after[0], "delta"), "0.45");
    assert_eq!(named(&after[0], "tense"), "FUTURE");
}

/// 同一個機制、同一份 `.chg`,δ 換一行就換一個值。
#[test]
fn the_delta_comes_from_the_row_not_from_the_recipe() {
    let document = base();
    for (row_delta, expected) in [("0.05", "0.05"), ("0.9", "0.9")] {
        let packages = with_extra_paths(&format!(
            "source\ttarget\tdelta\nTAKE\tFUTURE\t{row_delta}\n"
        ));
        let table = conlang_changeset::function::load_functions_from_resolved(&packages)
            .expect("functions");
        let found = candidates(
            evaluate_function_with_packages(&table, &goal("Future", "take"), &document, &packages)
                .expect("evaluates"),
        );
        assert_eq!(named(&found[0], "delta"), expected);
    }
}

/// 候選寫進 `.chg` 之後,執行時 entrenchment 真的加的是表上的 δ。
/// (0.2 起始 + 0.45 = 0.65;不是 recipe 舊有的寫死值 0.3。)
#[test]
fn the_selected_candidate_entrenches_by_the_table_delta() {
    let document = base();
    let packages = with_extra_paths("source\ttarget\tdelta\nTAKE\tFUTURE\t0.45\n");
    let table =
        conlang_changeset::function::load_functions_from_resolved(&packages).expect("functions");
    let found = candidates(
        evaluate_function_with_packages(&table, &goal("Future", "take"), &document, &packages)
            .expect("evaluates"),
    );

    let FunctionEvaluation::Executed(execution) =
        evaluate_function_with_packages(&table, &found[0], &document, &packages).expect("executes")
    else {
        panic!("the recipe is a plain sequence and must execute");
    };
    assert!(
        execution.document.source().contains("entrenchment = 0.65"),
        "δ 應為表上的 0.45:\n{}",
        execution.document.source()
    );
}

// ── 不在表上 = 零候選,不是錯誤 ───────────────────────────────────────

/// 詞是動詞、但它的核心義項不是那個目標的已知來源 ⇒ 候選清單為空。
/// 這是語言狀態的事實,不是失敗(P70)。
#[test]
fn a_verb_that_is_not_a_listed_source_yields_no_candidate() {
    let document = base();
    let table = conlang_changeset::function::load_functions(
        &LibraryCatalog::embedded().unwrap(),
        &LibrarySpec::default(),
    )
    .unwrap();

    // TAKE 不在 std 表上。
    assert!(candidates(
        evaluate_function_offline(
            &table,
            &goal("Future", "take"),
            &document,
            &LibrarySpec::default()
        )
        .expect("零候選不報錯")
    )
    .is_empty());

    // GO 在表上通往 FUTURE,但**不**通往 PERFECT——方向與目標都要對。
    assert!(candidates(
        evaluate_function_offline(
            &table,
            &goal("Perfect", "go"),
            &document,
            &LibrarySpec::default()
        )
        .expect("零候選不報錯")
    )
    .is_empty());

    // 反面控制組:表上有的就要出現,否則上面兩條可能只是「什麼都不給」。
    assert_eq!(
        candidates(
            evaluate_function_offline(
                &table,
                &goal("Future", "go"),
                &document,
                &LibrarySpec::default()
            )
            .expect("evaluates")
        )
        .len(),
        1
    );
}

/// 決定性:同樣的表、同樣的文件,兩次求值逐欄相同(P26)。
#[test]
fn candidate_enumeration_is_deterministic() {
    let document = base();
    let packages = with_extra_paths("source\ttarget\tdelta\nTAKE\tFUTURE\t0.45\n");
    let table =
        conlang_changeset::function::load_functions_from_resolved(&packages).expect("functions");
    let once = candidates(
        evaluate_function_with_packages(&table, &goal("Future", "take"), &document, &packages)
            .unwrap(),
    );
    let twice = candidates(
        evaluate_function_with_packages(&table, &goal("Future", "take"), &document, &packages)
            .unwrap(),
    );
    assert_eq!(once, twice);
}

// ── 畸形表一律拒絕 ────────────────────────────────────────────────────

fn rejection(rows: &str) -> String {
    let catalog = LibraryCatalog::with_packages([path_only_package(rows)]).expect("catalog");
    let packages: Vec<_> = catalog.packages().iter().collect();
    let error = path_db_from_packages(&packages).expect_err("畸形路徑表應被拒");
    error.to_string()
}

#[test]
fn a_malformed_path_table_is_rejected() {
    for (rows, code) in [
        ("source\tgoal\tdelta\nSIT\tFUTURE\t0.3\n", "PATH_DB_SCHEMA"),
        ("source\ttarget\tdelta\nSIT\tFUTURE\n", "PATH_DB_SCHEMA"),
        ("source\ttarget\tdelta\n\tFUTURE\t0.3\n", "PATH_DB_SCHEMA"),
        (
            "source\ttarget\tdelta\nSIT\tFUTURE\tnope\n",
            "PATH_DB_DELTA",
        ),
        ("source\ttarget\tdelta\nSIT\tFUTURE\t-1\n", "PATH_DB_DELTA"),
        (
            "source\ttarget\tdelta\nSIT\tFUTURE\t0.3\nSIT\tFUTURE\t0.4\n",
            "PATH_DB_DUPLICATE",
        ),
    ] {
        let message = rejection(rows);
        assert!(message.contains(code), "{rows:?} => {message}");
    }

    // 正向控制組。
    let catalog =
        LibraryCatalog::with_packages([path_only_package("source\ttarget\tdelta\nSIT\tX\t0.3\n")])
            .expect("catalog");
    let packages: Vec<_> = catalog.packages().iter().collect();
    assert!(path_db_from_packages(&packages).is_ok());
}

/// 兩個**同優先級**的套件宣告同一條路徑 ⇒ 強制消歧,不靜默取一個。
#[test]
fn an_equal_priority_collision_is_ambiguous() {
    let mut second = path_only_package("source\ttarget\tdelta\nSIT\tFUTURE\t0.9\n");
    second.config = second
        .config
        .replace("plugin:areal-paths", "plugin:other-paths");
    let catalog = LibraryCatalog::with_packages([
        path_only_package("source\ttarget\tdelta\nSIT\tFUTURE\t0.1\n"),
        second,
    ])
    .expect("catalog");
    let packages: Vec<_> = catalog.packages().iter().collect();
    let error = path_db_from_packages(&packages).expect_err("同優先級撞號應被拒");
    assert!(error.to_string().contains("PATH_DB_AMBIGUOUS"), "{error}");
}

// ── 內建寫錯 = Broken,不是環境問題 ──────────────────────────────────

fn class_of(error: &ReplayError) -> FunctionErrorClass {
    match error {
        ReplayError::Function { source, .. } => source.class(),
        other => panic!("expected a function error, got {other:?}"),
    }
}

/// `path(...)` / `path_delta(...)` 的引數寫錯是**作者寫錯**(Broken),
/// 不是環境變動——rebase 換幾個 base 都一樣錯,不該叫人去解衝突。
///
/// 合成套件直接建 `LibraryPackage`(不走 catalog),為的是只測這一件事:
/// 內建的引數檢查。
#[test]
fn a_malformed_path_builtin_is_a_broken_input() {
    let document = base();
    for (body, code) in [
        (
            "        VerbToTense(target, tense: FUTURE, result_category: Aux, delta: 0.1) / path(target, FUTURE)",
            "PATH_BUILTIN_ARITY",
        ),
        (
            "        VerbToTense(target, tense: FUTURE, result_category: Aux, delta: path_delta(target, core)) / path(target, core, FUTURE)",
            "PATH_BUILTIN_ARITY",
        ),
        (
            "        VerbToTense(target, tense: FUTURE, result_category: Aux, delta: 0.1) / path(FUTURE, core, FUTURE)",
            "PATH_BUILTIN_SUBJECT",
        ),
        (
            "        VerbToTense(target, tense: FUTURE, result_category: Aux, delta: 0.1) / path(target, nosuchsense, FUTURE)",
            "PATH_BUILTIN_NO_SENSE",
        ),
    ] {
        let source = format!(
            "package plugin:probe:\n    schema = conlang.functions/v1\n\n\
             function ProbeFuture(target [Verb]):\n    choose:\n{body}\n"
        );
        let table = functions_from_packages(&[&function_package(&source)]).expect("functions");
        let error = evaluate_function_offline(
            &table,
            // `go` 在 std 表上(GO→FUTURE),guard 才會成立、`path_delta` 才
            // 會被求值——用一個不在表上的詞會讓分支不成立,於是什麼都測不到。
            &goal("ProbeFuture", "go"),
            &document,
            &LibrarySpec::default(),
        )
        .expect_err("畸形內建應被拒");
        assert!(error.to_string().contains(code), "{body}\n=> {error}");
        assert_eq!(class_of(&error), FunctionErrorClass::Broken, "{error}");
    }
}

/// 合成 function 套件:只帶一份 `.chg`,不走 catalog 驗證。
fn function_package(source: &str) -> conlang_language::LibraryPackage {
    let id = conlang_language::LibraryId::new(conlang_language::LibraryKind::Plugin, "probe");
    conlang_language::LibraryPackage {
        name: id.name.clone(),
        rule_namespace: id.to_string(),
        version: "test".to_owned(),
        manifest_schema: 1,
        layer: conlang_language::PackageLayer::Overlay,
        capabilities: conlang_language::PackageCapabilities {
            functions: true,
            ..conlang_language::PackageCapabilities::default()
        },
        source: PackageSource::default(),
        enabled: true,
        priority: 0,
        requires: Vec::new(),
        code_paths: Vec::new(),
        code_path: String::new(),
        data_path: String::new(),
        data_paths: Vec::new(),
        data_sources: Vec::new(),
        function_paths: vec!["code/probe.chg".to_owned()],
        function_sources: vec![conlang_language::LibraryFunctionSource {
            path: "code/probe.chg".to_owned(),
            source: source.to_owned(),
        }],
        exports: vec![conlang_language::LibraryExport {
            package: id.name.clone(),
            package_id: id.clone(),
            stable_id: format!("{id}:ProbeFuture"),
            kind: conlang_language::LibraryExportKind::Function,
            alias: "ProbeFuture".to_owned(),
        }],
        id,
        code: String::new(),
        functions: String::new(),
        data: String::new(),
    }
}
