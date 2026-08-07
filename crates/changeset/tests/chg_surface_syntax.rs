//! `.chg` 表面語法:**語句標記 `#N:`** 與**區塊註解 `/* … */`**。
//!
//! 三種格式(`.qy`/`.lang`/`.chg`)現在共用同一套註解——擁有者 2026-07-12 定案
//! `/* … */`,理由是 **`#` 在 `.qy` 已被詞界(D19)佔用**。`.chg` 先前拿 `#` 當
//! 註解,與另兩者不一致;改用區塊註解後 `#` 空出來,正好當語句標記(`#` = 編號)。
//!
//! 舊形 `statement N:` **仍接受**,dump 一律排 `#N:`(非 canonical 正規化為
//! 不動點——與 `.lang` 的 `key = value` → `key: value` 同一作法)。

use conlang_changeset::{change_set_prelude, ReplayError, ResolvedChangeSet, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};

const SOURCE: &str = "Symbol a\n\ntrait LocalNoun:\n\nsign x:\n    belongs LocalNoun\n    phon:\n        /a/\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n";

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("base parses")
}

fn resolve(body: &str) -> Result<ResolvedChangeSet, ReplayError> {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:syntax").unwrap();
    source.push_str(body);
    UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
}

const UPDATE: &str = "        update sign(\"x\").feature[syn.category].value = aux\n";

#[test]
fn a_statement_is_marked_with_a_hash_and_its_ordinal() {
    let resolved = resolve(&format!("\n    #0:\n{UPDATE}")).expect("resolves");
    assert_eq!(resolved.statements.len(), 1);
    assert_eq!(resolved.statements[0].ordinal, 0);
}

#[test]
fn the_legacy_statement_keyword_still_parses() {
    let resolved = resolve(&format!("\n    statement 0:\n{UPDATE}")).expect("resolves");
    assert_eq!(resolved.statements[0].ordinal, 0);
}

#[test]
fn a_dump_always_uses_the_hash_form() {
    // 舊形進、新形出:正規化為不動點。
    let resolved = resolve(&format!("\n    statement 0:\n{UPDATE}")).unwrap();
    let dump = resolved.dump();
    assert!(dump.contains("\n    #0:\n"), "canonical 用 #N::\n{dump}");
    assert!(!dump.contains("statement 0:"), "舊形不再出現:\n{dump}");

    let base = base();
    let spec = LibrarySpec::default();
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump, "不動點");
}

#[test]
fn several_statements_keep_their_boundaries() {
    // 迴歸:`#1:` 曾被吞進前一句的 insert block(body 收集只認舊形)。
    let resolved = resolve(concat!(
        "\n    #0:\n        insert into sign(\"x\") at end:\n            sem:\n                senses:\n                    core = X\n",
        "\n    #1:\n        update sign(\"x\").feature[syn.category].value = aux\n",
    ))
    .expect("two statements");
    assert_eq!(resolved.statements.len(), 2);
    assert_eq!(resolved.statements[1].ordinal, 1);
}

// ── 註解 ──────────────────────────────────────────────────────────────────

#[test]
fn block_comments_are_stripped_everywhere() {
    let resolved = resolve(&format!(
        "\n    /* 整行註解 */\n    #0:\n{UPDATE}    /* 尾註 */\n"
    ))
    .expect("comments are ignored");
    assert_eq!(resolved.statements.len(), 1);
}

#[test]
fn a_hash_line_is_no_longer_treated_as_a_comment() {
    // 這正是換法的重點:`#` 從註解變成語句標記。若還被當註解,這份會解析成
    // 「沒有任何語句」而不是一句。
    let resolved = resolve(&format!("\n    #0:\n{UPDATE}")).unwrap();
    assert_eq!(
        resolved.statements.len(),
        1,
        "`#0:` 必須是語句標記,不是註解"
    );
}

#[test]
fn a_word_boundary_inside_a_rule_body_is_not_a_statement_marker() {
    // `#` 在 `.qy` 是詞界(D19)。規則本文裡的 `#` 出現在**行內**,不在行首,
    // 故不會被誤判成語句標記。
    let resolved = resolve(concat!(
        "\n    #0:\n",
        "        insert into sign(\"x\") at end:\n",
        "            phon:\n                a => b / _ #\n",
    ))
    .expect("a word-boundary rule survives");
    assert_eq!(resolved.statements.len(), 1);
}
