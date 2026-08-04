//! 步驟 19 的流 C 接點:**Generator 依有效分佈抽樣 + 注入式 phonotactics 過濾**。
//!
//! 流 C 圖上那格寫的是「Generator(唯讀 Language **+ E 抽樣**)」——抽樣在提議側。
//! 本檔釘住那個接點,以及 §4 的職責分工:**E 只管加權,硬約束事後過濾**。
//!
//! 出口目標(§6.1 修訂後):造詞「像**使用者宣告的分佈**」
//! ——不再是「像這個語言」,因為投影已退出抽樣棧。

use conlang_generate::{
    admissible, build, AdmitAll, DistributionGenerator, Generator, Need, NeedOrigin,
    PhonotacticFilter, Proposal, Strategies,
};
use conlang_language::{compile_system, LanguageDocument};
use conlang_stats::{EffectiveDistribution, OtherNode, WeightTable};

const BASE: &str = "Symbol k\nSymbol a\nSymbol t\nSymbol u\n\nClass vowel {a, u}\n";

fn table(rows: &[(&str, f64)]) -> WeightTable {
    rows.iter().map(|(k, w)| ((*k).to_owned(), *w)).collect()
}

fn need() -> Need {
    Need {
        name: "coined".to_owned(),
        categories: vec!["Noun".to_owned()],
        gloss: Some("RIVER".to_owned()),
        origin: NeedOrigin::Coined,
    }
}

/// 只收含 `a` 的形式——刻意簡單,重點是它**由外部注入**。
#[derive(Debug)]
struct RequiresVowelA;

impl PhonotacticFilter for RequiresVowelA {
    fn admits(&self, phon: &str) -> bool {
        phon.contains('a')
    }
}

#[test]
fn generated_forms_are_drawn_from_the_declared_distribution() {
    let document = LanguageDocument::import_new_root(BASE, "evo:root").expect("base");
    let system = compile_system(document.language().clone()).expect("compiles");
    // 分佈裡只有 k / a —— 抽出來的形式不該出現 t 或 u
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 1.0), ("a", 1.0)])).resolve();

    let generator = DistributionGenerator {
        distribution: &distribution,
        template: "CC",
        count: 12,
        seed: 7,
    };
    let proposals = generator.propose(&need(), &system);
    assert_eq!(proposals.len(), 12);
    for proposal in &proposals {
        let form = proposal.phon.trim_matches('/');
        assert!(
            form.chars().all(|c| c == 'k' || c == 'a'),
            "只能抽到分佈裡有的音素:{}",
            proposal.phon
        );
    }
}

/// 手動覆寫真的改變產出——三層棧接到了提議側,不是擺著好看。
#[test]
fn a_manual_override_changes_what_gets_generated() {
    let document = LanguageDocument::import_new_root(BASE, "evo:root").expect("base");
    let system = compile_system(document.language().clone()).expect("compiles");

    // 先驗兩者等權;手動把 k 壓到 0 → 產出應只剩 a
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 1.0), ("a", 1.0)]))
        .with_manual(table(&[("k", 0.0)]))
        .resolve();
    let generator = DistributionGenerator {
        distribution: &distribution,
        template: "CC",
        count: 20,
        seed: 3,
    };
    let forms: Vec<String> = generator
        .propose(&need(), &system)
        .into_iter()
        .map(|p| p.phon)
        .collect();
    assert!(
        forms.iter().all(|f| !f.contains('k')),
        "手動壓成 0 權重的音素不該出現:{forms:?}"
    );

    // 判別性:不覆寫時 k 會出現(否則上面可能只是模板不含 k)
    let baseline = EffectiveDistribution::from_prior(table(&[("k", 1.0), ("a", 1.0)])).resolve();
    let generator = DistributionGenerator {
        distribution: &baseline,
        template: "CC",
        count: 20,
        seed: 3,
    };
    assert!(generator
        .propose(&need(), &system)
        .iter()
        .any(|p| p.phon.contains('k')));
}

/// provider 導入層同樣接得上。
#[test]
fn an_imported_distribution_reaches_the_generator() {
    let document = LanguageDocument::import_new_root(BASE, "evo:root").expect("base");
    let system = compile_system(document.language().clone()).expect("compiles");
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 1.0)]))
        .with_imported(&OtherNode(table(&[("k", 0.0), ("u", 1.0)])))
        .resolve();
    let generator = DistributionGenerator {
        distribution: &distribution,
        template: "C",
        count: 8,
        seed: 11,
    };
    let forms: Vec<String> = generator
        .propose(&need(), &system)
        .into_iter()
        .map(|p| p.phon)
        .collect();
    assert!(forms.iter().all(|f| f.contains('u')), "{forms:?}");
}

/// 決定性(P26):同 seed 同分佈 ⇒ 逐位元同結果。
#[test]
fn generation_is_reproducible_from_the_seed() {
    let document = LanguageDocument::import_new_root(BASE, "evo:root").expect("base");
    let system = compile_system(document.language().clone()).expect("compiles");
    let distribution =
        EffectiveDistribution::from_prior(table(&[("k", 1.0), ("a", 2.0), ("t", 1.0)])).resolve();
    let make = |seed| {
        DistributionGenerator {
            distribution: &distribution,
            template: "CCC",
            count: 6,
            seed,
        }
        .propose(&need(), &system)
    };
    assert_eq!(make(42), make(42), "同 seed 必須逐位元相同");
    assert_ne!(make(42), make(43), "不同 seed 應給不同結果");
}

// ── §4 職責分工:E 只加權,硬約束事後過濾 ───────────────────────────────

#[test]
fn the_filter_is_injected_and_applied_after_sampling() {
    let document = LanguageDocument::import_new_root(BASE, "evo:root").expect("base");
    let system = compile_system(document.language().clone()).expect("compiles");
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 1.0), ("a", 1.0)])).resolve();
    let generator = DistributionGenerator {
        distribution: &distribution,
        template: "CC",
        count: 24,
        seed: 5,
    };

    let proposed = generator.propose(&need(), &system);
    let kept = admissible(proposed.clone(), &RequiresVowelA);

    // 「提了幾個、擋掉幾個」是自動模式要能審計的數字——故過濾在列舉之後,
    // 不藏進 Generator 內部。
    assert!(kept.len() < proposed.len(), "確實擋掉了一些");
    assert!(!kept.is_empty(), "也不是全擋");
    assert!(kept.iter().all(|p| p.phon.contains('a')));

    // 判別性:換成全放行的過濾器,一個都不少
    assert_eq!(admissible(proposed.clone(), &AdmitAll).len(), proposed.len());
}

/// 🔑 端到端:分佈 → 提議 → 過濾 → Builder → 四原語。
#[test]
fn a_distribution_drives_a_coinage_all_the_way_to_primitive_edits() {
    let document = LanguageDocument::import_new_root(BASE, "evo:root").expect("base");
    let system = compile_system(document.language().clone()).expect("compiles");
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 1.0), ("a", 1.0)])).resolve();

    let proposals = DistributionGenerator {
        distribution: &distribution,
        template: "CaC",
        count: 5,
        seed: 9,
    }
    .propose(&need(), &system);
    let kept = admissible(proposals, &RequiresVowelA);
    let chosen: &Proposal = kept.first().expect("有可用候選");

    let edits = build(&need(), chosen, &document, &Strategies::default()).expect("build");
    assert_eq!(edits.len(), 1, "一個新 sign = 一個 Insert");
}
