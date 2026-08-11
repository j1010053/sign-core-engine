//! **流 B 縱貫驗收**(《架構2.0總鳥瞰》§2「流 B:歷時演化」)。
//!
//! 鳥瞰宣稱的鏈是:
//!
//! ```text
//! [使用者在任一層介入 ← 層級介入 P17]
//! Goal(函數)      ──▶ Vec<Recipe 候選>
//!                       │ ◄── Weight DB(E)+ seeded 抽樣器
//! Recipe(函數)    ──▶ Vec<AtomicRewrite>
//! AtomicRewrite   ──▶ Vec<PrimitiveEdit>
//! PrimitiveEdit   ──▶ Language′
//!                 ──▶ 重新 Compile ──▶ Compiled Grammar′ ──▶ Engine ──▶ Surface′
//! ```
//!
//! ## 為什麼需要這個檔:每一環都綠,不代表鏈是通的
//!
//! 本檔之前,證據是**分段**的:
//!
//! | 環節 | 既有證據 | 最遠走到 |
//! |---|---|---|
//! | Goal → 候選 → 抽樣 | `goal_sampling.rs` | `execution.document.source()` |
//! | Recipe → AtomicRewrite | `std_function_roles.rs`、`step17_call_trace.rs` | 原語 |
//! | AtomicRewrite → 原語 | `atomic_rewrite*.rs` | `.lang` |
//! | 原語 → Language′ → Compile → Surface′ | `primitive_edits.rs` | Surface,但輸入是**手搭的單一 `Update`** |
//!
//! 也就是說:**沒有任何一條測試從 Goal/Recipe 走到 Surface′**,接縫從未被跨越過。
//! 分段測試抓不到的正是接縫錯——例如「抽樣選了 A、執行的卻是 B」,兩段各自的斷言
//! 都會照樣綠,因為沒有人把「選了誰」和「最後唸什麼」擺在同一條斷言裡。
//!
//! 故本檔的主力是 `the_sampler_choice_propagates_all_the_way_to_the_surface`:
//! 換一個 seed,**表層必須跟著變**。鏈上任何一環被短路,那條就會紅。

use conlang_changeset::evolution::{Edge, EvolutionGraph, Nativization, NodeId};
use conlang_changeset::function::{
    evaluate_function_offline, functions_from_packages, select_goal_candidate,
    weight_db_from_packages, FunctionCall, FunctionEvaluation, FunctionExecution, FunctionTable,
    GoalSelectionTrace, WeightDb,
};
use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, PrimitiveEdit, UnresolvedChangeSet,
};
use conlang_language::{
    codegen, word, LanguageDocument, LibraryDataSource, LibraryExport, LibraryExportKind,
    LibraryId, LibraryKind, LibraryPackage, LibrarySpec, WEIGHTED_SAMPLER_ALGORITHM,
};

/// `Class vowel` 是引擎切音節的前提;`global trait Core` 是 `sound_change` 的 RuleHome。
const SOURCE: &str = "Symbol b\nSymbol a\nSymbol k\nSymbol d\nClass vowel {a}\n\n\
                      global trait Core:\n\n\
                      sign ba:\n    phon:\n        /ba/\n";

/// 兩個 Recipe 各自把 `b` 換成不同的音,故**選了哪一個一路看得到表層**:
/// `ba` → `ka`(Lenite)或 `da`(Fortify)。這個設計是刻意的——若兩個 Recipe 的
/// 產物表層相同,本檔最重要的那條斷言就退化成恆真。
const FUNCTIONS: &str = r#"package plugin:flowb:
    schema = conlang.functions/v1

function Lenite(home):
    sound_change(home, body: "b => k")

function Fortify(home):
    sound_change(home, body: "b => d")

function ShiftOnset(home):
    choose:
        Lenite(home)
        Fortify(home)
"#;

const WEIGHTS: &str = "goal\trecipe\tweight\nShiftOnset\tLenite\t1\nShiftOnset\tFortify\t3\n";

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("fixture parses")
}

/// 鏈的最後兩環:重新 Compile → Engine → Surface′。
fn surface(document: &LanguageDocument) -> String {
    let artifacts = codegen::compile_full(document.language()).expect("compile grammar");
    word::derive(
        &artifacts,
        &word::PhraseSpec(vec![word::Component::sign("ba")]),
    )
    .expect("engine derives a surface")
    .surface
}

fn goal_call() -> FunctionCall {
    FunctionCall {
        name: "ShiftOnset".to_owned(),
        positional: Some("trait(\"Core\")".to_owned()),
        named: Vec::new(),
    }
}

fn package() -> LibraryPackage {
    let id = LibraryId::new(LibraryKind::Plugin, "flowb");
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
        priority: 0,
        requires: Vec::new(),
        code_paths: Vec::new(),
        code_path: String::new(),
        data_path: "data/weights.tsv".to_owned(),
        data_paths: vec!["data/weights.tsv".to_owned()],
        data_sources: vec![LibraryDataSource {
            path: "data/weights.tsv".to_owned(),
            source: WEIGHTS.to_owned(),
        }],
        function_paths: vec!["code/functions.chg".to_owned()],
        function_sources: Vec::new(),
        exports: ["Lenite", "Fortify", "ShiftOnset"]
            .iter()
            .map(|alias| LibraryExport {
                package: id.name.clone(),
                package_id: id.clone(),
                stable_id: format!("plugin:flowb:{alias}"),
                kind: LibraryExportKind::Function,
                alias: (*alias).to_owned(),
            })
            .collect(),
        id,
        code: String::new(),
        functions: FUNCTIONS.to_owned(),
        data: WEIGHTS.to_owned(),
    }
}

/// 流 B 的 B1→B2→B3:**列候選** → **選一個** → **執行被選中的那個**。
///
/// 三步刻意分開寫。選擇不屬於引擎層(P12:Goal 的型別到 `Vec<Recipe 候選>` 為止),
/// 這裡扮演的是應用層——UI 候選面板或批次迴圈在真實系統裡做的事。
fn enumerate_then_select(
    table: &FunctionTable,
    document: &LanguageDocument,
    weights: &WeightDb,
    seed: u64,
) -> (GoalSelectionTrace, FunctionExecution) {
    let libraries = LibrarySpec::default();
    let FunctionEvaluation::Candidates(candidates) =
        evaluate_function_offline(table, &goal_call(), document, &libraries).unwrap()
    else {
        panic!("ShiftOnset 必須產出候選而不是直接執行");
    };
    let selection = select_goal_candidate(&candidates, weights, seed)
        .unwrap()
        .expect("有候選才選得出來");
    let FunctionEvaluation::Executed(execution) =
        evaluate_function_offline(table, &selection.selected, document, &libraries).unwrap()
    else {
        panic!("被選中的候選必須是可直接執行的 Recipe");
    };
    (selection, *execution)
}

/// 用哪個 seed 會選到 `name`。**用搜尋而不是寫死 seed**:寫死的數字在抽樣器
/// 換實作時會靜默指向另一個候選,測試照樣綠但驗的已經不是原本那件事。
fn seed_selecting(name: &str) -> u64 {
    let package = package();
    let table = functions_from_packages(&[&package]).unwrap();
    let weights = weight_db_from_packages(&[&package]).unwrap();
    let document = base();
    let FunctionEvaluation::Candidates(candidates) =
        evaluate_function_offline(&table, &goal_call(), &document, &LibrarySpec::default())
            .unwrap()
    else {
        panic!("ShiftOnset 是 Goal");
    };
    (0..200)
        .find(|seed| {
            select_goal_candidate(&candidates, &weights, *seed)
                .unwrap()
                .unwrap()
                .selected
                .name
                == name
        })
        .unwrap_or_else(|| panic!("200 個 seed 內選不到 {name}"))
}

/// 走完整條鏈,並在**每一環**留下觀察點。
#[test]
fn the_whole_of_flow_b_runs_from_goal_to_surface() {
    let package = package();
    let table = functions_from_packages(&[&package]).unwrap();
    let weights = weight_db_from_packages(&[&package]).unwrap();
    let document = base();
    let libraries = LibrarySpec::default();

    // 前提:起點表層是 `ba`。沒有這一行,後面的 `ka` 可能一開始就是 `ka`。
    assert_eq!(surface(&document), "ba");

    // ── B1  Goal ──▶ Vec<Recipe 候選> ──
    let FunctionEvaluation::Candidates(candidates) =
        evaluate_function_offline(&table, &goal_call(), &document, &libraries).unwrap()
    else {
        panic!("Goal 必須產出候選而不是直接執行");
    };
    assert_eq!(
        candidates
            .candidates
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>(),
        ["Lenite", "Fortify"]
    );

    // ── B2  候選 ──▶ Weight DB + seeded 抽樣器 ──
    let seed = seed_selecting("Lenite");
    let (selection, execution) = enumerate_then_select(&table, &document, &weights, seed);
    assert_eq!(selection.algorithm, WEIGHTED_SAMPLER_ALGORITHM);
    assert_eq!(selection.seed, seed);
    assert_eq!(selection.ordered[0].1, 1.0, "Lenite 權重");
    assert_eq!(selection.ordered[1].1, 3.0, "Fortify 權重");
    assert_eq!(selection.selected.name, "Lenite");

    // ── B3  Recipe ──▶ Vec<AtomicRewrite> ──
    // trace 記的是**代換後**的原子改寫,而不是 Recipe 名字本身。
    let trace = &execution.trace;
    assert_eq!(trace.len(), 1, "{trace:?}");
    assert_eq!(trace[0].stack, ["Lenite"]);
    assert_eq!(trace[0].call.name, "sound_change");
    assert_eq!(trace[0].call.positional.as_deref(), Some("trait(\"Core\")"));

    // ── B4  AtomicRewrite ──▶ Vec<PrimitiveEdit> ──
    assert!(!execution.edits.is_empty());
    assert!(
        execution
            .edits
            .iter()
            .all(|edit| matches!(edit, PrimitiveEdit::Insert { .. })),
        "sound_change 降階為 Insert:{:?}",
        execution.edits
    );

    // ── B5  PrimitiveEdit ──▶ Language′ ──
    assert!(
        execution.document.source().contains("b => k"),
        "{}",
        execution.document.source()
    );
    // 授權期的抽樣是純的:來源文件不得被就地改動。
    assert_eq!(surface(&document), "ba", "Goal 求值不得污染輸入");

    // ── B6/B7  重新 Compile ──▶ Engine ──▶ Surface′ ──
    assert_eq!(surface(&execution.document), "ka");
}

/// **本檔的主力**。分段測試各自綠、鏈卻是斷的——這條把「抽樣選了誰」與
/// 「最後唸什麼」綁在同一個斷言裡。
#[test]
fn the_sampler_choice_propagates_all_the_way_to_the_surface() {
    let package = package();
    let table = functions_from_packages(&[&package]).unwrap();
    let weights = weight_db_from_packages(&[&package]).unwrap();
    let document = base();

    let (lenite_selection, lenite) =
        enumerate_then_select(&table, &document, &weights, seed_selecting("Lenite"));
    let (fortify_selection, fortify) =
        enumerate_then_select(&table, &document, &weights, seed_selecting("Fortify"));

    assert_eq!(lenite_selection.selected.name, "Lenite");
    assert_eq!(fortify_selection.selected.name, "Fortify");
    // 鏈上任何一環被短路(選了 A 卻執行 B、Language′ 沒重編、表層讀的是舊 grammar),
    // 這兩個表層就會相等。
    assert_eq!(surface(&lenite.document), "ka");
    assert_eq!(surface(&fortify.document), "da");
}

#[test]
fn the_same_seed_reproduces_the_same_surface() {
    // 決定性要驗到**鏈的末端**,不是只驗到 `.lang`——重編與引擎那兩環也可能引入
    // 非決定性(迭代序、浮點),那不會在 `.lang` 上顯形。
    let package = package();
    let table = functions_from_packages(&[&package]).unwrap();
    let weights = weight_db_from_packages(&[&package]).unwrap();
    let document = base();
    let seed = seed_selecting("Fortify");
    let run = || {
        surface(
            &enumerate_then_select(&table, &document, &weights, seed)
                .1
                .document,
        )
    };
    assert_eq!(run(), run());
}

/// 層級介入(P17):鳥瞰說使用者可以**在任一層切入**。那句話要成立,三個高度切進去
/// 就必須落在同一個表層上——否則「介入」等於換了一套語意。
///
/// 這是變形(metamorphic)斷言:不預設表層是什麼,只要求三條路徑一致。
#[test]
fn intervening_at_any_layer_reaches_the_same_surface() {
    let package = package();
    let table = functions_from_packages(&[&package]).unwrap();
    let weights = weight_db_from_packages(&[&package]).unwrap();
    let document = base();
    let libraries = LibrarySpec::default();

    // 層④ Goal:列候選 → 抽樣器選 → 執行。
    let (selection, from_goal) =
        enumerate_then_select(&table, &document, &weights, seed_selecting("Lenite"));

    // 層③ Recipe:跳過抽樣,直接點名被選中的那一個。
    let FunctionEvaluation::Executed(from_recipe) =
        evaluate_function_offline(&table, &selection.selected, &document, &libraries).unwrap()
    else {
        panic!("Recipe 直接執行");
    };

    // 層② AtomicRewrite:跳過函數層,直接在 `.chg` 寫原子改寫。
    let mut source = change_set_prelude(&document, &libraries, "evo:layer2").unwrap();
    source.push_str("\n    #0:\n        sound_change(trait(\"Core\"), body: \"b => k\")\n");
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&document, &libraries)
        .unwrap();
    let from_rewrite = ChangeInterpreter::new(document.clone(), libraries.clone(), "evo:layer2")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;

    // 層① PrimitiveEdit:`dump()` 排出的是降階後的純原語(步驟 14 封板契約),
    // 把它重跑一次就是「直接寫四原語」那一層。
    let primitives = resolved.dump();
    assert!(
        !primitives.contains("sound_change"),
        "dump 必須已降階為原語:\n{primitives}"
    );
    let from_primitives =
        ChangeInterpreter::new(document.clone(), LibrarySpec::default(), "evo:layer2")
            .unwrap()
            .run(
                &UnresolvedChangeSet::parse(&primitives)
                    .unwrap()
                    .resolve(&document, &LibrarySpec::default())
                    .unwrap(),
            )
            .unwrap()
            .document;

    let surfaces = [
        surface(&from_goal.document),
        surface(&from_recipe.document),
        surface(&from_rewrite),
        surface(&from_primitives),
    ];
    assert_eq!(surfaces[0], surfaces[1], "層④ 與層③ 必須落在同一個表層");
    assert_eq!(surfaces[1], surfaces[2], "層③ 與層② 必須落在同一個表層");
    assert_eq!(surfaces[2], surfaces[3], "層② 與層① 必須落在同一個表層");
    // 前提:這三條路真的做了事,不是三個都沒動。
    assert_ne!(surfaces[0], surface(&document));
}

/// 鳥瞰的 Replay 註記:「ChangeSet 序列展開 → 產物是 **Language**(不是 Compiled
/// Grammar);**每節點各自 compile**」。
///
/// 「各自 compile」用兩個並存的兄弟節點來驗:同一個 base、不同的音變,表層必須
/// 各自不同。若 compile 產物在節點間被共用或誤快取,兩者會相等。
#[test]
fn replay_produces_a_language_and_each_node_compiles_on_its_own() {
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    let root = graph.add_root(base()).expect("root");
    let root_doc = graph.snapshot(&root).expect("snapshot").clone();

    let commit = |graph: &mut EvolutionGraph, namespace: &str, body: &str| -> NodeId {
        let mut source = change_set_prelude(&root_doc, &LibrarySpec::default(), namespace).unwrap();
        source.push_str(body);
        graph
            .commit(
                vec![Edge::trunk(root.clone(), source)],
                Nativization::None,
                None,
            )
            .expect("commit")
    };
    let lenited = commit(
        &mut graph,
        "evo:lenite",
        "\n    #0:\n        sound_change(trait(\"Core\"), body: \"b => k\")\n",
    );
    let fortified = commit(
        &mut graph,
        "evo:fortify",
        "\n    #0:\n        sound_change(trait(\"Core\"), body: \"b => d\")\n",
    );

    // Replay 的產物是 Language(拿得到 `.lang` 原文並可再編),不是編好的 grammar。
    assert_eq!(surface(graph.snapshot(&lenited).expect("snapshot")), "ka");
    assert_eq!(surface(graph.snapshot(&fortified).expect("snapshot")), "da");
    // 祖先不動——兄弟節點各自 compile,沒有互相污染。
    assert_eq!(surface(graph.snapshot(&root).expect("snapshot")), "ba");
    graph.verify_all().expect("fsck");
}
