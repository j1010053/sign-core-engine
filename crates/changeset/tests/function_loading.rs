//! P50 ③ —— 套件的 `code/*.chg` 被載成 function 表。
//!
//! **auto-discovery,無顯式 import**(P50):啟用的套件自動被掃到;可重現性由既有的
//! `library <pkg>@<ver> sha256:` lock 提供(std 自動入鎖)。
//! `language` **不解析** `.chg`(P20 依賴方向),只 verbatim 承載;解析在此。

use conlang_changeset::function::load_functions;
use conlang_changeset::{change_set_prelude, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibraryCatalog, LibrarySpec};

fn catalog() -> LibraryCatalog {
    LibraryCatalog::embedded().expect("embedded catalog")
}

#[test]
fn the_standard_path_library_is_discovered_without_any_import() {
    let table = load_functions(&catalog(), &LibrarySpec::default()).expect("functions load");
    assert!(
        !table.is_empty(),
        "std::grammaticalization ships functions and std is auto-enabled"
    );
    let definition = table.get("VerbToTense").expect("exported by std");
    assert_eq!(definition.name, "VerbToTense");
}

#[test]
fn a_parameterised_path_carries_its_slot_style_constraint() {
    // P52:機制在 code(一個參數化 function),30–50 條路徑在 data。
    let table = load_functions(&catalog(), &LibrarySpec::default()).unwrap();
    let definition = table.get("VerbToTense").unwrap();
    assert_eq!(definition.params.len(), 2);
    assert_eq!(definition.params[0].name, "verb");
    assert_eq!(
        definition.params[0].constraint.as_deref(),
        Some("Verb"),
        "參數約束取代大部分 guard"
    );
}

#[test]
fn the_body_is_a_plain_sequence_of_atomic_rewrites() {
    use conlang_changeset::function::FunctionBody;
    let table = load_functions(&catalog(), &LibrarySpec::default()).unwrap();
    match &table.get("VerbToTense").unwrap().body {
        // 純序列 = 依序全跑(慣稱 Recipe);P48 不需要 layer 標記。
        FunctionBody::Sequence(calls) => {
            let names: Vec<&str> = calls.iter().map(|call| call.name.as_str()).collect();
            assert_eq!(names, ["drift", "reanalyze", "entrench"]);
        }
        other => panic!("expected a sequence, got {other:?}"),
    }
}

#[test]
fn an_unknown_function_name_is_rejected() {
    let table = load_functions(&catalog(), &LibrarySpec::default()).unwrap();
    assert!(table.get("NoSuchPath").is_err());
}

#[test]
fn a_qualified_name_resolves_to_its_package() {
    // P29:同名同 priority 才需要寫全名,但全名任何時候都該可用。
    let table = load_functions(&catalog(), &LibrarySpec::default()).unwrap();
    assert!(table.get("std:grammaticalization::VerbToTense").is_ok());
    assert!(table.get("std:nope::VerbToTense").is_err());
}

/// 可重現性:function 原始碼**必須進 library lock digest**,否則改了 recipe
/// 卻不會使既有 `.chg` 失效(破 P26)。
#[test]
fn the_function_source_is_covered_by_the_library_lock() {
    let document = LanguageDocument::import_new_root("Symbol a\n", "evo:root").unwrap();
    let prelude = change_set_prelude(&document, &LibrarySpec::default(), "evo:x").unwrap();
    assert!(
        prelude.contains("library std:grammaticalization@0.1.0 sha256:"),
        "路徑庫套件自動入鎖:\n{prelude}"
    );
    // 該 lock 必須能通過既有的 digest 驗證(不是隨便一個字串)。
    let mut source = prelude;
    source.push_str("\n    statement 0:\n        delete sign(\"nope\")\n");
    let err = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&document, &LibrarySpec::default())
        .unwrap_err();
    assert!(
        !format!("{err}").contains("library"),
        "lock 本身應通過驗證,失敗的是那個不存在的 sign:{err}"
    );

    // **關鍵**:digest 的輸入必須真的含 function 原始碼。只斷言「lock 行存在」
    // 是假綠燈——把 `functions` 從 digest 拿掉,那種測試照樣綠。
    let catalog = catalog();
    let package = catalog
        .packages()
        .iter()
        .find(|package| package.rule_namespace == "std:grammaticalization")
        .expect("path library is embedded");
    let content = conlang_changeset::__lock_content_for_tests(package);
    assert!(
        content.contains("function VerbToTense"),
        "改了 recipe 必須使 lock 失效(P26):\n{content}"
    );
}

// ── export 表是唯一穩定契約(P29)——用合成套件直接驗過濾與不一致 ────────────

use conlang_changeset::function::functions_from_packages;
use conlang_language::{LibraryExport, LibraryExportKind, LibraryId, LibraryKind, LibraryPackage};

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
        function_paths: vec!["code/f.chg".to_owned()],
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

const TWO_FUNCTIONS: &str = "package plugin:fixture:\n    schema = conlang.functions/v1\n\nfunction Public(x):\n    entrench(x, delta: 0.1)\n\nfunction Internal(x):\n    attrit(x, delta: 0.1)\n";

#[test]
fn only_exported_functions_enter_the_table() {
    // P29:「只有列在 exports.tsv 的符號對外可見」。
    let package = synthetic(TWO_FUNCTIONS, &["Public"]);
    let table = functions_from_packages(&[&package]).expect("loads");
    assert!(table.get("Public").is_ok());
    assert!(
        table.get("Internal").is_err(),
        "未 export 的 function 是套件內部,不得對外可見"
    );
}

#[test]
fn exporting_a_function_the_code_does_not_define_is_rejected() {
    let package = synthetic(TWO_FUNCTIONS, &["Public", "Missing"]);
    let err = functions_from_packages(&[&package]).expect_err("must be rejected");
    assert!(format!("{err}").contains("does not define"), "{err}");
}
