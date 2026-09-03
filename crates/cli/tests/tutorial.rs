//! **教學必須真的跑得起來。**
//!
//! `tutorials/README.md` 訂:「修正教學時必須同步更新可執行範例測試」。
//! 本檔把 `CLI操作教學_v1.md` 裡標記的起始語言抽出來,照教學的順序把每一節的
//! 命令真的跑一遍,並斷言教學宣稱的**關鍵事實**。
//!
//! 為什麼不比對逐字輸出:節點 id 是內容雜湊(64 字元),寫進教學會讓它不可讀,
//! 而且任何無關的引擎改動都會讓教學「壞掉」。故比對的是教學**說了什麼**
//! ——詞條數、群組數、旁註不進語言、State 不影響 replay。

use conlang_cli::{run, CliError};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const TUTORIAL: &str = include_str!("../../../tutorials/CLI操作教學_v1.md");

/// 抽出教學裡標記的起始語言。
fn tutorial_source() -> String {
    let marker = "<!-- conlang-test: tutorial-source -->";
    let normalized = TUTORIAL.replace("\r\n", "\n").replace('\r', "\n");
    normalized
        .split_once(marker)
        .expect("教學要有 tutorial-source 標記")
        .1
        .split_once("```lang\n")
        .expect("標記後要接一個 lang 區塊")
        .1
        .split_once("\n```")
        .expect("區塊要收尾")
        .0
        .to_owned()
}

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new() -> Sandbox {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("conlang-tutorial-{}-{ordinal}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("proto.lang"), tutorial_source()).unwrap();
        // §5 的權重表(欄位以 Tab 分隔——教學特別強調過)
        fs::write(
            dir.join("weights.tsv"),
            "segment\tweight\nk\t3.0\na\t2.0\nt\t1.0\nu\t1.0\n",
        )
        .unwrap();
        Sandbox { dir }
    }

    fn path(&self, name: &str) -> String {
        self.dir.join(name).display().to_string()
    }

    fn project(&self) -> String {
        self.path("myproject")
    }

    fn cli(&self, args: &[&str]) -> Result<String, CliError> {
        let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        let mut out = String::new();
        run(&owned, &mut out)?;
        Ok(out)
    }

    /// 教學 §1–§2:建專案並開起來。
    fn initialised() -> Sandbox {
        let sandbox = Sandbox::new();
        sandbox
            .cli(&[
                "init",
                &sandbox.project(),
                "--from",
                &sandbox.path("proto.lang"),
                "--name",
                "教學語",
                "--namespace",
                "proto",
            ])
            .expect("§1 init");
        sandbox
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.dir.exists() {
            fs::remove_dir_all(&self.dir).unwrap();
        }
    }
}

fn node_id(output: &str, prefix: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("輸出裡沒有 {prefix:?}:{output}"))
        .trim()
        .to_owned()
}

// ── §0–§2 起始語言與專案 ─────────────────────────────────────────────────

/// 🔑 **教學裡那份 `.lang` 真的建得起專案。**
#[test]
fn the_documented_source_initialises_a_project() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .cli(&[
            "init",
            &sandbox.project(),
            "--from",
            &sandbox.path("proto.lang"),
            "--name",
            "教學語",
            "--namespace",
            "proto",
        ])
        .expect("§1");
    assert!(out.contains("created: "), "{out}");
    assert!(out.contains("root: "), "{out}");

    // 教學說專案目錄長這樣
    for entry in ["format", "project.toml", "objects", "nodes"] {
        assert!(
            PathBuf::from(sandbox.project()).join(entry).exists(),
            "教學說會有 {entry}"
        );
    }

    let opened = sandbox.cli(&["open", &sandbox.project()]).expect("§2");
    assert!(opened.contains("project: 教學語"), "{opened}");
    assert!(opened.contains("declaration: project.toml"), "{opened}");
    assert!(opened.contains("nodes: 1"), "{opened}");
}

// ── §3 詞典 ──────────────────────────────────────────────────────────────

/// 🔑 教學宣稱的數字與「過濾走 ontology 閉包」都成立。
#[test]
fn the_lexicon_section_holds() {
    let sandbox = Sandbox::initialised();

    let all = sandbox.cli(&["lexicon", &sandbox.project()]).expect("§3");
    assert!(all.contains("2 / 2 entries"), "{all}");
    assert!(all.contains("STONE") && all.contains("CARRY"), "{all}");

    // 🔑 `kat` 宣告的是 `belongs Noun`,用 `Nominal` 卻篩得到——閉包,不是字串相等
    let nominal = sandbox
        .cli(&["lexicon", &sandbox.project(), "--category", "Nominal"])
        .expect("§3 filter");
    assert!(nominal.contains("1 / 2 entries"), "分母是過濾前:{nominal}");
    assert!(nominal.contains("kat"), "{nominal}");
    assert!(!nominal.contains("tuk"), "動詞不該入選:{nominal}");
    assert!(
        !tutorial_source().contains("belongs Nominal"),
        "教學的前提:來源裡並沒有直接宣告 Nominal"
    );

    // 排序不改集合
    let by_form = sandbox
        .cli(&["lexicon", &sandbox.project(), "--sort", "form"])
        .expect("§3 sort");
    assert!(by_form.contains("2 / 2 entries"), "{by_form}");
}

// ── §4 演化 ──────────────────────────────────────────────────────────────

/// 🔑 一條規則 = 一個**新**節點,舊的不變。
#[test]
fn the_evolution_section_adds_a_node_without_touching_the_old_one() {
    let sandbox = Sandbox::initialised();
    let before = sandbox.cli(&["open", &sandbox.project()]).expect("open");
    let root = node_id(&before, "active: ");
    let root_lexicon = sandbox
        .cli(&["lexicon", &sandbox.project()])
        .expect("lexicon");

    let out = sandbox
        .cli(&["evolve", &sandbox.project(), "--rule", "t => k"])
        .expect("§4");
    assert!(out.contains("nodes: 2"), "{out}");

    // 舊節點逐字元不變——immutable snapshot
    assert_eq!(
        sandbox
            .cli(&["lexicon", &sandbox.project(), "--node", &root])
            .expect("root lexicon"),
        root_lexicon,
        "演化不得改動既有節點"
    );
}

// ── §5 造詞 ──────────────────────────────────────────────────────────────

/// 🔑 候選 → 採用 → 新節點;而**候選分數刻意等權**。
#[test]
fn the_coining_section_lists_then_adopts() {
    let sandbox = Sandbox::initialised();
    let args = [
        "propose",
        &sandbox.project(),
        "--name",
        "miku",
        "--gloss",
        "WATER",
        "--category",
        "Noun",
        "--weights",
        &sandbox.path("weights.tsv"),
        "--template",
        "CVC",
        "--count",
        "5",
    ];
    let listed = sandbox.cli(&args).expect("§5 list");
    assert!(listed.contains("candidates for \"miku\""), "{listed}");
    // 教學說「分數都是 1.000——引擎不定義評分合成公式」
    let scores: Vec<&str> = listed
        .lines()
        .filter_map(|l| l.split("score=").nth(1))
        .collect();
    assert!(!scores.is_empty(), "{listed}");
    assert!(
        scores.iter().all(|s| s.trim() == "1.000"),
        "教學宣稱等權:{scores:?}"
    );

    // 教學的 `CVC` 是類別模板，不是字面字元。直接讀真實 CLI 輸出，避免只驗分數
    // 與採用流程而讓 `/aVa/` 這類假候選蒙混過關。
    let forms: Vec<&str> = listed
        .lines()
        .filter(|line| line.starts_with("  ["))
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    assert_eq!(forms.len(), 5, "教學要求列五個候選:{listed}");
    for form in &forms {
        let segments: Vec<char> = form.trim_matches('/').chars().collect();
        assert!(
            segments.len() == 3
                && matches!(segments[0], 'k' | 't')
                && matches!(segments[1], 'a' | 'u')
                && matches!(segments[2], 'k' | 't'),
            "CVC 必須使用教學宣告的子音／母音類別:{form}"
        );
    }

    // 範例輸出也必須等於同一個固定 seed 的真實候選；否則教學雖然描述對了模板，
    // 卻仍可能列出不存在的具體形式。
    let documented_forms: Vec<&str> = TUTORIAL
        .lines()
        .skip_while(|line| !line.contains("5 candidates for \"miku\""))
        .skip(1)
        .take_while(|line| line.starts_with("  ["))
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    assert!(
        documented_forms.len() >= 2,
        "教學應列出至少兩個具體候選:{TUTORIAL}"
    );
    assert!(
        documented_forms.len() <= forms.len(),
        "教學列出的候選不可多於命令要求的候選數:{documented_forms:?}"
    );
    assert_eq!(
        &forms[..documented_forms.len()],
        documented_forms.as_slice(),
        "教學中的候選必須與固定 seed 的 CLI 輸出一致:{listed}"
    );

    let mut adopt: Vec<&str> = args.to_vec();
    adopt.extend(["--adopt", "0"]);
    let adopted = sandbox.cli(&adopt).expect("§5 adopt");
    assert!(adopted.contains("adopted [0]"), "{adopted}");

    // 教學提醒:新詞在子節點上,root 沒有
    let new_node = adopted
        .lines()
        .find_map(|line| line.rsplit_once(" -> "))
        .map(|(_, id)| id.trim().to_owned())
        .expect("要回報新節點");
    assert!(sandbox
        .cli(&["lexicon", &sandbox.project(), "--node", &new_node])
        .expect("new lexicon")
        .contains("miku"));
    assert!(!sandbox
        .cli(&["lexicon", &sandbox.project()])
        .expect("root lexicon")
        .contains("miku"));
}

/// 🔑 **`stats` 的輸出不會自動變成 `propose` 的分佈**(§6.1)。
///
/// 這是教學 §6 ① 的主張,也是本工具最容易被誤解的一點。
#[test]
fn the_projection_never_becomes_a_sampling_source() {
    let sandbox = Sandbox::initialised();
    // 統計看得到東西
    let stats = sandbox.cli(&["stats", &sandbox.project()]).expect("§6");
    assert!(stats.contains("distinct"), "{stats}");
    assert!(stats.contains("非抽樣來源"), "{stats}");

    // 但沒給 --weights 就是提不出候選,而且說得出為什麼
    let error = sandbox
        .cli(&["propose", &sandbox.project(), "--name", "x", "--gloss", "X"])
        .expect_err("§5 沒有分佈就該拒絕");
    assert!(format!("{error}").contains("§6.1"), "{error}");
}

// ── §6 統計 ──────────────────────────────────────────────────────────────

/// 教學宣稱的兩種切分口徑都說得出自己是誰。
#[test]
fn the_stats_section_reports_its_segmentation() {
    let sandbox = Sandbox::initialised();
    let matched = sandbox
        .cli(&[
            "stats",
            &sandbox.project(),
            "--weights",
            &sandbox.path("weights.tsv"),
        ])
        .expect("§6 with weights");
    assert!(matched.contains("longest-match"), "{matched}");

    let bare = sandbox
        .cli(&["stats", &sandbox.project()])
        .expect("§6 bare");
    assert!(bare.contains("per-character"), "{bare}");
}

// ── §7 分群 ──────────────────────────────────────────────────────────────

/// 🔑 分數帶得出來源;閾值調高就切碎。
#[test]
fn the_grouping_section_holds() {
    let sandbox = Sandbox::initialised();
    sandbox
        .cli(&["evolve", &sandbox.project(), "--rule", "t => k"])
        .expect("evolve");

    let groups = sandbox.cli(&["groups", &sandbox.project()]).expect("§7");
    assert!(
        groups.contains("measure: exploratory_heuristic_v1"),
        "{groups}"
    );
    assert!(groups.contains("threshold: 0.6"), "{groups}");
    let count = |out: &str| out.lines().filter(|l| l.starts_with("  ")).count();
    assert_eq!(count(&groups), 1, "教學說還在同一群:{groups}");

    let split = sandbox
        .cli(&["groups", &sandbox.project(), "--threshold", "1.1"])
        .expect("§7 high");
    assert_eq!(count(&split), 2, "閾值調高就切碎:{split}");
}

#[test]
fn the_grouping_tutorial_explains_per_event_reach() {
    for required in [
        "係數 × Σ max(event.before, event.after)",
        "9 筆各只碰 1 個詞的 local 事件",
        "各碰全部 9 個詞的 global 事件",
        "0.9524",
        "0.5714",
    ] {
        assert!(
            TUTORIAL.contains(required),
            "grouping tutorial must retain the event-reach explanation: {required:?}"
        );
    }
}

// ── §8 旁註 ──────────────────────────────────────────────────────────────

/// 🔑 教學的主張:**寫旁註不會改變詞典或統計**(07 §5c 正交於本體)。
#[test]
fn annotations_do_not_change_the_language() {
    let sandbox = Sandbox::initialised();
    let lexicon_before = sandbox
        .cli(&["lexicon", &sandbox.project()])
        .expect("before");
    let stats_before = sandbox.cli(&["stats", &sandbox.project()]).expect("before");

    sandbox
        .cli(&[
            "annotate",
            &sandbox.project(),
            "--path",
            "culture.md",
            "--set",
            "石頭在此文化中象徵盟約",
        ])
        .expect("§8 write");

    let listed = sandbox
        .cli(&["annotate", &sandbox.project()])
        .expect("§8 list");
    assert!(
        listed.contains("annotations: 1") && listed.contains("culture.md"),
        "{listed}"
    );
    assert!(sandbox
        .cli(&["annotate", &sandbox.project(), "--path", "culture.md"])
        .expect("§8 read")
        .contains("象徵盟約"));

    assert_eq!(
        sandbox
            .cli(&["lexicon", &sandbox.project()])
            .expect("after"),
        lexicon_before,
        "旁註不是語言內容"
    );
    assert_eq!(
        sandbox.cli(&["stats", &sandbox.project()]).expect("after"),
        stats_before
    );
}

// ── §9 State ─────────────────────────────────────────────────────────────

/// 🔑 教學的核心主張:**State 影響「下一次生成什麼」,但 replay 逐位元不變**。
#[test]
fn state_is_editable_without_disturbing_any_existing_node() {
    let sandbox = Sandbox::initialised();
    sandbox
        .cli(&["evolve", &sandbox.project(), "--rule", "t => k"])
        .expect("evolve");
    let lexicon_before = sandbox
        .cli(&["lexicon", &sandbox.project()])
        .expect("before");
    let groups_before = sandbox
        .cli(&["groups", &sandbox.project()])
        .expect("before");

    let out = sandbox
        .cli(&[
            "state",
            &sandbox.project(),
            "--set-time",
            "約 800–1100",
            "--set-region",
            "河谷北岸",
        ])
        .expect("§9");
    assert!(out.contains("time: 約 800–1100"), "{out}");
    assert!(out.contains("region: 河谷北岸"), "{out}");
    assert!(out.contains("contacts: 0"), "{out}");

    // **既有節點的重放產物逐字元不變**——State 是雜湊外的
    assert_eq!(
        sandbox
            .cli(&["lexicon", &sandbox.project()])
            .expect("after"),
        lexicon_before
    );
    assert_eq!(
        sandbox.cli(&["groups", &sandbox.project()]).expect("after"),
        groups_before
    );

    // 而且真的落盤了
    assert!(sandbox
        .cli(&["state", &sandbox.project()])
        .expect("reread")
        .contains("河谷北岸"));
}
