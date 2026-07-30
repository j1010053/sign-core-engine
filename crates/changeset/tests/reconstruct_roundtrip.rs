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
use conlang_changeset::{apply_edit, change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};

const ROOT: &str = "Symbol a\nSymbol b\nSymbol k\n\n\
                    trait LocalNoun:\n\n\
                    global trait Core:\n\
                    \x20   phon:\n\
                    \x20       b => k\n\n\
                    sign book:\n\
                    \x20   belongs LocalNoun\n\
                    \x20   entrenchment = 0.5\n\
                    \x20   phon:\n\
                    \x20       /b a b/\n\
                    \x20   syn:\n\
                    \x20       category = noun\n\
                    \x20   sem:\n\
                    \x20       senses:\n\
                    \x20           core = BOOK\n\
                    \x20           log = LOGBOOK\n\
                    \x20       edges:\n\
                    \x20           log from core metonymy\n\n\
                    sign kobo:\n\
                    \x20   syn:\n\
                    \x20       category = noun\n";

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

// ── 更新(各 NodeUpdate 變體)────────────────────────────────────────────────

#[test]
fn a_definition_value_round_trips() {
    round_trip(
        "def value",
        &statement("update sign(\"book\").def[syn.category].value = verb"),
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
            "        update sign(\"book\").def[syn.category].value = verb\n",
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
