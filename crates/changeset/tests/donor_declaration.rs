//! 步驟 16 ⑥a —— **`donor` 宣告**(《修補11》P62 §7.3)。
//!
//! `ContactInjection` 是 docs/06 §3.2 早已指定、從未實作的跨語言引用機制。本刀只做
//! **宣告面**:prelude 多一類外部依賴,形狀比照既有的 `library` 鎖。
//!
//! ## 為什麼宣告要在 prelude 而不是 config
//!
//! P63/§9:`ContactInjection` 決定 replay **讀得到什麼**,故它改變語言狀態,必須落在
//! **進雜湊**的一側(邊上的 `.chg`)。放在雜湊外的 config 會使 ① P58 內容定址失效
//! (同一個雜湊可對應不同結果)、② 改 config 能悄悄換掉借用來源而不生新節點。
//!
//! ## 為什麼不需要另外的 digest 欄位
//!
//! P58 之下 `NodeId` **就是**內容雜湊,故 `donor <alias> <node-id>` 的 node-id 本身
//! 已經是 digest——donor 內容一變就是另一個 `NodeId`,引用自然失效。

use conlang_changeset::{change_set_prelude, DonorRef, ReplayError, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};

const ROOT: &str = "sign x:\n    syn:\n        category = noun\n";
const NODE: &str = "3f2a9b7c5d1e8046a2b3c4d5e6f70819a2b3c4d5e6f708192a3b4c5d6e7f8091";

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(ROOT, "evo:root").expect("root parses")
}

/// 在 prelude 尾端插入若干行,再接語句。`change_set_prelude` 只產生**推導得出**的
/// 部分(digest、library 鎖);donor 是**作者寫的**,故由測試自己補上。
fn changeset(extra: &[&str], body: &str) -> String {
    let mut source = change_set_prelude(&base(), &LibrarySpec::default(), "evo:n1").unwrap();
    for line in extra {
        source.push_str(&format!("    {line}\n"));
    }
    source.push_str(body);
    source
}

fn parse(extra: &[&str], body: &str) -> Result<UnresolvedChangeSet, ReplayError> {
    UnresolvedChangeSet::parse(&changeset(extra, body))
}

#[test]
fn a_donor_declaration_is_parsed() {
    let parsed = parse(&[&format!("donor fr {NODE}")], "").expect("parses");
    assert_eq!(
        parsed.donors,
        vec![DonorRef {
            alias: "fr".to_owned(),
            node: NODE.to_owned(),
        }]
    );
}

#[test]
fn several_donors_are_kept_in_order() {
    let other = NODE.replace("3", "9");
    let parsed = parse(
        &[&format!("donor fr {NODE}"), &format!("donor wo {other}")],
        "",
    )
    .expect("parses");
    assert_eq!(parsed.donors.len(), 2);
    assert_eq!(parsed.donors[0].alias, "fr");
    assert_eq!(parsed.donors[1].alias, "wo");
}

#[test]
fn donors_survive_resolve_and_dump() {
    // **斷言的是「保留」而不是「不動點」**:若 `dump` 整個漏掉 donor,再解析出來的是
    // 空的,與原本的 1 筆不等,測試會紅。只比對「dump 兩次相同」的寫法抓不到漏掉。
    let spec = LibrarySpec::default();
    let source = changeset(
        &[&format!("donor fr {NODE}")],
        "\n    #0:\n        update sign(\"x\").def[syn.category].value = verb\n",
    );
    let parsed = UnresolvedChangeSet::parse(&source).expect("parses");
    let resolved = parsed.resolve(&base(), &spec).expect("resolves");
    assert_eq!(resolved.donors, parsed.donors, "resolve 不得丟掉宣告");

    let dumped = resolved.dump();
    let again = UnresolvedChangeSet::parse(&dumped).expect("dump 可再解析");
    assert_eq!(again.donors, parsed.donors, "dump 不得丟掉宣告");
    assert!(
        dumped.contains(&format!("donor fr {NODE}")),
        "排出的形狀要是 `donor <別名> <node-id>`:{dumped}"
    );
}

#[test]
fn a_duplicate_alias_is_rejected() {
    // **靜默取錯語言的守門**。同一個別名宣告兩次,body 的引用指向哪一個是任意的
    // ——而那不會報錯,只是從錯的語言取材,結果照樣算得出來。
    let other = NODE.replace("3", "9");
    let err = parse(
        &[&format!("donor fr {NODE}"), &format!("donor fr {other}")],
        "",
    )
    .expect_err("重複別名");
    assert!(format!("{err}").contains("duplicate donor alias"), "{err}");
}

#[test]
fn a_donor_without_a_node_id_is_rejected() {
    let err = parse(&["donor fr"], "").expect_err("缺 node-id");
    assert!(format!("{err}").contains("donor"), "{err}");
}

#[test]
fn a_donor_with_an_empty_alias_is_rejected() {
    let err = parse(&[&format!("donor  {NODE}")], "").expect_err("空別名");
    assert!(format!("{err}").contains("donor"), "{err}");
}

#[test]
fn a_changeset_without_donors_is_unchanged() {
    // 相容性:沒宣告 donor 的 `.chg` 行為一如既往,dump 也不多排空行。
    let spec = LibrarySpec::default();
    let source = changeset(
        &[],
        "\n    #0:\n        update sign(\"x\").def[syn.category].value = verb\n",
    );
    let resolved = UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base(), &spec)
        .unwrap();
    assert!(resolved.donors.is_empty());
    assert!(!resolved.dump().contains("donor"));
}

#[test]
fn a_declared_donor_does_not_disturb_the_statements() {
    // 宣告只是**授權範圍**,不改變任何編輯的行為(本刀還沒有引用條目)。
    let spec = LibrarySpec::default();
    let body = "\n    #0:\n        update sign(\"x\").def[syn.category].value = verb\n";
    let with = UnresolvedChangeSet::parse(&changeset(&[&format!("donor fr {NODE}")], body))
        .unwrap()
        .resolve(&base(), &spec)
        .unwrap();
    let without = UnresolvedChangeSet::parse(&changeset(&[], body))
        .unwrap()
        .resolve(&base(), &spec)
        .unwrap();
    assert_eq!(with.statements, without.statements);
}
