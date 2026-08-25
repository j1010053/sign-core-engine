//! 表型宣告(`config/tables.tsv`)。
//!
//! P29:跨套件契約是**穩定 ID**,不是套件內部檔案路徑。在這個機制出現以前,
//! 引擎靠檔名尾綴認表(`path.ends_with("/weights.tsv")` /
//! `ends_with("/segments.tsv")`),於是:
//!
//! - 套件不能把已知表型的表放在自己選的路徑,
//! - 套件不能宣告新表型(表頭與路由都寫死在引擎),
//! - 一個套件不能有兩份同型別但不同來源的表。
//!
//! 這裡的測試釘住的是「認表型、不認路徑」——尤其那幾個**負例**:拿掉宣告
//! 就找不到表,才證明宣告真的是唯一入口,而不是檔名慣例在背後接住。

use conlang_language::{table_type, LibraryCatalog, PackageFile, PackageSources};

/// 一個帶 data 檔的 plugin 套件;`tables` 為空字串 = 不宣告任何表型。
fn package(name: &str, data_path: &str, rows: &str, tables: &str) -> PackageSources {
    PackageSources {
        config: format!(
            "kind = plugin\nname = {name}\nversion = 0.1.0\n\
             rule_namespace = plugin:{name}\nenabled = true\npriority = 0\n\
             requires =\ncode = code/main.lang\ndata = {data_path}\n"
        ),
        exports: format!("stable_id\tkind\talias\nplugin:{name}:Marker\ttrait\t{name}Marker\n"),
        tables: tables.to_owned(),
        code: format!("trait {name}Marker:\n"),
        data: rows.to_owned(),
        data_files: vec![PackageFile {
            path: data_path.to_owned(),
            source: rows.to_owned(),
        }],
        ..PackageSources::default()
    }
}

fn load(sources: PackageSources) -> LibraryCatalog {
    LibraryCatalog::with_packages([sources]).expect("catalog")
}

fn load_error(sources: PackageSources) -> conlang_language::LibraryLoadError {
    LibraryCatalog::with_packages([sources]).expect_err("畸形 tables.tsv 應被拒")
}

fn only_package(catalog: &LibraryCatalog, name: &str) -> conlang_language::LibraryPackage {
    catalog
        .packages()
        .iter()
        .find(|package| package.id.name == name)
        .expect("package in catalog")
        .clone()
}

const ROWS: &str = "segment\tweight\nk\t0.5\n";

/// 🔑 表放在**任意路徑**都找得到——這正是檔名慣例做不到的事。
#[test]
fn a_declared_table_is_found_by_type_not_by_path() {
    let catalog = load(package(
        "priors",
        "data/phoible/2019-subset.tsv",
        ROWS,
        "path\ttype\ndata/phoible/2019-subset.tsv\tengine:SegmentPrior\n",
    ));
    let package = only_package(&catalog, "priors");

    let found: Vec<_> = package.tables(table_type::SEGMENT_PRIOR).collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, "data/phoible/2019-subset.tsv");
    assert_eq!(found[0].source, ROWS);
}

/// 負例:同一份檔案、同一個路徑,只是**沒宣告**——就不該被任何消費者看到。
/// 若這條變綠,代表某處還留著檔名 fallback。
#[test]
fn an_undeclared_data_file_is_invisible_to_consumers() {
    let catalog = load(package("priors", "data/segments.tsv", ROWS, ""));
    let package = only_package(&catalog, "priors");

    assert_eq!(package.data_sources.len(), 1, "檔案仍在,只是沒有表型");
    assert_eq!(package.data_sources[0].table_type, None);
    assert_eq!(package.tables(table_type::SEGMENT_PRIOR).count(), 0);
}

/// 表型不對就不給——`tables()` 不做前綴或近似比對。
#[test]
fn a_table_is_not_visible_under_another_type() {
    let catalog = load(package(
        "priors",
        "data/p.tsv",
        ROWS,
        "path\ttype\ndata/p.tsv\tengine:SegmentPrior\n",
    ));
    let package = only_package(&catalog, "priors");

    assert_eq!(package.tables(table_type::WEIGHT_TABLE).count(), 0);
    assert_eq!(package.tables("engine:SegmentPrior2").count(), 0);
    assert_eq!(package.tables("engine").count(), 0);
}

/// 套件可以宣告**自己命名空間**的新表型,不必改引擎。
/// (誰來讀是另一回事——沒有消費者的表型就是沒有讀者,見上一條註解。)
#[test]
fn a_package_may_declare_its_own_table_type() {
    let catalog = load(package(
        "tonepack",
        "data/tones.tsv",
        "tone\tweight\nH\t1.0\n",
        "path\ttype\ndata/tones.tsv\tplugin:tonepack:ToneTable\n",
    ));
    let package = only_package(&catalog, "tonepack");

    assert_eq!(package.tables("plugin:tonepack:ToneTable").count(), 1);
}

/// 同一套件可有兩份同型別的表(先驗來源不只一個時需要)。
#[test]
fn two_tables_of_the_same_type_are_both_returned_in_manifest_order() {
    let mut sources = package(
        "priors",
        "data/a.tsv",
        ROWS,
        "path\ttype\ndata/a.tsv\tengine:SegmentPrior\ndata/b.tsv\tengine:SegmentPrior\n",
    );
    sources.config = sources
        .config
        .replace("data = data/a.tsv", "data = data/a.tsv, data/b.tsv");
    sources.data_files.push(PackageFile {
        path: "data/b.tsv".to_owned(),
        source: "segment\tweight\nm\t0.25\n".to_owned(),
    });
    let catalog = load(sources);
    let package = only_package(&catalog, "priors");

    let paths: Vec<_> = package
        .tables(table_type::SEGMENT_PRIOR)
        .map(|source| source.path.as_str())
        .collect();
    assert_eq!(paths, ["data/a.tsv", "data/b.tsv"]);
}

// ── 負例:畸形宣告一律報錯,不默默略過 ────────────────────────────────

fn rejection(tables: &str) -> String {
    let error = load_error(package("priors", "data/p.tsv", ROWS, tables));
    assert_eq!(error.code(), "LIBRARY_TABLES_INVALID", "{error}");
    error.to_string()
}

/// 宣告了 manifest 沒列的路徑 = 打錯字或忘了加進 `data`。若默默略過,
/// 症狀會變成「表明明宣告了卻沒被讀」——最難查的那種。
#[test]
fn a_path_outside_the_manifest_data_list_is_rejected() {
    let message = rejection("path\ttype\ndata/typo.tsv\tengine:SegmentPrior\n");
    assert!(
        message.contains("not declared in the manifest"),
        "{message}"
    );
}

#[test]
fn a_malformed_table_type_is_rejected() {
    for bad in [
        "SegmentPrior",
        "engine:",
        ":SegmentPrior",
        "engine:Segment Prior",
    ] {
        let message = rejection(&format!("path\ttype\ndata/p.tsv\t{bad}\n"));
        assert!(message.contains("table type"), "{bad}: {message}");
    }
}

#[test]
fn a_duplicate_path_entry_is_rejected() {
    let message =
        rejection("path\ttype\ndata/p.tsv\tengine:SegmentPrior\ndata/p.tsv\tplugin:x:Other\n");
    assert!(message.contains("duplicate entry"), "{message}");
}

#[test]
fn a_wrong_header_is_rejected() {
    let message = rejection("path\ttable_type\ndata/p.tsv\tengine:SegmentPrior\n");
    assert!(message.contains("path<TAB>type"), "{message}");
}

#[test]
fn a_short_row_is_rejected() {
    let message = rejection("path\ttype\ndata/p.tsv\n");
    assert!(message.contains("two non-empty"), "{message}");
}

/// 註解與空行照 exports.tsv 的既有慣例處理。
#[test]
fn comments_and_blank_lines_are_ignored() {
    let catalog = load(package(
        "priors",
        "data/p.tsv",
        ROWS,
        "# why this table exists\n\npath\ttype\n\n# and this row\ndata/p.tsv\tengine:SegmentPrior\n",
    ));
    assert_eq!(
        only_package(&catalog, "priors")
            .tables(table_type::SEGMENT_PRIOR)
            .count(),
        1
    );
}

// ── digest ────────────────────────────────────────────────────────────

/// 表型決定**誰來讀**這份表,是承載行為的宣告,故必須進 library lock。
/// 否則同樣的位元組在兩台機器上可以被不同的解析器讀走,而三道 digest
/// 檢查不出來。
#[test]
fn the_declared_table_type_enters_the_package_digest() {
    let typed = only_package(
        &load(package(
            "priors",
            "data/p.tsv",
            ROWS,
            "path\ttype\ndata/p.tsv\tengine:SegmentPrior\n",
        )),
        "priors",
    );
    let retyped = only_package(
        &load(package(
            "priors",
            "data/p.tsv",
            ROWS,
            "path\ttype\ndata/p.tsv\tplugin:x:Other\n",
        )),
        "priors",
    );
    let untyped = only_package(&load(package("priors", "data/p.tsv", ROWS, "")), "priors");

    let digest = conlang_language::library::package_digest;
    assert_ne!(digest(&typed), digest(&retyped), "換表型必須換 digest");
    assert_ne!(digest(&typed), digest(&untyped), "加表型必須換 digest");
}

/// 沒宣告表型的套件,lock 內容**逐位元**維持舊樣——這個機制因此不會把
/// 既有專案的 lock 全部作廢,只有真的宣告了表型的套件會變。
#[test]
fn an_undeclared_package_lock_gains_no_table_lines() {
    let untyped = only_package(&load(package("priors", "data/p.tsv", ROWS, "")), "priors");
    let content = conlang_language::library::package_lock_content(&untyped);
    assert!(
        !content.contains("\ntable "),
        "未宣告表型的套件不應出現 table 行:\n{content}"
    );
}

/// std:grammaticalization 的兩份表各有其型,且**互不可見**——同一個套件裡
/// 兩種表型不會因為都在 `data/` 底下就混在一起。
#[test]
fn the_std_tables_are_declared_with_distinct_types() {
    let catalog = LibraryCatalog::embedded().expect("embedded catalog");
    let package = only_package(&catalog, "grammaticalization");

    let weights: Vec<_> = package
        .tables(table_type::WEIGHT_TABLE)
        .map(|source| source.path.as_str())
        .collect();
    assert_eq!(weights, ["data/weights.tsv"]);

    let paths: Vec<_> = package
        .tables(table_type::GRAMMATICALIZATION_PATH_TABLE)
        .map(|source| source.path.as_str())
        .collect();
    assert_eq!(paths, ["data/paths.tsv"]);
}
