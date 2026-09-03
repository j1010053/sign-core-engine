//! Q2:slot 契約允許**收窄**,禁止放寬。
//!
//! 與 feature 值域(Q1「宣告一次」)不同的理由:值域的收窄有別的表達方式——寫
//! **賦值**(`number = singular`)即可,而賦值層還能表達未定案。slot 沒有:
//! 「*persuade* 的賓語必須是人」除了重新宣告 `object [Human]` 之外沒有寫法。
//! 禁止就會少掉一個真實需求的出口。
//!
//! 一條規則涵蓋三種收窄:**後代的合法填充集合 ⊆ 祖先的**。

use conlang_language::ontology::OntologyRegistry;
use conlang_language::{Language, SignItem, SlotConstraint};

fn parse(src: &str) -> Language {
    Language::parse(src).expect("parse")
}

fn diagnostics(src: &str) -> Vec<String> {
    let language = parse(src);
    let (registry, legacy) = OntologyRegistry::build(&[&language]);
    registry
        .validation_report(&[&language], &legacy)
        .diagnostics()
        .iter()
        .map(|d| d.code.to_owned())
        .collect()
}

fn effective_slot(src: &str, sign: &str, slot: &str) -> (SlotConstraint, bool) {
    let language = parse(src);
    let (registry, _) = OntologyRegistry::build(&[&language]);
    registry
        .effective_sign(language.sign_named(sign).expect("sign"))
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::Slot(found) if found.name == slot => {
                Some((found.constraint.clone(), found.optional))
            }
            _ => None,
        })
        .expect("slot")
}

const TREE: &str = "\
trait Nominal:
trait Human:
    belongs Nominal
trait Adposition:
";

// ── 放行 ──────────────────────────────────────────────────────────────────

/// 案例 A:範疇收窄。*persuade* 的賓語必須是人。
#[test]
fn a_descendant_category_narrows_the_inherited_slot() {
    let src = format!(
        "{TREE}\
trait Transitive:
    syn:
        slots:
            object [Nominal]

trait HumanObjectVerb:
    belongs Transitive
    syn:
        slots:
            object [Human]

sign persuade:
    belongs HumanObjectVerb
"
    );
    assert!(!diagnostics(&src).contains(&"SLOT_CONFLICT".to_owned()));
    assert_eq!(
        effective_slot(&src, "persuade", "object").0,
        SlotConstraint::Category("Human".to_owned())
    );
}

/// 案例 B:`[*]` 是上界,任何範疇都不寬於它。
#[test]
fn any_sign_is_the_top_and_a_category_narrows_it() {
    let src = format!(
        "{TREE}\
trait GenericHead:
    syn:
        slots:
            item [*]

trait NounPhrase:
    syn:
        slots:
            item [Nominal]

sign np:
    belongs GenericHead
    belongs NounPhrase
"
    );
    assert!(!diagnostics(&src).contains(&"SLOT_CONFLICT".to_owned()));
    assert_eq!(
        effective_slot(&src, "np", "item").0,
        SlotConstraint::Category("Nominal".to_owned())
    );
}

/// 案例 D:`optional` 收窄 = 可省 → 必填。
#[test]
fn optional_narrows_to_required() {
    let src = format!(
        "{TREE}\
trait Droppable:
    syn:
        slots:
            recipient [Nominal]?

trait Obligatory:
    syn:
        slots:
            recipient [Nominal]

sign give:
    belongs Droppable
    belongs Obligatory
"
    );
    assert!(!diagnostics(&src).contains(&"SLOT_CONFLICT".to_owned()));
    assert!(!effective_slot(&src, "give", "recipient").1, "必填勝");
}

/// **順序無關**:取最窄是可交換的運算,換邊寫結果一樣。
#[test]
fn narrowing_does_not_depend_on_the_order_of_the_mounts() {
    let one = format!(
        "{TREE}\
trait Wide:
    syn:
        slots:
            item [Nominal]

trait Narrow:
    syn:
        slots:
            item [Human]

sign s:
    belongs Wide
    belongs Narrow
"
    );
    let other = one.replace(
        "    belongs Wide\n    belongs Narrow\n",
        "    belongs Narrow\n    belongs Wide\n",
    );
    assert_eq!(
        effective_slot(&one, "s", "item"),
        effective_slot(&other, "s", "item")
    );
}

// ── 擋下 ──────────────────────────────────────────────────────────────────

/// 案例 C:兩個定義在分類樹上互不相干,沒有東西能同時填。
#[test]
fn two_unrelated_categories_remain_a_conflict() {
    let src = format!(
        "{TREE}\
trait NPStructure:
    syn:
        slots:
            head [Nominal]

trait PPStructure:
    syn:
        slots:
            head [Adposition]

sign s:
    belongs NPStructure
    belongs PPStructure
"
    );
    assert!(diagnostics(&src).contains(&"SLOT_CONFLICT".to_owned()));
}

/// 放寬要擋:後代把祖先的 `[Human]` 放回 `[Nominal]`,等於毀掉祖先的保證。
///
/// 這條與「兩個並列」那條不同——少了它,規則會退化成「只要可比較就放行」。
#[test]
fn widening_an_inherited_slot_is_rejected() {
    let src = format!(
        "{TREE}\
trait Strict:
    syn:
        slots:
            object [Human]

trait Loosened:
    belongs Strict
    syn:
        slots:
            object [Nominal]

sign s:
    belongs Loosened
"
    );
    assert!(
        diagnostics(&src).contains(&"SLOT_CONFLICT".to_owned()),
        "後代不得放寬祖先的 slot 契約"
    );
}

/// 必填 → 可省也是放寬。
#[test]
fn making_a_required_slot_optional_is_rejected() {
    let src = format!(
        "{TREE}\
trait Required:
    syn:
        slots:
            item [Nominal]

trait MadeOptional:
    belongs Required
    syn:
        slots:
            item [Nominal]?

sign s:
    belongs MadeOptional
"
    );
    assert!(diagnostics(&src).contains(&"SLOT_CONFLICT".to_owned()));
}
