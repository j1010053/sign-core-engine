//! P71-S 出口:**role 約束與 slot 同型**(`[*]`),`SemNode.types` **就是完整範疇閉包**。
//!
//! 此前 `sem.rs` 以兩處硬寫的 `"Semantic"` 支撐這一層:過濾出「語意型別」,
//! 再無條件把 `Semantic` 塞進每個 sign,好讓 `roles: [Semantic]` 能當 `[*]` 用。
//! 那是引擎硬寫 `std:core` 的詞彙——與已移除的 `Nominal→Entity` bridge 同一類。
//! 掃描確認 15 處 `[Semantic]` 全是「不想約束」的表達,無一真需要子樹成員檢查。
//!
//! **每條否定斷言都配正向控制組**:否則「約束根本沒被檢查」與「約束通過」
//! 在測試上分不出來——這正是本檔補上時,兩個突變體(role 不檢查、types 縮回
//! Semantic 子樹)雙雙存活所暴露的缺口。

use conlang_language::construction::{SlotFiller, SlotMap};
use conlang_language::sem::SemNode;
use conlang_language::system::SystemError;
use conlang_language::{compile_system, construction::CxgError, ontology, Language};

/// `x [*]` 讓 slot 不設限,**role 成為唯一的把關者**——否則測不出 role 這一層。
const SRC: &str = r#"Symbol n
Symbol v
Symbol b
Class vowel {n}

sign nouny:
    belongs Noun
    phon:
        /n/
sign verby:
    belongs Verb
    phon:
        /v/
sign bare:
    phon:
        /b/

sign NeedsEntity:
    syn:
        slots:
            x [*]
    sem:
        roles:
            referent [Entity]
            referent = {x}
    phon:
        /{x}/

sign NeedsAnything:
    syn:
        slots:
            x [*]
    sem:
        roles:
            anything [*]
            anything = {x}
    phon:
        /{x}/
"#;

fn system() -> conlang_language::CompiledSystem {
    compile_system(Language::parse(SRC).expect("parse")).expect("compiles")
}

fn fill(construction: &str, filler: &str) -> Result<(), SystemError> {
    system()
        .apply_construction(
            construction,
            &[SlotFiller::sign("x", filler)],
            &SlotMap::identity(),
        )
        .map(|_| ())
}

// ── role 約束真的把關 ──────────────────────────────────────────────────────

/// 否定:`verby` 走 `Verb → Event`,不是 `Entity` → 應被 role 擋下。
///
/// 少了這條,整個 role 約束檢查刪掉都不會紅(實測突變存活)。
#[test]
fn a_role_constraint_rejects_a_filler_outside_its_category() {
    let error = fill("NeedsEntity", "verby").expect_err("Verb 不是 Entity");
    assert!(
        matches!(
            error,
            SystemError::Construction(CxgError::RoleCategoryMismatch { ref role, ref required, .. })
                if role == "referent" && required == "Entity"
        ),
        "{error:?}"
    );
}

/// 正向控制組:`nouny` 經 `Noun → Nominal → Entity` 滿足同一條約束。
///
/// 沒有這條,上面那條可能只是因為「什麼都填不進去」而綠。
#[test]
fn the_same_role_accepts_a_filler_inside_its_category() {
    fill("NeedsEntity", "nouny").expect("Noun 經 Nominal 取得 Entity");
}

/// `[*]` 接受**完全沒有 `belongs`** 的 filler(閉包為空)。
///
/// 這是舊設計必須硬塞 `types.push("Semantic")` 才能成立的情境;
/// 現在由 `[*]` 正面表達,引擎不再假裝每個 sign 都繼承某個 std trait。
#[test]
fn an_any_node_role_accepts_a_filler_with_no_categories() {
    let language = Language::parse(SRC).unwrap();
    let (registry, _) = ontology::with_std(&language);
    let bare = SemNode::of_sign(language.sign_named("bare").unwrap(), &registry);
    assert!(bare.types.is_empty(), "前提:bare 無任何範疇:{:?}", bare.types);

    fill("NeedsAnything", "bare").expect("`[*]` 不設限");
    // 判別性:同一個 filler 填不進有約束的 role
    fill("NeedsEntity", "bare").expect_err("`[Entity]` 仍須把關");
}

// ── types = 完整閉包 ──────────────────────────────────────────────────────

/// `SemNode.types` 是**完整範疇閉包**,不是「Semantic 子樹」的過濾副本。
///
/// 單一中立樹(P38 v0.2)下只有一組範疇;引擎不得挑 `std:core` 的某個 trait
/// 當作特權篩子。少了這條,把過濾加回去不會紅(實測突變存活)。
#[test]
fn sem_types_is_the_full_category_closure() {
    let language = Language::parse(SRC).unwrap();
    let (registry, _) = ontology::with_std(&language);
    let types = SemNode::of_sign(language.sign_named("nouny").unwrap(), &registry).types;

    for expected in ["Noun", "Nominal", "Entity", "Semantic"] {
        assert!(types.contains(&expected.to_string()), "缺 {expected}:{types:?}");
    }
    // `Noun`/`Nominal` 是舊過濾器會濾掉的——它們正是本斷言的判別點。
    // `types` 另有 sort+dedup(決定性),閉包保 nearest-first,故比集合不比序。
    let mut closure =
        registry.sign_categories(&registry.effective_sign(language.sign_named("nouny").unwrap()));
    closure.sort();
    closure.dedup();
    assert_eq!(types, closure, "types 必須等於整個閉包,不得是任何子集");
}
