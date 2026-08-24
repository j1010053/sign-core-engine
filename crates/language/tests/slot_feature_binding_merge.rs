//! Q3:同一個 `slot.feature` 的多處綁定——**字面值取候選聯集,涉及引用則擋下**。
//!
//! 為什麼引用不能聯集:引用(`$slot.verb.syn.assigned_case`)不是值而是**計算**。
//! 要聯集就得先求值,而求出的未定案沒有更外層的構式能收斂它——這個構式就是最外
//! 層——結果會卡在未定案、產不出形式。
//!
//! 而且沒有固定策略是對的:德語 `object.case = accusative`(結構格)對
//! `= $slot.verb.syn.assigned_case`(*helfen* 的與格)要**引用**贏;英語無人稱
//! 構式 `verb.number = $slot.subject.syn.number` 對 `= singular`(*it rains*)要
//! **字面值**贏。語法上兩者一模一樣,挑任一邊都會做錯一整類。

use conlang_language::ontology::OntologyRegistry;
use conlang_language::{Language, SignItem};

fn parse(src: &str) -> Language {
    Language::parse(src).expect("parse")
}

fn codes(src: &str) -> Vec<String> {
    let language = parse(src);
    let (registry, legacy) = OntologyRegistry::build(&[&language]);
    registry
        .validation_report(&[&language], &legacy)
        .diagnostics()
        .iter()
        .map(|d| d.code.to_owned())
        .collect()
}

fn binding(src: &str, sign: &str, slot: &str, feature: &str) -> String {
    let language = parse(src);
    let (registry, _) = OntologyRegistry::build(&[&language]);
    registry
        .effective_sign(language.sign_named(sign).expect("sign"))
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::SlotFeatureBinding(found)
                if found.slot == slot && found.feature == feature =>
            {
                Some(found.value.clone())
            }
            _ => None,
        })
        .expect("binding")
}

const TREE: &str = "\
trait Bearer:
    syn:
        feature:
            case = enum(nominative, accusative, dative)
";

#[test]
fn two_literal_bindings_union_into_an_undecided_set() {
    let src = format!(
        "{TREE}\
trait AccusativeObject:
    syn:
        slot_features:
            object.case = accusative

trait DativeObject:
    syn:
        slot_features:
            object.case = dative

sign clause:
    belongs AccusativeObject
    belongs DativeObject
    syn:
        slots:
            object [Bearer]
"
    );
    let merged = binding(&src, "clause", "object", "case");
    assert!(
        merged.contains("accusative") && merged.contains("dative"),
        "字面值應聯集:{merged}"
    );
    assert!(!codes(&src).contains(&"SLOT_FEATURE_BINDING_CONFLICT".to_owned()));
}

/// 聯集可交換 —— 換邊寫候選集合相同。
#[test]
fn the_union_does_not_depend_on_the_order_of_the_mounts() {
    let one = format!(
        "{TREE}\
trait A:
    syn:
        slot_features:
            object.case = accusative
trait B:
    syn:
        slot_features:
            object.case = dative
sign s:
    belongs A
    belongs B
    syn:
        slots:
            object [Bearer]
"
    );
    let other = one.replace("    belongs A\n    belongs B\n", "    belongs B\n    belongs A\n");
    let one_value = binding(&one, "s", "object", "case");
    let other_value = binding(&other, "s", "object", "case");
    let mut a: Vec<&str> = one_value.split('|').map(str::trim).collect();
    let mut b: Vec<&str> = other_value.split('|').map(str::trim).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b);
}

/// 字面值撞上引用 → 擋下。
#[test]
fn a_literal_colliding_with_a_reference_is_rejected() {
    let src = format!(
        "{TREE}\
trait StructuralCase:
    syn:
        slot_features:
            object.case = accusative

trait LexicalCase:
    syn:
        slot_features:
            object.case = $slot.verb.syn.case

sign clause:
    belongs StructuralCase
    belongs LexicalCase
    syn:
        slots:
            object [Bearer]
            verb [Bearer]
"
    );
    assert!(
        codes(&src).contains(&"SLOT_FEATURE_BINDING_CONFLICT".to_owned()),
        "引用與字面值不得合併"
    );
}

/// 正向控制組一:**同一個**引用出現兩次不是衝突。
#[test]
fn the_same_reference_from_two_traits_is_not_a_conflict() {
    let src = format!(
        "{TREE}\
trait A:
    syn:
        slot_features:
            object.case = $slot.verb.syn.case
trait B:
    syn:
        slot_features:
            object.case = $slot.verb.syn.case
sign clause:
    belongs A
    belongs B
    syn:
        slots:
            object [Bearer]
            verb [Bearer]
"
    );
    assert!(!codes(&src).contains(&"SLOT_FEATURE_BINDING_CONFLICT".to_owned()));
}

/// 正向控制組二:單一綁定照舊,不得因為新增聯集就變成集合。
#[test]
fn a_single_binding_stays_a_single_value() {
    let src = format!(
        "{TREE}\
trait Only:
    syn:
        slot_features:
            object.case = accusative
sign clause:
    belongs Only
    syn:
        slots:
            object [Bearer]
"
    );
    assert_eq!(binding(&src, "clause", "object", "case"), "accusative");
}
