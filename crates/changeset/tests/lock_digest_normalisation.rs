//! lock digest 是「引擎讀到什麼」的函數,不是「磁碟上躺著什麼」的函數。
//!
//! # 起因
//!
//! 套件原始碼由 `include_str!` 在**編譯期直接讀工作樹**,於是 digest 吃的是
//! 工作樹上的原始位元組。工作樹帶有與內容無關的變異——Windows 檢出預設
//! `core.autocrlf=true`,沒被 `.gitattributes` 釘住的副檔名就是 CRLF。
//! 2026-08-07 的 CI 正是這樣掛的:`.gitattributes` 只釘了 `.lang`/`.chg`,
//! 而 `.conf`/`.tsv` 也進 digest。
//!
//! 釘 `.gitattributes` 只治症狀:下一個副檔名還會踩,而 R9-a 的注入式套件
//! (host 從使用者磁碟讀,如本檔的測試)根本不受 `.gitattributes` 保護。
//!
//! # 這組測試守的界線
//!
//! 正規化的範圍是「**引擎解析時本來就看不見的差異,一分不多**」。
//! 只放寬 `\r\n`,其餘一律維持敏感——因為讓兩份真的不同的內容撞同一個
//! digest,比 golden churn 嚴重得多(那是 P26 可重現性的破口)。
//!
//! 所以底下每一條「應相同」都配一條「應不同」的反例。

use conlang_changeset::__lock_content_for_tests;
use conlang_language::{LibraryCatalog, PackageFile, PackageSources};

const CODE: &str = "trait probeMarker:\n    belongs Noun\n";
const DATA: &str = "key\tvalue\nalpha\t1\n";

/// 造一個注入套件,`code` 與 `data` 由呼叫端指定。
fn sources(code: &str, data: &str) -> PackageSources {
    PackageSources {
        config: "kind = plugin\nname = probe\nversion = 0.1.0\n\
                 rule_namespace = plugin:probe\nenabled = true\npriority = 0\n\
                 requires = std:core\ncode = code/main.lang\ndata = data/notes.tsv\n"
            .to_owned(),
        exports: "stable_id\tkind\talias\nplugin:probe:Marker\ttrait\tprobeMarker\n".to_owned(),
        code: code.to_owned(),
        data: data.to_owned(),
        data_files: vec![PackageFile {
            path: "data/notes.tsv".to_owned(),
            source: data.to_owned(),
        }],
        ..PackageSources::default()
    }
}

/// 取該套件進 digest 的完整內容。
fn lock_content(code: &str, data: &str) -> String {
    let catalog =
        LibraryCatalog::with_packages([sources(code, data)]).expect("注入套件應通過同一套驗證");
    let package = catalog
        .packages()
        .iter()
        .find(|package| package.rule_namespace == "plugin:probe")
        .expect("剛注入的套件應在 catalog 內");
    __lock_content_for_tests(package)
}

fn crlf(text: &str) -> String {
    text.replace('\n', "\r\n")
}

/// 核心:CRLF 檢出與 LF 檢出必須得到同一份 lock 內容。
#[test]
fn crlf_and_lf_checkouts_lock_identically() {
    assert_eq!(
        lock_content(&crlf(CODE), &crlf(DATA)),
        lock_content(CODE, DATA),
        "同一份套件在 Windows(CRLF)與 Linux(LF)檢出下必須算出同一個 lock"
    );
}

/// 前一條的**前提**:兩份輸入的原始位元組真的不同。
///
/// 少了這條,`lock_content` 只要退化成常數就能讓上面那條過。
#[test]
fn the_two_checkouts_really_do_differ_in_bytes() {
    assert_ne!(crlf(CODE), CODE, "CRLF 版與 LF 版的位元組本就該不同");
    assert_ne!(crlf(DATA), DATA);
}

/// 陽性對照:內容**真的**變了,lock 必須跟著變。
///
/// 正規化不得把 digest 弄鈍——這是 P26 的底線。
#[test]
fn a_real_content_change_still_changes_the_lock() {
    let changed_code = "trait probeMarker:\n    belongs Verb\n";
    assert_ne!(
        lock_content(changed_code, DATA),
        lock_content(CODE, DATA),
        "改了 code 卻算出同一個 lock ⇒ digest 失效,破 P26"
    );

    let changed_data = "key\tvalue\nalpha\t2\n";
    assert_ne!(
        lock_content(CODE, changed_data),
        lock_content(CODE, DATA),
        "改了 data 卻算出同一個 lock ⇒ digest 失效,破 P26"
    );
}

/// 近似反例①:**落單的 `\r`** 不在正規化範圍內。
///
/// `str::lines()` 只切 `\n` 並丟掉行尾的 `\r`;行中間的 `\r` 會活過 `.trim()`,
/// 也就是**引擎看得見**。看得見的差異就必須反映在 digest 上。
///
/// 放在 `data` 而不是 `code`:把 `\r` 插進 trait 名字會直接讓 export alias
/// 驗證失敗(`MissingAlias`)——那本身就是「引擎看得見落單 `\r`」的旁證,
/// 但會讓測試掛在建構階段而不是斷言階段,失去鑑別力。
#[test]
fn a_bare_carriage_return_is_not_normalised_away() {
    let bare_cr = "key\tvalue\nal\rpha\t1\n";
    assert_ne!(
        lock_content(CODE, bare_cr),
        lock_content(CODE, DATA),
        "落單的 \\r 是引擎看得見的內容,不得被正規化吃掉"
    );
}

/// 近似反例②:**尾端空白**不在正規化範圍內。
///
/// 不是每個消費端都 trim(`data` 更是原文保存),所以不能假設它不可見。
#[test]
fn trailing_whitespace_is_not_normalised_away() {
    let trailing = "trait probeMarker:\n    belongs Noun   \n";
    assert_ne!(
        lock_content(trailing, DATA),
        lock_content(CODE, DATA),
        "尾端空白未經證實不可見,digest 應維持敏感"
    );
}

/// 逐檔來源(`data_sources`)也要走同一條正規化——否則多檔套件仍會漏。
///
/// **必須是兩個 data 檔**:`package_lock_content` 在 `data_sources.len() <= 1`
/// 時走的是合併後的 `package.data`,單檔套件根本進不了那個逐檔迴圈。
/// 先前這條測試只放一個檔,等於沒有覆蓋到它自稱在守的分支。
fn multi_data_sources(second: &str) -> PackageSources {
    PackageSources {
        config: "kind = plugin\nname = probe\nversion = 0.1.0\n\
                 rule_namespace = plugin:probe\nenabled = true\npriority = 0\n\
                 requires = std:core\ncode = code/main.lang\n\
                 data = data/notes.tsv, data/extra.tsv\n"
            .to_owned(),
        exports: "stable_id\tkind\talias\nplugin:probe:Marker\ttrait\tprobeMarker\n".to_owned(),
        code: CODE.to_owned(),
        data: format!("{DATA}{second}"),
        data_files: vec![
            PackageFile {
                path: "data/notes.tsv".to_owned(),
                source: DATA.to_owned(),
            },
            PackageFile {
                path: "data/extra.tsv".to_owned(),
                source: second.to_owned(),
            },
        ],
        ..PackageSources::default()
    }
}

#[test]
fn per_file_sources_are_normalised_too() {
    let extra = "beta\t2\n";
    let content = |s: PackageSources| {
        let catalog = LibraryCatalog::with_packages([s]).expect("注入套件應通過驗證");
        __lock_content_for_tests(
            catalog
                .packages()
                .iter()
                .find(|package| package.rule_namespace == "plugin:probe")
                .expect("套件在 catalog 內"),
        )
    };

    assert_eq!(
        content(multi_data_sources(&crlf(extra))),
        content(multi_data_sources(extra)),
        "逐檔來源的換行符差異同樣不該改變 lock"
    );
    // 陽性對照:逐檔迴圈仍看得見真的內容變動
    assert_ne!(
        content(multi_data_sources("beta\t3\n")),
        content(multi_data_sources(extra)),
        "逐檔來源改了內容卻算出同一個 lock ⇒ digest 失效"
    );
}
