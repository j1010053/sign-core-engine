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
    let dump = resolved.dump();
    // dump → parse → resolve 逐位元恆等。
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump, "round-trip stable:\n{dump}");
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
    let dump = resolved.dump();
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

// ── P46 不可見欄位:寧可報錯,不可靜默吞掉 ──────────────────────────────────
//
// `phon_block.is_some()` 時 canonical printer 只印 `name:` + 巢狀 block,
// `body`/`stage`/`dim`/`else_chain`/`then_chain` 一律不落地。對這些欄位的編輯
// 若照寫不誤,下一次 round-trip 就把它蒸發掉——文件逐位元不變、`diff_vector`
// 報零差異,而 `.chg` 卻宣稱做了一件沒發生的事(replay 決定性 P26 與三道
// digest 都抓不到,因為根本沒東西變)。故一律明確拒絕。

const MIXED: &str = r#"Symbol a
Symbol b
Symbol c
Symbol d

sign y:
    phon:
        /a/
        flat: a => b @stage word
        structured:
            c => d
"#;

fn mixed_base() -> LanguageDocument {
    LanguageDocument::import_new_root(MIXED, "evo:mixedroot").expect("mixed base parses")
}

fn mixed_resolve(chg_body: &str, ns: &str) -> Result<ResolvedChangeSet, ReplayError> {
    let base = mixed_base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(chg_body);
    UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
}

fn mixed_apply(chg_body: &str, ns: &str) -> LanguageDocument {
    let base = mixed_base();
    let spec = LibrarySpec::default();
    let resolved = mixed_resolve(chg_body, ns).expect("resolve");
    ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .unwrap()
        .document
}

#[test]
fn body_update_on_a_structured_phon_rule_is_rejected_not_swallowed() {
    let err = mixed_resolve(
        "\n    statement 0:\n        update sign(\"y\").rule[\"structured\"].body = c => a\n",
        "evo:blockbody",
    )
    .expect_err("`.body` on a structured phon rule must not be accepted");
    let text = format!("{err}");
    assert!(
        text.contains("structured phon block"),
        "error names the cause, got {text}"
    );
    assert!(
        text.contains(".leaf[k].body"),
        "error points at the address that works, got {text}"
    );

    // 對照:同一條規則走 `.leaf[0].body` 確實生效——被拒絕的不是編輯意圖,是位址。
    let doc = mixed_apply(
        "\n    statement 0:\n        update sign(\"y\").rule[\"structured\"].leaf[0].body = c => a\n",
        "evo:blockleaf",
    );
    assert!(doc.source().contains("c => a"), "{}", doc.source());
}

#[test]
fn dim_update_on_a_structured_phon_rule_is_rejected() {
    // 維度標記同樣不印,且 structured block 依 P46 限 phon 維——改維無從表達。
    let err = mixed_resolve(
        "\n    statement 0:\n        update sign(\"y\").rule[\"structured\"].dim = syn\n",
        "evo:blockdim",
    )
    .expect_err("`.dim` on a structured phon rule must not be accepted");
    let text = format!("{err}");
    assert!(
        text.contains("structured phon block") && text.contains("no surface syntax"),
        "`.dim` rejection explains itself, got {text}"
    );
}

#[test]
fn stage_update_on_a_structured_phon_rule_takes_effect() {
    // stage **不**在閘門之列:它有 surface 形(block 內 `stage:` 一行),
    // parser 提升進 `Rule.stage`、printer 印回去,故編輯真的落地。
    let doc = mixed_apply(
        "\n    statement 0:\n        update sign(\"y\").rule[\"structured\"].stage = stem\n",
        "evo:blockstage",
    );
    let src = doc.source();
    assert!(
        src.contains("        structured:\n            stage: stem\n"),
        "stage 印在 block 首行:\n{src}"
    );
    // flat rule 的 `@stage` 尾綴不受影響(兩種 surface 各自歸位)。
    assert!(src.contains("flat: a => b @stage word"), "{src}");
}

#[test]
fn stage_cannot_be_smuggled_in_through_the_statement_channel() {
    // `stage:` 是 rule 級屬性,不是語句:放它從語句通道進來,下一次 round-trip
    // 會把它從 Leaf 提走,那個語句節點就此消失——另一種靜默吞掉。
    for (label, statement) in [
        (
            "insert",
            "insert into sign(\"y\").rule[\"structured\"] at start:\n            leaf stage: phrase",
        ),
        (
            "update",
            "update sign(\"y\").rule[\"structured\"].leaf[0].body = stage: phrase",
        ),
    ] {
        let err = mixed_resolve(
            &format!("\n    statement 0:\n        {statement}\n"),
            &format!("evo:smuggle{label}"),
        )
        .expect_err("`stage:` must not enter as a phon statement");
        assert!(
            format!("{err}").contains("rule-level attribute"),
            "{label} rejection points at `.stage`, got {err}"
        );
    }
}

#[test]
fn else_branch_insert_into_a_structured_phon_rule_is_rejected() {
    // flat else/then 鏈在 phon_block 之下同樣不印。插入原先只落得一個
    // 不知所云的 ShapeMismatch;現在直接指出原因。
    let err = mixed_resolve(
        "\n    statement 0:\n        insert into sign(\"y\").rule[\"structured\"] at end:\n            else c => b\n",
        "evo:blockelse",
    )
    .expect_err("flat else insert under a phon block must not be accepted");
    let text = format!("{err}");
    assert!(
        text.contains("structured phon block"),
        "error names the cause, got {text}"
    );
}

#[test]
fn flat_else_and_then_are_unaddressable_under_a_structured_phon_rule() {
    // 閘門之外的第二道保險:structured rule 底下根本沒有 flat 分支節點,
    // 故 `.else[n]`/`.then[n]` 連定址都不成立(root 是 Leaf,無同名子 block)。
    for (field, ns) in [("else", "evo:noelse"), ("then", "evo:nothen")] {
        let err = mixed_resolve(
            &format!(
                "\n    statement 0:\n        update sign(\"y\").rule[\"structured\"].{field}[0].body = c => b\n"
            ),
            ns,
        )
        .expect_err("no flat branch node exists here");
        assert!(
            format!("{err}").contains("cannot resolve"),
            "expected addressing failure, got {err}"
        );
    }
}

#[test]
fn flat_rules_still_take_body_and_stage_updates() {
    // 閘門必須窄:同一份文件裡的 flat rule 不受影響。
    let doc = mixed_apply(
        "\n    statement 0:\n        update sign(\"y\").rule[\"flat\"].body = a => d\n",
        "evo:flatbody",
    );
    assert!(doc.source().contains("a => d"), "{}", doc.source());

    let doc = mixed_apply(
        "\n    statement 0:\n        update sign(\"y\").rule[\"flat\"].stage = stem\n",
        "evo:flatstage",
    );
    assert!(doc.source().contains("@stage stem"), "{}", doc.source());
}
