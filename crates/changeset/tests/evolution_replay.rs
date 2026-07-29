//! 步驟 16 ①② —— 演化圖節點 + **memoize replay**(docs/06 §1、§5)。
//!
//! 核心不變式:**狀態永遠是 ChangeSet 的函數**(不存副本),但求值結果可快取;
//! **parent 變動時沿依賴邊標 stale**。`replay_count` 是快取是否生效的唯一證據——
//! 只斷言「結果對」無法區分「有快取」與「每次重跑」。

use conlang_changeset::evolution::{EvolutionGraph, LanguageNode, NodeId};
use conlang_changeset::{change_set_prelude, UnresolvedChangeSet};
use conlang_language::{Language, LanguageDocument, LibrarySpec};

const ROOT: &str = r#"Symbol a
Symbol b
Symbol k

trait LocalNoun:

sign x:
    belongs LocalNoun
    entrenchment = 0.5
    phon:
        /a/
    syn:
        category = noun
"#;

fn root() -> LanguageDocument {
    LanguageDocument::import_new_root(ROOT, "evo:root").expect("root parses")
}

fn graph() -> EvolutionGraph {
    EvolutionGraph::new(root(), LibrarySpec::default())
}

fn id(name: &str) -> NodeId {
    NodeId(name.to_owned())
}

/// 針對 `base` 寫一份 `.chg`(prelude 的三道 digest 必須對得上 base——這正是
/// 節點要存**原文**、在 replay 當下才 resolve 的原因)。
fn changeset_for(base: &LanguageDocument, namespace: &str, body: &str) -> String {
    let mut source = change_set_prelude(base, &LibrarySpec::default(), namespace).unwrap();
    source.push_str(body);
    source
}

fn set_category(base: &LanguageDocument, namespace: &str, value: &str) -> String {
    changeset_for(
        base,
        namespace,
        &format!("\n    #0:\n        update sign(\"x\").def[syn.category].value = {value}\n"),
    )
}

fn node(parents: &[&str], changeset: String) -> LanguageNode {
    LanguageNode {
        parents: parents.iter().map(|p| id(p)).collect(),
        changeset,
        pin: None,
    }
}

/// 建一條 root → n1 → n2 的鏈,每代改一次 category。
fn chain() -> (EvolutionGraph, LanguageDocument) {
    let mut graph = graph();
    let root_doc = graph.root().clone();
    graph
        .insert(
            id("n1"),
            node(&[], set_category(&root_doc, "evo:n1", "verb")),
        )
        .expect("n1");
    // n2 的 base 是 n1 的求值結果,故先算出 n1。
    let mut probe = EvolutionGraph::new(root(), LibrarySpec::default());
    probe
        .insert(
            id("n1"),
            node(&[], set_category(&root_doc, "evo:n1", "verb")),
        )
        .unwrap();
    let n1_doc = probe.resolve(&id("n1")).expect("n1 resolves");
    graph
        .insert(
            id("n2"),
            node(&["n1"], set_category(&n1_doc, "evo:n2", "aux")),
        )
        .expect("n2");
    (graph, n1_doc)
}

// ── replay 正確性 ─────────────────────────────────────────────────────────

#[test]
fn a_node_is_the_replay_of_its_ancestor_chain() {
    let (mut graph, _) = chain();
    let resolved = graph.resolve(&id("n2")).expect("n2 resolves");
    let language = Language::parse(&resolved.source()).expect("re-parses");
    let sign = language.signs.iter().find(|s| s.name == "x").unwrap();
    let category = sign.items.iter().find_map(|item| match item {
        conlang_language::SignItem::Def(def) if def.path == "syn.category" => Some(&def.value),
        _ => None,
    });
    assert_eq!(
        category.map(String::as_str),
        Some("aux"),
        "n2 疊在 n1 之上:noun → verb → aux"
    );
}

#[test]
fn the_root_is_untouched_by_replay() {
    // 「狀態是 ChangeSet 的函數」——求值不得就地改動祖先。
    let (mut graph, _) = chain();
    let before = graph.root().source().to_owned();
    graph.resolve(&id("n2")).unwrap();
    assert_eq!(graph.root().source(), before);
}

// ── memoize(本刀的重點)───────────────────────────────────────────────────

#[test]
fn resolving_twice_replays_only_once() {
    let (mut graph, _) = chain();
    let first = graph.resolve(&id("n2")).unwrap();
    let count = graph.replay_count();
    let second = graph.resolve(&id("n2")).unwrap();
    assert_eq!(graph.replay_count(), count, "第二次必須命中快取,不得重跑");
    assert_eq!(first.source(), second.source());
}

#[test]
fn resolving_a_leaf_replays_each_ancestor_exactly_once() {
    let (mut graph, _) = chain();
    graph.resolve(&id("n2")).unwrap();
    assert_eq!(graph.replay_count(), 2, "n1 與 n2 各一次");
    // 祖先也順帶被快取了——再問 n1 不用重跑。
    let count = graph.replay_count();
    graph.resolve(&id("n1")).unwrap();
    assert_eq!(graph.replay_count(), count, "n1 已在鏈上被快取");
}

#[test]
fn a_second_branch_reuses_the_cached_ancestor() {
    // 這是 memoize 對**樹**的真正價值:兄弟分支共用祖先的求值結果。
    let (mut graph, n1_doc) = chain();
    graph
        .insert(
            id("n3"),
            node(&["n1"], set_category(&n1_doc, "evo:n3", "adj")),
        )
        .expect("n3");
    graph.resolve(&id("n2")).unwrap();
    let after_first_branch = graph.replay_count();
    graph.resolve(&id("n3")).unwrap();
    assert_eq!(
        graph.replay_count(),
        after_first_branch + 1,
        "n3 只需跑自己那一步,n1 沿用快取"
    );
}

// ── stale 傳播(docs/06 §5「沿依賴邊標 stale」)──────────────────────────────

#[test]
fn changing_a_node_invalidates_it_and_its_descendants_only() {
    let (mut graph, n1_doc) = chain();
    graph
        .insert(
            id("n3"),
            node(&["n1"], set_category(&n1_doc, "evo:n3", "adj")),
        )
        .unwrap();
    graph.resolve(&id("n2")).unwrap();
    graph.resolve(&id("n3")).unwrap();
    assert!(graph.is_cached(&id("n1")) && graph.is_cached(&id("n2")));

    // 改 n1 → n1/n2/n3 全部 stale。
    let root_doc = graph.root().clone();
    graph
        .set_changeset(&id("n1"), set_category(&root_doc, "evo:n1", "particle"))
        .unwrap();
    assert!(!graph.is_cached(&id("n1")), "自己 stale");
    assert!(!graph.is_cached(&id("n2")), "後代 stale");
    assert!(!graph.is_cached(&id("n3")), "另一個後代也 stale");
}

#[test]
fn changing_a_leaf_does_not_invalidate_its_ancestors() {
    // stale 只沿**後代**方向傳,祖先不受影響——否則快取等於沒用。
    let (mut graph, n1_doc) = chain();
    graph.resolve(&id("n2")).unwrap();
    assert!(graph.is_cached(&id("n1")));
    graph
        .set_changeset(&id("n2"), set_category(&n1_doc, "evo:n2", "adverb"))
        .unwrap();
    assert!(graph.is_cached(&id("n1")), "祖先必須留在快取裡");
    assert!(!graph.is_cached(&id("n2")));
}

#[test]
fn a_stale_node_recomputes_to_the_new_value() {
    // 快取失效後必須算出**新**結果,不是端出舊的。
    let (mut graph, _) = chain();
    graph.resolve(&id("n2")).unwrap();
    let root_doc = graph.root().clone();
    graph
        .set_changeset(&id("n1"), set_category(&root_doc, "evo:n1", "particle"))
        .unwrap();
    // n2 的 changeset 是對舊 n1 寫的,digest 對不上 → 明確失敗(而非默默用舊值)。
    let outcome = graph.resolve(&id("n2"));
    assert!(
        outcome.is_err(),
        "parent 換了內容,舊 changeset 的 base digest 必須擋下來"
    );
    // n1 自己則算得出新值。
    let n1 = graph.resolve(&id("n1")).expect("n1 recomputes");
    assert!(
        n1.source().contains("category = particle"),
        "{}",
        n1.source()
    );
}

// ── 圖不變式 ──────────────────────────────────────────────────────────────

#[test]
fn an_unknown_parent_is_rejected() {
    let mut graph = graph();
    let root_doc = graph.root().clone();
    let err = graph
        .insert(
            id("orphan"),
            node(&["ghost"], set_category(&root_doc, "evo:o", "verb")),
        )
        .expect_err("unknown parent");
    assert!(format!("{err}").contains("UNKNOWN_NODE"), "{err}");
}

#[test]
fn a_duplicate_node_is_rejected() {
    let (mut graph, _) = chain();
    let root_doc = graph.root().clone();
    let err = graph
        .insert(
            id("n1"),
            node(&[], set_category(&root_doc, "evo:d", "verb")),
        )
        .expect_err("duplicate");
    assert!(format!("{err}").contains("DUPLICATE"), "{err}");
}

#[test]
fn resolving_an_unknown_node_is_rejected() {
    let (mut graph, _) = chain();
    assert!(graph.resolve(&id("ghost")).is_err());
}

/// 多親 MVP 語意(docs/06 §5 v0.1.1):以 **`parents[0]` 為 replay 主幹**,
/// 其餘 parent 僅供條目引用取材(完整融合 replay 屬 M+)。
#[test]
fn a_multi_parent_node_replays_along_its_first_parent() {
    let (mut graph, n1_doc) = chain();
    graph
        .insert(
            id("n3"),
            node(&["n1"], set_category(&n1_doc, "evo:n3", "adj")),
        )
        .unwrap();
    let n2_doc = graph.resolve(&id("n2")).unwrap();
    graph
        .insert(
            id("creole"),
            // parents[0] = n2 → 主幹;n3 只是登記為第二來源。
            node(&["n2", "n3"], set_category(&n2_doc, "evo:c", "particle")),
        )
        .expect("多親節點合法");
    let resolved = graph.resolve(&id("creole")).expect("以 n2 為主幹求值");
    assert!(
        resolved.source().contains("category = particle"),
        "{}",
        resolved.source()
    );
}

#[test]
fn a_broken_changeset_surfaces_as_an_error_not_a_stale_value() {
    let mut graph = graph();
    graph
        .insert(id("bad"), node(&[], "changeset evo:bad:\n".to_owned()))
        .unwrap();
    assert!(graph.resolve(&id("bad")).is_err());
    assert!(!graph.is_cached(&id("bad")), "失敗不得留下快取");
}

#[test]
fn the_node_changeset_is_stored_verbatim() {
    // 存原文(而非已解析形):digest 綁 parent 的求值結果,只能在 replay 當下解析。
    let (graph, _) = chain();
    let source = &graph.node(&id("n2")).unwrap().changeset;
    assert!(source.starts_with("changeset evo:n2:"), "{source}");
    UnresolvedChangeSet::parse(source).expect("原文可再解析");
}
