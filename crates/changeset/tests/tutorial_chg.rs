//! `tutorials/歷時chg授權教學_v1.md` 的可執行契約。
//!
//! 每個帶 `chg-test` 標記的 `.chg` 都必須走過真正的 parse → resolve →
//! canonical dump → re-resolve → replay；成功案例核對正文宣稱的 source 片段，
//! 失敗案例核對診斷。這避免教學只剩看起來合理、實際已不能執行的範例。

use std::collections::BTreeSet;

use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{Language, LanguageDocument, LibrarySpec};

const TUTORIAL: &str = include_str!("../../../tutorials/歷時chg授權教學_v1.md");

#[derive(Debug)]
struct Example {
    directive: String,
    body: String,
}

fn normalized_tutorial() -> String {
    TUTORIAL.replace("\r\n", "\n").replace('\r', "\n")
}

fn tagged_lang_example(name: &str) -> String {
    let tutorial = normalized_tutorial();
    let marker = format!("<!-- chg-test: {name} -->");
    let after = tutorial
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing tutorial block {name:?}"))
        .1;
    let mut source = after
        .split_once("```lang\n")
        .unwrap_or_else(|| panic!("tutorial block {name:?} is not a lang fence"))
        .1
        .split_once("\n```")
        .unwrap()
        .0
        .to_owned();
    source.push('\n');
    source
}

fn chg_examples() -> Vec<Example> {
    let tutorial = normalized_tutorial();
    let lines = tutorial.lines().collect::<Vec<_>>();
    let mut examples = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index].trim();
        let Some(directive) = line
            .strip_prefix("<!-- chg-test: ")
            .and_then(|rest| rest.strip_suffix(" -->"))
        else {
            index += 1;
            continue;
        };

        let mut fence = index + 1;
        while fence < lines.len() && !lines[fence].trim().starts_with("```") {
            fence += 1;
        }
        if fence == lines.len() || lines[fence].trim() != "```chg" {
            index += 1;
            continue;
        }

        let mut end = fence + 1;
        while end < lines.len() && lines[end].trim() != "```" {
            end += 1;
        }
        assert!(end < lines.len(), "unclosed chg fence for {directive:?}");
        examples.push(Example {
            directive: directive.to_owned(),
            body: lines[fence + 1..end].join("\n"),
        });
        index = end + 1;
    }

    examples
}

fn directive_values<'a>(directive: &'a str, key: &str) -> Vec<&'a str> {
    directive
        .split(';')
        .map(str::trim)
        .filter_map(|part| part.strip_prefix(key)?.strip_prefix('='))
        .collect()
}

#[test]
fn every_documented_changeset_parses_resolves_dumps_and_replays() {
    let examples = chg_examples();
    let namespaces = examples
        .iter()
        .flat_map(|example| directive_values(&example.directive, "ns"))
        .collect::<BTreeSet<_>>();
    for required in [
        "evo:a",
        "evo:b",
        "evo:c",
        "evo:h",
        "evo:k",
        "evo:e",
        "evo:j",
        "evo:d",
        "evo:params",
        "evo:marker",
        "evo:trait-conflict",
        "evo:f",
        "evo:g",
        "evo:multi",
        "evo:i",
    ] {
        assert!(namespaces.contains(required), "missing example {required}");
    }
    assert_eq!(
        examples.len(),
        namespaces.len(),
        "every example needs one ns"
    );

    let root = LanguageDocument::import_new_root(&tagged_lang_example("base"), "evo:root")
        .expect("tutorial base parses");
    let spec = LibrarySpec::default();

    for example in examples {
        let namespace = directive_values(&example.directive, "ns")
            .into_iter()
            .next()
            .expect("every chg example has a namespace");
        let mut source = change_set_prelude(&root, &spec, namespace).expect("prelude");
        source.push('\n');
        source.push_str(&example.body);
        source.push('\n');

        let unresolved = UnresolvedChangeSet::parse(&source)
            .unwrap_or_else(|error| panic!("{namespace} must parse: {error}\n{source}"));
        let resolved = unresolved.resolve(&root, &spec);
        if let Some(expected) = directive_values(&example.directive, "error")
            .into_iter()
            .next()
        {
            let error = resolved.expect_err("documented failure must be rejected");
            let rendered = error.to_string();
            let debug = format!("{error:?}");
            assert!(
                rendered.contains(expected) || debug.contains(expected),
                "{namespace}: expected {expected:?}, got {rendered} / {debug}"
            );
            continue;
        }

        let resolved =
            resolved.unwrap_or_else(|error| panic!("{namespace} must resolve: {error:?}"));
        let dumped = resolved.dump().expect("resolved changeset must dump");
        let reparsed = UnresolvedChangeSet::parse(&dumped)
            .unwrap_or_else(|error| panic!("{namespace} dump must parse: {error}\n{dumped}"))
            .resolve(&root, &spec)
            .unwrap_or_else(|error| panic!("{namespace} dump must resolve: {error:?}"));
        assert_eq!(
            reparsed.dump().expect("round-trip dump"),
            dumped,
            "{namespace}: dump must be a fixed point"
        );

        let document = ChangeInterpreter::new(root.clone(), spec.clone(), namespace.to_owned())
            .expect("interpreter")
            .run(&reparsed)
            .unwrap_or_else(|error| panic!("{namespace} must replay: {error:?}"))
            .document;
        for expected in directive_values(&example.directive, "expect") {
            assert!(
                document.source().contains(expected),
                "{namespace}: expected {expected:?} in\n{}",
                document.source()
            );
        }
        for absent in directive_values(&example.directive, "absent") {
            assert!(
                !document.source().contains(absent),
                "{namespace}: did not expect {absent:?} in\n{}",
                document.source()
            );
        }

        if namespace == "evo:a" {
            let expected = Language::parse(&tagged_lang_example("result"))
                .expect("documented result parses")
                .dump();
            assert_eq!(document.source(), expected, "first example's exact result");
        }
    }
}
