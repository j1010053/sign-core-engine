//! `clone` 關鍵字:以四原語(單一 `Insert`)實做的授權層糖。
//!
//! 契約:`clone <sign-selector> as <name>` 在 resolve 時被降階為對來源 sign 深拷貝的
//! 一個 `Insert`。因為 `Insert` 會重配 SignId／RuleId／穩定 NodeId,clone 產物是**新實體**
//! (身分與來源完全獨立),來源不動;resolve 後的 ChangeSet 只含四原語,`clone` 不留痕。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, DetachedNode, PrimitiveEdit, ReplayError,
    UnresolvedChangeSet,
};
use conlang_language::{IdentityNamespace, LanguageDocument, LibrarySpec};

const SOURCE: &str = r#"Symbol d
Symbol o
Symbol g
Symbol h
Symbol a
Symbol u
Symbol n

trait LocalNoun:

sign dog:
    belongs LocalNoun
    phon:
        /dog/
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").unwrap()
}

/// 正例 + 內部身分獨立 + 內容保真 + 來源不動 + replay 決定性。
#[test]
fn clone_creates_a_new_sign_with_fresh_identity_and_preserves_source() {
    let base = base();
    let dog_id = base.ref_for_sign("dog").unwrap().id;
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:child").unwrap();
    source.push_str("\n    statement 0:\n        clone sign(\"dog\") as hound\n");

    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let interpreter = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:child").unwrap();
    let first = interpreter.run(&resolved).unwrap();
    let second = interpreter.run(&resolved).unwrap();
    // 跨執行決定性(逐位元相同)。
    assert_eq!(
        first.document.dump_pair().unwrap(),
        second.document.dump_pair().unwrap()
    );

    let document = &first.document;
    // 來源 sign 完全不動(同一穩定 NodeId 仍在)。
    assert_eq!(document.ref_for_sign("dog").unwrap().id, dog_id);
    // clone 是新實體:穩定 NodeId 與來源不同,且來自 active ChangeSet allocator。
    let hound = document.ref_for_sign("hound").expect("clone exists");
    assert_ne!(hound.id, dog_id);
    assert_eq!(
        hound.id.namespace,
        IdentityNamespace::Document("evo:child".to_owned())
    );
    // 內容保真:phon `/dog/` 被逐字複製 → 來源與 clone 各一份,共兩份。
    let rendered = document.source();
    assert!(rendered.contains("sign hound:"));
    assert!(rendered.contains("belongs LocalNoun"));
    assert_eq!(rendered.matches("/dog/").count(), 2);
}

/// 契約:clone 降階為**單一 `Insert`**(四原語),dump 只輸出 `insert sign`,不留 `clone`;
/// 且 dump → parse → resolve round-trip 穩定。
#[test]
fn clone_lowers_to_a_single_insert_primitive_and_leaves_no_trace() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:lower").unwrap();
    source.push_str("\n    statement 0:\n        clone sign(\"dog\") as hound\n");

    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(resolved.statements.len(), 1);
    assert_eq!(resolved.statements[0].edits.len(), 1);
    assert!(matches!(
        resolved.statements[0].edits[0],
        PrimitiveEdit::Insert {
            subtree: DetachedNode::Sign(_),
            ..
        }
    ));

    let dump = resolved.dump();
    assert!(dump.contains("insert sign under node(language, @"));
    assert!(dump.contains("at end:"));
    assert!(dump.contains("sign hound:"));
    assert!(!dump.contains("clone"), "clone 不應留痕於 resolved dump");

    let round = UnresolvedChangeSet::parse(&dump)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    assert_eq!(round.dump(), dump);
}

/// 反例:clone 不存在的 sign → replay 前的 selector 錯,base 未污染。
#[test]
fn clone_of_an_unknown_sign_is_rejected() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:missing").unwrap();
    source.push_str("\n    statement 0:\n        clone sign(\"wolf\") as lupus\n");

    let unresolved = UnresolvedChangeSet::parse(&source).unwrap();
    assert!(matches!(
        unresolved.resolve(&base, &spec),
        Err(ReplayError::Selector(_))
    ));
    assert!(base.ref_for_sign("wolf").is_none());
}

/// 近似反例:clone 成既存名字 → 重複 sign,statement 驗證失敗(降階後的 Insert 撞唯一性)。
#[test]
fn clone_to_an_existing_name_is_rejected_as_duplicate() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:dup").unwrap();
    source.push_str("\n    statement 0:\n        clone sign(\"dog\") as dog\n");

    let unresolved = UnresolvedChangeSet::parse(&source).unwrap();
    let err = unresolved.resolve(&base, &spec).unwrap_err();
    assert!(
        matches!(err, ReplayError::Statement { .. }),
        "expected statement validation error, got {err:?}"
    );
}

/// 多轉移 + 身分獨立:先 clone,再改 clone 的 phon;來源 sign 不受影響。
#[test]
fn editing_a_clone_does_not_affect_the_source() {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, "evo:multi").unwrap();
    source.push_str(concat!(
        "\n    statement 0:\n        clone sign(\"dog\") as hound\n",
        "\n    statement 1:\n        update sign(\"hound\").def[phon].value = /haund/\n",
    ));

    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
        .unwrap();
    let replay = ChangeInterpreter::new(base.clone(), spec.clone(), "evo:multi")
        .unwrap()
        .run(&resolved)
        .unwrap();

    let rendered = replay.document.source();
    // clone 拿到新 phon;來源 dog 仍是 /dog/ — 編輯 clone 不觸及來源。
    assert_eq!(rendered.matches("/haund/").count(), 1);
    assert!(rendered.contains("/dog/"));
    assert!(replay.document.ref_for_sign("dog").is_some());
    assert!(replay.document.ref_for_sign("hound").is_some());
}
