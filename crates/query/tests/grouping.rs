//! 步驟 21-4 出口:互通度接口 + `TreeEdgeCut` 分群。
//!
//! 釘住四件事:
//!
//! 1. 分數**帶得出是誰算的**(`measure_id`),不是裸 `f64`;
//! 2. 分群沿**主幹邊**切——引用邊(donor / 合併)不算世系鄰接;
//! 3. Override 是**分類指派**,不是 merge/split,故不可能衝突;
//! 4. `labels` 只改顯示,不改身分。

use conlang_changeset::evolution::{Edge, EvolutionGraph, Nativization, NodeId};
use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec};
use conlang_query::{
    intelligibility, periods, ExploratoryHeuristicV1, Grouping, GroupingOverride, TreeEdgeCut,
};

const BASE: &str = "Symbol k\nSymbol a\nSymbol t\nSymbol u\nSymbol s\n\nClass vowel {a, u}\n\n\
global trait Core:\n\n\
sign one:\n    belongs Noun\n    phon:\n        /kat/\n\
sign two:\n    belongs Noun\n    phon:\n        /tak/\n\
sign three:\n    belongs Noun\n    phon:\n        /kut/\n\
sign four:\n    belongs Noun\n    phon:\n        /tuk/\n";

fn measure() -> ExploratoryHeuristicV1 {
    ExploratoryHeuristicV1::suggested()
}

/// 建一條鏈:root → child,child 的 changeset 由 `edits` 給。
fn chain(steps: &[&str]) -> (EvolutionGraph, Vec<NodeId>) {
    let spec = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(spec.clone());
    let root = graph
        .add_root(LanguageDocument::import_new_root(BASE, "grp:root").expect("root"))
        .expect("add_root");
    let mut ids = vec![root.clone()];

    for (index, body) in steps.iter().enumerate() {
        let parent = ids.last().expect("有前一個").clone();
        let base = graph.snapshot(&parent).expect("snapshot").clone();
        let namespace = format!("grp:{index}");
        let mut text = change_set_prelude(&base, &spec, &namespace).expect("prelude");
        text.push_str(body);
        // prelude + body 直接當作邊上的 changeset 原文
        UnresolvedChangeSet::parse(&text).expect("changeset parses");
        let id = graph
            .commit(
                vec![Edge::trunk(parent, text)],
                Nativization::None,
                Some(namespace),
            )
            .expect("commit");
        ids.push(id);
    }
    (graph, ids)
}

fn apply_once(source: &str, body: &str) -> (LanguageDocument, LanguageDocument) {
    let spec = LibrarySpec::default();
    let before = LanguageDocument::import_new_root(source, "grp:event-reach").expect("root");
    let mut text = change_set_prelude(&before, &spec, "grp:event-reach-child").expect("prelude");
    text.push_str(body);
    let resolved = UnresolvedChangeSet::parse(&text)
        .expect("changeset parses")
        .resolve(&before, &spec)
        .expect("changeset resolves");
    let after = ChangeInterpreter::new(before.clone(), spec, "grp:event-reach-child".to_owned())
        .expect("interpreter")
        .run(&resolved)
        .expect("changeset runs")
        .document;
    (before, after)
}

/// 砍掉四分之三的詞彙 —— 差異很大,但留 `one` 供後續步驟改。
const WIPE: &str =
    "\n    #0:\n        delete sign(\"two\")\n        delete sign(\"three\")\n        \
delete sign(\"four\")\n";

/// 只改一個詞的底層形 —— 差異很小。
const TWEAK: &str = "\n    #0:\n        update sign(\"one\").def[phon].value = /kats/\n";

/// 在 `global trait Core` 底下加 `count` 條音變 —— 一條都不碰任何 sign。
///
/// 舊實作在這種輸入上回 `1.0`(`分層差異向量 v0.2 裁定` §0 的實測病灶):
/// diff 只走 `signs`,而音變一個 sign 都沒改。
///
/// 用**具名規則**(`shiftN: …`)而非單一 `rules:` 區塊:後者不論裝幾條語句
/// 都是**一個** `Rule` 節點(P46 結構化 block,語句住 `phon_block`),依 `RuleId`
/// 對齊時只算一個事件。要造出 `count` 個規則事件就得有 `count` 個規則節點。
fn sound_changes(count: usize) -> String {
    let mut body = String::from(
        "\n    #0:\n        insert into trait(\"Core\").block[0] at end:\n            phon:\n",
    );
    for index in 0..count {
        body.push_str(&format!(
            "                shift{index}: a => u{index} / _#\n"
        ));
    }
    body
}

// ── ① 分數帶得出來源 ─────────────────────────────────────────────────────

/// 🔑 **裸 `f64` 不合格**——結果必須說得出是哪套模型算的。
#[test]
fn a_score_always_carries_the_measure_that_produced_it() {
    let (graph, ids) = chain(&[TWEAK]);
    let score = intelligibility(
        graph.snapshot(&ids[0]).expect("a"),
        graph.snapshot(&ids[1]).expect("b"),
        &measure(),
    );
    assert_eq!(score.measure_id, "exploratory_heuristic_v1");
    assert!(score.symmetric, "【M】對稱版");
    assert!(score.value > 0.0 && score.value < 1.0, "{}", score.value);
}

/// 相同文件得 1.0;砍光詞彙的差得明顯較低。
///
/// 判別性:若公式忽略分量(恆回同一個數),這兩個斷言不會同時成立。
#[test]
fn identical_documents_score_one_and_a_wiped_lexicon_scores_much_lower() {
    let (graph, ids) = chain(&[TWEAK, WIPE]);
    let root = graph.snapshot(&ids[0]).expect("root");

    assert_eq!(
        intelligibility(root, root, &measure()).value,
        1.0,
        "自己與自己"
    );

    let tweaked = intelligibility(root, graph.snapshot(&ids[1]).expect("b"), &measure()).value;
    let wiped = intelligibility(root, graph.snapshot(&ids[2]).expect("c"), &measure()).value;
    assert!(
        wiped < tweaked,
        "砍光詞彙應遠低於只改一個音:{wiped} vs {tweaked}"
    );
    assert!(tweaked > 0.8, "只改一個詞仍應高度相通:{tweaked}");
}

/// 對稱:`intelligibility(a,b)` == `intelligibility(b,a)`(§6.2【M】)。
#[test]
fn the_symmetric_measure_is_actually_symmetric() {
    let (graph, ids) = chain(&[WIPE]);
    let (a, b) = (
        graph.snapshot(&ids[0]).expect("a"),
        graph.snapshot(&ids[1]).expect("b"),
    );
    assert_eq!(
        intelligibility(a, b, &measure()).value,
        intelligibility(b, a, &measure()).value
    );
}

/// 權重是**建構參數**,不是藏起來的常數——換掉會改變結果。
///
/// 判別性:若 `score` 忽略 `self.weights`,兩個分數會相同。
#[test]
fn the_weights_are_data_and_changing_them_changes_the_answer() {
    use conlang_query::DimensionWeights;
    let (graph, ids) = chain(&[TWEAK]);
    let (a, b) = (
        graph.snapshot(&ids[0]).expect("a"),
        graph.snapshot(&ids[1]).expect("b"),
    );

    let suggested = intelligibility(a, b, &measure()).value;
    let phon_only = ExploratoryHeuristicV1 {
        weights: DimensionWeights {
            phon: 100.0,
            syn: 0.0,
            sem: 0.0,
            prag: 0.0,
            structural: 0.0,
            birth_death: 0.0,
            trait_rule: 0.25,
            trait_content: 0.5,
        },
    };
    let heavy = intelligibility(a, b, &phon_only).value;
    assert_ne!(suggested, heavy, "換權重必須改變結果");
}

// ── ② 沿主幹邊切 ────────────────────────────────────────────────────────

fn group_of<'a>(grouping: &'a Grouping, id: &NodeId) -> &'a str {
    grouping.members.get(id.as_str()).expect("每個節點都有群")
}

/// 高互通度的鄰接點同群;切一刀就分家。
#[test]
fn a_low_intelligibility_edge_splits_the_chain_into_two_groups() {
    let (graph, ids) = chain(&[TWEAK, WIPE, TWEAK]);
    let strategy = TreeEdgeCut { threshold: 0.6 };
    let grouping = periods(&graph, &strategy, &measure(), &GroupingOverride::default());

    // root 與第一步同群(只改一個音)
    assert_eq!(group_of(&grouping, &ids[0]), group_of(&grouping, &ids[1]));
    // 第二步砍光詞彙 ⇒ 切斷
    assert_ne!(group_of(&grouping, &ids[1]), group_of(&grouping, &ids[2]));
    // 第三步又只改一點 ⇒ 與第二步同群
    assert_eq!(group_of(&grouping, &ids[2]), group_of(&grouping, &ids[3]));

    assert_eq!(grouping.groups().len(), 2);
    assert_eq!(grouping.measure_id, "exploratory_heuristic_v1");
    assert_eq!(grouping.threshold, 0.6);
}

/// 閾值調高 ⇒ 切得更碎;調到 0 ⇒ 全部一群。
///
/// 判別性:若閾值沒被使用,三種設定會給出相同分群。
#[test]
fn the_threshold_actually_controls_how_finely_the_tree_is_cut() {
    let (graph, ids) = chain(&[TWEAK, WIPE, TWEAK]);
    let count = |t: f64| {
        periods(
            &graph,
            &TreeEdgeCut { threshold: t },
            &measure(),
            &GroupingOverride::default(),
        )
        .groups()
        .len()
    };
    assert_eq!(count(0.0), 1, "門檻為 0:全連通");
    assert_eq!(count(0.6), 2);
    assert_eq!(count(1.1), ids.len(), "門檻高過 1:每個節點自成一群");
}

/// 🔑 **引用邊不算世系鄰接。**
///
/// 構造要讓兩條邊**指向不同答案**,否則測不出來:
///
/// ```text
/// root ──WIPE──> wiped          互通度低 ⇒ 這一刀切開
///  │                ↑
///  └─trunk─> merged ─reference──┘
/// ```
///
/// `merged` 的**主幹**父是 `root`(內容差很多 ⇒ 該切斷),**引用**父是 `wiped`
/// (內容幾乎相同 ⇒ 若被當成候選邊就會併群)。
///
/// 正解:三個群。把引用邊也算進去:兩個群(`merged` 被併進 `wiped`)。
#[test]
fn reference_edges_do_not_count_as_genealogical_adjacency() {
    let spec = LibrarySpec::default();
    let (mut graph, ids) = chain(&[WIPE]);
    let (root, wiped) = (ids[0].clone(), ids[1].clone());

    // 多親時 changeset 的基底是合併後的文件(P61)
    let base = graph
        .merged_base(&[root.clone(), wiped.clone()])
        .expect("merged base");
    let text = change_set_prelude(&base, &spec, "grp:ref").expect("prelude");
    let merged = graph
        .commit(
            vec![
                Edge::trunk(root.clone(), text),
                Edge::reference(wiped.clone()),
            ],
            Nativization::None,
            Some("grp:ref".to_owned()),
        )
        .expect("commit");

    // 前提一:引用邊確實在,且指向 wiped
    let parents = graph.node(&merged).expect("在").parents();
    assert_eq!(parents.len(), 2);
    assert_eq!(&parents[0].from, &root, "主幹邊指 root");
    assert_eq!(&parents[1].from, &wiped, "引用邊指 wiped");

    let m = measure();
    // 前提二:兩條邊的互通度**落在閾值兩側**,否則本測試無判別性
    let across_trunk = intelligibility(
        graph.snapshot(&root).expect("root"),
        graph.snapshot(&merged).expect("merged"),
        &m,
    )
    .value;
    let across_reference = intelligibility(
        graph.snapshot(&wiped).expect("wiped"),
        graph.snapshot(&merged).expect("merged"),
        &m,
    )
    .value;
    assert!(
        across_trunk < 0.6 && across_reference >= 0.6,
        "構造前提:主幹低、引用高 —— {across_trunk} / {across_reference}"
    );

    let grouping = periods(
        &graph,
        &TreeEdgeCut { threshold: 0.6 },
        &m,
        &GroupingOverride::default(),
    );

    assert_ne!(
        group_of(&grouping, &merged),
        group_of(&grouping, &wiped),
        "引用邊不得把 merged 併進 wiped 那群"
    );
    assert_ne!(group_of(&grouping, &merged), group_of(&grouping, &root));
    assert_eq!(grouping.groups().len(), 3, "三個節點三個群");
}

/// 決定性:同輸入兩次逐欄位相同。
#[test]
fn the_same_input_yields_the_same_grouping() {
    let (graph, _ids) = chain(&[TWEAK, WIPE]);
    let strategy = TreeEdgeCut { threshold: 0.6 };
    assert_eq!(
        periods(&graph, &strategy, &measure(), &GroupingOverride::default()),
        periods(&graph, &strategy, &measure(), &GroupingOverride::default())
    );
}

// ── ③④ Override 是分類指派 ──────────────────────────────────────────────

/// 🔑 **指派覆寫算出來的群;而且不可能衝突。**
///
/// merge/split 語意下,`A+B`、`B+C`、`A|C` 同時存在會無解;指派是函數,
/// 一個節點恰一個群,型別上就矛盾不起來。這裡用「把切開的兩支硬併成一群」
/// ——政治認可視角——來示範。
#[test]
fn an_assignment_override_wins_over_the_computed_group() {
    let (graph, ids) = chain(&[TWEAK, WIPE, TWEAK]);
    let strategy = TreeEdgeCut { threshold: 0.6 };
    let computed = periods(&graph, &strategy, &measure(), &GroupingOverride::default());
    assert_eq!(computed.groups().len(), 2, "前提:語言學上是兩群");

    let political = GroupingOverride {
        assignments: [ids[2].as_str(), ids[3].as_str()]
            .into_iter()
            .map(|node| (node.to_owned(), group_of(&computed, &ids[0]).to_owned()))
            .collect(),
        ..GroupingOverride::default()
    };
    let overridden = periods(&graph, &strategy, &measure(), &political);
    assert_eq!(overridden.groups().len(), 1, "政治視角:全部同一個語言");

    // 而**同一張圖**的語言學視角不受影響——R4 一套一檔的用意
    assert_eq!(
        periods(&graph, &strategy, &measure(), &GroupingOverride::default()),
        computed
    );
}

/// 指派指向不存在的節點**靜靜略過**——view 檔可能比圖舊。
#[test]
fn an_assignment_for_an_unknown_node_is_skipped() {
    let (graph, _ids) = chain(&[TWEAK]);
    let stale = GroupingOverride {
        assignments: [("no-such-node".to_owned(), "somewhere".to_owned())]
            .into_iter()
            .collect(),
        ..GroupingOverride::default()
    };
    let grouping = periods(&graph, &TreeEdgeCut { threshold: 0.6 }, &measure(), &stale);
    assert!(!grouping.members.contains_key("no-such-node"));
    assert_eq!(grouping.groups().len(), 1, "其餘照常");
}

/// 🔑 **`labels` 只改顯示,不改身分。**
///
/// 判別性:加標籤前後 `members` 必須逐欄位相同。若哪天標籤被拿去當群組 id,
/// 這裡會紅。
#[test]
fn labels_change_the_display_but_never_the_membership() {
    let (graph, ids) = chain(&[TWEAK, WIPE]);
    let strategy = TreeEdgeCut { threshold: 0.6 };
    let plain = periods(&graph, &strategy, &measure(), &GroupingOverride::default());

    let named = GroupingOverride {
        labels: [(group_of(&plain, &ids[0]).to_owned(), "古語群".to_owned())]
            .into_iter()
            .collect(),
        ..GroupingOverride::default()
    };
    let labelled = periods(&graph, &strategy, &measure(), &named);

    assert_eq!(labelled.members, plain.members, "身分逐欄位不變");
    assert_eq!(labelled.labels.len(), 1);
    assert_eq!(
        labelled
            .labels
            .get(group_of(&plain, &ids[0]))
            .map(String::as_str),
        Some("古語群")
    );
}

/// 指向不存在群組的標籤不會列出來——免得 UI 顯示空群。
#[test]
fn a_label_for_a_group_that_does_not_exist_is_dropped() {
    let (graph, _ids) = chain(&[TWEAK]);
    let ghost = GroupingOverride {
        labels: [("no-such-group".to_owned(), "幽靈".to_owned())]
            .into_iter()
            .collect(),
        ..GroupingOverride::default()
    };
    let grouping = periods(&graph, &TreeEdgeCut { threshold: 0.6 }, &measure(), &ghost);
    assert!(grouping.labels.is_empty());
}

/// 🔑 **群組 id = 該群成員中字典序最小的節點 id。**
///
/// 這不是實作細節而是**契約**:`views/<name>.json` 的 `assignments` 以群組 id
/// 為值,存了檔之後再開,id 必須指得回同一群。若代表改由 union 的呼叫順序決定,
/// 換一個遍歷順序就會讓所有存檔的指派失效。
///
/// (M5 的教訓:分割本身與代表怎麼選無關,所以只比「同群/不同群」的測試
/// 抓不到這件事——得直接比 id 的值。)
#[test]
fn a_group_id_is_the_smallest_node_id_among_its_members() {
    let (graph, _ids) = chain(&[TWEAK, WIPE, TWEAK]);
    let grouping = periods(
        &graph,
        &TreeEdgeCut { threshold: 0.6 },
        &measure(),
        &GroupingOverride::default(),
    );

    for group in grouping.groups() {
        let members = grouping.members_of(group);
        assert!(!members.is_empty());
        let smallest = members.iter().min().expect("非空");
        assert_eq!(
            &group, smallest,
            "群組 id 必須是最小的成員 id;成員 = {members:?}"
        );
    }
    assert_eq!(grouping.groups().len(), 2, "前提:確實有兩群可比");
}

// ── ⑤ 裁定 §4 的重新校準 ─────────────────────────────────────────────────────

/// 🔑 **一次音變不得再回 `1.0`**——這是《分層差異向量 v0.2 裁定》§0 的病灶。
///
/// 舊實作的 diff 只走 `signs`,而音變一個 sign 都沒改,於是兩節點的互通度是
/// 滿分、方言分群完全看不見它。這條測試是那個 bug 的迴歸鎖。
#[test]
fn a_sound_change_lowers_intelligibility_at_all() {
    let (graph, ids) = chain(&[&sound_changes(1)]);
    let score = intelligibility(
        graph.snapshot(&ids[0]).expect("a"),
        graph.snapshot(&ids[1]).expect("b"),
        &measure(),
    )
    .value;
    assert!(score < 1.0, "一條音變必須看得見:{score}");
}

/// 🔑 裁定 §4 的**硬約束**:一條音變不得讓互通度掉到預設閾值(0.6)以下。
///
/// > 若加一條音變就讓互通度掉到預設閾值以下,則每演化一步就分裂一次方言群,
/// > 分群功能等於報廢。
///
/// 這條測試釘的是**餘裕本身**,不只是「有沒有低於」:係數若被調到讓一條音變
/// 逼近閾值,分群就會變成一步一裂,而那種退化在功能測試裡看不出來——它只會
/// 表現為「方言樹莫名其妙很碎」。
#[test]
fn one_sound_change_stays_far_above_the_default_grouping_threshold() {
    let (graph, ids) = chain(&[&sound_changes(1)]);
    let score = intelligibility(
        graph.snapshot(&ids[0]).expect("a"),
        graph.snapshot(&ids[1]).expect("b"),
        &measure(),
    )
    .value;
    assert!(score >= 0.9, "一條音變要留大量餘裕,實得 {score}");
}

/// 分群層面的同一件事:一步一條音變的演化鏈**不會自己裂開**。
#[test]
fn a_chain_of_single_sound_changes_stays_one_dialect() {
    let (graph, ids) = chain(&[&sound_changes(1), &sound_changes(1), &sound_changes(1)]);
    let grouping = periods(
        &graph,
        &TreeEdgeCut { threshold: 0.6 },
        &measure(),
        &GroupingOverride::default(),
    );
    assert_eq!(grouping.groups().len(), 1, "三步各一條音變不該裂成方言");
    assert_eq!(grouping.members.len(), ids.len());
}

/// 但**一次改一大批**音變仍要判成方言分化——否則規則性音變等於沒進公式。
///
/// 判別性:前一條測試單獨看是「係數調到 0 也會過」;這一條把另一邊釘住。
#[test]
fn a_large_batch_of_sound_changes_does_split_a_dialect() {
    let (graph, _) = chain(&[&sound_changes(12)]);
    let grouping = periods(
        &graph,
        &TreeEdgeCut { threshold: 0.6 },
        &measure(),
        &GroupingOverride::default(),
    );
    assert_eq!(grouping.groups().len(), 2, "一次十二條音變應切開");
}

/// **餘裕的邊界釘在哪裡**:同一個 changeset 裡 8 條音變還在同群,9 條切開。
///
/// 這條測試的用途不是宣稱「9 是對的」——那是可被反駁的主張——而是讓**任何
/// 改動係數的人立刻看見邊界移到哪裡**。裁定 §4 的約束是「一條音變不得切開」,
/// 而係數若被悄悄調重,先崩的不是那條測試,是這裡。
#[test]
fn the_calibration_margin_sits_between_eight_and_nine_sound_changes() {
    let groups = |count: usize| {
        let (graph, _) = chain(&[&sound_changes(count)]);
        periods(
            &graph,
            &TreeEdgeCut { threshold: 0.6 },
            &measure(),
            &GroupingOverride::default(),
        )
        .groups()
        .len()
    };
    assert_eq!(groups(8), 1, "8 條仍在同一個方言群");
    assert_eq!(groups(9), 2, "9 條切開");
}

#[test]
fn disjoint_local_events_do_not_form_a_cartesian_product_with_union_reach() {
    let mut source = String::from("Symbol a\nSymbol b\n\n");
    for index in 0..9 {
        source.push_str(&format!(
            "trait Local{index}:\n\nsign word{index}:\n    belongs Local{index}\n    phon:\n        /a/\n\n"
        ));
    }
    let mut changes = String::from("\n    #0:\n");
    for index in 0..9 {
        changes.push_str(&format!(
            "        insert into trait(\"Local{index}\").block[0] at end:\n            phon:\n                shift{index}: a => b / _#\n"
        ));
    }
    let (before, after) = apply_once(&source, &changes);
    let local = intelligibility(&before, &after, &measure()).value;

    let (global_graph, global_ids) = chain(&[&sound_changes(9)]);
    let global = intelligibility(
        global_graph.snapshot(&global_ids[0]).expect("root"),
        global_graph.snapshot(&global_ids[1]).expect("child"),
        &measure(),
    )
    .value;

    assert!((local - 0.952_380_952_380_952_3).abs() < 1e-12, "{local}");
    assert!((global - 0.571_428_571_428_571_4).abs() < 1e-12, "{global}");
    assert!(local > global, "disjoint local events must cost less");
}

/// **trait 的生滅只算一次**——不因為它在幾個維度上有內容而被記幾次。
///
/// `trait_content` 的 `only_before`/`only_after` 是 **trait 集合**的性質,五個
/// leaf 的數字必然相同(同 `aligned_signs` 的道理)。公式若逐維把它加進去,
/// 一個帶 sem 內容的新 trait 就會被記兩次(sem 一次、structural 一次),
/// 帶四維內容的新 trait 記五次——「新增一個 trait」的傷害於是取決於它**碰巧
/// 寫了幾個維度的內容**,而不是它影響了幾個詞。
///
/// 判別方式刻意不綁係數:同一個編輯,新 trait 帶不帶 sem 內容,分數必須相同。
#[test]
fn a_new_trait_is_charged_once_no_matter_how_many_dimensions_it_carries() {
    let bare = "\n    #0:\n        insert into language at end:\n            trait Fancy:\n\
                \n    #1:\n        insert into sign(\"one\") at end:\n            belongs Fancy\n";
    let with_content = "\n    #0:\n        insert into language at end:\n            trait Fancy:\n                sem:\n                    senses:\n                        core = FANCY\n\
                \n    #1:\n        insert into sign(\"one\") at end:\n            belongs Fancy\n";

    let score = |body: &str| {
        let (graph, ids) = chain(&[body]);
        intelligibility(
            graph.snapshot(&ids[0]).expect("a"),
            graph.snapshot(&ids[1]).expect("b"),
            &measure(),
        )
        .value
    };
    let (bare, rich) = (score(bare), score(with_content));

    assert!(bare < 1.0, "前提:這個編輯本來就該壓低分數,實得 {bare}");
    assert_eq!(bare, rich, "新 trait 帶幾個維度的內容,不該改變它被記幾次");
}

// ── ⑥ 群的語意:主幹邊上的連通分量 ────────────────────────────────────────
//
// 這一段是**特性測試**(characterization),不是願望清單:它釘住現行語意
// 「群 = 主幹邊的連通分量」的兩個可觀察後果,好讓任何人改動分群策略時,
// 立刻看見自己改掉了什麼。兩條測試斷言的都**不是理想行為**。

/// 建一組兄弟:同一個 root 之下 `bodies.len()` 個子節點。
fn siblings(bodies: &[&str]) -> (EvolutionGraph, NodeId, Vec<NodeId>) {
    let spec = LibrarySpec::default();
    let mut graph = EvolutionGraph::new(spec.clone());
    let root = graph
        .add_root(LanguageDocument::import_new_root(BASE, "sib:root").expect("root"))
        .expect("add_root");
    let mut kids = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        let namespace = format!("sib:{index}");
        let base = graph.snapshot(&root).expect("snapshot").clone();
        let mut text = change_set_prelude(&base, &spec, &namespace).expect("prelude");
        text.push_str(body);
        kids.push(
            graph
                .commit(
                    vec![Edge::trunk(root.clone(), text)],
                    Nativization::None,
                    Some(namespace),
                )
                .expect("commit"),
        );
    }
    (graph, root, kids)
}

/// **兄弟從來沒有被比較過。** 兩個做了完全相同改動的兄弟,各自對父的邊都被
/// 切斷 ⇒ 三個群,即使它們彼此行為一模一樣。
///
/// 這條測試同時記下第二層原因:兄弟之間的互通度本身就低報——平行創新
/// (兩邊各加一條字面相同的規則)因 `RuleId` 不同而被算成一生一滅。
#[test]
fn siblings_are_never_compared_so_identical_branches_can_land_in_different_groups() {
    let change = sound_changes(9);
    let (graph, root, kids) = siblings(&[&change, &change]);
    let (a, b) = (&kids[0], &kids[1]);

    // 前提:兩條分支的 changeset 逐字相同
    let to_parent = intelligibility(
        graph.snapshot(&root).expect("root"),
        graph.snapshot(a).expect("a"),
        &measure(),
    )
    .value;
    let between = intelligibility(
        graph.snapshot(a).expect("a"),
        graph.snapshot(b).expect("b"),
        &measure(),
    )
    .value;

    assert!(to_parent < 0.6, "前提:各自都離父夠遠,邊會被切:{to_parent}");
    assert!(
        between < to_parent,
        "平行創新讓兄弟彼此比對父還遠(id 對齊的後果):{between} vs {to_parent}"
    );

    let grouping = periods(
        &graph,
        &TreeEdgeCut { threshold: 0.6 },
        &measure(),
        &GroupingOverride::default(),
    );
    assert_eq!(
        grouping.groups().len(),
        3,
        "現行語意:兩條邊各自斷開 ⇒ root / a / b 三個群"
    );
}

/// **群內不保證兩兩互通。** 兩個兄弟各自對父過閾值 ⇒ 同群,即使它們彼此
/// 低於閾值。
#[test]
fn a_group_does_not_promise_that_every_pair_inside_it_is_intelligible() {
    // 兩條分支各改不同的詞:對父都還很像,彼此的差異則是兩者相加
    let left = "\n    #0:\n        update sign(\"one\").def[phon].value = /kats/\n";
    let right = "\n    #0:\n        update sign(\"two\").def[phon].value = /taks/\n";
    let (graph, root, kids) = siblings(&[left, right]);

    let to_parent = intelligibility(
        graph.snapshot(&root).expect("root"),
        graph.snapshot(&kids[0]).expect("a"),
        &measure(),
    )
    .value;
    let between = intelligibility(
        graph.snapshot(&kids[0]).expect("a"),
        graph.snapshot(&kids[1]).expect("b"),
        &measure(),
    )
    .value;
    assert!(
        between < to_parent,
        "兄弟之間必然比各自對父遠(差異相加):{between} vs {to_parent}"
    );

    // 閾值卡在兩者之間:邊都留著,但兄弟彼此其實不到標準
    let threshold = (between + to_parent) / 2.0;
    let grouping = periods(
        &graph,
        &TreeEdgeCut { threshold },
        &measure(),
        &GroupingOverride::default(),
    );
    assert_eq!(
        grouping.groups().len(),
        1,
        "三點同群,但 a 與 b 的互通度 {between} 低於閾值 {threshold}"
    );
}

// ── ⑦ 年代切片與共時方言群 ──────────────────────────────────────────────────
//
// 分期(`periods`,沿主幹邊)與方言群(`dialect_groups`,切片內兩兩)是**兩個
// 問題**。這一段守住它們真的分開了:同一張圖上兩者給不同答案。

/// 🔑 **純鏈狀專案的方言群恆為一群**——任何時刻只有一個語言活著。
///
/// 同一張圖上 `periods` 會切出兩期(古/中/現代那種)。兩個函式在同一份輸入上
/// 給不同答案,正是「分開」的可觀察內容:若 `dialect_groups` 只是換名字包一層,
/// 這條測試會跟著 `periods` 一起變成 2。
#[test]
fn a_pure_chain_has_one_contemporary_language_however_many_periods_it_has() {
    use conlang_query::{dialect_groups, Slice};
    let (graph, ids) = chain(&[TWEAK, WIPE, TWEAK]);

    let slice = Slice::contemporary(&graph);
    assert_eq!(slice.len(), 1, "鏈的葉只有一個");
    assert_eq!(slice.nodes()[0], *ids.last().expect("最後一個"));

    let dialects = dialect_groups(
        &graph,
        &slice,
        0.6,
        &measure(),
        &GroupingOverride::default(),
    );
    assert_eq!(dialects.groups().len(), 1, "只有一個語言,只能有一群");
    assert_eq!(dialects.members.len(), 1, "結果只涵蓋切片成員");

    // 對照:同一張圖的分期切成兩段
    let stages = periods(
        &graph,
        &TreeEdgeCut { threshold: 0.6 },
        &measure(),
        &GroupingOverride::default(),
    );
    assert_eq!(stages.groups().len(), 2, "歷時分期仍是兩期");
    assert_eq!(stages.members.len(), ids.len(), "分期涵蓋全部節點");
}

/// 切片是**反鏈**:母語言與子語言不能放進同一個切片。
#[test]
fn a_slice_rejects_a_parent_and_its_child() {
    use conlang_query::{Slice, SliceError};
    let (graph, ids) = chain(&[TWEAK]);
    let error = Slice::new(&graph, [ids[0].clone(), ids[1].clone()]).expect_err("該被拒");
    assert_eq!(
        error,
        SliceError::NotAnAntichain {
            ancestor: ids[0].clone(),
            descendant: ids[1].clone(),
        }
    );
    // 祖孫也一樣(不只直接父子)
    let (graph, ids) = chain(&[TWEAK, TWEAK]);
    assert!(
        Slice::new(&graph, [ids[0].clone(), ids[2].clone()]).is_err(),
        "隔一代照樣不行"
    );
    // 各自單獨則合法
    assert!(Slice::new(&graph, [ids[2].clone()]).is_ok());
}

/// 兄弟是合法切片,而且方言群**真的兩兩比**——兩個兄弟同群不必經過父節點。
#[test]
fn siblings_form_a_slice_and_are_compared_directly() {
    use conlang_query::{dialect_groups, Slice};
    let left = "\n    #0:\n        update sign(\"one\").def[phon].value = /kats/\n";
    let right = "\n    #0:\n        update sign(\"two\").def[phon].value = /taks/\n";
    let (graph, root, kids) = siblings(&[left, right]);

    let slice = Slice::contemporary(&graph);
    assert_eq!(slice.len(), 2, "兩個葉");
    assert!(!slice.nodes().contains(&root), "祖先不在切片裡");

    let between = intelligibility(
        graph.snapshot(&kids[0]).expect("a"),
        graph.snapshot(&kids[1]).expect("b"),
        &measure(),
    )
    .value;

    // 閾值放在兩兄弟的互通度之下 ⇒ 同群;之上 ⇒ 分開。若沒有真的兩兩比,
    // 這兩個答案不會不同。
    let groups = |threshold: f64| {
        dialect_groups(
            &graph,
            &slice,
            threshold,
            &measure(),
            &GroupingOverride::default(),
        )
        .groups()
        .len()
    };
    assert_eq!(groups(between - 0.01), 1, "門檻在其下 ⇒ 同群");
    assert_eq!(groups(between + 0.01), 2, "門檻在其上 ⇒ 兩群");
}

/// 引用邊**不算**祖裔:克里奧爾與它的來源語可以同時存在。
#[test]
fn a_reference_parent_does_not_make_two_languages_non_contemporary() {
    use conlang_query::Slice;
    let spec = LibrarySpec::default();
    let (mut graph, ids) = chain(&[TWEAK]);
    let (root, tweaked) = (ids[0].clone(), ids[1].clone());

    // creole:主幹父是 root,引用父是 tweaked。
    // 多親時 changeset 的基底是合併後的文件(P61)
    let base = graph
        .merged_base(&[root.clone(), tweaked.clone()])
        .expect("merged base");
    let mut text = change_set_prelude(&base, &spec, "sib:creole").expect("prelude");
    text.push_str("\n    #0:\n        update sign(\"three\").def[phon].value = /kuts/\n");
    let creole = graph
        .commit(
            vec![
                Edge::trunk(root.clone(), text),
                Edge::reference(tweaked.clone()),
            ],
            Nativization::None,
            Some("sib:creole".to_owned()),
        )
        .expect("commit");

    // 兩者都是葉,且可以放進同一個切片——引用關係不是祖裔
    let slice = Slice::contemporary(&graph);
    assert!(slice.nodes().contains(&creole));
    assert!(slice.nodes().contains(&tweaked));
    assert!(
        Slice::new(&graph, [creole, tweaked]).is_ok(),
        "引用父不該讓兩者被判成不可能並存"
    );
}
