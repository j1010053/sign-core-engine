//! P46 S3 — phon `PhonBlock` 語句級定址 + 四原語(解 L2)。
//! phon 命名 rule 的 block(Leaf/Then/Else)裡的每一條語句與每一個子 block 都是穩定
//! 可定址節點,經 insert/delete/update/move 編輯。定址複用 `.leaf[k]`/`.then[n]`/
//! `.else[n]`(全遞迴);未具 phon_block 的 rule `.then/.else` 仍走 flat 鏈。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, ReplayError, ResolvedChangeSet, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};

// `plain` = 純 Leaf(可 `.leaf[k]` 直接定址);`layered` = Then(root 非 Leaf,
// 靠 `.then[n]` 進子 block,再 `.leaf[k]`)。每條語句唯一,子字串斷言無歧義。
const SOURCE: &str = r#"Symbol a
Symbol b
Symbol c
Symbol d
Symbol e
Symbol f
Symbol g
Symbol h

sign x:
    phon:
        /a/
        plain:
            a => b
            c => d
        layered:
            e => f
            Then:
                g => h
                h => e
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("base parses")
}

fn resolve(chg_body: &str, ns: &str) -> Result<ResolvedChangeSet, ReplayError> {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(chg_body);
    UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
}

fn apply(chg_body: &str, ns: &str) -> LanguageDocument {
    let base = base();
    let spec = LibrarySpec::default();
    let resolved = resolve(chg_body, ns).expect("resolve");
    ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document
}

// ── 定址 ──────────────────────────────────────────────────────────────────

#[test]
fn a_leaf_statement_and_a_nested_statement_are_addressable() {
    // 直接 leaf 定址(root 是 Leaf)。
    let resolved = resolve(
        "\n    statement 0:\n        update sign(\"x\").rule[\"plain\"].leaf[1].body = c => a\n",
        "evo:addr1",
    )
    .expect("resolve leaf[1]");
    assert_eq!(resolved.statements[0].edits.len(), 1);

    // 遞迴:root 是 Then → then[1] 是第二個子 block(Leaf),其 leaf[0] 是 `g => h`。
    let resolved = resolve(
        "\n    statement 0:\n        update sign(\"x\").rule[\"layered\"].then[1].leaf[0].body = g => a\n",
        "evo:addr2",
    )
    .expect("resolve then[1].leaf[0]");
    assert_eq!(resolved.statements[0].edits.len(), 1);
}

// ── update ────────────────────────────────────────────────────────────────

#[test]
fn update_rewrites_a_single_leaf_statement_only() {
    let doc = apply(
        "\n    statement 0:\n        update sign(\"x\").rule[\"plain\"].leaf[0].body = a => g\n",
        "evo:upd",
    );
    let src = doc.source();
    assert!(src.contains("a => g"), "leaf[0] rewritten:\n{src}");
    assert!(!src.contains("a => b"), "old leaf[0] gone:\n{src}");
    // 同 Leaf 的另一行不受影響。
    assert!(src.contains("c => d"), "leaf[1] untouched:\n{src}");
}

#[test]
fn update_reaches_into_a_nested_then_block() {
    let doc = apply(
        "\n    statement 0:\n        update sign(\"x\").rule[\"layered\"].then[1].leaf[1].body = h => a\n",
        "evo:updnest",
    );
    let src = doc.source();
    assert!(
        src.contains("h => a"),
        "nested h=>e rewritten to h=>a:\n{src}"
    );
    assert!(!src.contains("h => e"), "old nested line gone:\n{src}");
    assert!(
        src.contains("g => h"),
        "sibling nested line untouched:\n{src}"
    );
}

// ── insert ────────────────────────────────────────────────────────────────

#[test]
fn insert_adds_a_statement_into_an_existing_leaf() {
    let doc = apply(
        "\n    statement 0:\n        insert into sign(\"x\").rule[\"plain\"] at end:\n            leaf d => b\n",
        "evo:ins",
    );
    let src = doc.source();
    assert!(src.contains("d => b"), "new statement present:\n{src}");
}

#[test]
fn insert_into_a_nested_leaf_keeps_sibling_order() {
    // then[0] 是 layered 的前導 Leaf(`e => f`);插到其 start。
    let doc = apply(
        "\n    statement 0:\n        insert into sign(\"x\").rule[\"layered\"].then[0] at start:\n            leaf f => g\n",
        "evo:insnest",
    );
    let src = doc.source();
    let inserted = src.find("f => g").expect("inserted present");
    let existing = src.find("e => f").expect("existing present");
    assert!(inserted < existing, "inserted before sibling:\n{src}");
}

// ── delete ────────────────────────────────────────────────────────────────

#[test]
fn delete_removes_one_leaf_statement() {
    let doc = apply(
        "\n    statement 0:\n        delete sign(\"x\").rule[\"plain\"].leaf[0]\n",
        "evo:del",
    );
    let src = doc.source();
    assert!(!src.contains("a => b"), "leaf[0] removed:\n{src}");
    assert!(src.contains("c => d"), "leaf[1] survives:\n{src}");
}

// ── move ──────────────────────────────────────────────────────────────────

#[test]
fn move_reorders_statements_within_a_leaf() {
    // 把 leaf[0] (`a => b`) 移到 leaf 尾 → 順序反轉為 [c=>d, a=>b]。
    let doc = apply(
        "\n    statement 0:\n        move sign(\"x\").rule[\"plain\"].leaf[0] to sign(\"x\").rule[\"plain\"] at end\n",
        "evo:move",
    );
    let src = doc.source();
    assert!(
        src.find("c => d").unwrap() < src.find("a => b").unwrap(),
        "a=>b moved after c=>d:\n{src}"
    );
}

// ── 不變式:round-trip + 決定性 ─────────────────────────────────────────────

#[test]
fn edit_round_trips_and_is_deterministic() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:rt").unwrap();
    source.push_str(
        "\n    statement 0:\n        update sign(\"x\").rule[\"layered\"].then[1].leaf[0].body = g => a\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let dump = resolved.dump().expect("dump");
    // dump → parse → resolve 逐位元恆等。
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump().expect("dump"), dump, "round-trip stable:\n{dump}");
    // 重跑決定性:同 base+chg 兩次 apply 逐字相同。
    let a = apply(
        "\n    statement 0:\n        update sign(\"x\").rule[\"layered\"].then[1].leaf[0].body = g => a\n",
        "evo:det",
    );
    let b = apply(
        "\n    statement 0:\n        update sign(\"x\").rule[\"layered\"].then[1].leaf[0].body = g => a\n",
        "evo:det",
    );
    assert_eq!(a.source(), b.source());
}

#[test]
fn dumped_leaf_insert_uses_the_leaf_keyword() {
    let resolved = resolve(
        "\n    statement 0:\n        insert into sign(\"x\").rule[\"plain\"] at end:\n            leaf d => b\n",
        "evo:dumpins",
    )
    .unwrap();
    let dump = resolved.dump().expect("dump");
    assert!(
        dump.contains("leaf d => b"),
        "leaf keyword in dump:\n{dump}"
    );
}

// ── near-miss 負例 ─────────────────────────────────────────────────────────

#[test]
fn addressing_a_leaf_index_out_of_range_is_rejected() {
    let err = resolve(
        "\n    statement 0:\n        update sign(\"x\").rule[\"plain\"].leaf[9].body = a => d\n",
        "evo:oob",
    )
    .unwrap_err();
    assert!(
        format!("{err}").contains("cannot resolve") || format!("{err}").contains("leaf[9]"),
        "expected addressing failure, got {err}"
    );
}

#[test]
fn else_selector_on_a_then_block_does_not_resolve() {
    // layered 的 root 是 Then;`.else[0]` 無對應子節點。
    let err = resolve(
        "\n    statement 0:\n        update sign(\"x\").rule[\"layered\"].else[0].body = a => d\n",
        "evo:wrongkind",
    )
    .unwrap_err();
    assert!(
        format!("{err}").contains("cannot resolve") || format!("{err}").contains("else[0]"),
        "expected addressing failure, got {err}"
    );
}
