//! 步驟 12b 出口:construction-as-Sign + slots + application(修補07 P41/P42,I21)。
//! 🔑 驗收語料 = **德語現在式變位系統**(sag- 詞幹 + 人稱後綴;分離前綴 optional;
//! 過去分詞環綴)。獨立 oracle:paradigm golden(六形)、範疇授權負例、valence=slots
//! 殘餘、飽和性、P42 不就地改(來源 sign 恆定)、round-trip。

use conlang_language::codegen::{compile_full, Artifacts};
use conlang_language::construction::{self, CxgError};
use conlang_language::ontology::{self, OntologyRegistry};
use conlang_language::Language;

fn setup() -> (Language, Artifacts, OntologyRegistry) {
    let src = include_str!("fixtures/german_present.lang");
    let lang = Language::parse(src).expect("parse");
    let art = compile_full(&lang).expect("①–⑤");
    let (reg, diags) = ontology::with_std(&lang);
    assert!(diags.is_empty(), "ontology 建構不得有診斷:{diags:?}");
    (lang, art, reg)
}

fn conjugate(lang: &Language, art: &Artifacts, reg: &OntologyRegistry, suffix: &str) -> String {
    let tok = construction::apply(
        lang,
        reg,
        "PresentVerb",
        &[("stem", "sag"), ("suffix", suffix)],
    )
    .expect("apply");
    assert!(tok.is_saturated(), "prefix optional 未填仍應飽和");
    construction::surface(&art.grammar.program, &tok).expect("surface")
}

/// 🔑 現在式全變位(六形)。人稱後綴由 slot filling 提供,derived 表層經引擎導出。
#[test]
fn german_present_paradigm() {
    let (lang, art, reg) = setup();
    let paradigm = [
        ("suffix_1sg", "sage"),   // ich sage
        ("suffix_2sg", "sagst"),  // du sagst
        ("suffix_3sg", "sagt"),   // er/sie/es sagt
        ("suffix_1pl", "sagen"),  // wir sagen
        ("suffix_2pl", "sagt"),   // ihr sagt
        ("suffix_3pl", "sagen"),  // sie sagen
    ];
    for (suffix, expected) in paradigm {
        assert_eq!(
            conjugate(&lang, &art, &reg, suffix),
            expected,
            "{suffix} → {expected}"
        );
    }
}

/// optional 前綴 slot(`?`):填 auf- → 分離動詞;省略 → 光桿。
#[test]
fn optional_prefix_slot_marked_with_question_mark() {
    let (lang, art, reg) = setup();
    let with_prefix = construction::apply(
        &lang,
        &reg,
        "PresentVerb",
        &[("prefix", "auf"), ("stem", "sag"), ("suffix", "suffix_3pl")],
    )
    .unwrap();
    assert_eq!(construction::surface(&art.grammar.program, &with_prefix).unwrap(), "aufsagen");

    let without = construction::apply(
        &lang,
        &reg,
        "PresentVerb",
        &[("stem", "sag"), ("suffix", "suffix_3pl")],
    )
    .unwrap();
    assert_eq!(construction::surface(&art.grammar.program, &without).unwrap(), "sagen");
}

/// 環綴:模板字面素材(`ge{stem}t`)直通 → 過去分詞 gesagt。
#[test]
fn participle_circumfix_via_template_literals() {
    let (lang, art, reg) = setup();
    let tok = construction::apply(&lang, &reg, "Participle", &[("stem", "sag")]).unwrap();
    assert_eq!(construction::surface(&art.grammar.program, &tok).unwrap(), "gesagt");
}

/// derived token 是 Verb(承 construction 的 belongs 閉包);內部狀態可見。
#[test]
fn derived_token_carries_construction_syn_category() {
    let (lang, _art, reg) = setup();
    let tok = construction::apply(
        &lang,
        &reg,
        "PresentVerb",
        &[("stem", "sag"), ("suffix", "suffix_1sg")],
    )
    .unwrap();
    assert!(tok.syn_categories.contains(&"Verb".to_string()));
    assert!(tok.syn_categories.contains(&"Predicate".to_string()));
}

// ── valence = slots;部分套用 = 殘餘 valence(P41,slot mapping 基礎) ──

/// 只填詞幹 → 未飽和;殘餘 = [prefix(opt), suffix(req)];必填欠缺 = suffix。
#[test]
fn partial_application_leaves_residual_valence() {
    let (lang, art, reg) = setup();
    let tok = construction::apply(&lang, &reg, "PresentVerb", &[("stem", "sag")]).unwrap();
    assert!(!tok.is_saturated(), "缺必填 suffix → 未飽和");
    assert_eq!(tok.missing_required(), vec!["suffix".to_string()]);
    let residual: Vec<&str> = tok.residual_slots().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(residual, vec!["prefix", "suffix"]);
    // 未飽和 → 求表層報錯(不默默產出半形)
    assert!(matches!(
        construction::surface(&art.grammar.program, &tok),
        Err(CxgError::Unsaturated(ref m)) if m == &vec!["suffix".to_string()]
    ));
}

/// P42:construction application **不就地改**來源 sign——套用前後 slots 恆定。
#[test]
fn application_does_not_mutate_source_signs() {
    let (lang, art, reg) = setup();
    let before = construction::slots_of(lang.sign_named("PresentVerb").unwrap());
    let _ = conjugate(&lang, &art, &reg, "suffix_2sg");
    let after = construction::slots_of(lang.sign_named("PresentVerb").unwrap());
    assert_eq!(before, after, "來源 construction 的 slots 不得被套用改動(P42)");
    assert_eq!(before.len(), 3);
}

// ── syn validation(近失敗負例;不默默近似) ──

/// 範疇不匹配:把後綴填進 [VerbStem] slot → CategoryMismatch(P40 授權)。
#[test]
fn category_mismatch_is_rejected() {
    let (lang, _art, reg) = setup();
    let e = construction::apply(
        &lang,
        &reg,
        "PresentVerb",
        &[("stem", "suffix_2sg"), ("suffix", "suffix_2sg")],
    )
    .unwrap_err();
    assert!(
        matches!(e, CxgError::CategoryMismatch { ref slot, ref required, .. }
            if slot == "stem" && required == "VerbStem"),
        "{e:?}"
    );
}

#[test]
fn unknown_slot_duplicate_and_non_construction_are_rejected() {
    let (lang, _art, reg) = setup();
    // 未知 slot 名
    assert!(matches!(
        construction::apply(&lang, &reg, "PresentVerb", &[("xyz", "sag")]),
        Err(CxgError::UnknownSlot { .. })
    ));
    // 重複填同一 slot
    assert!(matches!(
        construction::apply(
            &lang,
            &reg,
            "PresentVerb",
            &[("stem", "sag"), ("stem", "sag")]
        ),
        Err(CxgError::DuplicateFill(_))
    ));
    // 詞彙 sign(無 slot)不是 construction
    assert!(matches!(
        construction::apply(&lang, &reg, "sag", &[]),
        Err(CxgError::NotAConstruction(_))
    ));
}

/// filler 授權跨繼承鏈:sag 的 syn 閉包 = [VerbStem, Verb, Predicate];
/// slot 要 [VerbStem] 命中葉、要 [Verb] 亦命中祖(P40 閉包成員)。
#[test]
fn filler_licensing_uses_belongs_closure() {
    let (lang, _art, reg) = setup();
    let cats = reg.sign_categories(lang.sign_named("sag").unwrap(), conlang_language::Dim::Syn);
    assert_eq!(cats, vec!["VerbStem", "Verb", "Predicate"]);
}

/// 固定素材:fixture round-trip 恆等(slot `?` 語法不破 P21)。
#[test]
fn fixture_round_trips() {
    let src = include_str!("fixtures/german_present.lang");
    let d1 = Language::parse(src).unwrap().dump();
    let d2 = Language::parse(&d1).unwrap().dump();
    assert_eq!(d1, d2);
    // slot `?` 保留於 canonical
    assert!(d1.contains("slot prefix [Prefix]?"), "optional `?` 應保留:\n{d1}");
    assert!(d1.contains("slot stem [VerbStem]\n"), "required 無 `?`");
}
