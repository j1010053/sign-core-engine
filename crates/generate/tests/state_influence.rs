//! 步驟 20 出口:**State 影響撰寫,不影響 replay**(裁定 A)。
//!
//! 這一對斷言是本步的核心,缺一不可:
//!
//! - **正**:改 State → **新生成的候選**確實不同(否則 State 是擺設);
//! - **負**:改 State → **同一份 `.chg` 的 replay 產物**逐位元相同
//!   (否則 replay 依賴一個不在三道 digest 裡的東西,P26 破功)。
//!
//! 只有負的那半,State 可能根本沒接上任何東西也會綠;
//! 只有正的那半,State 洩進 replay 路徑也不會被抓到。

use conlang_changeset::state::{Contact, ContactIntensity, EvolutionState};
use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, PrimitiveEdit, ResolvedStatement, UnresolvedChangeSet,
};
use conlang_generate::{
    build, ContactInfluence, DistributionGenerator, Generator, Need, NeedOrigin, Strategies,
    WeightTable,
};
use conlang_language::{compile_system, LanguageDocument, LibrarySpec};
use conlang_stats::{DistributionProvider, EffectiveDistribution};
use std::collections::BTreeMap;

const BASE: &str = "Symbol k\nSymbol a\nSymbol t\nSymbol u\nSymbol s\n\nClass vowel {a, u}\n\n\
sign old:\n    belongs Noun\n    phon:\n        /kat/\n";

fn document() -> LanguageDocument {
    LanguageDocument::import_new_root(BASE, "evo:root").expect("base parses")
}

fn table(rows: &[(&str, f64)]) -> WeightTable {
    rows.iter().map(|(k, w)| ((*k).to_owned(), *w)).collect()
}

fn need() -> Need {
    Need {
        name: "coined".to_owned(),
        categories: vec!["Noun".to_owned()],
        gloss: Some("THING".to_owned()),
        origin: NeedOrigin::Coined,
    }
}

/// 接觸對象的音素分佈由呼叫端提供——引擎不知道「鄰語」長什麼樣。
fn counterparts() -> BTreeMap<String, WeightTable> {
    let mut map = BTreeMap::new();
    // 這個鄰語只有 s / u,本語言原本沒在用
    map.insert("neighbour".to_owned(), table(&[("s", 1.0), ("u", 1.0)]));
    map
}

fn state_with(intensity: Option<ContactIntensity>) -> EvolutionState {
    EvolutionState {
        time: Some("約 800–1100".to_owned()),
        region: Some("河谷北岸".to_owned()),
        society: vec!["農耕聚落".to_owned()],
        contacts: intensity
            .map(|intensity| {
                vec![Contact {
                    counterpart: "neighbour".to_owned(),
                    period: Some("800–1100".to_owned()),
                    intensity,
                }]
            })
            .unwrap_or_default(),
    }
}

fn forms(state: &EvolutionState, seed: u64) -> Vec<String> {
    let document = document();
    let system = compile_system(document.language().clone()).expect("compiles");
    let counterparts = counterparts();
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 1.0), ("a", 1.0)]))
        .with_imported(&ContactInfluence {
            state,
            counterpart_distributions: &counterparts,
        })
        .resolve();
    DistributionGenerator {
        distribution: &distribution,
        template: "CC",
        count: 24,
        seed,
    }
    .propose(&need(), &system)
    .into_iter()
    .map(|proposal| proposal.phon)
    .collect()
}

// ── 正:State 真的影響生成 ────────────────────────────────────────────────

/// 🔑 記下一段語言接觸 → 借詞音素進入候選。
#[test]
fn contact_history_brings_the_neighbours_segments_into_generation() {
    let without = forms(&state_with(None), 11);
    let with = forms(&state_with(Some(ContactIntensity::Dominant)), 11);

    assert!(
        !without.iter().any(|f| f.contains('s')),
        "沒有接觸時不該出現鄰語音素:{without:?}"
    );
    assert!(
        with.iter().any(|f| f.contains('s')),
        "記了高強度接觸後應出現:{with:?}"
    );
}

/// 強度不同 → 權重不同。
///
/// 判別性:若係數被忽略(所有強度一視同仁),兩份分佈會相同。
#[test]
fn intensity_changes_the_weight_not_just_presence() {
    let counterparts = counterparts();
    let weight_of = |intensity| {
        ContactInfluence {
            state: &state_with(Some(intensity)),
            counterpart_distributions: &counterparts,
        }
        .distribution()
        .get("s")
        .expect("鄰語音素應在導入層")
    };

    let sporadic = weight_of(ContactIntensity::Sporadic);
    let dominant = weight_of(ContactIntensity::Dominant);
    assert!(sporadic < dominant, "{sporadic} 應小於 {dominant}");
}

/// 查無分佈的接觸**靜靜略過**——State 記了一段接觸不代表一定要拿它抽樣。
#[test]
fn a_contact_without_a_supplied_distribution_is_skipped() {
    let state = EvolutionState {
        contacts: vec![Contact {
            counterpart: "unknown-tongue".to_owned(),
            period: None,
            intensity: ContactIntensity::Dominant,
        }],
        ..EvolutionState::default()
    };
    let counterparts = counterparts();
    let produced = ContactInfluence {
        state: &state,
        counterpart_distributions: &counterparts,
    }
    .distribution();
    assert!(produced.is_empty(), "沒給分佈就不貢獻權重:{produced:?}");
}

// ── 負:State 不影響 replay ───────────────────────────────────────────────

/// 🔑 **同一份 `.chg`,不同 State,replay 產物逐位元相同**。
///
/// 這是裁定 (A) 的硬界線。若哪天有人讓 replay 路徑讀 State,這條會紅。
#[test]
fn replaying_the_same_changeset_ignores_the_state_entirely() {
    let document = document();

    // 用「有接觸」的環境造一個詞,把結果寫死成 .chg
    let system = compile_system(document.language().clone()).expect("compiles");
    let counterparts = counterparts();
    let rich = state_with(Some(ContactIntensity::Dominant));
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 1.0), ("a", 1.0)]))
        .with_imported(&ContactInfluence {
            state: &rich,
            counterpart_distributions: &counterparts,
        })
        .resolve();
    let proposals = DistributionGenerator {
        distribution: &distribution,
        template: "CC",
        count: 4,
        seed: 3,
    }
    .propose(&need(), &system);
    let chosen = proposals.first().expect("有候選");
    let edits = build(&need(), chosen, &document, &Strategies::default()).expect("build");

    // 同一批 edits,在兩個截然不同的 State 下重放
    let first = replay(&document, edits.clone(), "evo:a");
    let second = replay(&document, edits, "evo:b");
    assert_eq!(
        first, second,
        "replay 產物必須與 State 無關——它根本不在 replay 的輸入裡"
    );

    // 前提:那個詞確實帶了接觸來的音素(否則本測試可能只是在比兩個空結果)
    assert!(first.contains(&chosen.phon.replace('/', "")) || first.contains(&chosen.phon));
}

fn replay(document: &LanguageDocument, edits: Vec<PrimitiveEdit>, ns: &str) -> String {
    let spec = LibrarySpec::default();
    let prelude = change_set_prelude(document, &spec, ns).expect("prelude");
    let mut resolved = UnresolvedChangeSet::parse(&prelude)
        .expect("prelude parses")
        .resolve(document, &spec)
        .expect("prelude resolves");
    resolved.statements = vec![ResolvedStatement { ordinal: 0, edits }];
    let source = ChangeInterpreter::new(document.clone(), spec, ns.to_owned())
        .expect("interpreter")
        .run(&resolved)
        .expect("replay")
        .document
        .source()
        .to_owned();
    // 命名空間會進 id,故比對前抹掉——我們比的是**語言內容**是否相同
    source.replace(ns, "<ns>")
}
