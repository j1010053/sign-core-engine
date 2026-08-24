//! 前端邊界的出口。
//!
//! **這組測試就是前端的測試出口**——Tauri 那層是 1:1 的膠水
//! (`state.lock().unwrap().tree()`),沒有自己的邏輯可測。
//!
//! 釘的是 MVP 兩個面板加編輯頁真的走得通:
//! 演化樹 → 點節點 → 編輯頁 → 改 label/State/旁註 → **語言內容不動**。

use conlang_app::ipc::{
    BodyItemInput, LexiconQuery, MovePlacementInput, PackageSelectionInput, ProjectSlot,
    ProposalQuery, SegmentWeight, SoundChangeInput, StructuredEdit, StructuredEditInput, UiSession,
};
use conlang_changeset::evolution::{Edge, EvolutionGraph, Nativization};
use conlang_changeset::state::{Contact, ContactIntensity, EvolutionState};
use conlang_changeset::{change_set_prelude, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_persistence::{GraphStore, ProjectDocument};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "Symbol k\nSymbol a\nSymbol t\n\nClass consonant {k, t}\nClass vowel {a}\n\nglobal trait Core:\n\n\
sign kat:\n    belongs Noun\n    phon:\n        /kat/\n    sem:\n        senses:\n            core = STONE\n\
sign tak:\n    belongs Verb\n    phon:\n        /tak/\n    sem:\n        senses:\n            core = GO\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Temp {
        let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "conlang-ipc-{name}-{}-{ordinal}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        Temp(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

/// root(Proto)→ 子節點(Daughter)。
fn project(temp: &Temp) -> GraphStore {
    let store = GraphStore::init(&temp.0).expect("init");
    let spec = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(spec.clone());
    let root = graph
        .add_root(LanguageDocument::import_new_root(SOURCE, "ipc:root").expect("root"))
        .expect("add_root");
    graph
        .set_label(&root, Some("Proto".to_owned()))
        .expect("label");

    let base = graph.snapshot(&root).expect("snapshot").clone();
    let mut text = change_set_prelude(&base, &spec, "ipc:child").expect("prelude");
    text.push_str("\n    #0:\n        update sign(\"kat\").def[phon].value = /kats/\n");
    UnresolvedChangeSet::parse(&text).expect("parses");
    graph
        .commit(
            vec![Edge::trunk(root, text)],
            Nativization::None,
            Some("Daughter".to_owned()),
        )
        .expect("commit");

    store.save(&graph).expect("save");
    store
        .write_project(&ProjectDocument::from_spec(&spec))
        .expect("project");
    store
}

fn session(temp: &Temp) -> UiSession {
    project(temp);
    UiSession::open(&temp.0, LibrarySpec::default()).expect("open")
}

fn default_package_input() -> PackageSelectionInput {
    let spec = LibrarySpec::default();
    PackageSelectionInput {
        std: spec.std.iter().map(ToString::to_string).collect(),
        natural: spec.natural.as_ref().map(ToString::to_string),
        plugins: spec.plugins.iter().map(ToString::to_string).collect(),
        ..PackageSelectionInput::default()
    }
}

// ── 演化樹面板 ───────────────────────────────────────────────────────────

/// 🔑 **開啟 → 樹 → 點節點 → 編輯頁**,MVP 的主流程。
#[test]
fn the_tree_panel_leads_into_each_node_detail_page() {
    let temp = Temp::new("flow");
    let mut session = session(&temp);

    let tree = session.tree();
    assert_eq!(tree.nodes.len(), 2);
    let daughter = tree
        .nodes
        .iter()
        .find(|n| n.label.as_deref() == Some("Daughter"))
        .expect("有子節點")
        .clone();

    let detail = session.select_node(&daughter.id).expect("select");
    assert_eq!(detail.id, daughter.id);
    assert_eq!(detail.label.as_deref(), Some("Daughter"));
    assert_eq!(detail.sign_count, 2);

    // 樹的 active 跟著移動——UI 才知道現在停在哪
    assert_eq!(session.tree().active.as_deref(), Some(daughter.id.as_str()));
}

/// 點不存在的節點 ⇒ 明確的 `code`,不是靜默切換。
#[test]
fn selecting_an_unknown_node_reports_a_code() {
    let temp = Temp::new("unknown");
    let mut session = session(&temp);
    let error = session.select_node("not-a-digest").expect_err("應拒絕");
    assert_eq!(error.code, "APP_UNKNOWN_NODE");
    assert!(!error.code.contains(' '), "code 供比對,不是句子");
}

// ── 辭典面板 ─────────────────────────────────────────────────────────────

/// 🔑 **切換節點會換到那個節點的詞典。**
///
/// 判別性:root 的 `kat` 是 `/kat/`,子節點被改成 `/kats/`——若詞典沒跟著
/// active 走,兩邊會拿到同一份。
#[test]
fn the_lexicon_follows_the_selected_node() {
    let temp = Temp::new("lexicon");
    let mut session = session(&temp);
    let tree = session.tree();
    let root = tree
        .nodes
        .iter()
        .find(|n| n.parents.is_empty())
        .expect("root")
        .clone();
    let daughter = tree
        .nodes
        .iter()
        .find(|n| !n.parents.is_empty())
        .expect("child")
        .clone();

    session.select_node(&root.id).expect("root");
    let at_root = session.lexicon(&LexiconQuery::default()).expect("lexicon");
    assert_eq!(at_root.node, root.id);
    let kat = at_root
        .lexicon
        .entries
        .iter()
        .find(|e| e.name == "kat")
        .expect("kat");
    assert_eq!(kat.underlying_form.as_deref(), Some("kat"));

    session.select_node(&daughter.id).expect("daughter");
    let at_child = session.lexicon(&LexiconQuery::default()).expect("lexicon");
    assert_eq!(at_child.node, daughter.id);
    let kat = at_child
        .lexicon
        .entries
        .iter()
        .find(|e| e.name == "kat")
        .expect("kat");
    assert_eq!(
        kat.underlying_form.as_deref(),
        Some("kats"),
        "子節點的形已變"
    );
}

/// 前端送的查詢條件真的生效(範疇走 ontology 閉包、gloss 子字串、排序)。
#[test]
fn the_lexicon_query_from_the_front_end_is_applied() {
    let temp = Temp::new("query");
    let mut session = session(&temp);

    let all = session.lexicon(&LexiconQuery::default()).expect("all");
    assert_eq!(all.lexicon.entries.len(), 2);

    let nominal = session
        .lexicon(&LexiconQuery {
            category: Some("Nominal".to_owned()),
            ..LexiconQuery::default()
        })
        .expect("filtered");
    assert_eq!(
        nominal.lexicon.entries.len(),
        1,
        "belongs Noun 被 Nominal 選中"
    );
    assert_eq!(nominal.lexicon.total_before_filter, 2, "分母是過濾前");

    let by_gloss = session
        .lexicon(&LexiconQuery {
            gloss_contains: Some("STONE".to_owned()),
            ..LexiconQuery::default()
        })
        .expect("gloss");
    assert_eq!(by_gloss.lexicon.entries.len(), 1);

    let names = |query: &LexiconQuery| -> Vec<String> {
        let mut session = UiSession::open(&temp.0, LibrarySpec::default()).expect("open");
        session
            .lexicon(query)
            .expect("lexicon")
            .lexicon
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    };
    assert_eq!(names(&LexiconQuery::default()), vec!["kat", "tak"]);
    assert_eq!(
        names(&LexiconQuery {
            sort: Some("gloss".to_owned()),
            ..LexiconQuery::default()
        }),
        vec!["tak", "kat"],
        "GO < STONE"
    );
}

/// 未知的 `sort` 值視為 `name`,不是錯誤——前端傳了新值不該讓畫面炸掉。
#[test]
fn an_unrecognised_sort_falls_back_to_name_order() {
    let temp = Temp::new("sort-fallback");
    let mut session = session(&temp);
    let odd = session
        .lexicon(&LexiconQuery {
            sort: Some("由我發明的排序".to_owned()),
            ..LexiconQuery::default()
        })
        .expect("不該是錯誤");
    assert_eq!(
        odd.lexicon
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        vec!["kat", "tak"]
    );
}

// ── 節點編輯頁:改的全是雜湊外的 ─────────────────────────────────────────

/// 🔑 **改 label / State / 旁註,節點身分與語言內容全都不動。**
///
/// 這是編輯頁存在的前提:它改的東西在 P64 的雜湊外槽位,所以**不會弄壞演化史**。
/// 判別性:若哪個欄位其實進了雜湊,`id` 就會變。
#[test]
fn editing_a_node_never_disturbs_its_identity_or_language() {
    let temp = Temp::new("edit");
    let mut session = session(&temp);
    let before = session.node_detail().expect("detail");
    let lexicon_before = session.lexicon(&LexiconQuery::default()).expect("lexicon");

    let renamed = session.set_label(Some("原始語".to_owned())).expect("label");
    assert_eq!(renamed.label.as_deref(), Some("原始語"));

    let state = EvolutionState {
        time: Some("約 800".to_owned()),
        region: Some("河谷北岸".to_owned()),
        contacts: vec![Contact {
            counterpart: "鄰語".to_owned(),
            period: None,
            intensity: ContactIntensity::Trade,
        }],
        ..EvolutionState::default()
    };
    let with_state = session.set_state(&state).expect("state");
    assert_eq!(with_state.state, state);

    let annotated = session
        .write_annotation("culture.md", "石頭象徵盟約")
        .expect("annotation");
    assert_eq!(annotated.annotations, vec!["culture.md"]);
    assert_eq!(
        session.read_annotation("culture.md").expect("read"),
        "石頭象徵盟約"
    );

    // 🔑 三樣都改完了,而**身分與語言內容逐項不變**
    assert_eq!(annotated.id, before.id, "節點 id 不變——那些欄位在雜湊外");
    assert_eq!(annotated.sign_count, before.sign_count);
    assert_eq!(
        session
            .lexicon(&LexiconQuery::default())
            .expect("lexicon")
            .lexicon,
        lexicon_before.lexicon,
        "詞典逐欄位不變"
    );
}

/// label 落盤了——重開仍在。
#[test]
fn a_renamed_node_keeps_its_name_after_reopening() {
    let temp = Temp::new("rename");
    let mut session = session(&temp);
    let id = session.node_detail().expect("detail").id;
    session.set_label(Some("原始語".to_owned())).expect("label");
    drop(session);

    let reopened = UiSession::open(&temp.0, LibrarySpec::default()).expect("reopen");
    let node = reopened
        .tree()
        .nodes
        .into_iter()
        .find(|n| n.id == id)
        .expect("節點還在");
    assert_eq!(node.label.as_deref(), Some("原始語"));
}

/// 沒開節點時,編輯頁的操作回明確的 code。
#[test]
fn editing_without_a_node_reports_a_code() {
    let temp = Temp::new("no-node");
    GraphStore::init(&temp.0).expect("init");
    let mut session = UiSession::open(&temp.0, LibrarySpec::default()).expect("open");

    for error in [
        session.node_detail().expect_err("detail"),
        session.set_label(None).expect_err("label"),
        session
            .set_state(&EvolutionState::default())
            .expect_err("state"),
        session
            .write_annotation("a.md", "x")
            .expect_err("annotation"),
    ] {
        assert_eq!(error.code, "APP_NO_ACTIVE_NODE", "{error:?}");
    }
    // 但樹照樣給得出來(空的)——UI 開一個新專案時就是這個狀態
    assert!(session.tree().nodes.is_empty());
}

/// 錯誤的 `code` 取自既有錯誤字串的前綴,不另發明一套。
#[test]
fn error_codes_come_from_the_existing_diagnostic_convention() {
    let temp = Temp::new("codes");
    let error =
        UiSession::open(temp.0.join("nope"), LibrarySpec::default()).expect_err("不存在的專案");
    assert!(
        error.code.starts_with("PERSISTENCE_"),
        "沿用既有前綴而非新造:{error:?}"
    );
    assert!(!error.message.is_empty());
}

// ── F2:同一份 pending `.chg` 的 raw / structured / persist 邊界 ────────

#[test]
fn an_invalid_raw_draft_never_replaces_the_last_valid_pending_changeset() {
    let temp = Temp::new("raw-atomic");
    let mut session = session(&temp);
    let valid = session.begin_edit("ui:test:raw").expect("begin");

    session
        .replace_pending_source("this is not a changeset")
        .expect_err("invalid source");

    let still_valid = session.pending_change().expect("old pending remains");
    assert_eq!(still_valid.source, valid.source);
    assert_eq!(still_valid.statements, 0);
}

fn session_with_source(temp: &Temp, source: &str) -> UiSession {
    let store = GraphStore::init(&temp.0).expect("init");
    let spec = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(spec.clone());
    graph
        .add_root(LanguageDocument::import_new_root(source, "ipc:custom").expect("root"))
        .expect("add root");
    store.save(&graph).expect("save graph");
    store
        .write_project(&ProjectDocument::from_spec(&spec))
        .expect("project");
    UiSession::open(&temp.0, LibrarySpec::default()).expect("open")
}

#[test]
fn a_rejected_direct_sound_change_does_not_leave_an_empty_working_copy() {
    let temp = Temp::new("sound-change-no-home");
    let mut session = UiSession::create(
        &temp.0,
        None::<&std::path::Path>,
        Some("空白語".to_owned()),
        "evo:root",
    )
    .expect("create blank project");

    session
        .stage_sound_change(&SoundChangeInput {
            rule: "t => k".to_owned(),
            home: "Core".to_owned(),
            revision: None,
        })
        .expect_err("blank language has no Core trait");

    assert!(
        !session.summary().has_pending,
        "a failed one-click action must not create a phantom working copy"
    );
}

#[test]
fn structured_edit_wire_shape_deserializes_the_flattened_action_tag() {
    let trait_input: StructuredEditInput = serde_json::from_value(serde_json::json!({
        "revision": "revision-1",
        "action": "insert_trait",
        "name": "ShiftHome",
        "global": true
    }))
    .expect("Tauri's flattened Insert Trait payload must deserialize");
    assert!(matches!(
        trait_input.edit,
        StructuredEdit::InsertTrait {
            ref name,
            global: true,
            parent: None
        } if name == "ShiftHome"
    ));

    let sign_input: StructuredEditInput = serde_json::from_value(serde_json::json!({
        "revision": "revision-1",
        "action": "insert_sign",
        "name": "stone",
        "belongs": ["Noun"],
        "phon": "kat",
        "gloss": "STONE"
    }))
    .expect("Tauri's flattened Insert Sign payload must deserialize");
    assert!(matches!(
        sign_input.edit,
        StructuredEdit::InsertSign {
            ref name,
            ref belongs,
            phon: Some(ref phon),
            gloss: Some(ref gloss)
        } if name == "stone" && belongs == &["Noun"] && phon == "kat" && gloss == "STONE"
    ));

    let encoded = serde_json::to_value(&sign_input).expect("structured input serializes");
    assert_eq!(encoded["action"], "insert_sign");
    assert!(encoded.get("edit").is_none(), "the public DTO stays flat");

    let unexpected = serde_json::from_value::<StructuredEditInput>(serde_json::json!({
        "revision": "revision-1",
        "action": "insert_trait",
        "name": "ShiftHome",
        "global": true,
        "unexpected": "must not be ignored"
    }));
    assert!(
        unexpected.is_err(),
        "the tagged edit must still reject unknown variant fields"
    );
}

#[test]
fn structured_first_stage_is_atomic_and_pending_preview_drives_the_next_stage() {
    let temp = Temp::new("structured-preview");
    let mut session = session(&temp);
    let initial = session.authoring_catalog().expect("initial catalog");
    assert!(initial
        .traits
        .iter()
        .any(|item| item.name == "Noun" && item.source == "library"));

    let pending = session
        .stage_structured_edit(&StructuredEditInput {
            revision: initial.revision.clone(),
            edit: StructuredEdit::InsertTrait {
                name: "ShiftHome".to_owned(),
                global: true,
                parent: None,
            },
        })
        .expect("insert global trait without beginning a raw working copy");
    assert_eq!(pending.statements, 1);
    assert!(session.summary().has_pending);

    let preview = session.authoring_catalog().expect("pending-aware catalog");
    assert_ne!(preview.revision, initial.revision);
    assert!(preview
        .rule_homes
        .iter()
        .any(|choice| choice.value == "ShiftHome"));

    let pending = session
        .stage_sound_change(&SoundChangeInput {
            rule: "t => k".to_owned(),
            home: "ShiftHome".to_owned(),
            revision: Some(preview.revision.clone()),
        })
        .expect("new pending trait is a valid rule home");
    assert_eq!(pending.statements, 2);
    assert!(pending.source.contains("t => k"));

    let before_stale = pending.source.clone();
    let error = session
        .stage_structured_edit(&StructuredEditInput {
            revision: initial.revision,
            edit: StructuredEdit::InsertSign {
                name: "stale".to_owned(),
                belongs: Vec::new(),
                phon: None,
                gloss: None,
            },
        })
        .expect_err("old catalog revision must be rejected");
    assert_eq!(error.code, "APP_AUTHORING_STALE");
    assert_eq!(
        session
            .pending_change()
            .expect("pending survives stale input")
            .source,
        before_stale
    );
}

#[test]
fn rejected_structured_edits_neither_create_nor_mutate_pending() {
    let blank = Temp::new("structured-no-phantom");
    let mut blank_session = UiSession::create(
        &blank.0,
        None::<&std::path::Path>,
        Some("Blank".to_owned()),
        "evo:root",
    )
    .expect("blank project");
    let catalog = blank_session.authoring_catalog().expect("blank catalog");
    blank_session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::InsertSign {
                name: "bad".to_owned(),
                belongs: vec!["UnknownTrait".to_owned()],
                phon: None,
                gloss: None,
            },
        })
        .expect_err("unknown belongs target");
    assert!(!blank_session.summary().has_pending);

    let temp = Temp::new("structured-existing-atomic");
    let mut session = session(&temp);
    let valid = session
        .stage_sound_change(&SoundChangeInput {
            rule: "t => k".to_owned(),
            home: "Core".to_owned(),
            revision: None,
        })
        .expect("valid pending");
    let catalog = session.authoring_catalog().expect("preview catalog");
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::InsertTrait {
                name: "Core".to_owned(),
                global: true,
                parent: None,
            },
        })
        .expect_err("duplicate local trait");
    let after = session.pending_change().expect("old pending remains");
    assert_eq!(after.source, valid.source);
    assert_eq!(after.statements, valid.statements);

    let catalog = session
        .authoring_catalog()
        .expect("catalog after rejection");
    let kat = catalog
        .signs
        .iter()
        .find(|sign| sign.name == "kat")
        .expect("kat")
        .selector
        .clone();
    let root = catalog
        .nodes
        .iter()
        .find(|node| node.kind == "language")
        .expect("language root")
        .selector
        .clone();
    let error = session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::Move {
                target: kat,
                placement: MovePlacementInput {
                    parent: root,
                    position: "start".to_owned(),
                    sibling: None,
                },
            },
        })
        .expect_err("a no-op or unlisted move is rejected");
    assert_eq!(error.code, "APP_AUTHORING_MOVE_INVALID");
    assert_eq!(
        session.pending_change().expect("pending remains").source,
        valid.source
    );
}

#[test]
fn ontology_cycles_and_dangling_deletes_are_rejected_as_whole_statements() {
    let temp = Temp::new("structured-ontology-atomic");
    let mut session = session(&temp);
    let catalog = session.authoring_catalog().expect("catalog");
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::InsertTrait {
                name: "ParentA".to_owned(),
                global: false,
                parent: Some("Noun".to_owned()),
            },
        })
        .expect("insert A");
    let catalog = session.authoring_catalog().expect("A preview");
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::InsertTrait {
                name: "ChildB".to_owned(),
                global: false,
                parent: Some("ParentA".to_owned()),
            },
        })
        .expect("insert B");
    let catalog = session.authoring_catalog().expect("B preview");
    let parent_a = catalog
        .traits
        .iter()
        .find(|item| item.name == "ParentA")
        .and_then(|item| item.selector.clone())
        .expect("A selector");
    let parent_a_block = catalog
        .nodes
        .iter()
        .find(|node| node.kind == "block" && node.parent.as_deref() == Some(parent_a.as_str()))
        .expect("A block")
        .selector
        .clone();
    let belongs = catalog
        .nodes
        .iter()
        .find(|node| {
            node.kind == "belongs" && node.parent.as_deref() == Some(parent_a_block.as_str())
        })
        .expect("A belongs node")
        .selector
        .clone();
    let before_invalid = session.pending_change().expect("valid pending");
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::Update {
                target: belongs,
                field: "target".to_owned(),
                value: "ChildB".to_owned(),
            },
        })
        .expect_err("A -> B -> A cycle");
    assert_eq!(
        session.pending_change().expect("cycle is atomic").source,
        before_invalid.source
    );

    let catalog = session
        .authoring_catalog()
        .expect("catalog after cycle rejection");
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::Delete { target: parent_a },
        })
        .expect_err("B still refers to A");
    assert_eq!(
        session.pending_change().expect("delete is atomic").source,
        before_invalid.source
    );
}

#[test]
fn inserted_sign_can_be_updated_moved_deleted_and_reinserted_with_identity_rules() {
    let temp = Temp::new("structured-sign-lifecycle");
    let mut session = session(&temp);
    let catalog = session.authoring_catalog().expect("catalog");
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::InsertSign {
                name: "newsign".to_owned(),
                belongs: vec!["Noun".to_owned()],
                phon: Some("kat".to_owned()),
                gloss: Some("NEW".to_owned()),
            },
        })
        .expect("insert sign");

    let catalog = session.authoring_catalog().expect("insert preview");
    let original_selector = catalog
        .signs
        .iter()
        .find(|sign| sign.name == "newsign")
        .expect("inserted sign")
        .selector
        .clone();
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::InsertBody {
                container: original_selector.clone(),
                body: BodyItemInput::Slot {
                    name: "argument".to_owned(),
                    constraint: "Noun".to_owned(),
                    optional: true,
                },
            },
        })
        .expect("new sign accepts a slot in the same pending");

    let catalog = session.authoring_catalog().expect("slot preview");
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::Update {
                target: original_selector.clone(),
                field: "name".to_owned(),
                value: "renamed".to_owned(),
            },
        })
        .expect("rename sign");
    let catalog = session.authoring_catalog().expect("rename preview");
    assert_eq!(
        catalog
            .signs
            .iter()
            .find(|sign| sign.name == "renamed")
            .expect("renamed sign")
            .selector,
        original_selector,
        "update preserves identity"
    );
    let slot_selector = catalog
        .nodes
        .iter()
        .find(|node| node.kind == "slot" && node.path.contains("renamed"))
        .expect("inserted slot")
        .selector
        .clone();

    let moves = session
        .authoring_move_options(&slot_selector, &catalog.revision)
        .expect("validated move choices");
    let placement = moves.placements.first().expect("at least one real move");
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::Move {
                target: slot_selector.clone(),
                placement: MovePlacementInput {
                    parent: placement.parent.clone(),
                    position: placement.position.clone(),
                    sibling: placement.sibling.clone(),
                },
            },
        })
        .expect("move slot");
    let catalog = session.authoring_catalog().expect("move preview");
    assert!(catalog
        .nodes
        .iter()
        .any(|node| node.selector == slot_selector));
    assert!(catalog
        .signs
        .iter()
        .any(|sign| sign.selector == original_selector));

    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::Delete {
                target: original_selector.clone(),
            },
        })
        .expect("delete sign");
    let catalog = session.authoring_catalog().expect("delete preview");
    assert!(!catalog
        .signs
        .iter()
        .any(|sign| sign.selector == original_selector));

    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::InsertSign {
                name: "renamed".to_owned(),
                belongs: vec!["Noun".to_owned()],
                phon: Some("kat".to_owned()),
                gloss: Some("NEW".to_owned()),
            },
        })
        .expect("reinsert sign");
    let catalog = session.authoring_catalog().expect("reinsert preview");
    let replacement_selector = catalog
        .signs
        .iter()
        .find(|sign| sign.name == "renamed")
        .expect("replacement sign")
        .selector
        .clone();
    assert_ne!(
        replacement_selector, original_selector,
        "deleted ids are retired"
    );

    let committed = session
        .commit(Some("structured lifecycle".to_owned()))
        .expect("commit");
    session.save_project().expect("persist");
    let mut reopened = UiSession::open(&temp.0, LibrarySpec::default()).expect("reopen");
    reopened
        .select_node(&committed.id)
        .expect("select persisted commit");
    assert_eq!(
        reopened
            .authoring_catalog()
            .expect("reopened catalog")
            .signs
            .iter()
            .find(|sign| sign.name == "renamed")
            .expect("persisted replacement")
            .selector,
        replacement_selector,
        "commit/save/reopen preserves the preview identity"
    );
}

#[test]
fn clone_sign_allocates_a_disjoint_deep_identity_subtree() {
    fn descendants(catalog: &conlang_app::AuthoringCatalogV1, root: &str) -> Vec<String> {
        let mut found = vec![root.to_owned()];
        loop {
            let before = found.len();
            for node in &catalog.nodes {
                if node
                    .parent
                    .as_ref()
                    .is_some_and(|parent| found.contains(parent))
                    && !found.contains(&node.selector)
                {
                    found.push(node.selector.clone());
                }
            }
            if found.len() == before {
                break;
            }
        }
        found
    }

    let temp = Temp::new("structured-clone-ids");
    let mut session = session(&temp);
    let catalog = session.authoring_catalog().expect("catalog");
    let source = catalog
        .signs
        .iter()
        .find(|sign| sign.name == "kat")
        .expect("kat")
        .selector
        .clone();
    session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::CloneSign {
                source: source.clone(),
                name: "kat_clone".to_owned(),
            },
        })
        .expect("clone");
    let catalog = session.authoring_catalog().expect("clone preview");
    let clone = catalog
        .signs
        .iter()
        .find(|sign| sign.name == "kat_clone")
        .expect("clone sign")
        .selector
        .clone();
    let source_nodes = descendants(&catalog, &source);
    let clone_nodes = descendants(&catalog, &clone);
    assert!(source_nodes.len() > 1 && clone_nodes.len() > 1);
    assert!(source_nodes
        .iter()
        .all(|selector| !clone_nodes.contains(selector)));
}

#[test]
fn multi_block_trait_use_is_fanned_out_inside_one_atomic_statement() {
    let temp = Temp::new("structured-multi-block-trait");
    let mut session = session_with_source(&temp, "trait Multi:\n    ==\n\nsign host:\n");
    let catalog = session.authoring_catalog().expect("catalog");
    assert_eq!(
        catalog
            .traits
            .iter()
            .find(|item| item.name == "Multi")
            .expect("multi trait")
            .blocks,
        2
    );
    let host = catalog
        .signs
        .iter()
        .find(|sign| sign.name == "host")
        .expect("host")
        .selector
        .clone();
    let pending = session
        .stage_structured_edit(&StructuredEditInput {
            revision: catalog.revision,
            edit: StructuredEdit::InsertBody {
                container: host,
                body: BodyItemInput::TraitUse {
                    trait_name: "Multi".to_owned(),
                },
            },
        })
        .expect("insert every block reference");
    assert_eq!(pending.statements, 1, "fan-out is one transaction");
    assert!(pending.source.contains("Multi[0]"));
    assert!(pending.source.contains("Multi[1]"));
    assert_eq!(
        session
            .authoring_catalog()
            .expect("preview")
            .nodes
            .iter()
            .filter(|node| node.kind == "trait_use")
            .count(),
        2
    );
}

#[test]
fn every_guided_body_item_lowers_through_chg_and_replays() {
    let temp = Temp::new("structured-body-kinds");
    let mut session = session_with_source(
        &temp,
        "Symbol a\nSymbol b\n\ntrait Category:\n\ntrait Macro:\n    ==\n\nsign host:\n",
    );
    let bodies = vec![
        BodyItemInput::Belongs {
            trait_name: "Noun".to_owned(),
        },
        BodyItemInput::Phon {
            form: "a".to_owned(),
        },
        BodyItemInput::Slot {
            name: "argument".to_owned(),
            constraint: "Noun".to_owned(),
            optional: true,
        },
        BodyItemInput::TraitUse {
            trait_name: "Macro".to_owned(),
        },
        BodyItemInput::Feature {
            dim: "syn".to_owned(),
            // 不能用 `number`:這個 sign `belongs Noun`,而 `Noun` 經
            // `AgreementBearer` 已宣告 `syn.number`,再宣告一次會撞
            // Q1「值域宣告一次」(FEATURE_DECLARATION_SHADOWED)。
            // 這條測的是「每種 body item 都下得了 `.chg` 並重播」,與宣告
            // 唯一性無關,故換一個沒被繼承的欄位名。
            name: "hostshape".to_owned(),
            enum_values: vec!["round".to_owned(), "flat".to_owned()],
            value: "round".to_owned(),
        },
        BodyItemInput::Sense {
            name: "core".to_owned(),
            gloss: "HOST".to_owned(),
        },
        BodyItemInput::Definition {
            dim: "sem".to_owned(),
            path: "aspect".to_owned(),
            value: "completed".to_owned(),
        },
        BodyItemInput::Rule {
            dim: "phon".to_owned(),
            body: "a => b".to_owned(),
            name: Some("shift".to_owned()),
            stage: "word".to_owned(),
        },
    ];

    for body in bodies {
        let body_debug = format!("{body:?}");
        let catalog = session.authoring_catalog().expect("fresh catalog");
        let host = catalog
            .signs
            .iter()
            .find(|sign| sign.name == "host")
            .expect("host")
            .selector
            .clone();
        session
            .stage_structured_edit(&StructuredEditInput {
                revision: catalog.revision,
                edit: StructuredEdit::InsertBody {
                    container: host,
                    body,
                },
            })
            .unwrap_or_else(|error| panic!("guided body item {body_debug}: {error:?}"));
    }

    let pending = session.pending_change().expect("pending");
    assert_eq!(pending.statements, 8);
    let catalog = session.authoring_catalog().expect("final preview");
    for kind in [
        "belongs",
        "trait_use",
        "slot",
        "feature_declaration",
        "feature_value",
        "sense",
        "definition",
        "rule",
    ] {
        assert!(
            catalog.nodes.iter().any(|node| node.kind == kind),
            "missing catalog node kind {kind}"
        );
    }
}

#[test]
fn structured_authoring_commits_in_memory_then_persists_only_on_explicit_save() {
    let temp = Temp::new("authoring-boundaries");
    let mut session = session(&temp);
    let before = session.tree().nodes.len();

    let pending = session
        .stage_sound_change(&SoundChangeInput {
            rule: "t => k".to_owned(),
            home: "Core".to_owned(),
            revision: None,
        })
        .expect("stage");
    assert_eq!(pending.statements, 1);
    assert!(pending.source.contains("t => k"));

    session.commit(Some("shift".to_owned())).expect("commit");
    assert_eq!(session.tree().nodes.len(), before + 1);
    assert!(session.summary().graph_dirty);
    assert!(!session.summary().has_pending);

    let from_disk = UiSession::open(&temp.0, LibrarySpec::default()).expect("reopen before save");
    assert_eq!(from_disk.tree().nodes.len(), before, "commit 尚未落盤");

    session.save_project().expect("explicit save");
    let saved = UiSession::open(&temp.0, LibrarySpec::default()).expect("reopen after save");
    assert_eq!(saved.tree().nodes.len(), before + 1);
}

#[test]
fn project_slot_refuses_to_replace_a_dirty_session_without_explicit_discard() {
    let temp = Temp::new("dirty-slot");
    project(&temp);
    let mut slot = ProjectSlot::default();
    slot.open(&temp.0, false).expect("open");
    slot.session_mut()
        .expect("session")
        .begin_edit("ui:test:dirty")
        .expect("begin");

    let error = slot.open(&temp.0, false).expect_err("dirty guard");
    assert_eq!(error.code, "APP_DIRTY_PROJECT");
    assert!(slot.summary().expect("still open").has_pending);
}

#[test]
fn expert_source_reconcile_is_atomic_and_only_stages_a_pending_changeset() {
    let temp = Temp::new("source-reconcile");
    let mut session = session(&temp);
    let before_nodes = session.tree().nodes.len();

    let error = session
        .reconcile_source("not valid .lang")
        .expect_err("invalid source must be rejected");
    assert!(!error.code.is_empty());
    assert_eq!(session.tree().nodes.len(), before_nodes);
    assert!(
        !session.summary().has_pending,
        "failure must leave no partial working copy"
    );

    let canonical = session.source().expect("source").source;
    let edited = canonical.replace("STONE", "ROCK");
    assert_ne!(edited, canonical, "fixture must make a semantic edit");
    let report = session.reconcile_source(&edited).expect("reconcile");

    assert!(report.matched > 0);
    assert!(report.primitive_edits > 0);
    assert!(report.pending.diff.sem > 0);
    assert_eq!(
        session.tree().nodes.len(),
        before_nodes,
        "staging is not commit"
    );
    assert!(session.summary().has_pending);
    assert!(!session.summary().graph_dirty);
}

#[test]
fn rebase_preview_uses_a_graph_copy_and_apply_keeps_the_old_chain() {
    let temp = Temp::new("rebase-preview");
    let mut session = session(&temp);
    let original = session.tree();
    let root = original
        .nodes
        .iter()
        .find(|node| node.parents.is_empty())
        .expect("root")
        .id
        .clone();
    let daughter = original
        .nodes
        .iter()
        .find(|node| !node.parents.is_empty())
        .expect("daughter")
        .id
        .clone();

    session.select_node(&root).expect("select ancestor");
    session
        .stage_sound_change(&SoundChangeInput {
            rule: "t => k".to_owned(),
            home: "Core".to_owned(),
            revision: None,
        })
        .expect("edit ancestor");
    let edited_ancestor = session
        .commit(Some("edited ancestor".to_owned()))
        .expect("commit")
        .id;
    assert_eq!(session.tree().nodes.len(), original.nodes.len() + 1);

    let preview = session
        .preview_rebase(&daughter, &edited_ancestor)
        .expect("preview");
    assert_eq!(preview.status, "clean");
    assert!(preview.result.is_some());
    assert_eq!(
        session.tree().nodes.len(),
        original.nodes.len() + 1,
        "preview must not mutate the real graph"
    );

    let applied = session
        .apply_rebase(&daughter, &edited_ancestor)
        .expect("apply");
    assert_eq!(applied.status, "clean");
    let tree = session.tree();
    assert_eq!(tree.nodes.len(), original.nodes.len() + 2);
    assert!(
        tree.nodes.iter().any(|node| node.id == daughter),
        "old chain remains"
    );
    assert_eq!(tree.active, applied.result);
    assert!(session.summary().graph_dirty);
}

#[test]
fn embedded_package_catalog_marks_declared_and_transitive_packages() {
    let temp = Temp::new("package-catalog");
    let session = session(&temp);
    let catalog = session.package_catalog().expect("catalog");

    assert_eq!(catalog.schema, "conlang.ui/v1");
    assert!(!catalog.packages.is_empty());
    assert!(catalog
        .packages
        .iter()
        .all(|item| item.source == "embedded"));
    assert!(catalog
        .packages
        .iter()
        .any(|item| item.id == "std:core" && item.declared && item.selected));
}

#[test]
fn package_changes_validate_before_disk_write_and_reopen_the_session() {
    let temp = Temp::new("package-reopen");
    let mut session = session(&temp);
    let project_path = temp.0.join("project.toml");
    let before = fs::read_to_string(&project_path).expect("before");

    let error = session
        .configure_packages(PackageSelectionInput {
            std: vec!["std:not-installed".to_owned()],
            ..PackageSelectionInput::default()
        })
        .expect_err("unknown package");
    assert!(!error.code.is_empty());
    assert_eq!(
        fs::read_to_string(&project_path).expect("unchanged"),
        before
    );

    let summary = session
        .configure_packages(default_package_input())
        .expect("validated reopen");
    assert!(!summary.legacy);
    assert_eq!(summary.node_count, 2);
    assert!(!summary.graph_dirty);
    assert!(!summary.has_pending);
}

#[test]
fn dirty_project_blocks_package_reconfiguration() {
    let temp = Temp::new("package-dirty");
    let mut session = session(&temp);
    session.begin_edit("ui:test:packages").expect("begin");

    let error = session
        .configure_packages(default_package_input())
        .expect_err("dirty guard");
    assert_eq!(error.code, "APP_DIRTY_PROJECT");
    assert!(session.summary().has_pending);
}

#[test]
fn manual_weights_persist_and_seeded_proposals_only_stage_pending_work() {
    let temp = Temp::new("weights-proposals");
    let mut session = session(&temp);
    let entries = vec![
        SegmentWeight {
            segment: "a".to_owned(),
            weight: 1.0,
        },
        SegmentWeight {
            segment: "k".to_owned(),
            weight: 1.0,
        },
        SegmentWeight {
            segment: "t".to_owned(),
            weight: 0.5,
        },
    ];
    let config = session.set_weights(entries).expect("save weights");
    assert_eq!(config.declaration_source, "project.toml:weights");
    assert!(config
        .effective
        .iter()
        .any(|item| item.segment == "k" && item.source == "manual"));

    drop(session);
    let mut session = UiSession::open(&temp.0, LibrarySpec::default()).expect("reopen");
    assert_eq!(session.weight_config().expect("weights").manual.len(), 3);
    let query = ProposalQuery {
        name: "new_word".to_owned(),
        gloss: Some("TEST".to_owned()),
        categories: vec!["Noun".to_owned()],
        template: "CVC".to_owned(),
        count: 4,
        seed: 42,
        weights: Vec::new(),
    };
    let first = session.propose(&query).expect("propose");
    let second = session.propose(&query).expect("same seed");
    assert_eq!(first, second, "fixed seed must be reproducible");
    let before_nodes = session.tree().nodes.len();
    session.adopt_proposal(&query, 0).expect("adopt");
    assert_eq!(
        session.tree().nodes.len(),
        before_nodes,
        "adopt is not commit"
    );
    assert!(session.summary().has_pending);
    assert!(!session.summary().graph_dirty);
}

#[test]
fn language_state_changes_invalidate_the_proposal_cache() {
    let temp = Temp::new("proposal-cache");
    let mut session = session(&temp);
    let query = ProposalQuery {
        name: "cache_word".to_owned(),
        gloss: None,
        categories: vec!["Noun".to_owned()],
        template: "CVC".to_owned(),
        count: 2,
        seed: 7,
        weights: vec![
            SegmentWeight {
                segment: "a".to_owned(),
                weight: 1.0,
            },
            SegmentWeight {
                segment: "k".to_owned(),
                weight: 1.0,
            },
        ],
    };
    session.propose(&query).expect("propose");
    session
        .set_state(&EvolutionState::default())
        .expect("state write");
    let error = session
        .adopt_proposal(&query, 0)
        .expect_err("state invalidates proposals");
    assert_eq!(error.code, "APP_PROPOSALS_STALE");
}

// ── 空白專案 ─────────────────────────────────────────────────────────────

/// 🔑 **不給來源 `.lang` 也建得起專案**(P28:canonical empty root 永遠存在)。
///
/// 此前 `create` 硬性要一個檔案路徑,於是「新建專案」的第一個必填欄位要求
/// 使用者先有一份檔案——而多數人剛開始造語時什麼都還沒有,只能去手寫一份
/// 23 位元組的佔位檔。那是把實作細節外洩給使用者。
///
/// 判別性:空白專案必須**真的可用**——有 root 節點、開得了編輯、詞典是空的
/// 但查得動。只斷言「create 沒報錯」會漏掉「建出一個死專案」。
#[test]
fn a_project_can_start_from_a_blank_language() {
    let temp = Temp::new("blank");
    let mut slot = conlang_app::ipc::ProjectSlot::default();
    let summary = slot
        .create(
            &temp.0,
            None::<&std::path::Path>,
            Some("空白語".to_owned()),
            "evo:root",
            false,
        )
        .expect("不給來源也該建得起來");
    assert_eq!(summary.name.as_deref(), Some("空白語"));

    let session = slot.session().expect("有 session");
    let tree = session.tree();
    assert_eq!(tree.nodes.len(), 1, "空白專案仍有一個 root 節點");
    assert!(tree.active.is_some(), "而且停在它上面");

    // **真的可用**:詞典查得動,只是空的
    let lexicon = slot
        .session_mut()
        .expect("session")
        .lexicon(&LexiconQuery::default())
        .expect("空專案的詞典查得動");
    assert!(lexicon.lexicon.entries.is_empty());
    assert_eq!(lexicon.lexicon.total_before_filter, 0);

    // 空白語言沒有 `Core` trait，但「開始 working copy」本身只建立一份
    // 零 statement 的合法 `.chg`，不應預先要求任何 sound-change home。
    let pending = slot
        .session_mut()
        .expect("session")
        .begin_edit("ui:test:blank-working-copy")
        .expect("空白專案也能開始 working copy");
    assert_eq!(pending.statements, 0);
    assert!(pending.source.starts_with("changeset "));
    assert!(slot.summary().expect("summary").has_pending);

    // 落盤了——重開仍在
    let reopened = UiSession::open(&temp.0, LibrarySpec::default()).expect("重開");
    assert_eq!(reopened.tree().nodes.len(), 1);
}

/// 給了來源就照用——空白只是**選項**,不是取代。
#[test]
fn supplying_a_source_still_works() {
    let temp = Temp::new("with-source");
    let source = temp.0.join("proto.lang");
    std::fs::create_dir_all(&temp.0).expect("mkdir");
    std::fs::write(&source, SOURCE).expect("write");

    let mut slot = conlang_app::ipc::ProjectSlot::default();
    slot.create(
        temp.0.join("project"),
        Some(&source),
        None,
        "evo:root",
        false,
    )
    .expect("給來源照樣可以");

    let lexicon = slot
        .session_mut()
        .expect("session")
        .lexicon(&LexiconQuery::default())
        .expect("lexicon");
    assert_eq!(lexicon.lexicon.entries.len(), 2, "kat 與 tak 都在");
}
