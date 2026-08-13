//! `.chg` 重建 Standard English 語法的出口。
//!
//! # 為什麼 prelude 與 statements 分開比
//!
//! 這份 golden 混了兩種東西:
//!
//! | | 內容 | 性質 |
//! |---|---|---|
//! | **statements** | `reconstruct` 產出的 85 條原語 | **真正的 golden**,值得逐字元比 |
//! | **prelude** | base source / identity / 四個套件的 lock digest | **衍生值**,隨套件內容漂移 |
//!
//! 先前一條 `assert_eq!(dumped, RESTORE)` 把兩者綁在一起,於是套件內容一變,
//! 2 行的 digest 差異就被呈現成 50 KB 的字串 diff——沒人看得出重點,結果
//! 2026-08-07 有人直接手改了那兩行,把引擎算得出來的值改成算不出來的值,
//! 那份教學材料因此變成 resolve 不過的死檔。
//!
//! 現在:statements 逐字元比;prelude 交給 `resolve` 自己驗
//! (`LibraryLockMismatch` 一眼看得出是哪個套件)。
//!
//! **digest 不得手改**,只能由
//! `cargo run -p conlang-changeset --example bless_en_standard_restore` 重生。

use conlang_changeset::reconstruct::reconstruct;
use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, ReplayError, ResolvedStatement, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};

const BASE: &str = include_str!("../../../tutorials/en-standard-reconstruction/base.lang");
const RESTORE: &str = include_str!("../../../tutorials/en-standard-reconstruction/restore.chg");
const TARGET: &str = include_str!("../../language/lib/natural/en-standard/code/grammar.lang");

/// prelude 之後的部分(第一個 `#` 語句標記起)。與 bless 工具同一個切法。
fn statements_of(text: &str) -> &str {
    text.find("\n    #").map(|at| &text[at..]).unwrap_or("")
}

fn documents() -> (LanguageDocument, LanguageDocument) {
    let base = LanguageDocument::import_new_root(BASE, "evo:en-standard").expect("base parses");
    let target =
        LanguageDocument::import_new_root(TARGET, "evo:en-standard").expect("target parses");
    (base, target)
}

#[test]
fn reconstructs_the_english_grammar_through_a_dumped_changeset() {
    let (base, target) = documents();
    let base_identities = base.identities().clone();
    let edits = reconstruct(&base, &target).expect("English grammar reconstructs");
    assert!(!edits.is_empty());

    let libraries = LibrarySpec::default();
    let prelude =
        change_set_prelude(&base, &libraries, "evo:en-standard-restore").expect("prelude");
    let mut resolved = UnresolvedChangeSet::parse(&prelude)
        .expect("prelude parses")
        .resolve(&base, &libraries)
        .expect("prelude resolves");
    resolved.statements = vec![ResolvedStatement { ordinal: 0, edits }];

    let dumped = resolved.dump().expect("dump");

    // ── 兩段分開比,理由見檔頭 ─────────────────────────────────────────
    //
    // ① **statements 逐字元比**:那是 `reconstruct` 的 golden,不隨套件內容漂移。
    assert_eq!(
        statements_of(&dumped),
        statements_of(RESTORE),
        "reconstruct 的產物與簽入的 .chg 不符。\
         這是**真的迴歸**(不是 digest 漂移),先確認 reconstruct 改了什麼;\
         確認無誤才重生:cargo run -p conlang-changeset --example bless_en_standard_restore"
    );

    // ② **prelude 不比字面**:三道 digest 是衍生值,由 `resolve` 自己驗
    //    ——不符時得到 `LibraryLockMismatch` 之類的精確錯誤,而不是 50 KB 的
    //    字串 diff。先前把兩者混在一條 `assert_eq!` 裡,結果 2 行的 digest 差異
    //    被呈現成整份檔案的 diff,於是有人直接手改了那兩行了事(2026-08-07)。
    let reparsed = UnresolvedChangeSet::parse(RESTORE)
        .expect("dumped .chg parses")
        .resolve(&base, &libraries)
        .unwrap_or_else(|error| {
            panic!(
                "簽入的 .chg 對不上現行環境:{error}\n\
                 若 std 套件內容確實變了,重生 golden:\n    \
                 cargo run -p conlang-changeset --example bless_en_standard_restore"
            )
        });
    assert_eq!(reparsed.statements[0].edits.len(), 85);
    assert_eq!(
        reparsed.statements[0]
            .edits
            .iter()
            .filter(|edit| matches!(
                edit,
                conlang_changeset::PrimitiveEdit::Insert {
                    subtree: conlang_changeset::DetachedNode::DslDeclaration(_),
                    ..
                }
            ))
            .count(),
        18,
        "17 Symbol declarations and one Class are persisted in .chg"
    );
    let replayed =
        ChangeInterpreter::new(base.clone(), libraries.clone(), "evo:en-standard-restore")
            .expect("interpreter")
            .run(&reparsed.clone())
            .expect("dumped .chg replays")
            .document;
    let replayed_again = ChangeInterpreter::new(base, libraries, "evo:en-standard-restore")
        .expect("second interpreter")
        .run(&reparsed)
        .expect("dumped .chg replays deterministically")
        .document;

    assert_eq!(replayed.source(), target.source());
    assert_eq!(
        replayed.source(),
        replayed_again.source(),
        "replay source is deterministic"
    );
    assert_eq!(
        replayed.identities(),
        replayed_again.identities(),
        "replay identity allocation is deterministic"
    );
    for original in &base_identities.nodes {
        assert!(
            replayed
                .identities()
                .nodes
                .iter()
                .any(|entry| entry.id == original.id && entry.kind == original.kind),
            "base identity {} survives reconstruction",
            original.id
        );
    }
    assert_eq!(
        replayed.identities().active_namespace.to_string(),
        "evo:en-standard-restore",
        "new grammar nodes retain ChangeSet provenance"
    );
    assert_eq!(reparsed.dump().expect("dump"), dumped, ".chg dump is canonical");
}

#[test]
fn restore_rejects_a_different_base_before_replay() {
    let altered =
        LanguageDocument::import_new_root(&format!("{BASE}\nSymbol z\n"), "evo:en-standard")
            .expect("altered base parses");
    let error = UnresolvedChangeSet::parse(RESTORE)
        .expect("restore parses")
        .resolve(&altered, &LibrarySpec::default())
        .expect_err("digest mismatch must reject another base");
    assert!(matches!(error, ReplayError::BaseSourceMismatch));
}
