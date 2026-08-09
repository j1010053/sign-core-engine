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

    assert_eq!(vector.syn.signs.changed, 1, "syn 動了");
    assert_eq!(vector.phon.signs.changed, 0, "phon 不該動");
    assert_eq!(vector.sem.signs.changed, 0, "sem 不該動");
    assert_eq!(vector.prag.signs.changed, 0, "prag 不該動");
    assert_eq!(vector.structural.signs.changed, 0);
    assert_eq!((vector.born_signs(), vector.died_signs()), (0, 0), "沒有生滅");
    assert_eq!(vector.aligned_signs(), 2, "兩個 sign 都對得上");
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
    assert_eq!(vector.sem.signs.changed, 1);
    assert_eq!(
        (
            vector.phon.signs.changed,
            vector.syn.signs.changed,
            vector.prag.signs.changed
        ),
        (0, 0, 0)
    );
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
    assert_eq!(vector.dimension(Dim::Syn).signs.changed, vector.syn.signs.changed);
    assert_eq!(vector.dimension(Dim::Phon).signs.changed, vector.phon.signs.changed);
    assert_eq!(vector.dimension(Dim::Sem).signs.changed, vector.sem.signs.changed);
    assert_eq!(vector.dimension(Dim::Prag).signs.changed, vector.prag.signs.changed);
}

// ── 生滅(§6.1「無對應者計為生/滅」)────────────────────────────────────────

#[test]
fn a_new_sign_counts_as_a_birth() {
    let before = base();
    let after = apply(&before, "evo:n1", &statement("clone sign(\"x\") as z"));
    let vector = diff_vector(&before, &after);
    assert_eq!(vector.born_signs(), 1);
    assert_eq!(vector.died_signs(), 0);
    assert_eq!(vector.aligned_signs(), 2, "原本兩個仍對得上");
    assert_eq!(
        (
            vector.phon.signs.changed,
            vector.syn.signs.changed,
            vector.sem.signs.changed,
            vector.prag.signs.changed
        ),
        (0, 0, 0, 0),
        "新增不該讓既有 sign 的任何維動起來"
    );
}

#[test]
fn a_deleted_sign_counts_as_a_death() {
    let before = base();
    let after = apply(&before, "evo:n1", &statement("delete sign(\"y\")"));
    let vector = diff_vector(&before, &after);
    assert_eq!(vector.died_signs(), 1);
    assert_eq!(vector.born_signs(), 0);
    assert_eq!(vector.aligned_signs(), 1);
}

#[test]
fn generation_and_loss_are_mirrored_when_the_arguments_swap() {
    // §6.2【M】的對稱版互通度建立在此:交換兩邊,生滅互換、各維不變。
    let before = base();
    let after = apply(&before, "evo:n1", &statement("delete sign(\"y\")"));
    let forward = diff_vector(&before, &after);
    let backward = diff_vector(&after, &before);
    assert_eq!(forward.died_signs(), backward.born_signs());
    assert_eq!(forward.born_signs(), backward.died_signs());
    assert_eq!(forward.syn.signs.changed, backward.syn.signs.changed);
    assert_eq!(forward.aligned_signs(), backward.aligned_signs());
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
    assert_eq!((vector.born_signs(), vector.died_signs()), (0, 0), "改名不是生滅");
    assert_eq!(vector.aligned_signs(), 2);
}

// ── 邊界 ──────────────────────────────────────────────────────────────────

#[test]
fn a_document_does_not_differ_from_itself() {
    let document = base();
    let vector = diff_vector(&document, &document);
    assert!(vector.is_identical(), "{vector:?}");
    assert_eq!(vector.aligned_signs(), 2);
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
    assert_eq!(vector.aligned_signs(), 0);
    assert_eq!(vector.died_signs(), 2);
    assert_eq!(vector.born_signs(), 1);
    assert_eq!(
        (
            vector.phon.signs.changed,
            vector.syn.signs.changed,
            vector.sem.signs.changed,
            vector.prag.signs.changed
        ),
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
    assert_eq!(vector.structural.signs.changed, 1, "跨維項目要落在 structural");
    assert_eq!(
        (
            vector.phon.signs.changed,
            vector.syn.signs.changed,
            vector.sem.signs.changed,
            vector.prag.signs.changed
        ),
        (0, 0, 0, 0),
        "不得被算進任何一維"
    );
    assert_eq!((vector.born_signs(), vector.died_signs()), (0, 0));
}

// ── 階層形狀的不變量(裁定 ①)────────────────────────────────────────────────

/// 五個 leaf 的 `both`/`only_before`/`only_after` 必然相同——**只有 `changed`
/// 因維而異**。
///
/// 這是 `aligned_signs()` / `born_signs()` / `died_signs()` 能成立的前提:
/// sign 集合怎麼對齊是**集合的性質,不是維的性質**。每個 leaf 各自帶滿四個數
/// 是為了介面一致(呼叫端逐 leaf 迭代時不必特例),那份 duplication 必須是
/// derived 的,不能各自漂移。
///
/// 沒有這條測試,第 2 步補 `rules` 與 trait 容器時把某一維的 `both` 改成
/// 「該維的規則數」之類的東西,三個 accessor 會安靜地開始說謊。
#[test]
fn every_leaf_agrees_on_the_sign_set_only_changed_varies() {
    let before = base();
    // 一次同時製造:改動(syn)、生、滅——三種計數都非零才驗得出來
    let after = apply(
        &before,
        "evo:n1",
        concat!(
            "\n    #0:\n        update sign(\"x\").feature[syn.category].value = verb\n",
            "\n    #1:\n        clone sign(\"x\") as z\n",
            "\n    #2:\n        delete sign(\"y\")\n",
        ),
    );
    let vector = diff_vector(&before, &after);

    let leaves = [
        ("phon", vector.phon.signs),
        ("syn", vector.syn.signs),
        ("sem", vector.sem.signs),
        ("prag", vector.prag.signs),
        ("structural", vector.structural.signs),
    ];
    for (name, counts) in leaves {
        assert_eq!(
            (counts.both, counts.only_before, counts.only_after),
            (
                vector.aligned_signs(),
                vector.died_signs(),
                vector.born_signs()
            ),
            "{name} 的 sign 集合計數與其他 leaf 不一致"
        );
    }

    // 前提:這組輸入真的讓三種計數都非零,否則上面比的是一串 0
    assert!(vector.aligned_signs() > 0, "要有對齊的 sign");
    assert!(vector.born_signs() > 0, "要有生");
    assert!(vector.died_signs() > 0, "要有滅");
    // 且 `changed` 真的因維而異——否則「只有 changed 因維而異」是空話
    assert_ne!(
        vector.syn.signs.changed, vector.phon.signs.changed,
        "這組輸入應該只動 syn"
    );
}
