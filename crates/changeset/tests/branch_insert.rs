//! Branch insert:往既有 Rule 追加 else/then 分支。父 Rule 以 nameless 授權路徑
//! `sign("x").rule[n]` 定址(resolve_path_child 既有能力),分支 payload 為 opaque body。

use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol d
Symbol o
Symbol g

trait LocalNoun:

sign dog:
    belongs LocalNoun
    syn:
        feature:
            class = enum(transitive, other, special, third)
            class => transitive / [Verb]
    phon:
        /dog/
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").unwrap()
}

fn resolve(
    chg_body: &str,
    ns: &str,
) -> Result<conlang_changeset::ResolvedChangeSet, conlang_changeset::ReplayError> {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(chg_body);
    UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
}

#[test]
fn inserts_an_else_branch_into_an_addressed_rule() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:else").unwrap();
    source.push_str(
        "\n    statement 0:\n        insert into sign(\"dog\").rule[0] at end:\n            else class => other\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();

    // 降階為單一 Insert{RuleElseBranch};父定址成 stable node。
    assert_eq!(resolved.statements[0].edits.len(), 1);
    let dump = resolved.dump();
    assert!(
        dump.contains("insert into node(rule, @"),
        "父為 stable rule node:\n{dump}"
    );
    assert!(dump.contains("else class => other"));

    // round-trip 穩定。
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump);

    // apply:else 分支寫入 rule。
    let doc = ChangeInterpreter::new(base, spec, "evo:else")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    assert!(
        doc.source().contains("else class => other"),
        "{}",
        doc.source()
    );
}

#[test]
fn unknown_rule_ordinal_is_rejected() {
    // dog 只有 1 條 rule(index 0);rule[5] 無法定址。
    let err = resolve(
        "\n    statement 0:\n        insert into sign(\"dog\").rule[5] at end:\n            else class => other\n",
        "evo:badidx",
    )
    .unwrap_err();
    assert!(
        format!("{err}").contains("cannot resolve") || format!("{err}").contains("rule[5]"),
        "expected addressing failure, got {err}"
    );
}

/// 有序:第二個 else 以 `before <sibling>` 定址(nameless sibling = rule[0].else[0]),
/// 插在既有 else 之前 → 順序即優先序。
#[test]
fn else_branches_keep_insertion_order_via_sibling_anchor() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:order").unwrap();
    source.push_str(concat!(
        "\n    statement 0:\n        insert into sign(\"dog\").rule[0] at end:\n            else class => other\n",
        "\n    statement 1:\n        insert into sign(\"dog\").rule[0] at before sign(\"dog\").rule[0].else[0]:\n            else class => special\n",
    ));
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let doc = ChangeInterpreter::new(base, spec, "evo:order")
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document;
    let rendered = doc.source();
    // 比對**分支本文**而非裸字串:`class = enum(…, other, special, …)` 的宣告行
    // 也含這兩個詞,拿裸字串會比到宣告而不是分支。
    assert!(
        rendered.find("class => special").unwrap() < rendered.find("class => other").unwrap(),
        "special 應在 other 之前:\n{rendered}"
    );
}

/// else/then 互斥被強制:對已有 else 鏈的 rule 追加 then → statement 驗證失敗
/// (edit 重新序列化再 re-parse,`.lang` 的 parse-time 不變式在此把關)。
#[test]
fn then_into_a_rule_with_an_else_chain_is_rejected() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:mutex").unwrap();
    source.push_str(concat!(
        "\n    statement 0:\n        insert into sign(\"dog\").rule[0] at end:\n            else class => other\n",
        "\n    statement 1:\n        insert into sign(\"dog\").rule[0] at end:\n            then class => third\n",
    ));
    let err = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap_err();
    assert!(
        format!("{err}").contains("cannot mix `then` and `else`"),
        "expected mutual-exclusion rejection, got {err}"
    );
}
