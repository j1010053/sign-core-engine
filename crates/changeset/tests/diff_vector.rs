//! 步驟 16 ③ —— **分層差異向量**(docs/06 §6.1)。
//!
//! 驗三件事:
//!
//! 1. **分量真的分開**——改 phon 只動 phon 分量。用「總數」或「有沒有差異」測不出來,
//!    那正是規格說「差異**不是單一數字**」的原因。
//! 2. **以 `SignId` 對齊**,不是名字——改名不算生滅,是同一個 sign 的兩個階段。
//! 3. **生滅**:無對應者計為生/滅(§6.1)。

use conlang_changeset::diff::diff_vector;
use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{Dim, LanguageDocument, LibrarySpec};

const ROOT: &str = "Symbol a\nSymbol b\n\n\
                    trait LocalNoun:\n\n\
                    sign x:\n\
                    \x20   belongs LocalNoun\n\
                    \x20   phon:\n\
                    \x20       /a/\n\
                    \x20   syn:\n\
                    \x20       feature:\n\
                    \x20           category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n\
                    \x20           category = noun\n\
                    \x20   sem:\n\
                    \x20       senses:\n\
                    \x20           core = THING\n\n\
                    sign y:\n\
                    \x20   syn:\n\
                    \x20       feature:\n\
                    \x20           category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n\
                    \x20           category = noun\n";

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(ROOT, "evo:root").expect("root parses")
}

fn apply(base: &LanguageDocument, namespace: &str, body: &str) -> LanguageDocument {
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

fn statement(body: &str) -> String {
    format!("\n    #0:\n        {body}\n")
}

// ── 分量分得開嗎(規格的核心主張)────────────────────────────────────────────

#[test]
fn a_syn_change_moves_only_the_syn_component() {
    // **這是「差異不是單一數字」的實證**。只驗「有沒有差異」或「差幾個 sign」都
    // 分不出改的是哪一維——而規格說分層向量正是為此存在。
    let before = base();
    let after = apply(
        &before,
        "evo:n1",
        &statement("update sign(\"x\").feature[syn.category].value = verb"),
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(vector.syn, 1, "syn 動了");
    assert_eq!(vector.phon, 0, "phon 不該動");
    assert_eq!(vector.sem, 0, "sem 不該動");
    assert_eq!(vector.prag, 0, "prag 不該動");
    assert_eq!(vector.structural, 0);
    assert_eq!((vector.born, vector.died), (0, 0), "沒有生滅");
    assert_eq!(vector.aligned, 2, "兩個 sign 都對得上");
}

#[test]
fn a_sem_change_moves_only_the_sem_component() {
    let before = base();
    let after = apply(
        &before,
        "evo:n1",
        &statement("update sign(\"x\").sense[\"core\"].gloss = OBJECT"),
    );
    let vector = diff_vector(&before, &after);
    assert_eq!(vector.sem, 1);
    assert_eq!((vector.phon, vector.syn, vector.prag), (0, 0, 0));
}

#[test]
fn the_dimension_accessor_agrees_with_the_fields() {
    let before = base();
    let after = apply(
        &before,
        "evo:n1",
        &statement("update sign(\"x\").feature[syn.category].value = verb"),
    );
    let vector = diff_vector(&before, &after);
    assert_eq!(vector.dimension(Dim::Syn), vector.syn);
    assert_eq!(vector.dimension(Dim::Phon), vector.phon);
    assert_eq!(vector.dimension(Dim::Sem), vector.sem);
    assert_eq!(vector.dimension(Dim::Prag), vector.prag);
}

// ── 生滅(§6.1「無對應者計為生/滅」)────────────────────────────────────────

#[test]
fn a_new_sign_counts_as_a_birth() {
    let before = base();
    let after = apply(&before, "evo:n1", &statement("clone sign(\"x\") as z"));
    let vector = diff_vector(&before, &after);
    assert_eq!(vector.born, 1);
    assert_eq!(vector.died, 0);
    assert_eq!(vector.aligned, 2, "原本兩個仍對得上");
    assert_eq!(
        (vector.phon, vector.syn, vector.sem, vector.prag),
        (0, 0, 0, 0),
        "新增不該讓既有 sign 的任何維動起來"
    );
}

#[test]
fn a_deleted_sign_counts_as_a_death() {
    let before = base();
    let after = apply(&before, "evo:n1", &statement("delete sign(\"y\")"));
    let vector = diff_vector(&before, &after);
    assert_eq!(vector.died, 1);
    assert_eq!(vector.born, 0);
    assert_eq!(vector.aligned, 1);
}

#[test]
fn generation_and_loss_are_mirrored_when_the_arguments_swap() {
    // §6.2【M】的對稱版互通度建立在此:交換兩邊,生滅互換、各維不變。
    let before = base();
    let after = apply(&before, "evo:n1", &statement("delete sign(\"y\")"));
    let forward = diff_vector(&before, &after);
    let backward = diff_vector(&after, &before);
    assert_eq!(forward.died, backward.born);
    assert_eq!(forward.born, backward.died);
    assert_eq!(forward.syn, backward.syn);
    assert_eq!(forward.aligned, backward.aligned);
}

// ── 對齊鍵是 SignId,不是名字(§6.1)──────────────────────────────────────────

#[test]
fn renaming_a_sign_is_not_a_birth_and_death() {
    // **判別性**:用名字對齊會把改名算成「死一個、生一個」;用 `SignId` 對齊才知道
    // 那是同一個 sign 的兩個階段。合併(P61)與 `LanguageDiff` 用的是同一把鑰匙。
    let before = base();
    let after = apply(
        &before,
        "evo:n1",
        &statement("update sign(\"y\").name = ynew"),
    );
    assert!(
        after.source().contains("sign ynew:"),
        "前提:名字真的改了:{}",
        after.source()
    );
    let vector = diff_vector(&before, &after);
    assert_eq!((vector.born, vector.died), (0, 0), "改名不是生滅");
    assert_eq!(vector.aligned, 2);
}

// ── 邊界 ──────────────────────────────────────────────────────────────────

#[test]
fn a_document_does_not_differ_from_itself() {
    let document = base();
    let vector = diff_vector(&document, &document);
    assert!(vector.is_identical(), "{vector:?}");
    assert_eq!(vector.aligned, 2);
}

#[test]
fn unrelated_documents_share_nothing() {
    // 不同命名空間 ⇒ 沒有一個 id 對得上 ⇒ 全部計為生滅,沒有任何維分量。
    let left = base();
    let right = LanguageDocument::import_new_root(
        "sign only:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:r",
    )
    .unwrap();
    let vector = diff_vector(&left, &right);
    assert_eq!(vector.aligned, 0);
    assert_eq!(vector.died, 2);
    assert_eq!(vector.born, 1);
    assert_eq!(
        (vector.phon, vector.syn, vector.sem, vector.prag),
        (0, 0, 0, 0)
    );
}

#[test]
fn a_cross_dimension_item_moves_the_structural_component() {
    // `belongs` 不屬於任何一維——它**同時**影響 syn 與 sem 的繼承。硬歸給某一維會讓
    // 那一維的數字說謊,故獨立成一個分量。
    //
    // 沒有這個測試,`structural` 整個分量與 `item_dimension` 的 `None` 分支都是
    // 改成什麼樣都不會紅的死碼。
    let before = base();
    let after = apply(
        &before,
        "evo:n1",
        "\n    #0:\n        insert into sign(\"y\") at end:\n            belongs LocalNoun\n",
    );
    let vector = diff_vector(&before, &after);
    assert_eq!(vector.structural, 1, "跨維項目要落在 structural");
    assert_eq!(
        (vector.phon, vector.syn, vector.sem, vector.prag),
        (0, 0, 0, 0),
        "不得被算進任何一維"
    );
    assert_eq!((vector.born, vector.died), (0, 0));
}
