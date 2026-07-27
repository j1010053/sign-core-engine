//! 步驟 15c — **層②③④ 的統一呼叫語法**:`name(位置參數, key: value, …)`。
//!
//! 層級由**名字解析**決定,不靠關鍵字前綴——現階段只有 P16 的 12 個內建 Atomic
//! Rewrite 解析得出來(封閉內建集);Recipe/Goal(步驟 16–17)落地後沿用同一文法。
//! 與 `clone` 同構:呼叫只活在未解析層,`resolve` 就降成四原語,
//! **`ResolvedChangeSet` 維持 primitive-only**(步驟 14 契約)。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, ReplayError, ResolvedChangeSet, UnresolvedChangeSet,
};
use conlang_language::{Language, LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol b
Symbol o
Symbol k

trait LocalNoun:

global trait Core:

sign book:
    belongs LocalNoun
    entrenchment = 0.5
    phon:
        /bo/
    syn:
        category = noun
    sem:
        senses:
            core = BOOK
            log = LOGBOOK
        edges:
            log from core metonymy

sign kobo:
    belongs LocalNoun
    phon:
        /ko/
    sem:
        senses:
            only = KOBO
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("base parses")
}

fn resolve(chg: &str, ns: &str) -> Result<ResolvedChangeSet, ReplayError> {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(chg);
    UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
}

fn apply(chg: &str, ns: &str) -> String {
    let base = base();
    let spec = LibrarySpec::default();
    let resolved = resolve(chg, ns).expect("resolve");
    let lang = ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .expect("replay")
        .document
        .source()
        .to_owned();
    Language::parse(&lang).expect("result .lang re-parses");
    lang
}

fn statement(body: &str) -> String {
    format!("\n    statement 0:\n        {body}\n")
}

// ── 呼叫語法 ──────────────────────────────────────────────────────────────

#[test]
fn a_bare_call_lowers_to_primitives() {
    let lang = apply(
        &statement("reanalyze(sign(\"book\"), target: category, to: aux)"),
        "evo:re",
    );
    assert!(lang.contains("category = aux"), "{lang}");
}

#[test]
fn named_arguments_use_colons() {
    let lang = apply(
        &statement("drift(sign(\"book\"), sense: log, gloss: JOURNAL)"),
        "evo:drift",
    );
    assert!(lang.contains("log = JOURNAL"), "{lang}");
    assert!(lang.contains("core = BOOK"), "sibling untouched:\n{lang}");
}

#[test]
fn a_call_can_expand_to_several_primitives() {
    let resolved = resolve(
        &statement(
            "derive_sense(sign(\"book\"), from: core, name: fig, gloss: FIGURATIVE, kind: metaphor)",
        ),
        "evo:derive",
    )
    .expect("resolve");
    // derive_sense = insert(sense) + insert(edge)
    assert_eq!(resolved.statements[0].edits.len(), 2);
    let lang = apply(
        &statement(
            "derive_sense(sign(\"book\"), from: core, name: fig, gloss: FIGURATIVE, kind: metaphor)",
        ),
        "evo:derive2",
    );
    assert!(lang.contains("fig = FIGURATIVE") && lang.contains("fig from core metaphor"));
}

#[test]
fn a_call_with_a_block_carries_a_whole_sign() {
    let lang = apply(
        concat!(
            "\n    statement 0:\n        adopt(source: loan):\n",
            "            sign kaffe:\n",
            "                belongs LocalNoun\n",
            "                sem:\n                    senses:\n                        core = COFFEE\n",
        ),
        "evo:adopt",
    );
    assert!(lang.contains("sign kaffe:"), "{lang}");
    assert!(lang.contains("provenance = loan"), "{lang}");
}

#[test]
fn a_list_argument_is_space_separated() {
    // 逗號是參數分隔符,故清單用空白分隔。
    let lang = apply(
        &statement("split(sign(\"kobo\"), name: kobolet, senses: only)"),
        "evo:split",
    );
    assert!(lang.contains("sign kobolet:"), "{lang}");
    assert!(lang.contains("origin = sign(kobo)"), "{lang}");
}

#[test]
fn a_rule_home_argument_accepts_trait_and_sign() {
    let lang = apply(
        &statement("sound_change(trait(\"Core\"), body: \"b => k\")"),
        "evo:sc",
    );
    assert!(lang.contains("b => k"), "{lang}");
}

// ── 與步驟 14 契約的關係 ──────────────────────────────────────────────────

#[test]
fn the_resolved_changeset_stays_primitive_only() {
    // 呼叫是**未解析層**的糖:dump 出來只有四原語,沒有 `reanalyze(` 字樣。
    let resolved = resolve(
        &statement("reanalyze(sign(\"book\"), target: category, to: aux)"),
        "evo:dump",
    )
    .expect("resolve");
    let dump = resolved.dump();
    assert!(
        !dump.contains("reanalyze("),
        "the call must not survive into the resolved form:\n{dump}"
    );
    assert!(dump.contains("update "), "lowered to a primitive:\n{dump}");
    // round-trip 沿用既有機制。
    let base = base();
    let spec = LibrarySpec::default();
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump, "dump→parse→resolve 逐位元穩定");
}

#[test]
fn a_call_and_its_hand_written_primitive_agree() {
    // 差分:呼叫降階的結果,必須和直接手寫那條原語一模一樣。
    let via_call = apply(
        &statement("reanalyze(sign(\"book\"), target: category, to: aux)"),
        "evo:a",
    );
    let via_primitive = apply(
        &statement("update sign(\"book\").def[syn.category].value = aux"),
        "evo:b",
    );
    assert_eq!(via_call, via_primitive);
}

// ── 封閉內建集 ────────────────────────────────────────────────────────────

#[test]
fn an_unknown_name_is_rejected_as_a_closed_set() {
    let err = resolve(&statement("grammaticalize(sign(\"book\"))"), "evo:unk").unwrap_err();
    let text = format!("{err}");
    assert!(
        text.contains("closed built-in set"),
        "expected the closed-set error, got {text}"
    );
}

#[test]
fn a_missing_or_malformed_argument_is_rejected() {
    for (body, why) in [
        ("drift(sign(\"book\"), sense: log)", "missing gloss:"),
        (
            "entrench(sign(\"book\"), delta: lots)",
            "delta is not a number",
        ),
        (
            "derive_sense(sign(\"book\"), from: core, name: n, gloss: G, kind: sideways)",
            "unknown derivation kind",
        ),
        (
            "reanalyze(sign(\"book\"), target: colour, to: x)",
            "unknown target",
        ),
        (
            "drift(book, sense: log, gloss: G)",
            "positional is not a selector",
        ),
    ] {
        assert!(
            resolve(&statement(body), "evo:bad").is_err(),
            "expected rejection ({why}): {body}"
        );
    }
}

#[test]
fn a_primitive_statement_is_still_a_primitive_statement() {
    // 迴歸:層① 的關鍵字語句不因新增呼叫文法而被誤判。
    let lang = apply(
        &statement("update sign(\"book\").sense[\"log\"].gloss = JOURNAL"),
        "evo:prim",
    );
    assert!(lang.contains("log = JOURNAL"), "{lang}");
}
