//! **步驟 22 的測試出口**(§0.2:不存在沒有測試綠燈的階段)。
//!
//! UI 沒有天然的綠燈,故出口定在這裡:整條路端到端跑得通且**輸出決定性**。
//! UI 之後呼叫的是同一組 `conlang-command`,這組測試因此也守著它。
//!
//! 覆蓋:專案開啟 → 編譯 → 查詢 → 命令 → 提交 → 落盤 → 重開仍在。

use conlang_changeset::evolution::EvolutionGraph;
use conlang_cli::{run, CliError};
use conlang_language::{LanguageDocument, LibraryId, LibraryKind, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "Symbol k\nSymbol a\nSymbol t\nSymbol u\n\nClass consonant {k, t}\nClass vowel {a, u}\n\n\
global trait Core:\n\n\
sign dog:\n    belongs Noun\n    phon:\n        /tuk/\n    sem:\n        senses:\n            core = DOG\n\
sign run:\n    belongs Verb\n    phon:\n        /kat/\n    sem:\n        senses:\n            core = RUN\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct Project(PathBuf);

impl Project {
    /// 造一個真的專案目錄:store + 一個 root + `project.toml`。
    fn new(name: &str) -> Project {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-cli-{name}-{}-{ordinal}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        let store = GraphStore::init(&path).expect("init");
        let libraries = LibrarySpec::default();
        let mut graph = EvolutionGraph::new(libraries.clone());
        graph
            .add_root(LanguageDocument::import_new_root(SOURCE, "cli:root").expect("root"))
            .expect("add_root");
        store.save(&graph).expect("save");
        let mut project = ProjectDocument::from_spec(&libraries);
        project.name = Some(name.to_owned());
        store.write_project(&project).expect("project");
        Project(path)
    }

    fn arg(&self) -> String {
        self.0.display().to_string()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn cli(args: &[&str]) -> Result<String, CliError> {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let mut out = String::new();
    run(&owned, &mut out)?;
    Ok(out)
}

// ── 開啟 ─────────────────────────────────────────────────────────────────

/// 🔑 **`open` 讀得到 `project.toml`,而不是猜預設。**
#[test]
fn open_reports_the_declared_project() {
    let project = Project::new("tshiatun");
    let out = cli(&["open", &project.arg()]).expect("open");
    assert!(out.contains("project: tshiatun"), "{out}");
    assert!(out.contains("declaration: project.toml"), "{out}");
    assert!(out.contains("nodes: 1"), "{out}");
    assert!(out.contains("active: "), "{out}");
}

/// 沒有宣告檔的舊 store 照樣開得起來,且**說得出來**它在用預設。
#[test]
fn open_says_so_when_there_is_no_declaration() {
    let project = Project::new("legacy");
    fs::remove_file(project.0.join("project.toml")).expect("remove");
    let out = cli(&["open", &project.arg()]).expect("open");
    assert!(out.contains("使用預設套件組合"), "{out}");
}

// ── 查詢 ─────────────────────────────────────────────────────────────────

/// 🔑 **端到端:專案 → 編譯 → 詞典。**
#[test]
fn lexicon_lists_the_words_with_forms_and_glosses() {
    let project = Project::new("lex");
    let out = cli(&["lexicon", &project.arg()]).expect("lexicon");
    assert!(out.contains("2 / 2 entries"), "{out}");
    assert!(out.contains("dog"), "{out}");
    assert!(out.contains("kat"), "底層形:{out}");
    assert!(out.contains("DOG"), "gloss:{out}");
    assert!(out.contains("run"), "{out}");
}

/// 過濾走 ontology 閉包——`Nominal` 選得到 `belongs Noun`。
#[test]
fn lexicon_filters_by_category_through_the_ontology() {
    let project = Project::new("filter");
    let out = cli(&["lexicon", &project.arg(), "--category", "Nominal"]).expect("lexicon");
    assert!(out.contains("1 / 2 entries"), "分母是過濾前:{out}");
    assert!(out.contains("dog"), "{out}");
    assert!(!out.contains("run"), "動詞不該入選:{out}");
}

/// 依輸出順序取出詞條名。
fn order(out: &str) -> Vec<String> {
    out.lines()
        .filter(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect()
}

/// 🔑 **`--sort` 真的改變順序,而入選集合不變。**
///
/// fixture 刻意讓三種排序給出**不同**答案:`dog` 的底層形是 `/tuk/`、
/// `run` 是 `/kat/`,故名字序(dog, run)與底層形序(run, dog)相反。
///
/// 判別性:先前的 fixture 讓三種排序碰巧同序,於是「忽略 `--sort`」這個突變
/// **活了下來**——測試寫了,但任何斷言都分不開。
#[test]
fn lexicon_sorting_actually_reorders_without_changing_the_set() {
    let project = Project::new("sort");
    let by_name = cli(&["lexicon", &project.arg(), "--sort", "name"]).expect("name");
    let by_form = cli(&["lexicon", &project.arg(), "--sort", "form"]).expect("form");
    let by_gloss = cli(&["lexicon", &project.arg(), "--sort", "gloss"]).expect("gloss");

    assert_eq!(order(&by_name), vec!["dog", "run"], "名字序");
    assert_eq!(order(&by_form), vec!["run", "dog"], "底層形序 kat < tuk");
    assert_eq!(order(&by_gloss), vec!["dog", "run"], "gloss 序 DOG < RUN");

    // 入選集合不變——只是順序不同
    let mut sorted = order(&by_form);
    sorted.sort();
    assert_eq!(sorted, order(&by_name));
    for out in [&by_name, &by_form, &by_gloss] {
        assert!(out.contains("2 / 2 entries"), "{out}");
    }
}

/// 決定性:同一個命令跑兩次逐字元相同。
#[test]
fn the_same_command_produces_byte_identical_output() {
    let project = Project::new("determinism");
    assert_eq!(
        cli(&["lexicon", &project.arg()]).expect("a"),
        cli(&["lexicon", &project.arg()]).expect("b")
    );
}

// ── State(步驟 20 欠的「UI 顯示」,先在 CLI 還)──────────────────────────

/// 🔑 **讀得到、寫得進、再讀回來還在。**
#[test]
fn state_can_be_shown_and_edited() {
    let project = Project::new("state");

    let empty = cli(&["state", &project.arg()]).expect("state");
    assert!(empty.contains("time: -"), "{empty}");
    assert!(empty.contains("contacts: 0"), "{empty}");

    let written = cli(&[
        "state",
        &project.arg(),
        "--set-time",
        "約 800–1100",
        "--set-region",
        "河谷北岸",
    ])
    .expect("write");
    assert!(written.contains("time: 約 800–1100"), "{written}");
    assert!(written.contains("region: 河谷北岸"), "{written}");

    // 落盤了——重讀仍在
    let reread = cli(&["state", &project.arg()]).expect("reread");
    assert!(reread.contains("河谷北岸"), "{reread}");
}

// ── 命令 + 提交 + 落盤 ───────────────────────────────────────────────────

/// 🔑 **`evolve` 走完整條路:降階四原語 → statement → 提交 → 落盤。**
///
/// 判別性:重開專案時新節點必須還在(證明真的寫進去了,不是只在記憶體)。
#[test]
fn evolve_commits_a_node_that_survives_reopening() {
    let project = Project::new("evolve");
    assert!(cli(&["open", &project.arg()]).expect("before").contains("nodes: 1"));

    let out = cli(&["evolve", &project.arg(), "--rule", "t => k"]).expect("evolve");
    assert!(out.contains("committed: "), "{out}");
    assert!(out.contains("nodes: 2"), "{out}");

    // **重開**——這是「有沒有真的落盤」的唯一證明
    let after = cli(&["open", &project.arg()]).expect("after");
    assert!(after.contains("nodes: 2"), "重開後應仍是兩個節點:{after}");
}

/// 連續兩次演化各自成節點。
#[test]
fn two_evolutions_produce_two_nodes() {
    let project = Project::new("twice");
    cli(&["evolve", &project.arg(), "--rule", "t => k"]).expect("first");
    cli(&["evolve", &project.arg(), "--rule", "a => u"]).expect("second");
    assert!(cli(&["open", &project.arg()]).expect("open").contains("nodes: 3"));
}

// ── 分群 ─────────────────────────────────────────────────────────────────

/// 🔑 **分群結果帶得出是哪套互通度算的。**
#[test]
fn groups_reports_the_measure_and_threshold() {
    let project = Project::new("groups");
    cli(&["evolve", &project.arg(), "--rule", "t => k"]).expect("evolve");

    let out = cli(&["groups", &project.arg()]).expect("groups");
    assert!(out.contains("measure: exploratory_heuristic_v1"), "{out}");
    assert!(out.contains("threshold: 0.6"), "{out}");
    // 只改一條規則 ⇒ 高度相通 ⇒ 同一群
    assert_eq!(out.lines().filter(|l| l.starts_with("  ")).count(), 1, "{out}");
}

/// 閾值調到 1 以上 ⇒ 每個節點自成一群。
#[test]
fn a_high_threshold_splits_every_node_into_its_own_group() {
    let project = Project::new("threshold");
    cli(&["evolve", &project.arg(), "--rule", "t => k"]).expect("evolve");
    let out = cli(&["groups", &project.arg(), "--threshold", "1.1"]).expect("groups");
    assert_eq!(out.lines().filter(|l| l.starts_with("  ")).count(), 2, "{out}");
}

// ── 錯誤路徑 ─────────────────────────────────────────────────────────────

/// 用法錯誤說得出哪裡錯,並附上用法。
#[test]
fn usage_errors_explain_themselves() {
    for (args, needle) in [
        (vec![], "缺少子命令"),
        (vec!["nonsense"], "不認得的子命令"),
        (vec!["open"], "缺少專案路徑"),
        (vec!["lexicon", "/tmp", "--sort"], "缺少值"),
        (vec!["lexicon", "/tmp", "oops"], "預期旗標"),
    ] {
        let error = cli(&args).expect_err("應為錯誤");
        let text = format!("{error}");
        assert!(text.contains(needle), "{needle:?} 不在 {text:?}");
        assert!(text.contains("conlang <command>"), "要附用法:{text}");
    }
}

/// `evolve` 沒給規則 ⇒ 明確錯誤,不是靜默什麼都不做。
#[test]
fn evolve_without_a_rule_is_refused() {
    let project = Project::new("no-rule");
    let error = cli(&["evolve", &project.arg()]).expect_err("應拒絕");
    assert!(format!("{error}").contains("需要 --rule"));
    // 而且**沒有留下節點**
    assert!(cli(&["open", &project.arg()]).expect("open").contains("nodes: 1"));
}

/// 指向不存在的節點 ⇒ 明確錯誤。
#[test]
fn an_unknown_node_is_refused() {
    let project = Project::new("bad-node");
    let error = cli(&["lexicon", &project.arg(), "--node", "not-a-digest"]).expect_err("應拒絕");
    assert!(format!("{error}").contains("CLI_UNKNOWN_NODE"), "{error}");
}

/// 不是專案的目錄 ⇒ 明確錯誤。
#[test]
fn a_directory_that_is_not_a_project_is_refused() {
    let temp = std::env::temp_dir().join(format!("conlang-cli-notaproject-{}", std::process::id()));
    fs::create_dir_all(&temp).expect("mkdir");
    let error = cli(&["open", &temp.display().to_string()]).expect_err("應拒絕");
    assert!(format!("{error}").starts_with("PERSISTENCE_"), "{error}");
    fs::remove_dir_all(&temp).ok();
}

/// 🔑 **宣告了載不到的套件 ⇒ 開啟時就報錯,不是等到查詢才炸。**
///
/// 這條抓到一個真缺陷:只有一個 root 的專案**沒有任何 changeset 要 replay**,
/// 而 `open` 也不編譯——所以套件宣告在載入時完全不被碰到,打錯一個名字會一路
/// 安靜到使用者去查詢時才炸在看不懂的地方。`Session::open_project` 因此改為
/// 當場解析一次。
#[test]
fn a_project_declaring_an_unknown_package_fails_at_open() {
    let project = Project::new("bad-package");
    let store = GraphStore::open(&project.0).expect("open");
    let mut declaration = ProjectDocument::from_spec(&LibrarySpec {
        natural: Some(LibraryId::new(LibraryKind::Natural, "no-such-language")),
        ..LibrarySpec::default()
    });
    declaration.name = Some("bad".to_owned());
    store.write_project(&declaration).expect("write");

    let error = cli(&["open", &project.arg()]).expect_err("載不到就該報錯");
    let text = format!("{error}");
    assert!(text.contains("no-such-language"), "要指名是哪個套件:{text}");

    // 正向控制組:換回載得到的宣告就開得起來,否則這條可能只是「什麼都失敗」
    store
        .write_project(&ProjectDocument::from_spec(&LibrarySpec::default()))
        .expect("write");
    cli(&["open", &project.arg()]).expect("正常宣告要開得起來");
}

// ── 候選詞 / 統計 / 旁註 ─────────────────────────────────────────────────

fn weights_file(project: &Project, rows: &str) -> String {
    let path = project.0.join("weights.tsv");
    fs::write(&path, format!("segment\tweight\n{rows}")).expect("write weights");
    path.display().to_string()
}

/// 🔑 **沒有 `--weights` 就報錯,而且說得出為什麼。**
///
/// 分佈只有三層,而 E1 目前**沒有實際資料**(步驟 19 記明)。
/// 判別性:若哪天有人把 `stats` 的投影偷偷接成抽樣來源,這條會綠掉——
/// 而那正是 §6.1「統計投影已移出抽樣棧」禁止的。
#[test]
fn propose_refuses_to_guess_a_distribution() {
    let project = Project::new("propose-nodist");
    let error = cli(&[
        "propose", &project.arg(), "--name", "x", "--gloss", "X",
    ])
    .expect_err("應拒絕");
    let text = format!("{error}");
    assert!(text.contains("--weights"), "{text}");
    assert!(text.contains("§6.1"), "要說明為什麼不能拿投影頂:{text}");
}

/// **手動層分佈 → 候選;手動模式下引擎只排序,不替使用者選。**
///
/// 誠實記下:把 `ranked()` 換成不排序,本測試**不會紅**。
/// `DistributionGenerator` 刻意給每個候選 `score: 1.0`
/// (§6.4:引擎不定義評分合成公式——「它已依分佈抽樣,候選之間無進一步高下
/// 可言」),等權排序即恆等。那是**等價突變**,不是測試缺口。
///
/// `ranked()` 的排序行為在有分數差異的情境下另有判別測試:
/// `generate/tests/coining.rs` 斷言 `ordered[0].score == 0.99`(最高分排最前)。
#[test]
fn propose_lists_ranked_candidates_from_the_manual_layer() {
    let project = Project::new("propose");
    let path = weights_file(&project, "k\t3.0\na\t2.0\nt\t1.0\n");

    let out = cli(&[
        "propose", &project.arg(), "--name", "coined", "--gloss", "THING",
        "--weights", &path, "--template", "CV", "--count", "4",
    ])
    .expect("propose");

    assert!(out.contains("candidates for \"coined\""), "{out}");
    let scores: Vec<f64> = out
        .lines()
        .filter_map(|line| line.split("score=").nth(1))
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    assert!(!scores.is_empty(), "要列出候選:{out}");
    assert!(
        scores.windows(2).all(|w| w[0] >= w[1]),
        "必須依分數遞減:{scores:?}"
    );
    // 候選只由分佈的鍵組成
    for line in out.lines().filter(|l| l.starts_with("  [")) {
        let form: String = line.chars().filter(|c| "kat".contains(*c)).collect();
        assert!(!form.is_empty(), "{line}");
    }
}

/// 有一張權重表不等於它能抽樣；全零表不能被偽裝成 P70 的合法零候選。
#[test]
fn propose_rejects_an_all_zero_distribution() {
    let project = Project::new("propose-all-zero");
    let path = weights_file(&project, "k\t0.0\na\t0.0\n");

    let error = cli(&[
        "propose", &project.arg(), "--name", "coined", "--gloss", "THING", "--weights", &path,
    ])
    .expect_err("全零權重不得輸出合法的零候選");
    assert!(
        format!("{error}").contains("GENERATE_DISTRIBUTION_NO_POSITIVE_WEIGHT"),
        "要保留可行動的抽樣錯誤:{error}"
    );
}

/// 🔑 **`--adopt` 走完整條路:候選 → Builder → 四原語 → 節點 → 落盤。**
#[test]
fn adopting_a_candidate_commits_a_node_that_survives_reopening() {
    let project = Project::new("adopt");
    let path = weights_file(&project, "k\t1.0\na\t1.0\n");
    assert!(cli(&["open", &project.arg()]).expect("before").contains("nodes: 1"));

    let out = cli(&[
        "propose", &project.arg(), "--name", "coined", "--gloss", "THING",
        "--category", "Noun", "--weights", &path, "--template", "CV",
        "--count", "3", "--adopt", "0",
    ])
    .expect("adopt");
    assert!(out.contains("adopted [0]"), "{out}");

    let after = cli(&["open", &project.arg()]).expect("after");
    assert!(after.contains("nodes: 2"), "{after}");

    // **`open` 一律停在第一個 root**,故要看新詞得指名那個節點。
    // (adopt 的輸出就帶著它——UI 之後也是這樣拿。)
    let new_node = out
        .lines()
        .find_map(|line| line.rsplit_once(" -> "))
        .map(|(_, id)| id.trim().to_owned())
        .expect("adopt 要回報新節點 id");
    let lexicon = cli(&["lexicon", &project.arg(), "--node", &new_node]).expect("lexicon");
    assert!(lexicon.contains("coined"), "新詞應在新節點的詞典裡:{lexicon}");

    // 對照:root 的詞典**沒有**它——證明造詞真的落在子節點上
    let root_lexicon = cli(&["lexicon", &project.arg()]).expect("root lexicon");
    assert!(!root_lexicon.contains("coined"), "root 不該有:{root_lexicon}");
}

/// 序號超出範圍 ⇒ 明確錯誤,且不留下節點。
#[test]
fn adopting_a_candidate_that_does_not_exist_is_refused() {
    let project = Project::new("adopt-oob");
    let path = weights_file(&project, "k\t1.0\na\t1.0\n");
    let error = cli(&[
        "propose", &project.arg(), "--name", "x", "--gloss", "X",
        "--weights", &path, "--count", "2", "--adopt", "99",
    ])
    .expect_err("應拒絕");
    assert!(format!("{error}").contains("沒有第 99 個候選"));
    assert!(cli(&["open", &project.arg()]).expect("open").contains("nodes: 1"));
}

/// 🔑 **統計是報表,而且說得出自己的切分口徑。**
///
/// 判別性:給了 `--weights` 就用最長匹配,沒給就逐字元——輸出必須不同,
/// 否則使用者無從得知多字元音段有沒有被拆開(§6.6)。
#[test]
fn stats_reports_its_segmentation_basis() {
    let project = Project::new("stats");

    let bare = cli(&["stats", &project.arg()]).expect("stats");
    assert!(bare.contains("per-character"), "{bare}");
    assert!(bare.contains("非抽樣來源"), "要標明它不是先驗:{bare}");
    // /tuk/ + /kat/ 逐字元:t,u,k,k,a,t
    assert!(bare.contains("k        2"), "{bare}");

    let path = weights_file(&project, "k\t1.0\na\t1.0\nt\t1.0\nu\t1.0\n");
    let matched = cli(&["stats", &project.arg(), "--weights", &path]).expect("stats");
    assert!(matched.contains("longest-match"), "{matched}");
    assert_ne!(
        bare.lines().next(),
        matched.lines().next(),
        "兩種口徑必須說得出差別"
    );
}

/// 多字元音段:給了清單就整段算,沒給就被拆開——這正是 §6.6 的理由。
#[test]
fn a_multi_character_segment_needs_the_inventory_to_stay_whole() {
    let project = Project::new("affricate");
    let store = GraphStore::open(&project.0).expect("open");
    let libraries = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(libraries.clone());
    graph
        .add_root(
            LanguageDocument::import_new_root(
                "global trait Core:\n\nsign c:\n    belongs Noun\n    phon:\n        /t\u{361}\u{283}/\n",
                "affr:root",
            )
            .expect("root"),
        )
        .expect("add_root");
    // 換掉整個 store 的內容:重新 init 一個乾淨的
    fs::remove_dir_all(project.0.join("nodes")).expect("clear");
    fs::create_dir_all(project.0.join("nodes")).expect("mk");
    store.save(&graph).expect("save");

    let path = weights_file(&project, "t\u{361}\u{283}\t1.0\n");
    let whole = cli(&["stats", &project.arg(), "--weights", &path]).expect("stats");
    assert!(whole.contains("1 distinct"), "整段算一個:{whole}");

    let split = cli(&["stats", &project.arg()]).expect("stats");
    assert!(split.contains("3 distinct"), "逐字元拆成三個:{split}");
}

/// 🔑 **旁註可寫可讀可列,且不進 replay。**
#[test]
fn annotations_round_trip_and_stay_outside_the_language() {
    let project = Project::new("annotate");

    let empty = cli(&["annotate", &project.arg()]).expect("list");
    assert!(empty.contains("annotations: 0"), "{empty}");

    cli(&[
        "annotate", &project.arg(), "--path", "culture.md", "--set", "牧民用語,忌諱直呼",
    ])
    .expect("write");

    let listed = cli(&["annotate", &project.arg()]).expect("list");
    assert!(listed.contains("annotations: 1"), "{listed}");
    assert!(listed.contains("culture.md"), "{listed}");

    let read = cli(&["annotate", &project.arg(), "--path", "culture.md"]).expect("read");
    assert!(read.contains("忌諱直呼"), "{read}");

    // **不進語言**:詞典與統計完全不受影響(07 §5c 旁註層正交於本體)
    assert_eq!(
        cli(&["lexicon", &project.arg()]).expect("lexicon"),
        cli(&["lexicon", &project.arg()]).expect("lexicon 再一次")
    );
    assert!(!cli(&["lexicon", &project.arg()])
        .expect("lexicon")
        .contains("忌諱"));
}
