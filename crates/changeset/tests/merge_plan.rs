//! 步驟 16 ⑤a —— **多親機械合併的分析層**(《修補11》P61 §6)。
//!
//! 本檔驗三件事:
//!
//! 1. **3-way 的判別力**——「僅一邊改過」必須自動解決。這是整個機制可用與否的分界:
//!    2-way 只看得到「兩邊不同」,會把它誤報成衝突,而有共同祖先的合併裡這是常態。
//! 2. **衝突不被默默吞掉**——內容分歧、刪改衝突、id 碰撞、命名碰撞各有其類。
//! 3. **空基準**(無共同祖先)退化成聯集,且與有基準走**同一段程式**。

use conlang_changeset::merge::{plan_merge, MergeBlock, MergeConflict, MergeError};
use conlang_changeset::{change_set_prelude, ChangeInterpreter, UnresolvedChangeSet};
use conlang_language::{LanguageDocument, LibrarySpec, NodeId};

const ROOT: &str = "sign x:\n    syn:\n        category = noun\n\n\
                    sign y:\n    syn:\n        category = noun\n";

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
        &format!("\n    #0:\n        update sign(\"{sign}\").def[syn.category].value = {value}\n"),
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
        conlang_language::SignItem::Def(def) if def.path == "syn.category" => {
            Some(def.value.clone())
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
    let pick = plan.signs.iter().find(|pick| &pick.id == id)?;
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
    let pick = plan.signs.iter().find(|p| p.id == y).expect("y 在計畫裡");
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
            id: x.clone(),
            sides: vec![0, 1],
        }),
        "{:?}",
        plan.conflicts
    );
    assert!(
        !plan.signs.iter().any(|pick| pick.id == x),
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
            .any(|pick| pick.id == z && pick.from == Some(0)),
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
        !plan.signs.iter().any(|pick| pick.id == y),
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
            id: y,
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
        "sign eau:\n    syn:\n        category = noun\n",
        "evo:fr",
    )
    .unwrap();
    let wolof = LanguageDocument::import_new_root(
        "sign ndox:\n    syn:\n        category = noun\n",
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
        "sign eau:\n    syn:\n        category = noun\n",
        "evo:same",
    )
    .unwrap();
    let right = LanguageDocument::import_new_root(
        "sign ndox:\n    syn:\n        category = noun\n",
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
        "sign water:\n    syn:\n        category = noun\n",
        "evo:l",
    )
    .unwrap();
    let right = LanguageDocument::import_new_root(
        "sign water:\n    syn:\n        category = verb\n",
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
        "Symbol a\n\nsign one:\n    syn:\n        category = noun\n",
        "evo:l",
    )
    .unwrap();
    let right = LanguageDocument::import_new_root(
        "Symbol b\n\nsign two:\n    syn:\n        category = noun\n",
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
        "Symbol a\n\nsign one:\n    syn:\n        category = noun\n",
        "evo:l",
    )
    .unwrap();
    let right = LanguageDocument::import_new_root(
        "Symbol a\n\nsign two:\n    syn:\n        category = noun\n",
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
