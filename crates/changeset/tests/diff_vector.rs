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
    assert_eq!(
        (vector.born_signs(), vector.died_signs()),
        (0, 0),
        "沒有生滅"
    );
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
    assert_eq!(
        vector.dimension(Dim::Syn).signs.changed,
        vector.syn.signs.changed
    );
    assert_eq!(
        vector.dimension(Dim::Phon).signs.changed,
        vector.phon.signs.changed
    );
    assert_eq!(
        vector.dimension(Dim::Sem).signs.changed,
        vector.sem.signs.changed
    );
    assert_eq!(
        vector.dimension(Dim::Prag).signs.changed,
        vector.prag.signs.changed
    );
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
    assert_eq!(
        (vector.born_signs(), vector.died_signs()),
        (0, 0),
        "改名不是生滅"
    );
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
    assert_eq!(
        vector.structural.signs.changed, 1,
        "跨維項目要落在 structural"
    );
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

// ── trait 容器與規則(裁定 ③)───────────────────────────────────────────────
//
// 這一段守的是《分層差異向量 v0.2 裁定》§0 記的那個 bug:舊實作**只走
// `signs`**,於是一條音變、或 trait 上的一行 `belongs`,整個 diff 全零
// ——連帶讓兩節點的互通度算成 1.0,方言分群完全看不見。
//
// 每條測試都同時斷言「新 leaf 動了」**與**「signs 全零」。後者才是重點:
// 它證明這些資訊**只**來自新補的 leaf,舊路徑確實看不到——沒有這半邊,
// 測試就無法區分「補好了」與「本來就抓得到」。

const TRAIT_ROOT: &str = "Symbol a\nSymbol b\n\n\
                          global trait Core:\n\
                          \x20   phon:\n\
                          \x20       rules:\n\
                          \x20           a => b / _#\n\n\
                          trait LocalAnimate:\n\n\
                          trait LocalNoun:\n\n\
                          sign x:\n\
                          \x20   belongs LocalNoun\n\
                          \x20   phon:\n\
                          \x20       /a/\n";

fn trait_base() -> LanguageDocument {
    LanguageDocument::import_new_root(TRAIT_ROOT, "evo:troot").expect("trait root parses")
}

/// 每一維的 `signs` 分量都沒動——用來證明新資訊只來自新 leaf。
fn assert_signs_untouched(vector: &conlang_changeset::diff::DiffVector) {
    assert_eq!(
        (
            vector.phon.signs.changed,
            vector.syn.signs.changed,
            vector.sem.signs.changed,
            vector.prag.signs.changed,
            vector.structural.signs.changed,
        ),
        (0, 0, 0, 0, 0),
        "sign 沒被碰到,signs 分量就該全零——這是舊實作看得見的全部"
    );
    assert_eq!(
        (vector.born_signs(), vector.died_signs()),
        (0, 0),
        "沒有生滅"
    );
}

/// 一條音變改了 → `phon.trait_rules` 動,其餘不動。
///
/// **這正是 §0 的第一個例子。** 改動前這個向量是全零的。
///
/// 位址走 `.leaf[0].body` 而非 `.body`:`rules:` 這種 P46 結構化寫法把內容
/// 放進 `Rule.phon_block`,`Rule.body` 是空的(見 `Rule.phon_block` 註解)。
/// 寫成 `.body` 不會報錯**也不會生效**,那條測試就會在驗 diff 之前先被
/// 一個沒發生的編輯騙過去。
#[test]
fn a_sound_change_in_a_trait_moves_the_phon_trait_rules_component() {
    let before = trait_base();
    let after = apply(
        &before,
        "evo:t1",
        &statement("update trait(\"Core\").block[0].rule[0].leaf[0].body = a => b / #_"),
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(
        vector.phon.trait_rules.counts.changed, 1,
        "音變要落在 phon.trait_rules"
    );
    assert_eq!(
        vector.phon.trait_rules.counts.both, 1,
        "規則依 RuleId 對得上,不是一生一滅"
    );
    // 規則**只**記在 `rules`。`traits` 也記一次就是雙重計數,而那兩個數字下游
    // 很可能相加——一條音變會變成兩條。
    assert_eq!(
        (
            vector.phon.trait_content.counts.changed,
            vector.structural.trait_content.counts.changed
        ),
        (0, 0),
        "規則不得同時記進 trait_content leaf"
    );
    assert_eq!(
        (
            vector.syn.trait_rules.counts.changed,
            vector.sem.trait_rules.counts.changed,
            vector.prag.trait_rules.counts.changed
        ),
        (0, 0, 0),
        "只有 phon 的規則動了"
    );
    assert!(!vector.is_identical(), "音變不得被判成毫無差異");
    assert_signs_untouched(&vector);
}

/// trait 上加一行 `belongs` → `structural.trait_content` 動。
///
/// **§0 的第二個例子。** `belongs` 跨維(同時影響 syn 與 sem 的繼承),
/// 故落在 structural 而不是任何單一維。
///
/// 位址要寫到 `.block[0]`:trait 的項目住在 `Block` 裡(`TraitDef.blocks`,
/// P27 的 `==` 分塊),不像 sign 可以直接 insert 進容器本身。
#[test]
fn a_belongs_edge_on_a_trait_moves_the_structural_trait_content_component() {
    let before = trait_base();
    let after = apply(
        &before,
        "evo:t2",
        "\n    #0:\n        insert into trait(\"LocalNoun\").block[0] at end:\n            belongs LocalAnimate\n",
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(
        vector.structural.trait_content.counts.changed, 1,
        "belongs 要落在 structural.trait_content"
    );
    assert_eq!(
        (
            vector.phon.trait_content.counts.changed,
            vector.syn.trait_content.counts.changed,
            vector.sem.trait_content.counts.changed,
            vector.prag.trait_content.counts.changed
        ),
        (0, 0, 0, 0),
        "跨維項目不得被算進任何一維"
    );
    // 裁定丙:宣告處算 1(上面),波及面另記——`sign x` 是 belongs LocalNoun 的。
    assert_eq!(
        (
            vector.structural.trait_content.reach_before,
            vector.structural.trait_content.reach_after
        ),
        (1, 1),
        "改的是 LocalNoun,而 x belongs LocalNoun"
    );
    assert!(!vector.is_identical());
    assert_signs_untouched(&vector);
}

/// trait 上的 **sem 內容**(義項)→ 只動 `sem.trait_content`。
///
/// 前兩條測試只驗到 `structural.trait_content` 與 `trait_rules`,四個**維度**的 `trait_content` leaf
/// 至此全是被斷言為零的一方——沒有這條,`trait_projection` 把整個 trait 歸錯維
/// 也不會有人發現。
///
/// 順帶守住**裁定丙**:`sign x` 是 `belongs LocalNoun` 的,它的 sem 投影因此
/// 真的變了,但 `signs` 分量仍須是零——改動記在**宣告處**(算 1),波及面
/// 另記於 `reach_*`。
#[test]
fn a_sense_on_a_trait_moves_only_the_sem_trait_content_component() {
    let before = trait_base();
    let after = apply(
        &before,
        "evo:t5",
        "\n    #0:\n        insert into trait(\"LocalNoun\").block[0] at end:\n            sem:\n                senses:\n                    core = THING\n",
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(
        vector.sem.trait_content.counts.changed, 1,
        "義項是 sem 內容"
    );
    assert_eq!(
        (
            vector.phon.trait_content.counts.changed,
            vector.syn.trait_content.counts.changed,
            vector.prag.trait_content.counts.changed,
            vector.structural.trait_content.counts.changed,
        ),
        (0, 0, 0, 0),
        "其餘維的 trait_content leaf 不得跟著動"
    );
    assert_eq!(
        vector.sem.trait_rules.counts.changed, 0,
        "加的是內容不是規則,trait_rules leaf 不該動"
    );
    assert_eq!(
        (
            vector.sem.trait_content.reach_before,
            vector.sem.trait_content.reach_after
        ),
        (1, 1),
        "一個詞 belongs LocalNoun,波及面就是 1"
    );
    assert_eq!(
        (
            vector.syn.trait_content.reach_before,
            vector.syn.trait_content.reach_after
        ),
        (0, 0),
        "沒有事件的 leaf 不得有 reach——否則下游會以為 syn 也被波及了"
    );
    assert_signs_untouched(&vector);
}

/// trait 上的 **slot**(valence,P41)→ 只動 `syn.trait_content`。
///
/// slot 與 Def 是不同的項目型別,分開驗:`item_dimension` 對它們各有一條分支,
/// 一起驗會讓其中一條錯了也照樣綠。
#[test]
fn a_slot_on_a_trait_moves_only_the_syn_trait_content_component() {
    let before = trait_base();
    let after = apply(
        &before,
        "evo:t6",
        "\n    #0:\n        insert into trait(\"LocalNoun\").block[0] at end:\n            syn:\n                slots:\n                    AGENT [LocalAnimate]\n",
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(
        vector.syn.trait_content.counts.changed, 1,
        "slot 是 syn 內容(P41 valence=slots)"
    );
    assert_eq!(
        (
            vector.phon.trait_content.counts.changed,
            vector.sem.trait_content.counts.changed,
            vector.prag.trait_content.counts.changed,
            vector.structural.trait_content.counts.changed,
        ),
        (0, 0, 0, 0),
        "slot 不得溢到別的維,也不是跨維項目"
    );
    assert_signs_untouched(&vector);
}

// ── 影響範圍旁註(裁定丙,§3.1)──────────────────────────────────────────────
//
// reach 回答的是「波及多廣」,與四元組的「改了幾處」是**兩個問題**。這一段
// 守的就是它們不互相污染:改動記在宣告處(永遠是 1),波及面另記,而且
// **零波及不等於沒改**。

/// **零波及不等於沒改**(§3.1 明文要求測試守住)。
///
/// 改一個沒有任何詞 `belongs` 的 trait:`changed` 仍是 1,reach 是 0。
/// 若這兩者被壓成一個數字,「改了一個沒人用的 trait」就會與「什麼都沒改」
/// 撞在一起——那正是丙相對於乙的全部意義。
#[test]
fn changing_a_trait_nobody_uses_still_counts_but_reaches_nothing() {
    let before = trait_base();
    // `LocalAnimate` 在 TRAIT_ROOT 裡沒有任何 sign belongs 它。
    let after = apply(
        &before,
        "evo:r1",
        "\n    #0:\n        insert into trait(\"LocalAnimate\").block[0] at end:\n            sem:\n                senses:\n                    core = BEAST\n",
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(vector.sem.trait_content.counts.changed, 1, "改了就是改了");
    assert!(!vector.is_identical(), "零波及不得被判成毫無差異");
    assert_eq!(
        (
            vector.sem.trait_content.reach_before,
            vector.sem.trait_content.reach_after
        ),
        (0, 0),
        "沒有詞 belongs LocalAnimate"
    );
}

/// reach 沿 **`belongs` 閉包**算,不是只算直接 `belongs`。
///
/// `x → LocalNoun → LocalAnimate`:改 `LocalAnimate` 波及 `x`,雖然 `x` 從未
/// 直接提過 `LocalAnimate`。只看直接邊會讓愈上層、影響愈大的 trait 反而
/// 報愈小的波及面——正好把這個數字的用途弄反。
#[test]
fn reach_follows_the_belongs_closure_transitively() {
    let before = LanguageDocument::import_new_root(
        "Symbol a\n\n\
         trait LocalAnimate:\n\n\
         trait LocalNoun:\n\
         \x20   belongs LocalAnimate\n\n\
         sign x:\n\
         \x20   belongs LocalNoun\n\
         \x20   phon:\n\
         \x20       /a/\n",
        "evo:nested",
    )
    .expect("nested root parses");
    let after = apply(
        &before,
        "evo:r2",
        "\n    #0:\n        insert into trait(\"LocalAnimate\").block[0] at end:\n            sem:\n                senses:\n                    core = BEAST\n",
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(vector.sem.trait_content.counts.changed, 1);
    assert_eq!(
        (
            vector.sem.trait_content.reach_before,
            vector.sem.trait_content.reach_after
        ),
        (1, 1),
        "x 經 LocalNoun 間接 belongs LocalAnimate"
    );
}

/// `global trait` 的波及面是**全部的詞**,不是 0。
///
/// global trait 是自動引用的 phon-rule macro(P6),沒有任何詞需要 `belongs`
/// 它。只數 `belongs` 閉包會讓一條全語言音變——§0 的第一個例子、最該被看見的
/// 那種改動——報出 0 的波及面。
#[test]
fn a_global_trait_reaches_every_sign() {
    let before = LanguageDocument::import_new_root(
        "Symbol a\nSymbol b\n\n\
         global trait Core:\n\
         \x20   phon:\n\
         \x20       rules:\n\
         \x20           a => b / _#\n\n\
         trait LocalNoun:\n\n\
         sign x:\n\
         \x20   belongs LocalNoun\n\
         \x20   phon:\n\
         \x20       /a/\n\n\
         sign y:\n\
         \x20   phon:\n\
         \x20       /a/\n",
        "evo:global",
    )
    .expect("global root parses");
    let after = apply(
        &before,
        "evo:r3",
        &statement("update trait(\"Core\").block[0].rule[0].leaf[0].body = a => b / #_"),
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(vector.phon.trait_rules.counts.changed, 1);
    assert_eq!(
        (
            vector.phon.trait_rules.reach_before,
            vector.phon.trait_rules.reach_after
        ),
        (2, 2),
        "一條音變作用在整份語言上,兩個詞都算"
    );
    // 判別性:同一份文件裡,非 global 的 trait 只波及它自己那一片。
    let local = apply(
        &before,
        "evo:r4",
        "\n    #0:\n        insert into trait(\"LocalNoun\").block[0] at end:\n            sem:\n                senses:\n                    core = THING\n",
    );
    let local_vector = diff_vector(&before, &local);
    assert_eq!(
        (
            local_vector.sem.trait_content.reach_before,
            local_vector.sem.trait_content.reach_after
        ),
        (1, 1),
        "只有 x belongs LocalNoun,y 不在裡面"
    );
}

/// **兩個數,不是一個**:同一批編輯裡波及面本身會移動。
///
/// 這裡一次做兩件事:改 `LocalNoun` 的內容,並複製一個 `belongs LocalNoun`
/// 的詞。於是同一個 leaf 的前後波及面不同(1 → 2)。只發一個數就得先挑
/// 「用哪一邊」,而那是 §6.4 已裁定引擎不做的合成選擇。
#[test]
fn reach_before_and_after_can_differ() {
    let before = trait_base();
    let after = apply(
        &before,
        "evo:r5",
        concat!(
            "\n    #0:\n        insert into trait(\"LocalNoun\").block[0] at end:\n            sem:\n                senses:\n                    core = THING\n",
            "\n    #1:\n        clone sign(\"x\") as z\n",
        ),
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(
        vector.sem.trait_content.counts.changed, 1,
        "宣告處仍然只算 1"
    );
    assert_eq!(
        (
            vector.sem.trait_content.reach_before,
            vector.sem.trait_content.reach_after
        ),
        (1, 2),
        "複製出來的 z 也 belongs LocalNoun,後狀態的波及面因此變大"
    );
}

/// reach **不是差異分量**:它不參與 `is_identical`,也不會讓沒事的 leaf 說謊。
///
/// 改一個 sign 的 `belongs`(完全沒動任何 trait):trait 那側的每個 leaf 都必須
/// 是零計數**且**零 reach。若 reach 改成「所有 trait 的涵蓋數」而不是「有事件的
/// trait 的涵蓋數」,這裡就會冒出非零的波及面,下游把它當第五個計數加總,
/// 一次 sign 編輯就會被記成一次語言級改動。
#[test]
fn reach_stays_zero_when_no_trait_has_an_event() {
    let before = trait_base();
    let after = apply(
        &before,
        "evo:r6",
        "\n    #0:\n        insert into sign(\"x\") at end:\n            belongs LocalAnimate\n",
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(
        vector.structural.signs.changed, 1,
        "前提:改的是 sign 上的 belongs"
    );
    for (name, leaf) in [
        ("phon.trait_content", vector.phon.trait_content),
        ("syn.trait_content", vector.syn.trait_content),
        ("sem.trait_content", vector.sem.trait_content),
        ("prag.trait_content", vector.prag.trait_content),
        ("structural.trait_content", vector.structural.trait_content),
        ("phon.trait_rules", vector.phon.trait_rules),
    ] {
        assert!(leaf.is_identical(), "{name} 不該有事件");
        assert_eq!(
            (leaf.reach_before, leaf.reach_after),
            (0, 0),
            "{name} 沒有事件就不該有波及面"
        );
    }
}

/// 新增一個 trait → `traits` 的 `only_after`,不是 `changed`。
#[test]
fn a_new_trait_is_a_birth_not_a_change() {
    let before = trait_base();
    let after = apply(
        &before,
        "evo:t3",
        "\n    #0:\n        insert into language at end:\n            trait Nocturnal:\n",
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(
        vector.structural.trait_content.counts.only_after, 1,
        "新 trait 是生"
    );
    assert_eq!(vector.structural.trait_content.counts.only_before, 0);
    assert_eq!(
        vector.structural.trait_content.counts.changed, 0,
        "既有 trait 沒被改,changed 就該是 0"
    );
    assert_signs_untouched(&vector);
}

/// **範圍邊界**:掛在 sign 上的規則留在 `signs`,不進 `rules`。
///
/// `rules` 收的是 trait 宣告、經 `belongs` 閉包作用到一整片詞的語言級規則;
/// 掛在某個 sign 上的規則是那一個詞的內容。混在一起就分不出「改了一個詞」
/// 與「改了一條全語言的音變」——而 §6.1 說詞彙差異與規則性音變的權重不同,
/// 分不開就沒得加權。
#[test]
fn a_rule_on_a_sign_stays_in_the_signs_component() {
    let before = trait_base();
    let after = apply(
        &before,
        "evo:t4",
        "\n    #0:\n        insert into sign(\"x\") at end:\n            phon:\n                rules:\n                    b => a / _#\n",
    );
    let vector = diff_vector(&before, &after);

    assert_eq!(
        vector.phon.signs.changed, 1,
        "sign 自帶的規則算在該 sign 頭上"
    );
    assert_eq!(
        vector.phon.trait_rules,
        conlang_changeset::diff::TraitDiff {
            counts: conlang_changeset::diff::DiffCounts {
                both: 1,
                changed: 0,
                only_before: 0,
                only_after: 0
            },
            reach_before: 0,
            reach_after: 0,
        },
        "trait 的規則一條沒動,trait_rules leaf 不該有任何變化(reach 也不該憑空出現)"
    );
}
