//! R9-a 出口:**package 可由 host 注入,不必是編譯期常數**。
//!
//! 此前 `LibraryCatalog::embedded()` 是唯一入口,吃 `EMBEDDED_PACKAGES` 常數陣列,
//! 且 `LibraryPackage` 的 `code`/`functions`/`data` 全是 `&'static str`
//! ——**型別層面就把 package 鎖死在編譯期**。後果:
//!
//! - `LibraryKind::Plugin` 在任何執行路徑上都不可達(`lib/plugin/` 只有 README);
//! - E1 先驗庫(PHOIBLE / Grambank 全集)無處可去——它們比現有 58 KB 的內嵌內容
//!   大好幾個數量級,不可能 `include_str!`。步驟 19 的真正阻塞點即此。
//!
//! `language` 仍不碰 `std::fs`(§4、wasm 綠):讀檔是 host 的事,此處只收 bytes。

use conlang_language::{
    LibraryCatalog, LibraryId, LibraryKind, LibrarySpec, PackageFile, PackageSources,
};

fn plugin(name: &str, requires: &str, code: &str) -> PackageSources {
    PackageSources {
        config: format!(
            "kind = plugin\nname = {name}\nversion = 0.1.0\n\
             rule_namespace = plugin:{name}\nenabled = true\npriority = 0\n\
             requires ={requires}\ncode = code/main.lang\ndata = data/notes.tsv\n"
        ),
        exports: format!("stable_id\tkind\talias\nplugin:{name}:Marker\ttrait\t{name}Marker\n"),
        code: code.to_owned(),
        // manifest 要求至少一個 data 檔
        data: "key\tvalue\n".to_owned(),
        data_files: vec![PackageFile {
            path: "data/notes.tsv".to_owned(),
            source: "key\tvalue\n".to_owned(),
        }],
        ..PackageSources::default()
    }
}

fn id(kind: LibraryKind, name: &str) -> LibraryId {
    LibraryId::new(kind, name)
}

/// 注入的 package 進得了 catalog,而且**參與選取**。
#[test]
fn an_injected_plugin_participates_in_selection() {
    let catalog = LibraryCatalog::with_packages([plugin(
        "tonal",
        " std:core",
        "trait tonalMarker:\n    belongs Noun\n",
    )])
    .expect("注入的 package 應通過同一套驗證");

    let spec = LibrarySpec::default().with_plugin(id(LibraryKind::Plugin, "tonal"));
    let selection = catalog.select(&spec).expect("select");
    let loaded: Vec<_> = selection.packages.iter().map(ToString::to_string).collect();

    assert!(loaded.contains(&"plugin:tonal".to_owned()), "{loaded:?}");
    // 遞移依賴由 package 自己的 `requires` 帶出來,不必在 spec 裡列
    assert!(loaded.contains(&"std:core".to_owned()), "{loaded:?}");
    // 其內容真的併進 overlay
    assert!(selection
        .overlay
        .traits
        .iter()
        .any(|t| t.name == "tonalMarker"));
}

/// **不宣告就不載入**——注入 ≠ 自動啟用(與 R12 同一條原則)。
#[test]
fn an_injected_plugin_is_not_loaded_unless_declared() {
    let catalog = LibraryCatalog::with_packages([plugin(
        "tonal",
        " std:core",
        "trait tonalMarker:\n    belongs Noun\n",
    )])
    .unwrap();
    let loaded: Vec<_> = catalog
        .select(&LibrarySpec::default())
        .unwrap()
        .packages
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(!loaded.contains(&"plugin:tonal".to_owned()), "{loaded:?}");
}

/// 注入者**受同一套把關**:內嵌與外部不得有兩套規則。
#[test]
fn an_injected_package_faces_the_same_validation() {
    // rule_namespace 必須等於 package id
    let mut bad = plugin("bogus", "", "trait bogusMarker:\n");
    bad.config = bad
        .config
        .replace("rule_namespace = plugin:bogus", "rule_namespace = wrong");
    assert!(
        LibraryCatalog::with_packages([bad]).is_err(),
        "namespace 不符應被拒"
    );

    // 依賴不存在的 package
    let missing = plugin("orphan", " plugin:nope", "trait orphanMarker:\n");
    let catalog = LibraryCatalog::with_packages([missing]).expect("catalog 本身可建");
    assert!(
        catalog
            .select(&LibrarySpec::default().with_plugin(id(LibraryKind::Plugin, "orphan")))
            .is_err(),
        "未知依賴應在選取時被拒"
    );
}

/// 注入者與內嵌者**同處一個 catalog**,故跨 package 的唯一性一併把關。
///
/// 走的是 `validate_catalog`(重複 id / alias),與上一條的 `load_sources`
/// (單一 package 的 manifest 與匯出存在性)是不同關卡。
///
/// **這條初版寫錯過**:當時讓 package 匯出 `Noun` 卻沒在自己的 code 定義它,
/// 於是被 `load_sources` 的「匯出必須存在」先擋下——測試通過,但**理由不對**,
/// `with_packages` 拿掉 `validate_catalog` 照樣全綠。突變測試抓到後才改成
/// 「package 自己定義 `Noun`」,讓它必須撞到 catalog 層才會失敗。
#[test]
fn an_injected_package_may_not_collide_with_an_embedded_one() {
    // 自己定義 `Noun` 並匯出——單一 package 完全合法,但 alias 撞上 std:core
    let mut clash = plugin("clash", "", "trait Noun:\n");
    clash.exports = "stable_id\tkind\talias\nplugin:clash:Noun\ttrait\tNoun\n".to_owned();
    let error = LibraryCatalog::with_packages([clash]).expect_err("alias 撞名應被拒");
    assert!(
        format!("{error}").contains("Noun"),
        "要是 catalog 層的撞名,不是匯出不存在:{error}"
    );

    // 同一個 package 注入兩次:各自合法,catalog 層才看得出重複
    let once = plugin("twice", "", "trait twiceMarker:\n");
    let again = once.clone();
    let error = LibraryCatalog::with_packages([once, again]).expect_err("重複 id 應被拒");
    assert!(format!("{error}").contains("twice"), "{error}");
}

/// 注入的 `data` 走一般 `data_sources` 通道——這正是 E1 先驗庫要用的路。
#[test]
fn injected_data_files_are_carried_like_any_other_package_data() {
    let mut sources = plugin("priors", "", "trait priorsMarker:\n");
    sources.config = sources
        .config
        .replace("data = data/notes.tsv\n", "data = data/freq.tsv\n");
    sources.data = "symbol\tweight\nk\t0.15\n".to_owned();
    sources.data_files = vec![PackageFile {
        path: "data/freq.tsv".to_owned(),
        source: "symbol\tweight\nk\t0.15\n".to_owned(),
    }];

    let catalog = LibraryCatalog::with_packages([sources]).expect("帶 data 的 package");
    let package = catalog
        .packages()
        .iter()
        .find(|p| p.id.name == "priors")
        .expect("在 catalog 裡");
    assert_eq!(package.data_paths, vec!["data/freq.tsv".to_owned()]);
    assert!(package.data_sources[0].source.contains("k\t0.15"));
}

/// 正向控制組:不注入任何東西時,`with_packages` 等同 `embedded`。
#[test]
fn with_no_extra_packages_it_matches_the_embedded_catalog() {
    let injected = LibraryCatalog::with_packages([]).unwrap();
    let embedded = LibraryCatalog::embedded().unwrap();
    let names = |c: &LibraryCatalog| {
        c.packages()
            .iter()
            .map(|p| p.id.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&injected), names(&embedded));
}
