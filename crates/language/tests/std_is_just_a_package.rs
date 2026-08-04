//! R12 出口:**`std:*` 由 import 表宣告,不再憑 `kind` 自動入列**。
//!
//! 裁定 S:`std:*` 如同 C++ 的 std——隨引擎發布,但只是一個 library。
//! 此前 `select()` 憑 `kind == Std` 無條件把它們當作依賴解析的起點,
//! 而 `natural`/`plugin` 都要點名;三種 kind 走兩套規則。
//!
//! 本檔釘的是**拆完才做得到的事**:選擇性載入、完全不載入。
//! 若特權仍在,下列斷言會因為「std 反正都會進來」而失敗。

use conlang_language::library::{default_std_packages, embedded_catalog};
use conlang_language::{Language, LibraryId, LibraryKind, LibrarySpec, Severity};

fn ids(spec: &LibrarySpec) -> Vec<String> {
    embedded_catalog()
        .expect("catalog")
        .select(spec)
        .expect("select")
        .packages
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// 預設仍是隨引擎發布的那一組——降級為預設值,不是拿掉。
#[test]
fn the_default_spec_still_loads_the_shipped_std_set() {
    let loaded = ids(&LibrarySpec::default());
    for package in default_std_packages() {
        assert!(
            loaded.contains(&package.to_string()),
            "{package} 應在預設組合裡:{loaded:?}"
        );
    }
}

/// **拆完才做得到**:只載入部分 std。
#[test]
fn a_project_may_load_only_some_std_packages() {
    let spec = LibrarySpec {
        std: vec![LibraryId::new(LibraryKind::Std, "core")],
        ..LibrarySpec::default()
    };
    let loaded = ids(&spec);
    assert!(loaded.iter().any(|id| id == "std:core"));
    assert!(
        !loaded.iter().any(|id| id == "std:grambank"),
        "未宣告的 std 不得自動入列:{loaded:?}"
    );
}

/// **拆完才做得到**:完全不載入 std。
///
/// 引擎對 ontology 不可知,故這是合法狀態——只是失去共通詞彙。
#[test]
fn a_project_may_load_no_std_at_all() {
    let loaded = ids(&LibrarySpec::default().without_std());
    assert!(loaded.is_empty(), "不該有任何 package:{loaded:?}");
}

/// 缺 std 時使用共通詞彙 → **出聲**,而且附 R13 指路。
#[test]
fn using_std_vocabulary_without_declaring_it_is_loud() {
    let language = Language::parse("sign s:\n    belongs Noun\n").expect("parse");
    let report = conlang_language::check_language_with_libraries(
        &language,
        &LibrarySpec::default().without_std(),
    );
    let message = report
        .diagnostics()
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .find(|m| m.contains("Noun"))
        .expect("未宣告 std 卻用 Noun 必須報錯");
    assert!(
        message.contains("std:core") && message.contains("import table"),
        "要指出出處與作法:{message}"
    );
}

/// 正向控制組:宣告了就正常,不報錯。
#[test]
fn declaring_std_core_makes_the_same_source_compile() {
    let language = Language::parse("sign s:\n    belongs Noun\n").expect("parse");
    let spec = LibrarySpec {
        std: vec![LibraryId::new(LibraryKind::Std, "core")],
        ..LibrarySpec::default()
    };
    let report = conlang_language::check_language_with_libraries(&language, &spec);
    assert!(
        !report
            .diagnostics()
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("Noun")),
        "{:?}",
        report.diagnostics()
    );
}
