//! 步驟 16 ④ —— **演化圖節點模型**(《修補11》P56/P58;docs/06 §1–§5)。
//!
//! 前一版驗的是 memoize(`replay_count` 證明快取生效)。P56 之後那整層不存在:
//! 節點物化 snapshot、邊持 changeset、兩者不可變,故沒有快取可證。改驗三件事:
//!
//! 1. **因果契約**(§2.2):snapshot 永遠 = `apply(parent.snapshot, edge.changeset)`
//!    ——由 `verify()`(fsck)斷言,而非只看結果對不對;
//! 2. **內容定址**(P58):id 由內容算出,`label` 不入身分,重複提交冪等;
//! 3. **不可變性**(§2.3):改 changeset 生成**並存的新鏈**,舊鏈始終有效。
//!
//! 註:「讀取是 O(1)、不 replay 祖先」無法用執行期斷言區分(結果一樣)。它由
//! **型別**保證——`snapshot()` 取 `&self` 而非 `&mut self`,不可能邊讀邊算邊快取。
//! 誠實標記:本檔不為此寫測試。

use conlang_changeset::evolution::{Edge, EvolutionGraph, Nativization, NodeId, RebaseOutcome};
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

sign y:
    syn:
        category = noun
"#;

fn root() -> LanguageDocument {
    LanguageDocument::import_new_root(ROOT, "evo:root").expect("root parses")
}

fn graph() -> EvolutionGraph {
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    graph.add_root(root()).expect("root added");
    graph
}

/// 單 root 圖的 root。多 root 的案例自己呼叫 `add_root` 並持有回傳的 id。
fn only_root(graph: &EvolutionGraph) -> NodeId {
    let mut roots = graph.roots();
    let first = roots.next().expect("圖裡有 root").clone();
    assert!(roots.next().is_none(), "本輔助函式只用於單 root 的圖");
    first
}

/// 針對 `base` 寫一份 `.chg`(prelude 的三道 digest 必須對得上 base——邊存**原文**、
/// 在建節點當下才 resolve,正是為此)。
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

fn category_of(document: &LanguageDocument) -> Option<String> {
    let language = Language::parse(&document.source()).expect("re-parses");
    let sign = language.signs.iter().find(|s| s.name == "x")?;
    sign.items.iter().find_map(|item| match item {
        conlang_language::SignItem::Def(def) if def.path == "syn.category" => {
            Some(def.value.clone())
        }
        _ => None,
    })
}

/// root → n1(verb)→ n2(aux)。回傳兩個 id。
fn chain(graph: &mut EvolutionGraph) -> (NodeId, NodeId) {
    let root_id = only_root(graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let n1 = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                set_category(&root_doc, "evo:n1", "verb"),
            )],
            Nativization::None,
            None,
        )
        .expect("n1 commits");
    let n1_doc = graph.snapshot(&n1).unwrap().clone();
    let n2 = graph
        .commit(
            vec![Edge::trunk(
                n1.clone(),
                set_category(&n1_doc, "evo:n2", "aux"),
            )],
            Nativization::None,
            None,
        )
        .expect("n2 commits");
    (n1, n2)
}

// ── 因果契約(P56 §2.2)─────────────────────────────────────────────────────

#[test]
fn a_committed_node_materialises_the_replay_of_its_trunk_edge() {
    let mut graph = graph();
    let (_, n2) = chain(&mut graph);
    assert_eq!(
        category_of(graph.snapshot(&n2).unwrap()).as_deref(),
        Some("aux"),
        "n2 疊在 n1 之上:noun → verb → aux"
    );
}

#[test]
fn fsck_holds_for_every_node() {
    // **這是 P56 的核心斷言**:物化的 snapshot 確實等於 replay 的結果,故「存副本」
    // 沒有失去單一資訊源。只斷言 `category == aux` 證明不了這件事——那只看得到結果。
    let mut graph = graph();
    chain(&mut graph);
    graph.verify_all().expect("整棵樹的不變式都成立");
}

#[test]
fn the_root_has_no_parents_and_is_untouched() {
    let mut graph = graph();
    let root_id = only_root(&graph);
    let before = graph.snapshot(&root_id).unwrap().source().to_owned();
    chain(&mut graph);
    assert!(graph.node(&root_id).unwrap().parents().is_empty());
    assert_eq!(graph.snapshot(&root_id).unwrap().source(), before);
}

#[test]
fn the_trunk_edge_stores_the_changeset_verbatim() {
    // 存原文(而非已解析形):digest 綁 parent 的 snapshot,只能在建節點當下解析。
    let mut graph = graph();
    let (_, n2) = chain(&mut graph);
    let edge = graph.node(&n2).unwrap().trunk().expect("n2 有主幹邊");
    let source = edge.changeset.as_deref().expect("主幹邊帶 changeset");
    assert!(source.starts_with("changeset evo:n2:"), "{source}");
    UnresolvedChangeSet::parse(source).expect("原文可再解析");
}

// ── 內容定址(P58)─────────────────────────────────────────────────────────

#[test]
fn committing_identical_content_is_idempotent() {
    // 內容定址 ⇒ 同內容是同一個物件(git 語意);前一版的 `Duplicate` 錯誤因此消失。
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let changeset = set_category(&root_doc, "evo:n1", "verb");
    let first = graph
        .commit(
            vec![Edge::trunk(root_id.clone(), changeset.clone())],
            Nativization::None,
            None,
        )
        .unwrap();
    let count = graph.len();
    let second = graph
        .commit(
            vec![Edge::trunk(root_id, changeset)],
            Nativization::None,
            None,
        )
        .unwrap();
    assert_eq!(first, second, "同內容必須是同一個 id");
    assert_eq!(graph.len(), count, "不得多出一個節點");
}

#[test]
fn a_different_changeset_yields_a_different_node() {
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let a = graph
        .commit(
            vec![Edge::trunk(
                root_id.clone(),
                set_category(&root_doc, "evo:n1", "verb"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let b = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                set_category(&root_doc, "evo:n1", "adj"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    assert_ne!(a, b);
}

#[test]
fn two_routes_to_the_same_state_are_two_nodes() {
    // **P58 解讀的判別案例**。上一個測試用的兩份 changeset 產生**不同的 snapshot**,
    // 故就算 `node_id` 完全不看 changeset,id 一樣會不同——它證明不了「邊的 changeset
    // 入雜湊」。這裡兩份 changeset 走到**逐字相同的 snapshot**:
    //   A: 一步 noun → verb
    //   B: 兩步 noun → adj → verb
    // 若 changeset 不入雜湊,兩者會摺疊成同一個節點,**其中一份 changeset 被靜默丟棄**
    // ——違反 docs/06「存事實(ChangeSet)」。故必須是兩個節點。
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();

    let direct = set_category(&root_doc, "evo:n1", "verb");
    let detour = changeset_for(
        &root_doc,
        "evo:n1",
        "\n    #0:\n        update sign(\"x\").def[syn.category].value = adj\
         \n\n    #1:\n        update sign(\"x\").def[syn.category].value = verb\n",
    );
    assert_ne!(direct, detour, "兩份 changeset 本身不同");

    let a = graph
        .commit(
            vec![Edge::trunk(root_id.clone(), direct)],
            Nativization::None,
            None,
        )
        .unwrap();
    let b = graph
        .commit(vec![Edge::trunk(root_id, detour)], Nativization::None, None)
        .unwrap();

    assert_eq!(
        graph.snapshot(&a).unwrap().source(),
        graph.snapshot(&b).unwrap().source(),
        "前提:兩條路徑的 snapshot 必須逐字相同,否則本測試沒有判別力"
    );
    assert_ne!(a, b, "P58「身分含出身」:來歷不同即不同節點");
    graph.verify_all().unwrap();
}

#[test]
fn nativization_is_part_of_identity() {
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let changeset = set_category(&root_doc, "evo:n1", "verb");
    let plain = graph
        .commit(
            vec![Edge::trunk(root_id.clone(), changeset.clone())],
            Nativization::None,
            None,
        )
        .unwrap();
    let creole = graph
        .commit(
            vec![Edge::trunk(root_id, changeset)],
            Nativization::Creole { generation: 1 },
            None,
        )
        .unwrap();
    assert_ne!(plain, creole, "docs/06 §4:nativization 是節點的獨立屬性");
}

#[test]
fn the_label_is_not_part_of_identity() {
    // P58/P45:人類可讀名字是另一層,不是身分。
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let changeset = set_category(&root_doc, "evo:n1", "verb");
    let bare = graph
        .commit(
            vec![Edge::trunk(root_id.clone(), changeset.clone())],
            Nativization::None,
            None,
        )
        .unwrap();
    let named = graph
        .commit(
            vec![Edge::trunk(root_id, changeset)],
            Nativization::None,
            Some("Old Tongue".to_owned()),
        )
        .unwrap();
    assert_eq!(bare, named);
}

#[test]
fn a_node_id_is_deterministic_across_graphs() {
    // P26:同輸入同輸出,無隨機/時間戳來源。
    let (mut first, mut second) = (graph(), graph());
    assert_eq!(
        only_root(&first),
        only_root(&second),
        "同一份 root 必得同一個 id"
    );
    let (_, a) = chain(&mut first);
    let (_, b) = chain(&mut second);
    assert_eq!(a, b);
}

// ── 不可變性與並存的鏈(P56 §2.3)──────────────────────────────────────────

#[test]
fn editing_a_changeset_forks_a_parallel_chain_and_leaves_the_old_one_valid() {
    // §2.3 的核心:改 e1 不是就地改邊,而是生成新邊 e1' → 新節點 n1';
    // 舊鏈 root→n1→n2 **始終有效**。這正是前一版做不到的事——前一版
    // `set_changeset` 會就地改掉 n1,讓 n2 因 digest 不符而永久失效(§0 的缺陷)。
    let mut graph = graph();
    let (n1, n2) = chain(&mut graph);
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();

    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                set_category(&root_doc, "evo:n1", "particle"),
            )],
            Nativization::None,
            None,
        )
        .expect("新邊生成新節點");

    assert_ne!(n1, n1_prime);
    assert_eq!(
        category_of(graph.snapshot(&n1).unwrap()).as_deref(),
        Some("verb"),
        "舊節點不得被改動"
    );
    assert_eq!(
        category_of(graph.snapshot(&n2).unwrap()).as_deref(),
        Some("aux"),
        "舊鏈的後代仍然有效"
    );
    assert_eq!(
        category_of(graph.snapshot(&n1_prime).unwrap()).as_deref(),
        Some("particle")
    );
    graph.verify_all().expect("兩條鏈的不變式都成立");
}

// ── 圖不變式 ──────────────────────────────────────────────────────────────

#[test]
fn an_unknown_parent_is_rejected() {
    // 這同時是**無環的機制**:parent 必須已存在才引用得到,而節點 id 由 parents 的 id
    // 算出,故成環需要一個雜湊包含自己——不可能。前一版的 `check_acyclic` 因此移除。
    let mut graph = graph();
    let root_doc = graph.snapshot(&only_root(&graph)).unwrap().clone();
    let real = graph
        .commit(
            vec![Edge::trunk(
                only_root(&graph),
                set_category(&root_doc, "evo:n1", "verb"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    // 拿一個「看起來像 id 但不在圖裡」的值:用真 id 的節點刪掉後不可能重現,
    // 故改用另一張圖算出的、本圖沒有的 id。
    let mut other = EvolutionGraph::new(LibrarySpec::default());
    other
        .add_root(
            LanguageDocument::import_new_root(&ROOT.replace("Symbol k", "Symbol z"), "evo:other")
                .unwrap(),
        )
        .unwrap();
    let ghost = only_root(&other);
    assert_ne!(ghost, real);
    let err = graph
        .commit(
            vec![Edge::trunk(ghost, "changeset evo:g:\n".to_owned())],
            Nativization::None,
            None,
        )
        .expect_err("未知 parent");
    assert!(format!("{err}").contains("UNKNOWN_NODE"), "{err}");
}

#[test]
fn a_node_without_parents_is_rejected() {
    let mut graph = graph();
    let err = graph
        .commit(Vec::new(), Nativization::None, None)
        .expect_err("只有 root 沒有 parent");
    assert!(format!("{err}").contains("NO_PARENT"), "{err}");
}

#[test]
fn a_trunk_edge_without_a_changeset_is_rejected() {
    // 沒有 changeset 就沒有「因」,snapshot 無從產生(§2.2 因果契約)。
    let mut graph = graph();
    let err = graph
        .commit(
            vec![Edge::reference(only_root(&graph))],
            Nativization::None,
            None,
        )
        .expect_err("主幹邊必須帶 changeset");
    assert!(
        format!("{err}").contains("TRUNK_WITHOUT_CHANGESET"),
        "{err}"
    );
}

#[test]
fn a_reference_edge_carrying_a_changeset_is_rejected() {
    // P56:多親時只有 parents[0] 帶 changeset,其餘是引用邊。
    let mut graph = graph();
    let (n1, _) = chain(&mut graph);
    let n1_doc = graph.snapshot(&n1).unwrap().clone();
    let err = graph
        .commit(
            vec![
                Edge::trunk(n1.clone(), set_category(&n1_doc, "evo:c", "particle")),
                Edge::trunk(only_root(&graph), set_category(&n1_doc, "evo:d", "adj")),
            ],
            Nativization::None,
            None,
        )
        .expect_err("引用邊不得帶 changeset");
    assert!(
        format!("{err}").contains("REFERENCE_EDGE_WITH_CHANGESET"),
        "{err}"
    );
}

#[test]
fn a_multi_parent_node_records_its_reference_edges() {
    // 多親可宣告且 fsck 成立;但**求值仍只沿主幹**——全 parent 機械合併是 P61,
    // 尚未實作。此測試釘住「現況是什麼」,避免把未實作誤讀為已實作。
    let mut graph = graph();
    let (n1, n2) = chain(&mut graph);
    let n2_doc = graph.snapshot(&n2).unwrap().clone();
    let creole = graph
        .commit(
            vec![
                Edge::trunk(n2, set_category(&n2_doc, "evo:c", "particle")),
                Edge::reference(n1),
            ],
            Nativization::Creole { generation: 1 },
            Some("creole".to_owned()),
        )
        .expect("多親節點合法");
    let node = graph.node(&creole).unwrap();
    assert_eq!(node.parents().len(), 2);
    assert!(node.parents()[1].changeset.is_none());
    assert_eq!(
        category_of(node.snapshot()).as_deref(),
        Some("particle"),
        "P61 未落地:引用邊不影響求值"
    );
    graph.verify_all().unwrap();
}

#[test]
fn a_broken_changeset_leaves_no_node_behind() {
    let mut graph = graph();
    let before = graph.len();
    let err = graph
        .commit(
            vec![Edge::trunk(
                only_root(&graph),
                "changeset evo:bad:\n".to_owned(),
            )],
            Nativization::None,
            None,
        )
        .expect_err("壞 changeset");
    assert!(format!("{err}").contains("CHANGESET_"), "{err}");
    assert_eq!(graph.len(), before, "失敗不得留下半個節點");
}

#[test]
fn a_changeset_written_against_another_base_is_rejected() {
    // digest 的職責之一(P59):防掉包。對 root 寫的 changeset 套不到 n1 上。
    let mut graph = graph();
    let (n1, _) = chain(&mut graph);
    let root_doc = graph.snapshot(&only_root(&graph)).unwrap().clone();
    let err = graph
        .commit(
            vec![Edge::trunk(n1, set_category(&root_doc, "evo:x", "adj"))],
            Nativization::None,
            None,
        )
        .expect_err("base digest 對不上");
    assert!(format!("{err}").contains("BASE_SOURCE_MISMATCH"), "{err}");
}

#[test]
fn reading_an_unknown_node_is_rejected() {
    let graph = graph();
    let mut other = EvolutionGraph::new(LibrarySpec::default());
    other
        .add_root(
            LanguageDocument::import_new_root(&ROOT.replace("Symbol k", "Symbol z"), "evo:other")
                .unwrap(),
        )
        .unwrap();
    assert!(graph.snapshot(&only_root(&other)).is_err());
}

// ── rebase(P57《修補11》§2.3/§3.2)──────────────────────────────────────────

/// 改了 n1 之後,n2 要不要跟過去——這正是前一版**做不到**的事(§0 缺陷:
/// stale 標記完後代,後代必然因 digest 不符而永久失敗)。
#[test]
fn a_clean_rebase_carries_the_edit_onto_the_new_ancestor() {
    let mut graph = graph();
    let (_, n2) = chain(&mut graph);
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();

    // n1' = 從 root 走另一條路(particle 而非 verb)。
    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                set_category(&root_doc, "evo:n1", "particle"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();

    let outcome = graph.rebase(&n2, &n1_prime).expect("rebase 可執行");
    let RebaseOutcome::Clean(n2_prime) = outcome else {
        panic!("預期乾淨 rebase,得到 {outcome:?}");
    };
    assert_eq!(
        category_of(graph.snapshot(&n2_prime).unwrap()).as_deref(),
        Some("aux"),
        "n2 的編輯要套到新祖先上"
    );
    assert_eq!(
        graph.node(&n2_prime).unwrap().trunk().unwrap().from,
        n1_prime,
        "新節點掛在新祖先下"
    );
    graph.verify_all().expect("rebase 產物必須通過 fsck");
}

#[test]
fn a_rebase_leaves_the_original_chain_untouched() {
    // 不可變:rebase 產生**並存**的新鏈,不是搬動舊的。
    let mut graph = graph();
    let (n1, n2) = chain(&mut graph);
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                set_category(&root_doc, "evo:n1", "particle"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let n2_edge_before = graph.node(&n2).unwrap().trunk().unwrap().clone();

    let outcome = graph.rebase(&n2, &n1_prime).unwrap();
    assert!(outcome.is_clean());
    assert_eq!(
        graph.node(&n2).unwrap().trunk().unwrap(),
        &n2_edge_before,
        "舊邊一個位元都不該變"
    );
    assert_eq!(
        graph.node(&n2).unwrap().trunk().unwrap().from,
        n1,
        "舊節點仍掛在舊祖先下"
    );
}

#[test]
fn a_conflicting_rebase_names_the_failing_statement() {
    // **判別性**:n2 有兩句,第 0 句在新 base 上仍成立、第 1 句不成立。若 `ordinal`
    // 寫死成 0 或根本沒傳,這個測試會紅——單句 changeset 測不出來(ordinal 恆為 0,
    // 對錯不分)。這是 P57「`Statement { ordinal }` 免費指出哪一句」的實證。
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let n1 = graph
        .commit(
            vec![Edge::trunk(
                root_id.clone(),
                set_category(&root_doc, "evo:n1", "verb"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let n1_doc = graph.snapshot(&n1).unwrap().clone();
    let n2 = graph
        .commit(
            vec![Edge::trunk(
                n1,
                changeset_for(
                    &n1_doc,
                    "evo:n2",
                    "\n    #0:\n        update sign(\"x\").def[syn.category].value = aux\
                     \n\n    #1:\n        clone sign(\"x\") as w\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();

    // n1' 自己先造了一個 w → n2 的第 1 句變成重名。
    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                changeset_for(
                    &root_doc,
                    "evo:n1",
                    "\n    #0:\n        clone sign(\"x\") as w\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();

    let outcome = graph.rebase(&n2, &n1_prime).expect("rebase 可執行");
    let RebaseOutcome::Conflict { statement, error } = outcome else {
        panic!("預期衝突,得到 {outcome:?}");
    };
    assert_eq!(statement, Some(1), "衝突在第 1 句,不是第 0 句:{error}");
}

#[test]
fn a_missing_target_conflict_names_its_statement() {
    // **rebase 最常見的衝突**:祖先把某個東西刪了/改名了,後代的某一句就失去目標。
    //
    // 這條路徑在 v0.3 之前**說不出是哪一句**——依名字定址找不到目標時,錯誤發生在
    // selector 解析層(`ReplayError::Selector`),比 `Statement` 層更早,不帶 ordinal
    // (《修補11》§3.3 記錄了這個落差)。現由 `StatementSelector` 補上。
    //
    // **判別性**:失敗的是第 1 句而非第 0 句,故「句號寫死成 0」會被抓到。
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let n1 = graph
        .commit(
            vec![Edge::trunk(
                root_id.clone(),
                set_category(&root_doc, "evo:n1", "verb"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let n1_doc = graph.snapshot(&n1).unwrap().clone();
    let n2 = graph
        .commit(
            vec![Edge::trunk(
                n1,
                changeset_for(
                    &n1_doc,
                    "evo:n2",
                    "\n    #0:\n        update sign(\"x\").def[syn.category].value = aux\
                     \n\n    #1:\n        update sign(\"y\").def[syn.category].value = aux\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                changeset_for(
                    &root_doc,
                    "evo:n1",
                    "\n    #0:\n        delete sign(\"y\")\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();

    let outcome = graph.rebase(&n2, &n1_prime).expect("rebase 可執行");
    let RebaseOutcome::Conflict { statement, error } = outcome else {
        panic!("必須分類為衝突,得到 {outcome:?}");
    };
    assert_eq!(
        statement,
        Some(1),
        "失去目標的是第 1 句,不是第 0 句:{error}"
    );
}

#[test]
fn a_conflicting_rebase_creates_no_node() {
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let n1 = graph
        .commit(
            vec![Edge::trunk(
                root_id.clone(),
                set_category(&root_doc, "evo:n1", "verb"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let n1_doc = graph.snapshot(&n1).unwrap().clone();
    let n2 = graph
        .commit(
            vec![Edge::trunk(
                n1,
                changeset_for(
                    &n1_doc,
                    "evo:n2",
                    "\n    #0:\n        update sign(\"y\").def[syn.category].value = aux\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                changeset_for(
                    &root_doc,
                    "evo:n1",
                    "\n    #0:\n        delete sign(\"y\")\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let before = graph.len();
    let outcome = graph.rebase(&n2, &n1_prime).unwrap();
    assert!(!outcome.is_clean(), "{outcome:?}");
    assert_eq!(graph.len(), before, "衝突不得留下半個節點");
    graph.verify_all().unwrap();
}

#[test]
fn the_rebased_edge_is_verbatim_apart_from_the_base_digests() {
    // 保住原文:只有兩行 digest 該變。若改用「解析後重新 dump」,授權糖
    // (`rewrite`/`clone`)會被抹平成原語,使用者的書寫意圖就丟了。
    let mut graph = graph();
    let (_, n2) = chain(&mut graph);
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let n1_prime = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                set_category(&root_doc, "evo:n1", "particle"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let before = graph
        .node(&n2)
        .unwrap()
        .trunk()
        .unwrap()
        .changeset
        .clone()
        .unwrap();

    let RebaseOutcome::Clean(n2_prime) = graph.rebase(&n2, &n1_prime).unwrap() else {
        panic!("預期乾淨 rebase");
    };
    let after = graph
        .node(&n2_prime)
        .unwrap()
        .trunk()
        .unwrap()
        .changeset
        .clone()
        .unwrap();

    let differing: Vec<_> = before
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .map(|(a, _)| a.trim().split(' ').next().unwrap_or("").to_owned())
        .collect();
    assert_eq!(
        differing,
        vec!["base_source", "base_identities"],
        "只有兩行 base digest 該變"
    );
    assert_eq!(before.lines().count(), after.lines().count());
}

#[test]
fn rebasing_an_unknown_node_is_an_error_not_an_outcome() {
    // 圖層面的錯不是 rebase 的三分結果之一,不該被吞成「衝突」。
    let mut graph = graph();
    let (_, n2) = chain(&mut graph);
    let mut other = EvolutionGraph::new(LibrarySpec::default());
    other
        .add_root(
            LanguageDocument::import_new_root(&ROOT.replace("Symbol k", "Symbol z"), "evo:other")
                .unwrap(),
        )
        .unwrap();
    assert!(graph.rebase(&n2, &only_root(&other)).is_err());
    assert!(graph.rebase(&only_root(&other), &n2).is_err());
}

// ── 多 root(《修補11》§12.5;P61 的前置)──────────────────────────────────

/// 第二個起點語言:與第一個**無血緣**。這是 P61「無共同祖先 → 空基準」路徑的
/// 唯一輸入來源;單 root 之下造不出來。
fn second_root() -> LanguageDocument {
    LanguageDocument::import_new_root(
        "Symbol p\n\nsign wolof:\n    syn:\n        category = noun\n",
        "evo:wolof",
    )
    .expect("second root parses")
}

#[test]
fn a_fresh_graph_has_no_roots() {
    let graph = EvolutionGraph::new(LibrarySpec::default());
    assert!(graph.is_empty());
    assert_eq!(graph.roots().count(), 0);
}

#[test]
fn two_unrelated_roots_coexist() {
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    let first = graph.add_root(root()).unwrap();
    let second = graph.add_root(second_root()).unwrap();
    assert_ne!(first, second);
    assert_eq!(graph.roots().count(), 2);
    assert!(graph.node(&first).unwrap().parents().is_empty());
    assert!(graph.node(&second).unwrap().parents().is_empty());
    graph.verify_all().unwrap();
}

#[test]
fn descendants_of_unrelated_roots_share_no_ancestor() {
    // **這是多 root 存在的理由**:兩支的祖先集合不相交,故 P61 的最近共同祖先
    // 不存在 → 基準為空。單 root 之下這個斷言恆假。
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    let a_root = graph.add_root(root()).unwrap();
    let b_root = graph.add_root(second_root()).unwrap();

    let a_doc = graph.snapshot(&a_root).unwrap().clone();
    let a = graph
        .commit(
            vec![Edge::trunk(a_root, set_category(&a_doc, "evo:a1", "verb"))],
            Nativization::None,
            None,
        )
        .unwrap();
    let b_doc = graph.snapshot(&b_root).unwrap().clone();
    let b = graph
        .commit(
            vec![Edge::trunk(
                b_root,
                changeset_for(
                    &b_doc,
                    "evo:b1",
                    "\n    #0:\n        update sign(\"wolof\").def[syn.category].value = verb\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();

    let ancestors = |graph: &EvolutionGraph, start: &NodeId| {
        let mut seen = std::collections::BTreeSet::new();
        let mut frontier = vec![start.clone()];
        while let Some(current) = frontier.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            for edge in graph.node(&current).unwrap().parents() {
                frontier.push(edge.from.clone());
            }
        }
        seen
    };
    let (left, right) = (ancestors(&graph, &a), ancestors(&graph, &b));
    assert!(
        left.is_disjoint(&right),
        "兩支必須毫無交集,否則空基準路徑仍不可達"
    );
    graph.verify_all().unwrap();
}

#[test]
fn a_second_root_reusing_a_namespace_is_rejected() {
    // **靜默毀損的守門員**。namespace 決定穩定 id,而合併以穩定 id 對齊
    // (docs/06 §6.1)。若兩個 root 共用 namespace,一邊的 `evo:root:5` 與另一邊的
    // `evo:root:5` 會被合併器當成同一個 sign 的兩個階段而**默默併掉**——
    // 不報錯、沒有跡象。故在加 root 的當下就擋。
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    graph.add_root(root()).unwrap();
    let clash = LanguageDocument::import_new_root(
        "Symbol p\n\nsign wolof:\n    syn:\n        category = noun\n",
        "evo:root", // ← 與第一個 root 相同
    )
    .unwrap();
    let err = graph.add_root(clash).expect_err("namespace 撞了");
    assert!(
        format!("{err}").contains("DUPLICATE_ROOT_NAMESPACE"),
        "{err}"
    );
    assert_eq!(graph.roots().count(), 1, "失敗不得留下半個 root");
}

#[test]
fn re_adding_the_same_root_is_idempotent() {
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    let first = graph.add_root(root()).unwrap();
    let again = graph.add_root(root()).expect("同一份 root 可重加");
    assert_eq!(first, again);
    assert_eq!(graph.roots().count(), 1);
}

#[test]
fn the_same_source_under_a_different_namespace_is_rejected() {
    // `NodeId` 只雜湊 `.lang` 原文,而 namespace 不在原文裡 → 兩者 id 相同。
    // 若放行,呼叫端指定的 namespace 會被**默默忽略**(冪等路徑回舊節點),
    // 之後才在 `base_identities` 不符時爆掉,錯誤離成因很遠。
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    graph.add_root(root()).unwrap();
    let same_text_other_namespace =
        LanguageDocument::import_new_root(ROOT, "evo:elsewhere").unwrap();
    let err = graph
        .add_root(same_text_other_namespace)
        .expect_err("同原文不同 namespace 必須擋下");
    assert!(format!("{err}").contains("ROOT_IDENTITY_CONFLICT"), "{err}");
}

// ── 合併基準:最近共同祖先(P61 §6.3)───────────────────────────────────────

#[test]
fn the_merge_base_of_two_branches_is_their_fork_point() {
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let fork = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                set_category(&root_doc, "evo:fork", "verb"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let fork_doc = graph.snapshot(&fork).unwrap().clone();
    let left = graph
        .commit(
            vec![Edge::trunk(
                fork.clone(),
                set_category(&fork_doc, "evo:l", "adj"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let right = graph
        .commit(
            vec![Edge::trunk(
                fork.clone(),
                set_category(&fork_doc, "evo:r", "aux"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();

    assert_eq!(
        graph.merge_base(&[left, right]).unwrap(),
        Some(fork),
        "分叉點才是基準,不是 root"
    );
}

#[test]
fn branches_of_unrelated_roots_have_an_empty_merge_base() {
    // 空基準:P61 三態規則因此退化成聯集。多 root 之前這個回傳值不可能出現。
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    let a_root = graph.add_root(root()).unwrap();
    let b_root = graph.add_root(second_root()).unwrap();
    assert_eq!(graph.merge_base(&[a_root, b_root]).unwrap(), None);
}

#[test]
fn a_node_is_its_own_merge_base_with_a_descendant() {
    let mut graph = graph();
    let (n1, n2) = chain(&mut graph);
    assert_eq!(
        graph.merge_base(&[n1.clone(), n2]).unwrap(),
        Some(n1),
        "祖先與後代的最近共同祖先就是祖先自己"
    );
}

#[test]
fn several_lowest_common_ancestors_are_reported_not_guessed() {
    // 兩個 root 各自被兩個融合節點共用 ⇒ 共同祖先有兩個且互不為祖先。
    // 挑錯基準會讓整份合併悄悄偏掉,故 §12.6 定為報錯要求人指定,**不自行猜**。
    let mut graph = EvolutionGraph::new(LibrarySpec::default());
    let a_root = graph.add_root(root()).unwrap();
    let b_root = graph.add_root(second_root()).unwrap();
    let a_doc = graph.snapshot(&a_root).unwrap().clone();

    let merge_one = graph
        .commit(
            vec![
                Edge::trunk(a_root.clone(), set_category(&a_doc, "evo:m1", "verb")),
                Edge::reference(b_root.clone()),
            ],
            Nativization::None,
            None,
        )
        .unwrap();
    let merge_two = graph
        .commit(
            vec![
                Edge::trunk(a_root, set_category(&a_doc, "evo:m2", "adj")),
                Edge::reference(b_root),
            ],
            Nativization::None,
            None,
        )
        .unwrap();

    let err = graph
        .merge_base(&[merge_one, merge_two])
        .expect_err("兩個候選必須報錯");
    assert!(format!("{err}").contains("AMBIGUOUS_MERGE_BASE"), "{err}");
}

#[test]
fn a_merge_plan_uses_the_fork_point_as_its_base() {
    // 端到端:圖 → LCA → 三態計畫。左支改 x、右支改 y ⇒ 各自單邊改動 ⇒ 乾淨。
    let mut graph = graph();
    let root_id = only_root(&graph);
    let root_doc = graph.snapshot(&root_id).unwrap().clone();
    let fork = graph
        .commit(
            vec![Edge::trunk(
                root_id,
                set_category(&root_doc, "evo:fork", "verb"),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let fork_doc = graph.snapshot(&fork).unwrap().clone();
    let left = graph
        .commit(
            vec![Edge::trunk(
                fork.clone(),
                changeset_for(
                    &fork_doc,
                    "evo:l",
                    "\n    #0:\n        update sign(\"x\").def[syn.category].value = adj\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();
    let right = graph
        .commit(
            vec![Edge::trunk(
                fork,
                changeset_for(
                    &fork_doc,
                    "evo:r",
                    "\n    #0:\n        update sign(\"y\").def[syn.category].value = aux\n",
                ),
            )],
            Nativization::None,
            None,
        )
        .unwrap();

    let plan = graph.merge_plan(&[left, right]).expect("計畫算得出來");
    assert!(plan.is_clean(), "各改各的,不該衝突:{:?}", plan.conflicts);
    assert_eq!(plan.signs.len(), 2, "x 與 y 都要在計畫裡");
}
