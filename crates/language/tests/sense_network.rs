//! 步驟 15a — **義項(sense)網絡與衍生邊成為 sem 維一級節點**
//! (《修補05》§10.3「sign 內:… sem(senses + 衍生邊)」;docs/07 §5)。
//!
//! 此前多義是**土法**:用自創欄位名(`sense2 = LOGBOOK`)假裝第二義項,義項沒有身分、
//! 沒有衍生邊、沒有 transparency,`SemNode` 的 `senses`/`edges` 只是註解裡的預留位。
//! 本檔釘住補實後的行為,並保證 LLM 交換界面(`conlang.semantic/v1`)相容。

use conlang_language::patch::Patch;
use conlang_language::sem::SemNode;
use conlang_language::semantic_dto::SemanticDocumentV1;
use conlang_language::{
    check_language, DerivationKind, Dim, Language, SenseTransparency, SignDef, SignItem,
};

const SRC: &str = r#"Symbol b
Symbol o

trait Noun:

sign book:
    belongs Noun
    phon:
        /bo/
    sem:
        senses:
            core = BOOK
            log = LOGBOOK
        edges:
            log from core metonymy
"#;

fn parse(src: &str) -> Language {
    Language::parse(src).expect("parse")
}

fn sign<'a>(l: &'a Language, name: &str) -> &'a SignDef {
    l.signs.iter().find(|s| s.name == name).expect("sign")
}

// ── AST / parse ───────────────────────────────────────────────────────────

#[test]
fn senses_and_edges_parse_as_first_class_items() {
    let l = parse(SRC);
    let items = &sign(&l, "book").items;
    let senses = items
        .iter()
        .filter_map(|i| match i {
            SignItem::Sense(s) => Some(s),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(senses.len(), 2, "two senses have identity of their own");
    assert_eq!(senses[0].name, "core");
    assert_eq!(senses[0].gloss, "BOOK");
    assert_eq!(senses[1].name, "log");

    let edges = items
        .iter()
        .filter_map(|i| match i {
            SignItem::SenseEdge(e) => Some(e),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to, "log");
    assert_eq!(edges[0].from, "core");
    assert_eq!(edges[0].kind, DerivationKind::Metonymy);
    assert_eq!(
        edges[0].transparency,
        SenseTransparency::Transparent,
        "default is transparent (not yet lexicalized)"
    );
}

#[test]
fn an_opaque_edge_records_lexicalized_derivation() {
    let src = SRC.replace("log from core metonymy", "log from core metonymy opaque");
    let l = parse(&src);
    let edge = sign(&l, "book")
        .items
        .iter()
        .find_map(|i| match i {
            SignItem::SenseEdge(e) => Some(e),
            _ => None,
        })
        .expect("edge");
    assert_eq!(edge.transparency, SenseTransparency::Opaque);
}

#[test]
fn senses_and_edges_round_trip_through_the_printer() {
    // Regression guard: a `_ =>` fallback in the printer would silently drop
    // these items and lose data on every dump.
    let l = parse(SRC);
    let dumped = l.dump();
    assert!(
        dumped.contains("senses:"),
        "senses block printed:\n{dumped}"
    );
    assert!(dumped.contains("core = BOOK"), "{dumped}");
    assert!(
        dumped.contains("log from core metonymy"),
        "edge printed:\n{dumped}"
    );
    // **保存**(不只是不動點):re-parse 後義項/邊的數量必須一樣。只斷言
    // `parse(dump).dump() == dump` 是不夠的——若 printer 整段丟掉,兩邊會
    // 「一致地」都缺,測試照樣綠(這正是本檔一度漏掉的假綠燈)。
    assert_eq!(
        count_senses(&Language::parse(&dumped).unwrap(), "book"),
        count_senses(&l, "book"),
        "re-parsing the dump preserves senses and edges"
    );
    assert_eq!(
        Language::parse(&dumped).unwrap().dump(),
        dumped,
        "round-trip is a fixed point"
    );
}

fn count_senses(l: &Language, sign_name: &str) -> (usize, usize) {
    let items = &sign(l, sign_name).items;
    (
        items
            .iter()
            .filter(|i| matches!(i, SignItem::Sense(_)))
            .count(),
        items
            .iter()
            .filter(|i| matches!(i, SignItem::SenseEdge(_)))
            .count(),
    )
}

/// 迴歸:`sem:` 區塊**只有**義項/衍生邊(沒有任何純量欄位)時,維度區塊的輸出閘
/// 曾經整段跳過,導致 dump 掉光義項——而「不動點」測試仍是綠的。
#[test]
fn a_sem_block_containing_only_senses_still_prints() {
    let src = "Symbol b\n\ntrait Noun:\n\nsign book:\n    belongs Noun\n    sem:\n        senses:\n            core = BOOK\n";
    let l = parse(src);
    assert_eq!(count_senses(&l, "book"), (1, 0));
    let dumped = l.dump();
    assert!(
        dumped.contains("sem:") && dumped.contains("core = BOOK"),
        "a senses-only sem block must still be emitted:\n{dumped}"
    );
    assert_eq!(
        count_senses(&Language::parse(&dumped).unwrap(), "book"),
        (1, 0),
        "and survive a re-parse"
    );
}

/// P71 §4.1:主義項是 `core`,**不是宣告序首項**。
///
/// 繼承會把祖先的義項排在本地之前(12a:遠祖→近祖、本地最後),因此「取首項」
/// 會讓一個繼承來的義項冒充主義項。此測試即該分歧點:`archaic` 在前、`core` 在後。
#[test]
fn the_primary_sense_is_core_not_merely_the_first() {
    let src = "trait Ancestral:\n    sem:\n        senses:\n            archaic = OLD\n\n\
               sign heir:\n    belongs Ancestral\n    phon:\n        /bo/\n    \
               sem:\n        senses:\n            core = CURRENT\n";
    let l = parse(src);
    let (registry, _) = conlang_language::ontology::OntologyRegistry::build(&[&l]);
    let node = SemNode::of_sign(sign(&l, "heir"), &registry);

    // 前提:繼承來的義項確實排在本地 `core` 之前(否則本測試不具判別性)
    assert_eq!(
        node.senses
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["archaic", "core"],
        "繼承義項在前,本地在後"
    );
    assert_eq!(node.primary_sense().map(|s| s.name.as_str()), Some("core"));
    assert_eq!(node.field("gloss"), Some("CURRENT"), "不得取到 OLD");
}

/// P71 §4.1 單一資訊源:縱使有 `sem.gloss` Def 混入,**義項仍是唯一真相**。
///
/// `.lang` 作者在 §4.2/§7 A1 之後已寫不出這條路徑,但 `Patch` 是 Rust API、
/// 不經來源驗證,仍寫得出來——這正是 `fields` 必須排除 `gloss` 的理由。
/// 首個斷言是**正向控制組**:先證 Def 真的寫進去了,否則本測試會空轉而恆綠。
#[test]
fn a_stray_sem_gloss_def_never_shadows_the_sense() {
    let l = parse(SRC);
    let (registry, _) = conlang_language::ontology::OntologyRegistry::build(&[&l]);
    let patched = Patch::sem().set("gloss", "STRAY").apply(sign(&l, "book"));
    assert_eq!(
        patched.project(Dim::Sem, &registry).get("sem.gloss"),
        Some("STRAY"),
        "前提:Def 確實寫進去了"
    );

    let node = SemNode::of_sign(&patched, &registry);
    assert_eq!(node.field("gloss"), Some("BOOK"), "義項優先,Def 不遮蔽");
    assert!(
        !node.fields.iter().any(|(k, _)| k == "gloss"),
        "gloss 不得混進純量欄位:{:?}",
        node.fields
    );
}

#[test]
fn senses_are_only_valid_under_sem() {
    let bad = SRC.replace("    sem:\n        senses:\n", "    syn:\n        senses:\n");
    let err = Language::parse(&bad).expect_err("senses under syn: must be rejected");
    assert!(
        format!("{err}").contains("senses:"),
        "expected a located `senses:` error, got {err}"
    );
}

#[test]
fn a_malformed_edge_is_rejected_not_ignored() {
    for (bad, why) in [
        ("log core metonymy", "missing `from`"),
        ("log from core sideways", "unknown kind"),
        ("log from core metonymy fuzzy", "unknown modifier"),
    ] {
        let src = SRC.replace("log from core metonymy", bad);
        assert!(
            Language::parse(&src).is_err(),
            "expected rejection for {why}: {bad:?}"
        );
    }
}

// ── validation ────────────────────────────────────────────────────────────

#[test]
fn an_edge_referring_to_an_undeclared_sense_is_diagnosed() {
    // Without this, `lexicalize_sense`/`derive_sense` (step 15b) would act on a
    // ghost node.
    let src = SRC.replace("log from core metonymy", "log from ghost metonymy");
    let report = check_language(&parse(&src));
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|d| d.code == "SENSE_EDGE_UNKNOWN"),
        "expected SENSE_EDGE_UNKNOWN, got {:?}",
        report.diagnostics()
    );
}

#[test]
fn duplicate_sense_names_and_self_derivation_are_diagnosed() {
    let dup = SRC.replace("            log = LOGBOOK", "            core = AGAIN");
    assert!(check_language(&parse(&dup))
        .diagnostics()
        .iter()
        .any(|d| d.code == "SENSE_DUPLICATE"));

    let looped = SRC.replace("log from core metonymy", "log from log metonymy");
    assert!(check_language(&parse(&looped))
        .diagnostics()
        .iter()
        .any(|d| d.code == "SENSE_EDGE_SELF"));
}

#[test]
fn a_well_formed_sense_network_has_no_diagnostics() {
    // Near-miss control for the three tests above.
    let report = check_language(&parse(SRC));
    assert!(
        !report
            .diagnostics()
            .iter()
            .any(|d| d.code.starts_with("SENSE_")),
        "clean network should be silent, got {:?}",
        report.diagnostics()
    );
}

// ── projection(SemNode 預留位填實) ────────────────────────────────────────

#[test]
fn sem_node_projection_exposes_senses_and_edges() {
    let l = parse(SRC);
    let (registry, _) = conlang_language::ontology::OntologyRegistry::build(&[&l]);
    let node: SemNode = SemNode::of_sign(sign(&l, "book"), &registry);
    assert_eq!(node.senses.len(), 2, "reserved slot is now populated");
    assert_eq!(node.senses[1].name, "log");
    assert_eq!(node.edges.len(), 1);
    assert_eq!(node.edges[0].kind, DerivationKind::Metonymy);
}

// ── LLM 交換界面(補缺 + v1 相容) ─────────────────────────────────────────

#[test]
fn the_semantic_dto_now_carries_fields_senses_and_edges() {
    // 缺口回歸:`fields` 此前**不出境**(from_sem_node 不讀、into_sem_node 給空)。
    let l = parse(SRC);
    let (registry, _) = conlang_language::ontology::OntologyRegistry::build(&[&l]);
    let mut node = SemNode::of_sign(sign(&l, "book"), &registry);
    // P71 §4.1 後 `gloss` 住 `senses`,已不是純量欄位——純量欄位改由清單上的路徑
    // (如套件座標)承載。此處直接注入一個,單獨釘住 DTO 的出境行為本身。
    assert!(
        !node.fields.iter().any(|(k, _)| k == "gloss"),
        "gloss 不再是純量欄位:{:?}",
        node.fields
    );
    node.fields
        .push(("aspect.perfectivity".to_owned(), "1".to_owned()));
    let document = SemanticDocumentV1::from_sem_node(&node);
    assert_eq!(
        document
            .root
            .fields
            .get("aspect.perfectivity")
            .map(String::as_str),
        Some("1"),
        "scalar fields now cross the boundary: {:?}",
        document.root.fields
    );
    assert_eq!(document.root.senses.len(), 2);
    assert_eq!(document.root.edges.len(), 1);
    assert_eq!(document.root.edges[0].kind, "metonymy");
    assert_eq!(document.root.edges[0].transparency, "transparent");

    // 往返:DTO → SemNode → DTO 不掉資料。
    let back = SemanticDocumentV1::from_sem_node(&document.root.clone().into_sem_node());
    assert_eq!(back.root.senses, document.root.senses);
    assert_eq!(back.root.edges, document.root.edges);
    assert_eq!(back.root.fields, document.root.fields);
}

#[test]
fn a_v1_document_without_the_new_keys_still_loads() {
    // 相容性:舊文件沒有 fields/senses/edges 三個鍵,必須照樣讀得進來。
    let json = r#"{"schema":"conlang.semantic/v1","root":{"source":{"sign":"x"},"types":[],"features":{},"roles":{}}}"#;
    let document = SemanticDocumentV1::from_json(json).expect("v1 document still parses");
    assert!(document.root.senses.is_empty());
    assert!(document.root.edges.is_empty());
    assert!(document.root.fields.is_empty());
}
