//! R13 出口:名字查無時,診斷**指出哪個未宣告的套件匯出它**。
//!
//! 裁定 S 之下不為 `std:*` 設專用診斷碼——「沒宣告 `std:core` 卻用了 `Noun`」
//! 與「沒宣告任何定義 `Noun` 的套件」是同一個錯誤,既有的
//! `ONTOLOGY_UNKNOWN_TRAIT` / `SLOT_UNKNOWN_CATEGORY` / `ROLE_UNKNOWN_CONSTRAINT`
//! 已經精確說明。R13 只是把 catalog 已知的出處附上,對使用者自己的 plugin
//! 一視同仁——對映 C++ 的 `did you forget to #include <vector>?`。

use conlang_language::{compile_system, CompileSystemError, Language, Severity};

fn errors(src: &str) -> Vec<String> {
    let language = Language::parse(src).expect("parse");
    let collect = |report: &conlang_language::ValidationReport| {
        report
            .diagnostics()
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    };
    match compile_system(language) {
        Ok(system) => collect(&system.validation),
        Err(CompileSystemError::Validation(report)) => collect(&report),
        Err(other) => vec![format!("{other:?}")],
    }
}

fn message_for(src: &str, needle: &str) -> String {
    errors(src)
        .into_iter()
        .find(|message| message.contains(needle))
        .unwrap_or_else(|| panic!("找不到提及 {needle:?} 的診斷"))
}

/// `belongs` 指向未宣告套件的匯出名 → 指出該套件。
#[test]
fn an_unknown_trait_names_the_package_that_exports_it() {
    let message = message_for("sign s:\n    belongs EnglishCaseBearer\n", "EnglishCaseBearer");
    assert!(
        message.contains("natural:en-standard"),
        "要指出出處:{message}"
    );
    assert!(message.contains("import table"), "要說怎麼辦:{message}");
}

/// slot 約束走同一條指路。
#[test]
fn an_unknown_slot_category_names_the_package_too() {
    let message = message_for(
        "sign C:\n    syn:\n        slots:\n            x [EnglishCaseBearer]\n    phon:\n        /{$slot.x}/\n",
        "EnglishCaseBearer",
    );
    assert!(message.contains("natural:en-standard"), "{message}");
}

/// role 約束走同一條指路。
#[test]
fn an_unknown_role_constraint_names_the_package_too() {
    let message = message_for(
        "sign C:\n    syn:\n        slots:\n            x [*]\n    sem:\n        roles:\n            r [EnglishCaseBearer]\n            r = {$slot.x}\n    phon:\n        /{$slot.x}/\n",
        "EnglishCaseBearer",
    );
    assert!(message.contains("natural:en-standard"), "{message}");
}

/// **判別性**:真的沒人匯出的名字**不得**憑空長出指路。
///
/// 少了這條,把指路寫成無條件附加也不會紅。
#[test]
fn a_name_no_package_exports_gets_no_hint() {
    let message = message_for("sign s:\n    belongs NoSuchTraitAnywhere\n", "NoSuchTraitAnywhere");
    assert!(
        !message.contains("import table"),
        "無出處可指時不得亂指:{message}"
    );
}

/// **正向控制組**:已載入的名字根本不該產生這類診斷。
#[test]
fn a_loaded_name_produces_no_unknown_diagnostic() {
    let found = errors("sign s:\n    belongs Noun\n");
    assert!(
        !found.iter().any(|m| m.contains("unknown trait")),
        "std:core 已載入,`Noun` 不該是未知:{found:?}"
    );
}
