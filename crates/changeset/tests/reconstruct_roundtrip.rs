//! #8 —— **從兩份狀態還原成四原語**。
//!
//! ## 為什麼主力是「往返性質」而不是逐項斷言
//!
//! 還原的主要風險是**漏比某個欄位**——少寫一個比較不是型別錯誤,編譯器抓不到,
//! 後果是「那種改動永遠不出現在 changeset 裡」,靜默丟改動。
//!
//! 逐項斷言要記得為 ~30 個 `NodeUpdate` 變體各寫一個測試,而「記得」正是會失效的環節。
//! 往返性質改成:
//!
//! ```text
//! before ──(已知 .chg)──► after
//! before ──(還原)──────► edits
//!            apply(before, edits) == after      ← 漏比任何欄位,這裡就不等
//! ```
//!
//! 漏一個欄位 ⇒ 還原出的序列少一筆 ⇒ 套出來的 `.lang` 不等於 `after` ⇒ **紅**。
//! 新增一種改動只要加一行輸入,不用記得加斷言。

use conlang_changeset::reconstruct::reconstruct;
use conlang_changeset::{
    apply_edit, change_set_prelude, Anchor, ChangeInterpreter, DetachedNode, NodeUpdate,
    PrimitiveEdit, UnresolvedChangeSet,
};
use conlang_language::{Expression, LanguageDocument, LibrarySpec, NodeKind, NodeRef};

const ROOT: &str = "Symbol a\nSymbol b\nSymbol k\n\n\
                    trait LocalNoun:\n\n\
                    global trait Core:\n\
                    \x20   phon:\n\
                    \x20       b => k\n\
                    \x20       layered:\n\
                    \x20           a => b\n\
                    \x20           Then propagate:\n\
                    \x20               b => k\n\n\
                    sign book:\n\
                    \x20   belongs LocalNoun\n\
                    \x20   entrenchment = 0.5\n\
                    \x20   phon:\n\
                    \x20       /b a b/\n\
                    \x20   syn:\n\
                    \x20       feature:\n\
                    \x20           mark = enum(off, on)\n\
                    \x20           mark => on\n\
                    \x20           category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n\
                    \x20           category = noun\n\
                    \x20   sem:\n\
                    \x20       senses:\n\
                    \x20           core = BOOK\n\
                    \x20           log = LOGBOOK\n\
                    \x20       edges:\n\
                    \x20           log from core metonymy\n\n\
                    sign kobo:\n\
                    \x20   syn:\n\
                    \x20       feature:\n\
                    \x20           category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n\
                    \x20           category = noun\n";

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(ROOT, "evo:root").expect("root parses")
}

fn apply_changeset(base: &LanguageDocument, namespace: &str, body: &str) -> LanguageDocument {
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(base, &spec, namespace).expect("prelude");
    source.push_str(body);
    let resolved = UnresolvedChangeSet::parse(&source)
        .expect("parses")
        .resolve(base, &spec)
        .expect("resolves");
    ChangeInterpreter::new(base.clone(), spec, namespace.to_owned())
        .expect("interpreter")
        .run(&resolved)
        .expect("runs")
        .document
}

/// **往返性質**。`body` 是一段已知的 `.chg`;還原出的原語套回 `before` 必須得到 `after`。
fn round_trip(label: &str, body: &str) {
    let before = base();
    let after = apply_changeset(&before, "evo:n1", body);
    assert_ne!(
        before.source(),
        after.source(),
        "{label}: 前提——這段 changeset 必須真的改到東西,否則往返性質是空的"
    );

    let edits = reconstruct(&before, &after).unwrap_or_else(|error| panic!("{label}: {error}"));
    assert!(!edits.is_empty(), "{label}: 有改動卻還原出空序列");

    let spec = LibrarySpec::default();
    let mut document = before.clone();
    for edit in edits {
        document = apply_edit(&document, edit.clone(), &spec)
            .unwrap_or_else(|error| panic!("{label}: 套用 {edit:?} 失敗: {error}"))
            .document;
    }
    assert_eq!(
        document.source(),
        after.source(),
        "{label}: 還原出的序列套回去必須逐字等於目標狀態"
    );
}

fn statement(body: &str) -> String {
    format!("\n    #0:\n        {body}\n")
}

fn children(document: &LanguageDocument, parent: &NodeRef, kind: NodeKind) -> Vec<NodeRef> {
    let mut nodes = document
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.parent.as_ref() == Some(&parent.id) && entry.kind == kind)
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.address.cmp(&right.address));
    nodes
        .into_iter()
        .map(|entry| NodeRef::new(entry.id.clone(), entry.kind))
        .collect()
}

fn reconstruct_and_replay(
    before: &LanguageDocument,
    after: &LanguageDocument,
) -> Vec<PrimitiveEdit> {
    let edits = reconstruct(before, after).expect("reconstruct");
    let mut replayed = before.clone();
    for edit in &edits {
        replayed = apply_edit(&replayed, edit.clone(), &LibrarySpec::default())
            .expect("replay reconstructed edit")
            .document;
    }
    assert_eq!(replayed.source(), after.source());
    edits
}

// ── 更新(各 NodeUpdate 變體)────────────────────────────────────────────────

#[test]
fn a_definition_value_round_trips() {
    round_trip(
        "def value",
        &statement("update sign(\"book\").feature[syn.category].value = verb"),
    );
}

#[test]
fn a_feature_rule_update_round_trips() {
    round_trip(
        "feature rule",
        &statement("update sign(\"book\").rule[0].body = mark => off"),
    );
}

#[test]
fn a_sign_rename_round_trips() {
    round_trip("rename", &statement("update sign(\"kobo\").name = kobolet"));
}

#[test]
fn an_entrenchment_change_round_trips() {
    round_trip(
        "entrenchment",
        &statement("update sign(\"book\").def[entrenchment].value = 0.9"),
    );
}

#[test]
fn a_sense_gloss_round_trips() {
    round_trip(
        "sense gloss",
        &statement("update sign(\"book\").sense[\"log\"].gloss = JOURNAL"),
    );
}

#[test]
fn a_sense_edge_transparency_round_trips() {
    round_trip(
        "edge transparency",
        &statement("update sign(\"book\").edge[0].transparency = opaque"),
    );
}

#[test]
fn a_trait_rename_round_trips() {
    round_trip(
        "trait rename",
        &statement("update trait(\"LocalNoun\").name = LocalThing"),
    );
}

#[test]
fn a_phon_statement_update_round_trips_as_one_typed_update() {
    let before = base();
    let after = apply_changeset(
        &before,
        "evo:n1",
        &statement(
            "update trait(\"Core\").block[0].rule[\"layered\"].then[1].leaf[0].body = b => a",
        ),
    );
    let edits = reconstruct(&before, &after).expect("reconstruct phon statement");
    assert!(
        matches!(
            edits.as_slice(),
            [PrimitiveEdit::Update {
                node,
                change: NodeUpdate::RuleBranchBody(body),
            }] if node.expected == NodeKind::PhonStatement && body == "b => a"
        ),
        "expected one typed phon-statement update: {edits:#?}"
    );
    round_trip(
        "phon statement",
        &statement(
            "update trait(\"Core\").block[0].rule[\"layered\"].then[1].leaf[0].body = b => a",
        ),
    );
}

#[test]
fn a_phon_block_propagate_toggle_round_trips_as_one_typed_update() {
    let before = base();
    let after = apply_changeset(
        &before,
        "evo:n1",
        &statement("update trait(\"Core\").block[0].rule[\"layered\"].then[1].propagate = false"),
    );
    let edits = reconstruct(&before, &after).expect("reconstruct block propagate");
    assert!(
        matches!(
            edits.as_slice(),
            [PrimitiveEdit::Update {
                node,
                change: NodeUpdate::Propagate(false),
            }] if node.expected == NodeKind::PhonBlockNode
        ),
        "expected one typed phon-block update: {edits:#?}"
    );
    round_trip(
        "phon block propagate",
        &statement("update trait(\"Core\").block[0].rule[\"layered\"].then[1].propagate = false"),
    );
}

#[test]
fn a_flat_to_structured_phon_root_reconstructs_as_an_explicit_update() {
    let body = r#"
    #0:
        update trait("Core").block[0].rule[0].phon_block:
            b => k
            Then:
                k => a
"#;
    let before = LanguageDocument::import_new_root(
        "Symbol a\nSymbol b\nSymbol k\n\nglobal trait Core:\n    phon:\n        shift: b => k\n",
        "evo:phon-flat",
    )
    .unwrap();
    let after = apply_changeset(&before, "evo:phon-root-update", body);
    let edits = reconstruct(&before, &after).expect("reconstruct phon root");
    assert!(edits.iter().any(|edit| matches!(
        edit,
        PrimitiveEdit::Update {
            change: NodeUpdate::PhonBlockRoot(_),
            ..
        }
    )));
    reconstruct_and_replay(&before, &after);
}

const EXPRESSION_ROOT: &str = r#"
trait Marked:
trait Other:

sign alt:
    syn:
        slots:
            value [*]
    phon:
        /a{value}/

sign wrap:
    syn:
        slots:
            value [*]
    phon:
        /w{value}/

sign root:
    phon:
        /r/
    case:
        else:
            wrap(value: wrap(value: {$self}))
            belongs Marked
"#;

#[test]
fn a_nested_application_update_is_not_duplicated_on_its_parent() {
    let before =
        LanguageDocument::import_new_root(EXPRESSION_ROOT, "evo:expression-nested").unwrap();
    let root = before.ref_for_sign("root").unwrap();
    let case = children(&before, &root, NodeKind::Case)[0].clone();
    let branch = children(&before, &case, NodeKind::CaseBranch)[0].clone();
    let outer = children(&before, &branch, NodeKind::Application)[0].clone();
    let inner = children(&before, &outer, NodeKind::Application)[0].clone();
    let mut replacement = before
        .language()
        .sign_named("root")
        .unwrap()
        .items
        .iter()
        .find_map(|item| match item {
            conlang_language::SignItem::SignExpression(expression) => {
                let Expression::Case(case) = &expression.expression else {
                    return None;
                };
                let Expression::SignApplication(outer) = &case.branches[0].result else {
                    return None;
                };
                let conlang_language::SignArgumentValue::Application(inner) =
                    &outer.arguments[0].value
                else {
                    return None;
                };
                Some((**inner).clone())
            }
            _ => None,
        })
        .unwrap();
    replacement.callee = "alt".to_owned();
    let after = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: inner.clone(),
            change: NodeUpdate::SignApplication(replacement),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let edits = reconstruct_and_replay(&before, &after);
    assert!(matches!(
        edits.as_slice(),
        [PrimitiveEdit::Update {
            node,
            change: NodeUpdate::SignApplication(_),
        }] if node == &inner
    ));
}

#[test]
fn case_header_and_branch_changes_are_typed_and_replayable() {
    let before = LanguageDocument::import_new_root(
        r#"
sign root:
    syn:
        feature:
            trigger = enum(on, off)
            trigger = on
            result = enum(base, selected)
            result = base
    phon:
        /r/
    case:
        $self.syn.trigger == on:
            syn:
                feature:
                    result = selected
"#,
        "evo:expression-case",
    )
    .unwrap();
    let root = before.ref_for_sign("root").unwrap();
    let case = children(&before, &root, NodeKind::Case)[0].clone();
    let selected = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: case.clone(),
            change: NodeUpdate::CaseSelection(conlang_language::CaseSelection::Accumulate),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let case_edits = reconstruct_and_replay(&before, &selected);
    assert!(matches!(
        case_edits.as_slice(),
        [PrimitiveEdit::Update {
            node,
            change: NodeUpdate::CaseHeader { .. },
        }] if node == &case
    ));

    let branch_before = LanguageDocument::import_new_root(
        "trait A:\ntrait B:\n\nsign root:\n    phon:\n        /r/\n    case:\n        else:\n            $self\n            belongs A\n",
        "evo:expression-branch",
    )
    .unwrap();
    let branch_root = branch_before.ref_for_sign("root").unwrap();
    let branch_case = children(&branch_before, &branch_root, NodeKind::Case)[0].clone();
    let branch = children(&branch_before, &branch_case, NodeKind::CaseBranch)[0].clone();
    let mut changed_branch = branch_before
        .language()
        .sign_named("root")
        .unwrap()
        .items
        .iter()
        .find_map(|item| match item {
            conlang_language::SignItem::SignExpression(expression) => {
                let Expression::Case(case) = &expression.expression else {
                    return None;
                };
                Some(case.branches[0].clone())
            }
            _ => None,
        })
        .unwrap();
    changed_branch.belongs = vec!["B".to_owned()];
    let guarded = apply_edit(
        &branch_before,
        PrimitiveEdit::Update {
            node: branch.clone(),
            change: NodeUpdate::CaseBranch(changed_branch),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let branch_edits = reconstruct_and_replay(&branch_before, &guarded);
    assert!(matches!(
        branch_edits.as_slice(),
        [PrimitiveEdit::Update {
            node,
            change: NodeUpdate::CaseBranch(_),
        }] if node == &branch
    ));
}

#[test]
fn same_parent_case_branch_rotation_uses_one_lcs_move() {
    let source = r#"
trait A:
trait B:

sign root:
    phon:
        /r/
    case:
        $self == [A]:
            $self
        $self == [B]:
            $self
        else:
            $self
"#;
    let before = LanguageDocument::import_new_root(source, "evo:reorder-case").unwrap();
    let root = before.ref_for_sign("root").unwrap();
    let case = children(&before, &root, NodeKind::Case)[0].clone();
    let branches = children(&before, &case, NodeKind::CaseBranch);
    let after = apply_edit(
        &before,
        PrimitiveEdit::Move {
            node: branches[0].clone(),
            new_parent: case,
            anchor: Anchor::After(branches[1].clone()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let edits = reconstruct_and_replay(&before, &after);
    assert_eq!(
        edits
            .iter()
            .filter(|edit| matches!(edit, PrimitiveEdit::Move { .. }))
            .count(),
        1
    );
}

#[test]
fn reorder_with_insert_and_delete_keeps_final_item_order() {
    let before = LanguageDocument::import_new_root(
        concat!(
            "trait A:\ntrait B:\ntrait C:\ntrait D:\n\n",
            "sign root:\n",
            "    belongs A\n",
            "    belongs B\n",
            "    belongs C\n",
            "    phon:\n",
            "        /r/\n",
        ),
        "evo:reorder-mixed",
    )
    .unwrap();
    let root = before.ref_for_sign("root").unwrap();
    let definitions = children(&before, &root, NodeKind::Belongs);
    let moved = apply_edit(
        &before,
        PrimitiveEdit::Move {
            node: definitions[2].clone(),
            new_parent: root.clone(),
            anchor: Anchor::Before(definitions[0].clone()),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let inserted = apply_edit(
        &moved,
        PrimitiveEdit::Insert {
            parent: root,
            anchor: Anchor::Before(definitions[1].clone()),
            subtree: DetachedNode::Item(conlang_language::SignItem::TraitMount { name: "D".to_owned(), kind: conlang_language::TraitMountKind::Declaration, args: vec![] }),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let after = apply_edit(
        &inserted,
        PrimitiveEdit::Delete {
            node: definitions[0].clone(),
        },
        &LibrarySpec::default(),
    )
    .unwrap()
    .document;
    let edits = reconstruct_and_replay(&before, &after);
    let kinds = edits
        .iter()
        .map(|edit| match edit {
            PrimitiveEdit::Update { .. } => "update",
            PrimitiveEdit::Move { .. } => "move",
            PrimitiveEdit::Insert { .. } => "insert",
            PrimitiveEdit::Delete { .. } => "delete",
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds.last(), Some(&"delete"));
    assert!(kinds.contains(&"insert"));
    assert!(kinds.contains(&"move"));
}

// ── 生與滅 ────────────────────────────────────────────────────────────────

#[test]
fn a_cloned_sign_round_trips() {
    round_trip("clone", &statement("clone sign(\"book\") as tome"));
}

#[test]
fn a_deleted_sign_round_trips() {
    round_trip("delete sign", &statement("delete sign(\"kobo\")"));
}

#[test]
fn a_deleted_item_round_trips() {
    round_trip(
        "delete item",
        // 刪衍生邊而非義項:義項被邊引用著,刪掉會讓邊指向不存在的義項而驗證失敗
        // ——那是 fixture 的問題,不是還原的問題。
        &statement("delete sign(\"book\").edge[0]"),
    );
}

#[test]
fn an_inserted_item_round_trips() {
    round_trip(
        "insert item",
        "\n    #0:\n        insert into sign(\"kobo\") at end:\n            belongs LocalNoun\n",
    );
}

#[test]
fn an_inserted_trait_round_trips() {
    round_trip(
        "insert trait",
        "\n    #0:\n        insert into language at end:\n            trait LocalAnimate:\n",
    );
}

// ── 多筆混合 ──────────────────────────────────────────────────────────────

#[test]
fn several_changes_in_one_step_round_trip() {
    // 混合案:更新 + 新增 + 刪除同時發生。單筆的測試證明不了發出順序是對的。
    round_trip(
        "mixed",
        concat!(
            "\n    #0:\n",
            "        update sign(\"book\").feature[syn.category].value = verb\n",
            "        clone sign(\"book\") as tome\n",
            "\n    #1:\n",
            "        delete sign(\"kobo\")\n",
        ),
    );
}

// ── 邊界 ──────────────────────────────────────────────────────────────────

#[test]
fn identical_documents_reconstruct_to_nothing() {
    let document = base();
    assert!(reconstruct(&document, &document)
        .expect("同一份")
        .is_empty());
}

#[test]
fn deleting_a_subtree_emits_one_delete_not_one_per_node() {
    // **最小性**。刪一個 sign 只該發**一筆** Delete——後代隨父節點一起消失。
    //
    // 逐節點各發一筆也「會過」往返性質(排序是深度優先降序,子節點本來就排在父節點
    // 之前),所以往返性質**抓不到這件事**。而 changeset 是給人讀的歷史紀錄,多出
    // N 筆冗餘刪除會讓它難讀 —— 這是獨立的品質性質,要獨立斷言。
    let before = base();
    let after = apply_changeset(&before, "evo:n1", &statement("delete sign(\"book\")"));
    let edits = reconstruct(&before, &after).expect("還原");
    assert_eq!(
        edits.len(),
        1,
        "book 底下有 7 個項目,但只該發一筆 Delete:{edits:#?}"
    );
}

#[test]
fn inserting_a_subtree_emits_one_insert_not_one_per_node() {
    let before = base();
    let after = apply_changeset(
        &before,
        "evo:n1",
        &statement("clone sign(\"book\") as tome"),
    );
    let edits = reconstruct(&before, &after).expect("還原");
    assert_eq!(
        edits.len(),
        1,
        "整棵子樹一次插入(承 P16「優先一次完整 Insert」):{edits:#?}"
    );
}

#[test]
fn belongs_type_args_roundtrip() {
    let src = "trait Schema<C>:\n    pass\n\nsign Foo:\n    belongs Schema<Noun>\n    Schema\n";
    let before = LanguageDocument::import_new_root(src, "evo:root").expect("parses");

    let mut after_lang = before.language().clone();
    for item in &mut after_lang.signs[0].items {
        if let conlang_language::SignItem::TraitMount {
            kind: conlang_language::TraitMountKind::Declaration,
            args,
            ..
        } = item
        {
            args.clear();
            args.push("Verb".to_owned());
        }
    }
    let after = LanguageDocument::import_new_root(&after_lang.dump(), "evo:root").expect("re-parses");

    assert_ne!(before.source(), after.source(), "precondition: sources differ");
    let edits = reconstruct(&before, &after).expect("reconstruct");
    assert!(!edits.is_empty(), "changing type args should produce edits");

    let spec = LibrarySpec::default();
    let mut replayed = before.clone();
    for edit in &edits {
        replayed = apply_edit(&replayed, edit.clone(), &spec)
            .expect("replay")
            .document;
    }
    assert_eq!(
        replayed.source(),
        after.source(),
        "roundtrip: reconstructed edits must reproduce the target"
    );
}
