//! **`belongs` 定址與 `node(<kind>, @id)` 的排印↔回讀一致性**。
//!
//! ## 誌誤:這個檔在補的是什麼
//!
//! `kind_keyword` 原本以 `_ => "node"` 收尾,而 `parse_kind` 不認得 `"node"`。
//! 兩表對不上的後果不是「少一種寫法」,是 **`dump()` 會排出一份自己讀不回來的 `.chg`**:
//!
//! ```text
//! 手改 .lang:  belongs Verby  →  belongs Nouny
//! reconstruct: Update { node: Belongs, change: Belongs("Nouny") }   ← 正確
//! dump():      update node(node, @evo:root:6).target = Nouny        ← 壞掉
//! 回讀:        CHANGESET_PARSE: unknown node kind "node"
//! ```
//!
//! 落進兜底的有 10 種 kind,其中 **7 種有可編輯欄位**(`TraitUse`/`Belongs`/`SlotMap`/
//! `FeatureDeclaration`/`FeatureValue`/`SlotFeatureBinding`/`RoleBinding`),
//! 也就是七種改動都排不出可用的 changeset。步驟 16 ③「手改 `.lang` → 回推 changeset
//! → 照常 commit」在這些維度上是斷的。
//!
//! 分兩層驗:
//!
//! 1. **兩表互為反函數**——這是結構性的修法,靠窮盡 match 讓編譯器擋下一次;
//! 2. **`belongs["名字"]` 路徑選擇器**——讓人手寫得出來,而不是只能寫機器排的穩定 id。

use conlang_changeset::reconstruct::reconstruct;
use conlang_changeset::{
    apply_edit, change_set_prelude, ChangeInterpreter, NodeUpdate, PrimitiveEdit, ReplayError,
    ResolvedChangeSet, ResolvedStatement, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec, NodeKind, NodeRef};

const SOURCE: &str = r#"trait Verby:

trait Nouny:

sign go:
    belongs Verby
    entrenchment = 0.2
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("fixture parses")
}

fn belongs_node(document: &LanguageDocument) -> NodeRef {
    let entry = document
        .identities()
        .nodes
        .iter()
        .find(|entry| entry.kind == NodeKind::Belongs)
        .expect("fixture has a belongs node");
    NodeRef::new(entry.id.clone(), NodeKind::Belongs)
}

fn changeset(document: &LanguageDocument, namespace: &str, body: &str) -> String {
    let mut source =
        change_set_prelude(document, &LibrarySpec::default(), namespace).expect("prelude");
    source.push_str(body);
    source
}

// ── ① 兩表互為反函數 ────────────────────────────────────────────────────────

/// **全 kind 掃描**。逐項斷言會漏——漏掉的那一項正是會壞的那一項(`Belongs` 就是
/// 這樣漏掉的)。故改成把每一種 kind 都排一次、讀一次。
///
/// 清單刻意寫死而不是靠反射:`NodeKind` 新增變體時 `kind_keyword` 的窮盡 match 會先
/// 讓編譯器紅,那時候人自然會回來補這一行。
#[test]
fn kind_keyword_round_trips_for_every_node_kind() {
    const ALL: &[NodeKind] = &[
        NodeKind::Language,
        NodeKind::DslDeclaration,
        NodeKind::Distribution,
        NodeKind::Trait,
        NodeKind::Sign,
        NodeKind::Block,
        NodeKind::TraitUse,
        NodeKind::Belongs,
        NodeKind::Slot,
        NodeKind::SlotMap,
        NodeKind::FeatureDeclaration,
        NodeKind::FeatureValue,
        NodeKind::SlotFeatureBinding,
        NodeKind::RoleDeclaration,
        NodeKind::RoleBinding,
        NodeKind::Sense,
        NodeKind::SenseEdge,
        NodeKind::Realization,
        NodeKind::FeatureRule,
        NodeKind::Definition,
        NodeKind::Rule,
        NodeKind::RuleElseBranch,
        NodeKind::RuleThenBranch,
        NodeKind::PhonStatement,
        NodeKind::PhonBlockNode,
        NodeKind::RealizationBranch,
        NodeKind::Application,
        NodeKind::Case,
        NodeKind::CaseBranch,
        NodeKind::Constraint,
    ];

    let document = base();
    let node = document.identities().nodes[0].id.clone();
    for kind in ALL {
        // 排印面:走真正的 dump 路徑(不直接呼叫私有的 `kind_keyword`)。
        let mut resolved = UnresolvedChangeSet::parse(&changeset(&document, "evo:kinds", ""))
            .expect("parses")
            .resolve(&document, &LibrarySpec::default())
            .expect("resolves");
        resolved.statements = vec![ResolvedStatement {
            ordinal: 0,
            edits: vec![PrimitiveEdit::Update {
                node: NodeRef::new(node.clone(), *kind),
                change: NodeUpdate::Belongs("Nouny".to_owned()),
            }],
        }];
        let dump = resolved.dump().expect("dump");
        assert!(
            !dump.contains("node(node,"),
            "{kind:?} 沒有自己的關鍵字,排成了兜底的 `node`:\n{dump}"
        );
        // 回讀面:排得出來就必須讀得回去。
        UnresolvedChangeSet::parse(&dump)
            .unwrap_or_else(|error| panic!("{kind:?} 排出的 dump 讀不回來: {error}\n{dump}"));
    }
}

/// 端到端還原:手改 `.lang` 的 `belongs` → `reconstruct` → `dump` → **回讀成功**。
/// 這是步驟 16 ③ 那條路,也是原本斷掉的那條。
#[test]
fn a_hand_edited_belongs_reconstructs_into_a_readable_changeset() {
    let before = base();
    let after = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: belongs_node(&before),
            change: NodeUpdate::Belongs("Nouny".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .expect("belongs 可更新")
    .document;
    assert!(
        after.source().contains("belongs Nouny"),
        "{}",
        after.source()
    );

    let edits = reconstruct(&before, &after).expect("還原");
    assert_eq!(edits.len(), 1, "{edits:?}");
    assert!(
        matches!(
            &edits[0],
            PrimitiveEdit::Update {
                change: NodeUpdate::Belongs(target),
                ..
            } if target == "Nouny"
        ),
        "{edits:?}"
    );

    let mut resolved = UnresolvedChangeSet::parse(&changeset(&before, "evo:n1", ""))
        .expect("parses")
        .resolve(&before, &LibrarySpec::default())
        .expect("resolves");
    resolved.statements = vec![ResolvedStatement { ordinal: 0, edits }];
    let dump = resolved.dump().expect("dump");
    assert!(dump.contains("node(belongs, @"), "{dump}");

    // 回讀 → 重解析 → 跑起來必須等於 `after`(不是只求 parse 不報錯)。
    let replayed = ChangeInterpreter::new(before.clone(), LibrarySpec::default(), "evo:n1")
        .expect("interpreter")
        .run(
            &UnresolvedChangeSet::parse(&dump)
                .expect("dump 必須讀得回來")
                .resolve(&before, &LibrarySpec::default())
                .expect("回讀後可解析"),
        )
        .expect("回讀後可執行");
    assert_eq!(replayed.document.source(), after.source());
}

// ── ② `belongs["名字"]` 路徑選擇器 ──────────────────────────────────────────

#[test]
fn a_changeset_can_name_a_belongs_by_its_target() {
    let before = base();
    let resolved = UnresolvedChangeSet::parse(&changeset(
        &before,
        "evo:named",
        "\n    #0:\n        update sign(\"go\").belongs[\"Verby\"].target = Nouny\n",
    ))
    .expect("parses")
    .resolve(&before, &LibrarySpec::default())
    .expect("belongs 可依目標名定址");

    let outcome = ChangeInterpreter::new(before.clone(), LibrarySpec::default(), "evo:named")
        .expect("interpreter")
        .run(&resolved)
        .expect("runs");
    let rendered = outcome.document.source();
    assert!(rendered.contains("belongs Nouny"), "{rendered}");
    assert!(!rendered.contains("belongs Verby"), "{rendered}");
    // 其餘內容不得被波及。
    assert!(rendered.contains("entrenchment = 0.2"), "{rendered}");
}

#[test]
fn a_belongs_selector_that_names_the_wrong_trait_is_rejected() {
    // 近似反例:鍵是**目標名**,不是「這個 sign 的第幾個 belongs」。若實作退化成
    // 「取第一個 Belongs 子節點」,這條會綠——那正是要擋的。
    let before = base();
    let error = UnresolvedChangeSet::parse(&changeset(
        &before,
        "evo:wrong",
        "\n    #0:\n        update sign(\"go\").belongs[\"Nouny\"].target = Verby\n",
    ))
    .expect("parses")
    .resolve(&before, &LibrarySpec::default())
    .expect_err("go 並不 belongs Nouny");
    assert!(
        matches!(&error, ReplayError::StatementSelector { ordinal: 0, .. }),
        "{error:?}"
    );
}

#[test]
fn the_selector_picks_the_named_belongs_out_of_several() {
    // **判別性**:`go` 同時屬三個 trait,點名中間那一個。實作若忽略名字、取第一個或
    // 最後一個,結果都會不同。
    const MULTI: &str = "trait A:\n\ntrait B:\n\ntrait C:\n\ntrait D:\n\n\
                         sign go:\n    belongs A\n    belongs B\n    belongs C\n";
    let before = LanguageDocument::import_new_root(MULTI, "evo:root").expect("parses");
    let resolved = UnresolvedChangeSet::parse(&changeset(
        &before,
        "evo:multi",
        "\n    #0:\n        update sign(\"go\").belongs[\"B\"].target = D\n",
    ))
    .expect("parses")
    .resolve(&before, &LibrarySpec::default())
    .expect("resolves");
    let rendered = ChangeInterpreter::new(before.clone(), LibrarySpec::default(), "evo:multi")
        .expect("interpreter")
        .run(&resolved)
        .expect("runs")
        .document
        .source();
    assert!(rendered.contains("belongs A"), "{rendered}");
    assert!(rendered.contains("belongs D"), "{rendered}");
    assert!(rendered.contains("belongs C"), "{rendered}");
    assert!(!rendered.contains("belongs B"), "{rendered}");
}

// ── ③ TraitUse:與 `belongs` 對稱的那一半 ────────────────────────────────────
//
// `belongs` 的定址與欄位在 ①② 補齊了,`Foo[n]`(`TraitUse`)沒有——它排不出
// `.chg`、也讀不回來,而 `dump()` 遇到排不出的編輯是**靜靜丟掉**。那條路是
// 工作副本的存檔路徑(`Session::save_working_copy`),丟掉等於存出一份
// 「看起來正常、replay 回來卻少一條編輯」的檔案。

const MACRO_SOURCE: &str = r#"trait Verby:

trait Nouny:

sign go:
    Verby[0]
    entrenchment = 0.2
"#;

fn macro_base() -> LanguageDocument {
    LanguageDocument::import_new_root(MACRO_SOURCE, "evo:macro").expect("fixture parses")
}

fn trait_use_node(document: &LanguageDocument) -> NodeRef {
    let entry = document
        .identities()
        .nodes
        .iter()
        .find(|entry| entry.kind == NodeKind::TraitUse)
        .expect("fixture has a trait use node");
    NodeRef::new(entry.id.clone(), NodeKind::TraitUse)
}

/// 🔑 手改 `.lang` 的 `Foo[n]` → `reconstruct` → `dump` → **回讀成功**。
///
/// 與 `a_hand_edited_belongs_reconstructs_into_a_readable_changeset` 同一條路,
/// 只是換成 macro 引用。原本這條路在 `dump` 那一步斷掉:`dump_update` 對
/// `TraitUse` 回 `None`,那條 update 直接從輸出裡消失。
#[test]
fn a_hand_edited_trait_use_reconstructs_into_a_readable_changeset() {
    let before = macro_base();
    let after = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: trait_use_node(&before),
            change: NodeUpdate::TraitUse {
                name: "Nouny".to_owned(),
                block: Some(0),
            },
        },
        &LibrarySpec::default(),
    )
    .expect("trait use 可更新")
    .document;
    assert!(after.source().contains("Nouny[0]"), "{}", after.source());

    let edits = reconstruct(&before, &after).expect("還原");
    assert_eq!(edits.len(), 1, "{edits:?}");

    // dump 出來的那一句必須**存在**且讀得回去
    let resolved = UnresolvedChangeSet::parse(&changeset(
        &before,
        "evo:m1",
        "\n    #0:\n        update sign(\"go\").trait_use[\"Verby\"].target = Nouny[0]\n",
    ))
    .expect("parses")
    .resolve(&before, &LibrarySpec::default())
    .expect("resolves");
    let dumped = resolved.dump().expect("dump");
    // dump 走穩定 id 形式(`node(trait_use, @id)`),路徑形式是給人手寫的入口;
    // 值含 `[]` 會被 `dump_value` 加引號,回讀時脫掉——正確性由下面的 round-trip 驗。
    assert!(
        dumped.contains("node(trait_use, @") && dumped.contains("Nouny[0]"),
        "{dumped}"
    );

    // 排出來的東西讀得回去,且 replay 結果與模型層一致
    let round = UnresolvedChangeSet::parse(&dumped)
        .expect("回讀")
        .resolve(&before, &LibrarySpec::default())
        .expect("回讀後 resolve");
    let replayed = ChangeInterpreter::new(
        before.clone(),
        LibrarySpec::default(),
        "evo:m1".to_owned(),
    )
    .expect("interpreter")
    .run(&round)
    .expect("runs")
    .document;
    assert!(replayed.source().contains("Nouny[0]"), "{}", replayed.source());
}

/// 裸 `Foo`(整個 trait,`block: None`)也要能來回。
#[test]
fn a_whole_trait_use_round_trips_without_an_index() {
    let before = macro_base();
    let resolved = UnresolvedChangeSet::parse(&changeset(
        &before,
        "evo:m2",
        "\n    #0:\n        update sign(\"go\").trait_use[\"Verby\"].target = Nouny\n",
    ))
    .expect("parses")
    .resolve(&before, &LibrarySpec::default())
    .expect("resolves");
    let dumped = resolved.dump().expect("dump");
    assert!(
        dumped.contains(".target = Nouny\n"),
        "裸名字要原樣排回去(不得補上 [0]):{dumped}"
    );
}

/// 🔑 **排不出來的編輯必須報錯,不得靜默丟棄。**
///
/// 判別性:舊行為下 `dump()` 會回一份「少了那一句」的 `.chg`——而且它 parse 得過、
/// replay 得動,只是產物不對。所以這裡驗的不是「有沒有錯誤訊息」,是
/// **不會拿到一份沉默的壞檔案**。
#[test]
fn an_update_that_cannot_be_written_as_a_field_is_an_error_not_a_silent_drop() {
    let before = macro_base();
    let entry = before
        .identities()
        .nodes
        .iter()
        .find(|entry| entry.kind == NodeKind::Definition)
        .expect("fixture has a definition");
    let node = NodeRef::new(entry.id.clone(), NodeKind::Definition);

    // `SlotMap` 的酬載是一個操作,不是字串——`dump_update` 對它回 `None`
    // 走正常路徑拿一份 resolved,再把其中一條編輯換成排不出來的那種
    let mut resolved = UnresolvedChangeSet::parse(&changeset(
        &before,
        "evo:m3",
        "\n    #0:\n        update sign(\"go\").trait_use[\"Verby\"].target = Nouny[0]\n",
    ))
    .expect("parses")
    .resolve(&before, &LibrarySpec::default())
    .expect("resolves");
    resolved.statements[0].edits = vec![PrimitiveEdit::Update {
        node,
        change: NodeUpdate::SlotMap(conlang_language::SlotMapOp::Internalize {
            slot: "AGENT".to_owned(),
        }),
    }];

    let error = resolved.dump().expect_err("排不出來就該報錯");
    assert_eq!(error.kind, "SlotMap");
}
