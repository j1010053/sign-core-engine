//! 構式對未定案值域**求交**:同一個 sign 在不同構式裡收斂到不同的值。
//!
//! 這是未定案值域存在的理由。英語 *fish* 單複同形、*the police **are*** 對
//! *the police **is***、*the committee has* 對 *have*——同一個詞項的數在不同構式
//! (與不同變體)裡是不同的,那是語言事實而非建模錯誤。若在投影時就靠優先序挑
//! 一個贏家,這個事實會在構式看到它之前就被刪掉。
//!
//! 收斂只寫進**構式局部的副本**;已存的 sign 保持未定案,否則第一個用到它的構式
//! 就會把後面所有構式的答案定死。

use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::{compile_system, Dim, Language};

const SOURCE: &str = r#"Symbol a
Class vowel {a}

trait Agreeing:
    belongs Noun
    syn:
        feature:
            number = enum(singular, plural)

sign fish:
    belongs Agreeing
    syn:
        feature:
            number = singular | plural
    phon:
        /a/

sign john:
    belongs Agreeing
    syn:
        feature:
            number = singular
    phon:
        /a/

sign SingularNP:
    syn:
        slots:
            item [Agreeing]
        slot_features:
            item.number = singular
    phon:
        /{$slot.item}/

sign PluralNP:
    syn:
        slots:
            item [Agreeing]
        slot_features:
            item.number = plural
    phon:
        /{$slot.item}/
"#;

fn narrowed_number(construction: &str, filler: &str) -> Result<Option<String>, String> {
    let system = compile_system(Language::parse(SOURCE).expect("parse")).expect("compiles");
    let derived = system
        .derive(
            construction,
            &[SlotFiller::sign("item", filler)],
            &SlotMap::identity(),
        )
        .map_err(|error| error.to_string())?;
    Ok(derived
        .token
        .fillers
        .iter()
        .find(|snapshot| snapshot.slot == "item")
        .and_then(|snapshot| snapshot.scalar(Dim::Syn, "number"))
        .map(str::to_owned))
}

/// 判準測試:**同一個 fish**,兩個構式各自收斂到不同的值。
#[test]
fn one_undecided_sign_narrows_differently_in_two_constructions() {
    assert_eq!(
        narrowed_number("SingularNP", "fish").expect("單數構式必須成立"),
        Some("singular".to_owned())
    );
    assert_eq!(
        narrowed_number("PluralNP", "fish").expect("複數構式必須成立"),
        Some("plural".to_owned())
    );
}

/// 收斂不得回寫已存的 sign——否則第一個用到它的構式就把答案定死了。
#[test]
fn narrowing_does_not_write_back_to_the_stored_sign() {
    let system = compile_system(Language::parse(SOURCE).expect("parse")).expect("compiles");
    system
        .derive(
            "SingularNP",
            &[SlotFiller::sign("item", "fish")],
            &SlotMap::identity(),
        )
        .expect("先跑一次單數構式");
    let stored = system
        .effective_language()
        .sign_named("fish")
        .expect("fish")
        .project(Dim::Syn, &system.ontology)
        .get("syn.number")
        .map(str::to_owned);
    assert_eq!(
        stored,
        Some("singular | plural".to_owned()),
        "已存的 fish 必須保持未定案"
    );
}

/// 反向控制組一:**已定案**的 sign 進不了要求相反值的構式。
///
/// 少了這條,「求交」退化成「構式說了算」也會讓上面兩條全綠。
#[test]
fn a_decided_sign_still_conflicts_with_the_wrong_construction() {
    let error = narrowed_number("PluralNP", "john").expect_err("john 是單數,不得進複數構式");
    assert!(
        error.contains("singular") && error.contains("plural"),
        "訊息要說出撞在哪:{error}"
    );
}

/// 反向控制組二:已定案且相符時照常通過。
#[test]
fn a_decided_sign_passes_the_matching_construction() {
    assert_eq!(
        narrowed_number("SingularNP", "john").expect("john 進單數構式"),
        Some("singular".to_owned())
    );
}
