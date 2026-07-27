//! 步驟 15a 出口驗證:**`.chg` 經四原語編輯義項/衍生邊,產出 `.lang`**。
//!
//! 15a 把 sense/edge 補成 sem 維一級節點;本檔證明它們在**真正的公開入口**
//! (`.chg` parse → resolve → replay)上被四原語完整支援——不是只有 Rust API 能動。
//! 這是步驟 15b(Atomic Rewrite `derive_sense`/`drift`/`lexicalize_sense`)的前提:
//! 那三項就是降階成這裡的 insert/update 原語。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, ReplayError, ResolvedChangeSet, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol b
Symbol o

trait LocalNoun:

sign book:
    belongs LocalNoun
    phon:
        /bo/
    sem:
        senses:
            core = BOOK
            log = LOGBOOK
        edges:
            log from core metonymy
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

/// 跑完一份 `.chg`,回傳產出的 `.lang` 原文。
fn apply(chg: &str, ns: &str) -> String {
    let base = base();
    let spec = LibrarySpec::default();
    let resolved = resolve(chg, ns).expect("resolve");
    ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .expect("replay")
        .document
        .source()
        .to_owned()
}

// ── ① insert ──────────────────────────────────────────────────────────────

#[test]
fn insert_adds_a_sense_and_a_derivation_edge() {
    // `derive_sense{kind: metaphor}` 未來就降階成這兩個 insert。
    let lang = apply(
        concat!(
            "\n    statement 0:\n        insert into sign(\"book\") at end:\n",
            "            sem:\n                senses:\n                    fig = FIGURATIVE\n",
            "\n    statement 1:\n        insert into sign(\"book\") at end:\n",
            "            sem:\n                edges:\n                    fig from core metaphor\n",
        ),
        "evo:ins",
    );
    assert!(
        lang.contains("fig = FIGURATIVE"),
        "new sense in the .lang:\n{lang}"
    );
    assert!(
        lang.contains("fig from core metaphor"),
        "new edge in the .lang:\n{lang}"
    );
    // 既有義項/邊不受影響。
    assert!(lang.contains("core = BOOK") && lang.contains("log from core metonymy"));
    // 產出的 .lang 必須自己讀得回去(不是只會印字串)。
    conlang_language::Language::parse(&lang).expect("generated .lang re-parses");
}

// ── ② update ──────────────────────────────────────────────────────────────

#[test]
fn update_rewrites_a_sense_gloss() {
    // `drift`(語意漂移)的落點:改某個義項的內容,其餘不動。
    let lang = apply(
        "\n    statement 0:\n        update sign(\"book\").sense[\"log\"].gloss = JOURNAL\n",
        "evo:drift",
    );
    assert!(lang.contains("log = JOURNAL"), "sense drifted:\n{lang}");
    assert!(!lang.contains("LOGBOOK"), "old gloss gone:\n{lang}");
    assert!(lang.contains("core = BOOK"), "sibling untouched:\n{lang}");
}

#[test]
fn update_flips_an_edge_to_opaque() {
    // `lexicalize_sense` 的落點:衍生邊固化,語源不再透明。
    let lang = apply(
        "\n    statement 0:\n        update sign(\"book\").edge[0].transparency = opaque\n",
        "evo:lex",
    );
    assert!(
        lang.contains("log from core metonymy opaque"),
        "edge lexicalized:\n{lang}"
    );
}

#[test]
fn update_can_retype_a_derivation_edge() {
    let lang = apply(
        "\n    statement 0:\n        update sign(\"book\").edge[0].kind = metaphor\n",
        "evo:kind",
    );
    assert!(lang.contains("log from core metaphor"), "{lang}");
    assert!(!lang.contains("metonymy"), "{lang}");
}

// ── ③ delete ──────────────────────────────────────────────────────────────

/// 刪除**仍被衍生邊引用**的義項必須被擋下——四原語不得繞過參照完整性,
/// 否則會留下指向幽靈義項的邊。
#[test]
fn deleting_a_sense_that_an_edge_still_references_is_rejected() {
    let err = resolve(
        "\n    statement 0:\n        delete sign(\"book\").sense[\"log\"]\n",
        "evo:delref",
    )
    .unwrap_err();
    assert!(
        format!("{err:?}").contains("SENSE_EDGE_UNKNOWN"),
        "expected referential integrity to reject it, got {err:?}"
    );
}

#[test]
fn delete_removes_a_sense_once_its_edge_is_gone() {
    // 先刪邊、再刪義項:一份 `.chg` 兩個 statement 走完。
    let lang = apply(
        concat!(
            "\n    statement 0:\n        delete sign(\"book\").edge[0]\n",
            "\n    statement 1:\n        delete sign(\"book\").sense[\"log\"]\n",
        ),
        "evo:del",
    );
    assert!(!lang.contains("log = LOGBOOK"), "sense removed:\n{lang}");
    assert!(!lang.contains("from core"), "edge removed:\n{lang}");
    assert!(lang.contains("core = BOOK"), "sibling survives:\n{lang}");
    conlang_language::Language::parse(&lang).expect("generated .lang re-parses");
}

// ── ④ move ────────────────────────────────────────────────────────────────

#[test]
fn move_reorders_senses() {
    let lang = apply(
        "\n    statement 0:\n        move sign(\"book\").sense[\"log\"] to sign(\"book\") at start\n",
        "evo:move",
    );
    assert!(
        lang.find("log = LOGBOOK").unwrap() < lang.find("core = BOOK").unwrap(),
        "log moved ahead of core:\n{lang}"
    );
}

// ── 不變式 ────────────────────────────────────────────────────────────────

#[test]
fn a_sense_changeset_round_trips_and_is_deterministic() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:rt").unwrap();
    source.push_str(
        "\n    statement 0:\n        update sign(\"book\").sense[\"log\"].gloss = JOURNAL\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let dump = resolved.dump();
    assert!(dump.contains("gloss = JOURNAL"), "field in dump:\n{dump}");
    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump, "dump→parse→resolve 逐位元穩定");

    let once = apply(
        "\n    statement 0:\n        update sign(\"book\").sense[\"log\"].gloss = JOURNAL\n",
        "evo:det",
    );
    let twice = apply(
        "\n    statement 0:\n        update sign(\"book\").sense[\"log\"].gloss = JOURNAL\n",
        "evo:det",
    );
    assert_eq!(once, twice, "replay 決定性");
}

/// 多步交易:insert 義項 → insert 邊 → 固化該邊,一份 `.chg` 走完,結果 `.lang` 正確。
#[test]
fn a_multi_statement_changeset_builds_a_derived_sense_end_to_end() {
    let lang = apply(
        concat!(
            "\n    statement 0:\n        insert into sign(\"book\") at end:\n",
            "            sem:\n                senses:\n                    fig = FIGURATIVE\n",
            "\n    statement 1:\n        insert into sign(\"book\") at end:\n",
            "            sem:\n                edges:\n                    fig from core metaphor\n",
            "\n    statement 2:\n        update sign(\"book\").edge[1].transparency = opaque\n",
        ),
        "evo:chain",
    );
    assert!(lang.contains("fig = FIGURATIVE"), "{lang}");
    assert!(
        lang.contains("fig from core metaphor opaque"),
        "derived sense then lexicalized:\n{lang}"
    );
    assert!(
        lang.contains("log from core metonymy"),
        "the untouched edge kept its transparency:\n{lang}"
    );
    conlang_language::Language::parse(&lang).expect("generated .lang re-parses");
}

// ── near-miss 負例 ────────────────────────────────────────────────────────

#[test]
fn editing_an_unknown_sense_is_rejected() {
    let err = resolve(
        "\n    statement 0:\n        update sign(\"book\").sense[\"ghost\"].gloss = X\n",
        "evo:ghost",
    )
    .unwrap_err();
    assert!(
        format!("{err}").contains("cannot resolve"),
        "expected addressing failure, got {err}"
    );
}

#[test]
fn an_invalid_edge_kind_is_rejected() {
    let err = resolve(
        "\n    statement 0:\n        update sign(\"book\").edge[0].kind = sideways\n",
        "evo:badkind",
    )
    .unwrap_err();
    assert!(
        format!("{err}").contains("metaphor"),
        "expected the enumerated-kind error, got {err}"
    );
}

#[test]
fn an_edge_inserted_against_an_unknown_sense_fails_validation() {
    // 四原語不得繞過語意驗證:邊指向不存在的義項 → 整條 statement 失敗。
    let err = resolve(
        concat!(
            "\n    statement 0:\n        insert into sign(\"book\") at end:\n",
            "            sem:\n                edges:\n                    ghost from core metaphor\n",
        ),
        "evo:badedge",
    )
    .unwrap_err();
    assert!(
        format!("{err:?}").contains("SENSE_EDGE_UNKNOWN")
            || format!("{err}").contains("VALIDATION"),
        "expected the sense-edge validation to reject it, got {err:?}"
    );
}
