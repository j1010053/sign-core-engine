//! 步驟 16 ⑤a —— **多親機械合併的分析層**(《修補11》P61 §6)。
//!
//! 本檔驗三件事:
//!
//! 1. **3-way 的判別力**——「僅一邊改過」必須自動解決。這是整個機制可用與否的分界:
//!    2-way 只看得到「兩邊不同」,會把它誤報成衝突,而有共同祖先的合併裡這是常態。
//! 2. **衝突不被默默吞掉**——內容分歧、刪改衝突、id 碰撞、命名碰撞各有其類。
//! 3. **空基準**(無共同祖先)退化成聯集,且與有基準走**同一段程式**。

use conlang_changeset::merge::{
    plan_merge, MergeBlock, MergeCollection, MergeConflict, MergeError, MergeKey,
};
use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec, NodeId};

const ROOT: &str = "sign x:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n\n\
                    sign y:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n";

fn root() -> LanguageDocument {
    LanguageDocument::import_new_root(ROOT, "evo:root").expect("root parses")
}

/// 對 `base` 套一份 changeset,得到一個**保留穩定 id** 的後代文件。
fn apply(base: &LanguageDocument, namespace: &str, body: &str) -> LanguageDocument {
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(base, &spec, namespace).expect("prelude");
    source.push_str(body);
    let parsed = UnresolvedChangeSet::parse(&source).expect("parses");
    let resolved = parsed.resolve(base, &spec).expect("resolves");
    ChangeInterpreter::new(base.clone(), spec, namespace.to_owned())
        .expect("interpreter")
        .run(&resolved)
        .expect("runs")
        .document
}

fn set_category(
    base: &LanguageDocument,
    namespace: &str,
    sign: &str,
    value: &str,
) -> LanguageDocument {
    apply(
        base,
        namespace,
        &format!("\n    #0:\n        update sign(\"{sign}\").feature[syn.category].value = {value}\n"),
    )
}

fn id_of(document: &LanguageDocument, name: &str) -> NodeId {
    document
        .language()
        .signs
        .iter()
        .find(|sign| sign.name == name)
        .expect("sign exists")
        .id
        .0
        .clone()
}

fn category_of(document: &LanguageDocument, id: &NodeId) -> Option<String> {
    let sign = document.language().signs.iter().find(|s| &s.id.0 == id)?;
    sign.items.iter().find_map(|item| match item {
        // P71 §4.3:category 已遷入 `syn: feature:`,節點是 FeatureValue 而非 Def。
        conlang_language::SignItem::FeatureValue(value)
            if value.dim == conlang_language::Dim::Syn && value.name == "category" =>
        {
            Some(value.value.clone())
        }
        _ => None,
    })
}

/// 依計畫取出某個 id 最終的值(`None` = 被刪掉)。
fn resolved_category(
    plan: &conlang_changeset::merge::MergePlan,
    base: &LanguageDocument,
    sides: &[&LanguageDocument],
    id: &NodeId,
) -> Option<String> {
    let pick = plan
        .signs
        .iter()
        .find(|pick| pick.key == MergeKey::Node(id.clone()))?;
    match pick.from {
        Some(index) => category_of(sides[index], id),
        None => category_of(base, id),
    }
}

// ── 3-way 的判別力(§6.3)───────────────────────────────────────────────────

#[test]
fn a_change_on_one_side_only_is_taken_automatically() {
    // **整個機制的分界點**。A 改了 x、B 沒動 x。兩兩比對只看得到「A 的 x ≠ B 的 x」
    // 而報衝突;3-way 因為看得到基準,知道 B 還是祖先的樣子,自動取 A。
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "y", "adj"); // 只動 y,沒動 x
    let x = id_of(&base, "x");

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    assert!(plan.is_clean(), "不該有衝突:{:?}", plan.conflicts);
    assert_eq!(
        resolved_category(&plan, &base, &[&a, &b], &x).as_deref(),
        Some("verb"),
        "x 只有 A 改過 → 取 A"
    );
    // 對稱:y 只有 B 改過。
    let y = id_of(&base, "y");
    assert_eq!(
        resolved_category(&plan, &base, &[&a, &b], &y).as_deref(),
        Some("adj")
    );
}

#[test]
fn a_sign_untouched_by_everyone_keeps_the_base_value() {
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "x", "verb");
    let y = id_of(&base, "y");

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    let pick = plan
        .signs
        .iter()
        .find(|p| p.key == MergeKey::Node(y.clone()))
        .expect("y 在計畫裡");
    assert_eq!(pick.from, None, "沒人動過 → 沿用基準,不必歸給任何一方");
}

#[test]
fn both_sides_changing_to_the_same_value_is_clean() {
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "x", "verb");
    let x = id_of(&base, "x");

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    assert!(plan.is_clean(), "{:?}", plan.conflicts);
    assert_eq!(
        resolved_category(&plan, &base, &[&a, &b], &x).as_deref(),
        Some("verb")
    );
}

#[test]
fn both_sides_changing_differently_conflicts() {
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "x", "adj");
    let x = id_of(&base, "x");

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    assert!(
        plan.conflicts.contains(&MergeConflict::Content {
            collection: MergeCollection::Signs,
            key: MergeKey::Node(x.clone()),
            sides: vec![0, 1],
        }),
        "{:?}",
        plan.conflicts
    );
    assert!(
        !plan
            .signs
            .iter()
            .any(|pick| pick.key == MergeKey::Node(x.clone())),
        "衝突的 sign 不得偷偷進計畫"
    );
}

// ── 生與滅 ────────────────────────────────────────────────────────────────

#[test]
fn an_addition_on_one_side_is_taken() {
    let base = root();
    let a = apply(
        &base,
        "evo:a",
        "\n    #0:\n        clone sign(\"x\") as z\n",
    );
    let b = set_category(&base, "evo:b", "y", "adj");

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    assert!(plan.is_clean(), "{:?}", plan.conflicts);
    let z = id_of(&a, "z");
    assert!(
        plan.signs
            .iter()
            .any(|pick| pick.key == MergeKey::Node(z.clone()) && pick.from == Some(0)),
        "新增的 z 必須從 A 取"
    );
}

#[test]
fn a_deletion_on_one_side_is_taken() {
    let base = root();
    let a = apply(&base, "evo:a", "\n    #0:\n        delete sign(\"y\")\n");
    let b = set_category(&base, "evo:b", "x", "verb");
    let y = id_of(&base, "y");

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    assert!(plan.is_clean(), "{:?}", plan.conflicts);
    assert!(
        !plan
            .signs
            .iter()
            .any(|pick| pick.key == MergeKey::Node(y.clone())),
        "A 刪了、B 沒動 → 採用刪除"
    );
}

#[test]
fn delete_versus_modify_conflicts() {
    // git 的經典案例:一邊刪、一邊改,機械上無從決定。
    let base = root();
    let a = apply(&base, "evo:a", "\n    #0:\n        delete sign(\"y\")\n");
    let b = set_category(&base, "evo:b", "y", "adj");
    let y = id_of(&base, "y");

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    assert!(
        plan.conflicts.contains(&MergeConflict::DeleteModify {
            collection: MergeCollection::Signs,
            key: MergeKey::Node(y),
            deleted_by: vec![0],
            modified_by: vec![1],
        }),
        "{:?}",
        plan.conflicts
    );
}

// ── 空基準:無共同祖先(§6.3)────────────────────────────────────────────────

#[test]
fn unrelated_documents_merge_as_a_union() {
    // 沒有共同祖先 ⇒ 基準為空 ⇒ 每個 sign 都是「只有一方有」⇒ 全部納入。
    // **與有基準走同一段程式**,不是另一條路徑。
    let french = LanguageDocument::import_new_root(
        "sign eau:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:fr",
    )
    .unwrap();
    let wolof = LanguageDocument::import_new_root(
        "sign ndox:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:wo",
    )
    .unwrap();

    let plan = plan_merge(None, &[&french, &wolof]).expect("plan");
    assert!(plan.is_clean(), "{:?}", plan.conflicts);
    assert_eq!(plan.signs.len(), 2, "兩邊的詞都要在");
    assert!(plan.signs.iter().any(|p| p.from == Some(0)));
    assert!(plan.signs.iter().any(|p| p.from == Some(1)));
}

#[test]
fn an_id_shared_without_a_common_base_is_a_collision() {
    // 合法共有的 id **只可能來自共同祖先**。基準沒有卻兩邊都有 ⇒ 各自獨立配發後撞號,
    // 不是同一個 sign。這正是 `add_root` 的 namespace 守門擋不住的那一半
    // (跨家族的 fork 撞號),故必須在合併當下攔下。
    let left = LanguageDocument::import_new_root(
        "sign eau:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:same",
    )
    .unwrap();
    let right = LanguageDocument::import_new_root(
        "sign ndox:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:same", // ← 同一個 namespace
    )
    .unwrap();
    assert_eq!(
        id_of(&left, "eau"),
        id_of(&right, "ndox"),
        "前提:兩份文件的 id 確實撞了,否則本測試沒有判別力"
    );

    let plan = plan_merge(None, &[&left, &right]).expect("plan");
    assert!(
        plan.conflicts
            .iter()
            .any(|conflict| matches!(conflict, MergeConflict::IdCollision { .. })),
        "{:?}",
        plan.conflicts
    );
}

#[test]
fn two_signs_sharing_a_name_collide() {
    // `.lang` 名字唯一。融合不同源語言時這是主要工作量,而非語意分歧(§6.2)。
    let left = LanguageDocument::import_new_root(
        "sign water:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:l",
    )
    .unwrap();
    let right = LanguageDocument::import_new_root(
        "sign water:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = verb\n",
        "evo:r",
    )
    .unwrap();
    assert_ne!(
        id_of(&left, "water"),
        id_of(&right, "water"),
        "是兩個不同的詞"
    );

    let plan = plan_merge(None, &[&left, &right]).expect("plan");
    assert!(
        plan.conflicts
            .iter()
            .any(|conflict| matches!(conflict, MergeConflict::NameCollision { name, .. } if name == "water")),
        "{:?}",
        plan.conflicts
    );
}

// ── 無鍵區段(§6.2)────────────────────────────────────────────────────────

#[test]
fn unrelated_documents_with_different_declarations_conflict_per_block() {
    // 空基準下,無對齊鍵的區段只要不一致就是衝突——它們沒有「聯集」語意,
    // 也不可解析(`dsl_decls` 是不透明 verbatim,I15-a),機械上無從合併。
    let left = LanguageDocument::import_new_root(
        "Symbol a\n\nsign one:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:l",
    )
    .unwrap();
    let right = LanguageDocument::import_new_root(
        "Symbol b\n\nsign two:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:r",
    )
    .unwrap();

    let plan = plan_merge(None, &[&left, &right]).expect("plan");
    assert!(
        plan.conflicts
            .contains(&MergeConflict::Block(MergeBlock::DslDecls)),
        "{:?}",
        plan.conflicts
    );
}

#[test]
fn identical_declarations_merge_without_a_conflict() {
    let left = LanguageDocument::import_new_root(
        "Symbol a\n\nsign one:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:l",
    )
    .unwrap();
    let right = LanguageDocument::import_new_root(
        "Symbol a\n\nsign two:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:r",
    )
    .unwrap();

    let plan = plan_merge(None, &[&left, &right]).expect("plan");
    assert!(
        !plan
            .conflicts
            .iter()
            .any(|c| matches!(c, MergeConflict::Block(_))),
        "{:?}",
        plan.conflicts
    );
}

// ── 邊界 ──────────────────────────────────────────────────────────────────

#[test]
fn merging_fewer_than_two_parents_is_rejected() {
    let base = root();
    assert!(matches!(
        plan_merge(Some(&base), &[&base]),
        Err(MergeError::TooFewParents)
    ));
}

#[test]
fn the_plan_is_deterministic() {
    // P26:同輸入同輸出。計畫依 id 排序,故兩次必須逐項相同。
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "y", "adj");
    let first = plan_merge(Some(&base), &[&a, &b]).unwrap();
    let second = plan_merge(Some(&base), &[&a, &b]).unwrap();
    assert_eq!(first, second);
}

// ── 逐項合併也套用在 traits 與 distribution(§6.2 修訂)────────────────────

const WITH_TRAITS: &str = "trait LocalNoun:\n\ntrait LocalVerb:\n\n\
                           sign x:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n";

fn traited_root() -> LanguageDocument {
    LanguageDocument::import_new_root(WITH_TRAITS, "evo:root").expect("root parses")
}

fn trait_key(document: &LanguageDocument, name: &str) -> MergeKey {
    MergeKey::Node(document.ref_for_trait(name).expect("trait exists").id)
}

#[test]
fn two_branches_adding_different_traits_merge_cleanly() {
    // **這就是「甲」要修的事**。整塊比對之下,兩支都動過文法 ⇒ traits 清單都跟祖先
    // 不同 ⇒ 整塊衝突。逐項比對之下,兩支加的是**不同的 trait**,互不相干。
    //
    // 而「兩支都碰過文法」在演化裡幾乎是必然,所以整塊比對等於每次合併都要人工。
    let base = traited_root();
    let a = apply(
        &base,
        "evo:a",
        "\n    #0:\n        insert into language at end:\n            trait LocalAnimate:\n",
    );
    let b = apply(
        &base,
        "evo:b",
        "\n    #0:\n        insert into language at end:\n            trait LocalTelic:\n",
    );

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    assert!(
        plan.is_clean(),
        "各加各的 trait,不該衝突:{:?}",
        plan.conflicts
    );
    assert!(
        plan.traits
            .iter()
            .any(|p| p.key == trait_key(&a, "LocalAnimate")),
        "A 新增的 trait 要在計畫裡"
    );
    assert!(
        plan.traits
            .iter()
            .any(|p| p.key == trait_key(&b, "LocalTelic")),
        "B 新增的 trait 要在計畫裡"
    );
    // 沒人動過的 trait 沿用基準。
    assert!(plan
        .traits
        .iter()
        .any(|p| p.key == trait_key(&base, "LocalNoun") && p.from.is_none()));
}

#[test]
fn a_trait_deleted_by_one_side_is_taken() {
    let base = traited_root();
    let a = apply(
        &base,
        "evo:a",
        "\n    #0:\n        delete trait(\"LocalVerb\")\n",
    );
    let b = apply(
        &base,
        "evo:b",
        "\n    #0:\n        insert into language at end:\n            trait LocalTelic:\n",
    );
    let verb = trait_key(&base, "LocalVerb");

    let plan = plan_merge(Some(&base), &[&a, &b]).expect("plan");
    assert!(plan.is_clean(), "{:?}", plan.conflicts);
    assert!(
        !plan.traits.iter().any(|p| p.key == verb),
        "A 刪了、B 沒動 → 採用刪除"
    );
}

#[test]
fn traits_sharing_a_name_across_unrelated_roots_collide() {
    // 「兩邊的 Noun 是同一個範疇嗎」——id 已經回答了:不同 id ⇒ 兩個不同的範疇
    // 剛好撞名 ⇒ 命名碰撞,與 sign 的處理一模一樣,不是語言學判斷。
    let left = LanguageDocument::import_new_root("trait LocalNoun:\n", "evo:l").unwrap();
    let right = LanguageDocument::import_new_root("trait LocalNoun:\n", "evo:r").unwrap();
    assert_ne!(
        trait_key(&left, "LocalNoun"),
        trait_key(&right, "LocalNoun")
    );

    let plan = plan_merge(None, &[&left, &right]).expect("plan");
    assert!(
        plan.conflicts.iter().any(|conflict| matches!(
            conflict,
            MergeConflict::NameCollision {
                collection: MergeCollection::Traits,
                name,
                ..
            } if name == "LocalNoun"
        )),
        "{:?}",
        plan.conflicts
    );
}

#[test]
fn a_conflict_is_labelled_with_the_collection_it_came_from() {
    // 衝突要說得出是**哪個區段**——否則 trait 衝突與 sign 衝突在回報上分不開。
    //
    // **判別性**:這裡刻意用 trait 層的 `IdCollision`。若只驗 sign 的衝突,
    // 「區段標籤寫死成 Signs」這種改壞法**測不出來**(問過了:它會全綠)。
    let left = LanguageDocument::import_new_root("trait LocalAlpha:\n", "evo:same").unwrap();
    let right = LanguageDocument::import_new_root("trait LocalBeta:\n", "evo:same").unwrap();
    assert_eq!(
        trait_key(&left, "LocalAlpha"),
        trait_key(&right, "LocalBeta"),
        "前提:同 namespace 下兩個 trait 的 id 確實撞了"
    );

    let plan = plan_merge(None, &[&left, &right]).expect("plan");
    assert!(
        plan.conflicts.iter().any(|c| matches!(
            c,
            MergeConflict::IdCollision {
                collection: MergeCollection::Traits,
                ..
            }
        )),
        "必須標成 Traits:{:?}",
        plan.conflicts
    );
    assert!(
        !plan.conflicts.iter().any(|c| matches!(
            c,
            MergeConflict::IdCollision {
                collection: MergeCollection::Signs,
                ..
            }
        )),
        "不得誤標成 Signs:{:?}",
        plan.conflicts
    );
}

// ── 物化(⑤b)──────────────────────────────────────────────────────────────

use conlang_changeset::merge::materialize;
use conlang_language::AddressSegment;

/// 某個 sign 子樹底下的全部穩定 id(含 sign 自己)。
fn subtree_ids(document: &LanguageDocument, sign: &str) -> Vec<NodeId> {
    let index = document
        .language()
        .signs
        .iter()
        .position(|item| item.name == sign)
        .expect("sign exists");
    let head = AddressSegment::Signs(index);
    document
        .identities()
        .nodes
        .iter()
        .filter(|entry| entry.address.0.first() == Some(&head))
        .map(|entry| entry.id.clone())
        .collect()
}

fn name_of(document: &LanguageDocument, id: &NodeId) -> Option<String> {
    document
        .language()
        .signs
        .iter()
        .find(|sign| &sign.id.0 == id)
        .map(|sign| sign.name.clone())
}

#[test]
fn a_clean_plan_materialises_into_a_document() {
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "y", "adj");
    let plan = plan_merge(Some(&base), &[&a, &b]).unwrap();

    let merged = materialize(&plan, Some(&base), &[&a, &b], "evo:merged").expect("物化");
    let x = id_of(&base, "x");
    let y = id_of(&base, "y");
    assert_eq!(
        category_of(&merged, &x).as_deref(),
        Some("verb"),
        "A 的改動要在"
    );
    assert_eq!(
        category_of(&merged, &y).as_deref(),
        Some("adj"),
        "B 的改動也要在"
    );
}

#[test]
fn merged_signs_keep_the_ids_they_inherited() {
    // **物化的核心**。合併後 sign 會按名字重排,位址整份位移;若身分清單跟著重配 id,
    // 每個詞都會變成「一個新詞」,演化史就斷了。這裡逐一比對子樹的全部 id。
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "y", "adj");
    let plan = plan_merge(Some(&base), &[&a, &b]).unwrap();
    let merged = materialize(&plan, Some(&base), &[&a, &b], "evo:merged").expect("物化");

    for (source, sign) in [(&a, "x"), (&b, "y")] {
        let inherited = subtree_ids(source, sign);
        let kept = subtree_ids(&merged, sign);
        assert_eq!(
            inherited, kept,
            "{sign} 整棵子樹的 id 必須原封不動(含底下的項目)"
        );
        assert!(!inherited.is_empty(), "前提:子樹不是空的");
    }
    // 連 `SignDef.id` 與 manifest 也要一致。
    assert_eq!(
        merged.ref_for_sign("x").expect("x 在").id,
        id_of(&base, "x")
    );
}

#[test]
fn an_unrelated_merge_carries_both_lexicons_with_their_ids() {
    // 空基準 + 兩個不同命名空間:這是真克里奧爾的形狀。
    let french = LanguageDocument::import_new_root(
        "sign eau:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:fr",
    )
    .unwrap();
    let wolof = LanguageDocument::import_new_root(
        "sign ndox:\n    syn:\n        feature:\n            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)\n            category = noun\n",
        "evo:wo",
    )
    .unwrap();
    let plan = plan_merge(None, &[&french, &wolof]).unwrap();

    let merged = materialize(&plan, None, &[&french, &wolof], "evo:creole").expect("物化");
    assert_eq!(merged.language().signs.len(), 2);
    assert_eq!(
        name_of(&merged, &id_of(&french, "eau")).as_deref(),
        Some("eau"),
        "法語支的 id 要跟著詞過來"
    );
    assert_eq!(
        name_of(&merged, &id_of(&wolof, "ndox")).as_deref(),
        Some("ndox")
    );
}

#[test]
fn the_merged_document_keeps_every_parents_allocator() {
    // **靜默毀損的守門**。配號器記著「下一個號碼發到幾」。只拿其中一方的 → 日後新增
    // 節點會發出已經用過的號碼,兩個不同節點共用一個 id,而且**不報錯**
    // (`from_edit_parts` 的形狀驗證看不到配號器)。
    let base = root();
    let a = apply(
        &base,
        "evo:a",
        "\n    #0:\n        clone sign(\"x\") as z\n",
    );
    let b = apply(
        &base,
        "evo:b",
        "\n    #0:\n        clone sign(\"y\") as w\n",
    );
    let plan = plan_merge(Some(&base), &[&a, &b]).unwrap();
    let merged = materialize(&plan, Some(&base), &[&a, &b], "evo:merged").expect("物化");

    for parent in [&base, &a, &b] {
        for allocator in &parent.identities().allocators {
            let kept = merged
                .identities()
                .allocators
                .iter()
                .find(|candidate| candidate.namespace == allocator.namespace)
                .unwrap_or_else(|| panic!("{} 的配號器不見了", allocator.namespace));
            assert!(
                kept.next_ordinal >= allocator.next_ordinal,
                "{} 的號碼倒退了:{} < {}",
                allocator.namespace,
                kept.next_ordinal,
                allocator.next_ordinal
            );
        }
    }
}

#[test]
fn a_conflicting_plan_cannot_be_materialised() {
    // §6.4:有衝突就建不出來,不存在「先建起來之後再解」的中間狀態。
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "x", "adj");
    let plan = plan_merge(Some(&base), &[&a, &b]).unwrap();
    assert!(!plan.is_clean(), "前提:這份計畫有衝突");
    assert!(matches!(
        materialize(&plan, Some(&base), &[&a, &b], "evo:merged"),
        Err(MergeError::UnresolvedConflicts(_))
    ));
}

#[test]
fn reusing_a_parents_namespace_is_rejected() {
    // 新配的 id 會撞上繼承來的 id —— 又是一個不報錯就毀損的路徑。
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "y", "adj");
    let plan = plan_merge(Some(&base), &[&a, &b]).unwrap();
    assert!(matches!(
        materialize(&plan, Some(&base), &[&a, &b], "evo:a"),
        Err(MergeError::NamespaceInUse(_))
    ));
}

#[test]
fn materialisation_is_deterministic() {
    let base = root();
    let a = set_category(&base, "evo:a", "x", "verb");
    let b = set_category(&base, "evo:b", "y", "adj");
    let plan = plan_merge(Some(&base), &[&a, &b]).unwrap();
    let first = materialize(&plan, Some(&base), &[&a, &b], "evo:merged").unwrap();
    let second = materialize(&plan, Some(&base), &[&a, &b], "evo:merged").unwrap();
    assert_eq!(first.source(), second.source());
    assert_eq!(
        first.manifest_json().unwrap(),
        second.manifest_json().unwrap()
    );
}

#[test]
fn merged_traits_keep_their_ids_too() {
    let base = traited_root();
    let a = apply(
        &base,
        "evo:a",
        "\n    #0:\n        insert into language at end:\n            trait LocalAnimate:\n",
    );
    let b = apply(
        &base,
        "evo:b",
        "\n    #0:\n        insert into language at end:\n            trait LocalTelic:\n",
    );
    let plan = plan_merge(Some(&base), &[&a, &b]).unwrap();
    let merged = materialize(&plan, Some(&base), &[&a, &b], "evo:merged").expect("物化");

    assert_eq!(
        merged.ref_for_trait("LocalAnimate").expect("在").id,
        a.ref_for_trait("LocalAnimate").expect("在").id
    );
    assert_eq!(
        merged.ref_for_trait("LocalTelic").expect("在").id,
        b.ref_for_trait("LocalTelic").expect("在").id
    );
    assert_eq!(
        merged.ref_for_trait("LocalNoun").expect("在").id,
        base.ref_for_trait("LocalNoun").expect("在").id,
        "沒人動過的 trait 也要保住 id"
    );
}
