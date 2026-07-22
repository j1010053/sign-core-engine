//! Tshiatūn CLI 功能回測(步驟 11 隨附;新編,擁有者要求)。
//!
//! 以子程序呼叫 submodule 內的 `tshiatun` 二進位(路徑 A 的公開入口),
//! 驗證產品契約(crates/cli:`(規則檔, 詞表) → 詞表′`):成功/失敗/空/邊界
//! 輸入、exit code 語意(0=成功、1=IO/編譯/推導錯、2=用法錯)、`--trace`、
//! Spell-out 表層 vs 無 Spell-out 骨架緊排。**只讀取** tshiatun repo,
//! 不寫入(擁有者裁決 2026-07-20:CLI 測試住工作台)。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Once;

fn tshiatun_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tshiatun")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cli")
        .join(name)
}

fn example(name: &str) -> PathBuf {
    tshiatun_dir().join("examples").join(name)
}

/// 每個測試進程建置一次(冪等;cargo 有快取,只寫 target/ 編譯產物)。
fn binary() -> PathBuf {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let st = Command::new(cargo)
            .args(["build", "-q", "--bin", "tshiatun"])
            .current_dir(tshiatun_dir())
            .status()
            .expect("spawn cargo build");
        assert!(st.success(), "tshiatun binary build failed");
    });
    tshiatun_dir().join("target/debug/tshiatun")
}

fn run(args: &[&dyn AsRef<std::ffi::OsStr>]) -> Output {
    let mut c = Command::new(binary());
    for a in args {
        c.arg(a.as_ref());
    }
    c.output().expect("spawn tshiatun")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

// ── 成功路徑 ──

/// 契約核心:詞表′輸出,一行一詞,Spell-out 表層(8.1)。
#[test]
fn cli_words_prime_with_spellout() {
    let o = run(&[&example("8_1_tonogenesis.qy"), &example("words.txt")]);
    assert!(o.status.success(), "stderr: {}", stderr(&o));
    assert_eq!(stdout(&o), "pa[H]\npa[L]\npa[L]pa[L]\na[M]\n");
    assert!(stderr(&o).is_empty());
}

/// 無 Spell-out 宣告 → 末狀態骨架緊排(8.2:mata 骨架不變)。
#[test]
fn cli_words_prime_without_spellout_prints_skeleton() {
    let o = run(&[&example("8_2_nasal_harmony.qy"), &fixture("words_mata.txt")]);
    assert!(o.status.success(), "stderr: {}", stderr(&o));
    assert_eq!(stdout(&o), "mata\n");
}

/// `--trace`:逐規則推導表(規則名、輸入行、`⇒` 結果行)。
#[test]
fn cli_trace_prints_derivation_table() {
    let o = run(&[
        &"--trace",
        &example("8_1_tonogenesis.qy"),
        &example("words.txt"),
    ]);
    assert!(o.status.success());
    let out = stdout(&o);
    assert!(out.contains("*pa"), "{out}");
    assert!(out.contains("tonogenesis"), "{out}");
    assert!(out.contains("input"), "{out}");
    // trace 模式仍以詞表′收尾(每詞一個 output 行;520e0c8 起取代 ⇒)
    assert_eq!(out.matches("output").count(), 4, "{out}");
    assert!(out.contains("output         pa[H]"), "{out}");
}

#[test]
fn cli_help_and_version() {
    let o = run(&[&"--help"]);
    assert!(o.status.success());
    assert!(stdout(&o).contains("usage:"));
    let o = run(&[&"--version"]);
    assert!(o.status.success());
    assert!(stdout(&o).starts_with("tshiatun "));
}

/// 詞表:空行與 `/*` 註解行跳過。
#[test]
fn cli_word_list_skips_comments_and_blanks() {
    let o = run(&[&example("8_1_tonogenesis.qy"), &fixture("words_mixed.txt")]);
    assert!(o.status.success(), "stderr: {}", stderr(&o));
    assert_eq!(stdout(&o), "pa[H]\n");
}

/// 邊界:空詞表 → 空輸出、exit 0。
#[test]
fn cli_empty_word_list_is_empty_success() {
    let o = run(&[&example("8_1_tonogenesis.qy"), &fixture("empty.txt")]);
    assert!(o.status.success());
    assert_eq!(stdout(&o), "");
}

// ── 失敗路徑(exit code 語意)──

/// 用法錯 = exit 2 + usage。
#[test]
fn cli_usage_errors_exit_2() {
    let o = run(&[&"--bogus-flag"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("usage:"));
    let o = run(&[&example("8_1_tonogenesis.qy")]); // 缺詞表參數
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("usage:"));
}

/// 規則檔不存在 = exit 1 + cannot read。
#[test]
fn cli_missing_rules_file_exit_1() {
    let o = run(&[&"no_such_file.qy", &fixture("words_a.txt")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("cannot read"), "{}", stderr(&o));
}

/// 規則檔語法錯 = exit 1 + 帶行號的錯誤。
#[test]
fn cli_bad_rules_syntax_exit_1_with_line() {
    let o = run(&[&fixture("bad_syntax.qy"), &fixture("words_a.txt")]);
    assert_eq!(o.status.code(), Some(1));
    let e = stderr(&o);
    assert!(e.contains("error:"), "{e}");
    assert!(e.contains("line"), "應含行號定位:{e}");
    assert!(stdout(&o).is_empty(), "失敗不得產生部分輸出");
}

/// 詞含未宣告音段 = exit 1 + 明確訊息(不默默近似)。
#[test]
fn cli_unknown_segment_exit_1() {
    let o = run(&[
        &example("8_1_tonogenesis.qy"),
        &fixture("words_unknown.txt"),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("no declared symbol"), "{}", stderr(&o));
}

/// 零規則規則檔 = 詞表′恆等(原詞照出)。曾為已知缺陷(空詞取代輸入,
/// 記於 docs/13 §6),上游 `520e0c8` 修復(`unwrap_or(fallback)`),
/// 2026-07-20 submodule bump 後翻正為正確契約。
#[test]
fn cli_zero_rules_is_identity() {
    let o = run(&[&fixture("decls_only.qy"), &fixture("words_a.txt")]);
    assert!(o.status.success());
    assert_eq!(stdout(&o), "pa\na\n");
}
