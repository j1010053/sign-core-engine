//! Package RuleId namespaces must survive every runtime-facing representation.

use conlang_language::codegen;
use conlang_language::{Language, RuleId, SignItem};

#[test]
fn phon_source_map_retains_package_owned_rule_identity() {
    let mut language = Language::parse(
        "Symbol a\nSymbol b\n\nglobal trait StandardPhon:\n    phon:\n        a => b\n",
    )
    .expect("fixture parses");
    let rule = language.traits[0].blocks[0]
        .items
        .iter_mut()
        .find_map(|item| match item {
            SignItem::Rule(rule) => Some(rule),
            _ => None,
        })
        .expect("fixture has one phon rule");
    rule.id = RuleId::package("std:fixture", 0);

    let artifacts = codegen::compile_full(&language).expect("fixture codegens");
    assert!(!artifacts.grammar.source_map.is_empty());
    assert!(artifacts
        .grammar
        .source_map
        .iter()
        .all(|entry| entry.rule_id == RuleId::package("std:fixture", 0)));
}
