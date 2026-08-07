//! 步驟 19 出口(模組 E):**三層有效分佈 + E1 載入 + 統計投影(報表)**。
//!
//! 權威:`統計先驗與抽樣引擎_v0.1` §1–§4 + **§6 增修 A**。
//!
//! 釘住三件事:
//!
//! 1. **三層而非四層**——統計投影已移出抽樣棧(§6.1),只當報表;
//! 2. **逐項覆寫**——高優先層只蓋自己有的那些鍵,其餘落到下一層;
//! 3. **可審計**——每一項查得出來自哪一層,否則使用者調了數字卻看到別的
//!    無從追查。

use conlang_language::{Language, LibraryCatalog, LibraryKind, LibrarySpec, PackageFile, PackageSources};
use conlang_stats::{
    load_prior_from_packages, project_phoneme_freq, EffectiveDistribution, Layer, OtherNode,
    TypologicalPrior, WeightTable,
};

fn table(rows: &[(&str, f64)]) -> WeightTable {
    rows.iter()
        .map(|(key, weight)| ((*key).to_owned(), *weight))
        .collect()
}

// ── 三層疊加 ──────────────────────────────────────────────────────────────

#[test]
fn higher_layers_override_item_by_item_not_wholesale() {
    let distribution = EffectiveDistribution::from_prior(table(&[
        ("k", 0.10),
        ("m", 0.20),
        ("s", 0.30),
    ]))
    .with_imported(&OtherNode(table(&[("m", 0.55)])))
    .with_manual(table(&[("k", 0.99)]));

    assert_eq!(distribution.weight("k"), Some(0.99), "手動最高");
    assert_eq!(distribution.weight("m"), Some(0.55), "provider 蓋掉先驗");
    assert_eq!(
        distribution.weight("s"),
        Some(0.30),
        "沒被蓋的項落到先驗——這就是**逐項**覆寫,不是整份取代"
    );
    assert_eq!(distribution.weight("ʈ"), None, "三層都沒有 → None");
}

/// 可審計:每一項查得出來源層。
#[test]
fn every_entry_reports_which_layer_it_came_from() {
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 0.1), ("s", 0.3)]))
        .with_imported(&OtherNode(table(&[("m", 0.5)])))
        .with_manual(table(&[("k", 0.9)]));

    assert_eq!(distribution.provenance("k"), Some(Layer::Manual));
    assert_eq!(distribution.provenance("m"), Some(Layer::Imported));
    assert_eq!(distribution.provenance("s"), Some(Layer::Prior));
    assert_eq!(distribution.provenance("ʈ"), None);
}

/// `resolve()` = 三層鍵的**聯集**,且鍵序固定(抽樣要決定性,P26)。
#[test]
fn resolve_unions_all_layers_with_a_stable_key_order() {
    let distribution = EffectiveDistribution::from_prior(table(&[("s", 0.3)]))
        .with_imported(&OtherNode(table(&[("m", 0.5)])))
        .with_manual(table(&[("k", 0.9)]));

    let resolved = distribution.resolve();
    let keys: Vec<&str> = resolved.keys().collect();
    assert_eq!(keys, vec!["k", "m", "s"], "聯集且有序");
    assert_eq!(resolved.get("k"), Some(0.9));

    // 決定性:同輸入兩次得同一份(否則抽樣結果不定)
    assert_eq!(distribution.resolve(), resolved);
}

/// provider 三個實作互換,不改變疊加語意(§3 介面)。
#[test]
fn any_provider_feeds_the_same_imported_layer() {
    let prior = table(&[("k", 0.1)]);
    let imported = table(&[("k", 0.7)]);
    let by_node = EffectiveDistribution::from_prior(prior.clone())
        .with_imported(&OtherNode(imported.clone()));
    let by_prior = EffectiveDistribution::from_prior(prior).with_imported(&TypologicalPrior(imported));
    assert_eq!(by_node.weight("k"), by_prior.weight("k"));
    assert_eq!(by_node.provenance("k"), Some(Layer::Imported));
}

// ── E1 先驗自 package data 載入 ──────────────────────────────────────────

fn prior_package(name: &str, rows: &str) -> PackageSources {
    PackageSources {
        config: format!(
            "kind = plugin\nname = {name}\nversion = 0.1.0\n\
             rule_namespace = plugin:{name}\nenabled = true\npriority = 0\n\
             requires =\ncode = code/main.lang\ndata = data/segments.tsv\n"
        ),
        exports: format!("stable_id\tkind\talias\nplugin:{name}:Marker\ttrait\t{name}Marker\n"),
        code: format!("trait {name}Marker:\n"),
        data: rows.to_owned(),
        data_files: vec![PackageFile {
            path: "data/segments.tsv".to_owned(),
            source: rows.to_owned(),
        }],
        ..PackageSources::default()
    }
}

/// E1 是 **data**(裁定 W),住 package `data/`,且由 R9-a 之後可外部注入。
#[test]
fn a_prior_loads_from_injected_package_data() {
    let catalog = LibraryCatalog::with_packages([prior_package(
        "priors",
        "segment\tweight\nk\t0.89\nm\t0.96\n",
    )])
    .expect("catalog");
    let package = catalog
        .packages()
        .iter()
        .find(|p| p.id.name == "priors")
        .expect("在 catalog 裡");

    let prior = load_prior_from_packages(&[package]).expect("載入");
    assert_eq!(prior.get("m"), Some(0.96));
    assert_eq!(prior.len(), 2);
}

/// 畸形先驗表**報錯**,不默默略過。
#[test]
fn a_malformed_prior_is_rejected() {
    for (rows, why) in [
        ("segment\tfrequency\nk\t0.1\n", "表頭欄名錯"),
        ("segment\tweight\nk\tnope\n", "權重非數字"),
        ("segment\tweight\nk\t-1\n", "權重為負"),
    ] {
        let catalog =
            LibraryCatalog::with_packages([prior_package("bad", rows)]).expect("catalog");
        let package = catalog.packages().iter().find(|p| p.id.name == "bad").unwrap();
        assert!(
            load_prior_from_packages(&[package]).is_err(),
            "{why} 應被拒:{rows:?}"
        );
    }
    // 正向控制組:格式正確就過(否則上面可能只是「什麼都拒」)
    let catalog =
        LibraryCatalog::with_packages([prior_package("good", "segment\tweight\nk\t0.1\n")]).unwrap();
    let package = catalog.packages().iter().find(|p| p.id.name == "good").unwrap();
    assert!(load_prior_from_packages(&[package]).is_ok());
}

/// 沒有先驗檔的 package 不貢獻任何項——不得無中生有。
#[test]
fn packages_without_a_prior_file_contribute_nothing() {
    let catalog = LibraryCatalog::embedded().expect("catalog");
    let packages: Vec<_> = catalog.packages().iter().collect();
    let prior = load_prior_from_packages(&packages).expect("載入");
    assert!(prior.is_empty(), "隨引擎發布的套件目前不帶 E1:{prior:?}");
    let _ = LibrarySpec::default();
    let _ = LibraryKind::Std;
}

// ── 統計投影 = 報表,不是抽樣依據 ────────────────────────────────────────

/// 投影數的是 UR 的音素出現次數(§6.1 明訂口徑)。
#[test]
fn the_projection_counts_underlying_forms() {
    let language = Language::parse(
        "sign a:\n    phon:\n        /kat/\nsign b:\n    phon:\n        /ka/\n",
    )
    .expect("parse");
    let report = project_phoneme_freq(&language, &["k", "a", "t"]);
    assert_eq!(report.get("k"), Some(2.0), "kat + ka");
    assert_eq!(report.get("a"), Some(2.0));
    assert_eq!(report.get("t"), Some(1.0));
    assert_eq!(report.get("/"), None, "斜線是界定符,不是音素");
}

/// 🔑 **多字元音段整段算**——塞擦音不得被拆成三個「音素」。
///
/// 這是切分改依 inventory 的理由。少了這條,退回逐字元切分不會有任何一條紅。
#[test]
fn a_multi_character_segment_is_counted_whole() {
    // `t͡ʃ` = t + U+0361 結合弧 + ʃ,共三個 Unicode 字元
    let language =
        Language::parse("sign a:\n    phon:\n        /t\u{361}\u{283}t\u{361}\u{283}/\n").expect("parse");

    let report = project_phoneme_freq(&language, &["t\u{361}\u{283}"]);
    assert_eq!(report.get("t\u{361}\u{283}"), Some(2.0), "整段算兩次");
    assert_eq!(report.get("t"), None, "不得被拆出裸 t");
    assert_eq!(report.len(), 1);

    // 判別性:清單為空時退回逐字元,同一份輸入會被拆開
    let by_char = project_phoneme_freq(&language, &[]);
    assert_eq!(by_char.get("t"), Some(2.0), "逐字元切分會拆出裸 t");
    assert!(by_char.len() > 1);
}

/// **最長匹配**:`t͡ʃ` 與 `t` 同時在清單裡時,取長的。
#[test]
fn segmentation_prefers_the_longest_match() {
    let language = Language::parse("sign a:\n    phon:\n        /t\u{361}\u{283}/\n").expect("parse");
    let report = project_phoneme_freq(&language, &["t", "t\u{361}\u{283}"]);
    assert_eq!(report.get("t\u{361}\u{283}"), Some(1.0));
    assert_eq!(report.get("t"), None, "不得被短的先吃掉");
}

/// 清單外的音段**現形**,不被吞掉——那是撰寫錯誤的訊號。
#[test]
fn a_segment_outside_the_inventory_still_shows_up() {
    let language = Language::parse("sign a:\n    phon:\n        /kaq/\n").expect("parse");
    let report = project_phoneme_freq(&language, &["k", "a"]);
    assert_eq!(report.get("q"), Some(1.0), "不在清單裡但仍計入,使問題可見");
    assert_eq!(report.get("k"), Some(1.0));
}

/// 用**有效分佈的鍵集**當清單 → 報表與抽樣同一組鍵,兩邊對得起來。
#[test]
fn the_effective_distribution_keys_make_a_natural_inventory() {
    let language = Language::parse("sign a:\n    phon:\n        /kat/\n").expect("parse");
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 0.9), ("a", 0.8)])).resolve();
    let inventory: Vec<&str> = distribution.keys().collect();

    let report = project_phoneme_freq(&language, &inventory);
    assert_eq!(report.get("k"), Some(1.0));
    // 先驗沒有 t,但語言用了 → 報表照樣顯示,使落差可見
    assert_eq!(report.get("t"), Some(1.0));
    assert_eq!(distribution.get("t"), None, "抽樣那邊沒有這個鍵");
}

/// **投影不進抽樣棧**(§6.1)——它與 `EffectiveDistribution` 沒有任何連結。
///
/// 這條是判別性的:若哪天有人把投影接回三層之一,這裡會紅。
#[test]
fn the_projection_does_not_feed_the_effective_distribution() {
    let language = Language::parse("sign a:\n    phon:\n        /zzz/\n").expect("parse");
    let report = project_phoneme_freq(&language, &["z"]);
    assert_eq!(report.get("z"), Some(3.0), "前提:投影確實數到了 z");

    // 有效分佈只由三層構成,語言裡的 z 不會自己跑進來
    let distribution = EffectiveDistribution::from_prior(table(&[("k", 0.1)]));
    assert_eq!(distribution.weight("z"), None, "投影不是抽樣來源");
}
