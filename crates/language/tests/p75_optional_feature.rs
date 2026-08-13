//! P75 出口:**feature 宣告的 `?` = 可以沒有值**;沒有 `?` 時讀到缺席是執行期
//! Error 而非靜默 `Unmatched`。
//!
//! 這條補的是 P71 增修 D/E 關不到的一半:D/E 管「這條路徑合不合法」,P75 管
//! 「這條合法路徑在這個主體上沒有值時該怎麼辦」。原本兩者都塌成 `Unmatched`,
//! 於是缺席會依 P43 落進 `else` 分支——靜默地產出一個錯的值。
//!
//! 每條否定斷言都配正向控制組:`?` 版本必須仍然 `Unmatched`,否則這個檢查只是
//! 把「缺席」一律變成錯誤,而缺席本來就是合法的語言事實(如由外層構式填入的
//! occurrence feature)。

use conlang_language::synchronic::RuleStatus;
use conlang_language::{compile_system, Language};

/// 回傳 sign 求值後所有規則的 `(status, diag)`。
fn records(src: &str, sign: &str) -> Vec<(RuleStatus, Option<String>)> {
    let system = compile_system(Language::parse(src).expect("parses")).expect("compiles");
    system
        .evaluate_sign(sign)
        .expect("sign evaluates")
        .records
        .into_iter()
        .map(|record| (record.status, record.diag))
        .collect()
}

fn diag_containing(src: &str, sign: &str, needle: &str) -> Option<String> {
    records(src, sign)
        .into_iter()
        .filter_map(|(status, diag)| (status == RuleStatus::Error).then_some(diag).flatten())
        .find(|diag| diag.contains(needle))
}

// ── 值表達式的讀取 ────────────────────────────────────────────────────────

/// 沒有 `?`:宣告過但沒有值 → Error,而且訊息要說出 `?` 是正解。
#[test]
fn reading_an_unset_required_feature_is_an_error_that_points_at_the_marker() {
    let src = "sign s:\n    syn:\n        feature:\n            source = enum(a, b)\n            copy = enum(a, b)\n            copy => $self.syn.source\n";
    let diag = diag_containing(src, "s", "syn.source")
        .unwrap_or_else(|| panic!("缺席未報 Error:{:?}", records(src, "s")));
    assert!(diag.contains('?'), "訊息要指出 `?` 才是正解:{diag}");
    assert!(diag.contains("s\""), "訊息要指出是哪個 sign:{diag}");
}

/// **正向控制組**:同一份來源加上 `?` → 回到 `Unmatched`(可落 else),不是 Error。
/// 少了這條,上面那條可能只是「缺席一律變錯誤」,而那會擋掉合法的語言事實。
#[test]
fn the_marker_restores_unmatched() {
    let src = "sign s:\n    syn:\n        feature:\n            source = enum(a, b)?\n            copy = enum(a, b)\n            copy => $self.syn.source\n";
    let statuses: Vec<_> = records(src, "s").into_iter().map(|(s, _)| s).collect();
    assert!(
        !statuses.contains(&RuleStatus::Error),
        "`?` 宣告的 feature 缺席不該是 Error:{:?}",
        records(src, "s")
    );
    assert!(statuses.contains(&RuleStatus::Unmatched), "{statuses:?}");
}

/// `?` 讓 `else` 分支照常拿得到控制權——這正是「缺席即預設值」的合法寫法。
#[test]
fn an_optional_feature_falls_through_to_else() {
    let src = "sign s:\n    syn:\n        feature:\n            source = enum(a, b)?\n            copy = enum(a, b)\n            copy => $self.syn.source\n                else copy => b\n";
    let system = compile_system(Language::parse(src).expect("parses")).expect("compiles");
    let evaluated = system.evaluate_sign("s").expect("evaluates");
    let syn = evaluated
        .sign
        .project(conlang_language::Dim::Syn, &system.ontology);
    assert_eq!(syn.get("syn.copy"), Some("b"), "else 應接手");
}

// ── guard 的讀取 ──────────────────────────────────────────────────────────

/// guard 讀到缺席同樣是 Error(否則規則永遠不觸發而作者沒有訊號)。
#[test]
fn a_guard_reading_an_unset_required_feature_is_an_error() {
    let src = "sign s:\n    syn:\n        feature:\n            source = enum(a, b)\n            copy = enum(a, b)\n            copy => a / $self.syn.source == a\n";
    assert!(
        diag_containing(src, "s", "syn.source").is_some(),
        "{:?}",
        records(src, "s")
    );
    // 正向控制組
    let optional = src.replace("source = enum(a, b)", "source = enum(a, b)?");
    assert!(
        diag_containing(&optional, "s", "syn.source").is_none(),
        "{:?}",
        records(&optional, "s")
    );
}

// ── 範圍:裁定 (a) 限 typed feature ────────────────────────────────────────

/// 封閉清單座標**沒有宣告處**可掛 `?`,故維持缺席容忍(Unmatched),不受 P75 影響。
/// 收緊它屬 Phase 2 的座標宣告機制。
#[test]
fn closed_list_coordinates_stay_absence_tolerant() {
    let src = "sign s:\n    syn:\n        feature:\n            copy = enum(a, b)\n            copy => a / tam.present == 1\n";
    let statuses: Vec<_> = records(src, "s").into_iter().map(|(s, _)| s).collect();
    assert!(
        !statuses.contains(&RuleStatus::Error),
        "座標缺席不該是 Error(範圍限 feature):{:?}",
        records(src, "s")
    );
}

// ── canonical 可省略(裁定 b:零 digest churn 的證據) ───────────────────────

/// 沒寫 `?` 的宣告,canonical form **逐字不變**——這是「未使用此語法的套件
/// digest 不變」的直接證據,不是推論。
#[test]
fn the_marker_is_omitted_from_canonical_form_when_unset() {
    let plain = "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n";
    let dumped = Language::parse(plain).expect("parses").dump();
    assert!(
        dumped.contains("n = enum(sg, pl)\n"),
        "沒有 `?` 的宣告不該印出 `?`:{dumped}"
    );
    assert!(!dumped.contains("enum(sg, pl)?"), "{dumped}");
}

/// 寫了 `?` 就印得出來,而且 round-trip 恆等(canonical 是不動點)。
#[test]
fn the_marker_round_trips() {
    let src = "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)?\n";
    let once = Language::parse(src).expect("parses").dump();
    assert!(once.contains("n = enum(sg, pl)?"), "{once}");
    let twice = Language::parse(&once).expect("re-parses").dump();
    assert_eq!(once, twice, "canonical 必須是不動點");
}

/// `?` 只對宣告有意義;貼到賦值上要明確拒絕,而不是默默忽略。
#[test]
fn the_marker_is_rejected_on_an_assignment() {
    let error = Language::parse(
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n = sg?\n",
    )
    .expect_err("賦值不接受 `?`");
    assert!(
        format!("{error:?}").contains("declaration"),
        "訊息要說明 `?` 屬於宣告:{error:?}"
    );
}

// ── `$slot`:宣告住在 filler 上,規約相同 ───────────────────────────────────

const SLOT_SRC: &str = "\
Symbol x
Class vowel {x}

trait Fillable:
    syn:
        feature:
            supplied = enum(a, b)MARK

sign filler:
    belongs Fillable
    phon:
        /x/

sign Cxn:
    syn:
        slots:
            head [Fillable]
        feature:
            copy = enum(a, b)
            copy => $slot.head.syn.supplied
    phon:
        /{head}/
";

fn derive_diags(mark: &str) -> Vec<String> {
    use conlang_language::construction::{SlotFiller, SlotMap};
    let src = SLOT_SRC.replace("MARK", mark);
    let system = compile_system(Language::parse(&src).expect("parses")).expect("compiles");
    let token = system
        .derive(
            "Cxn",
            &[SlotFiller::sign("head", "filler")],
            &SlotMap::identity(),
        )
        .expect("derives");
    token
        .rules
        .iter()
        .map(|unit| &unit.record)
        .filter(|record| record.status == RuleStatus::Error)
        .filter_map(|record| record.diag.clone())
        .collect()
}

/// filler 宣告過(無 `?`)但沒有值 → Error,訊息指出是哪個 filler。
#[test]
fn reading_an_unset_required_filler_feature_is_an_error() {
    let diags = derive_diags("");
    let diag = diags
        .iter()
        .find(|diag| diag.contains("syn.supplied"))
        .unwrap_or_else(|| panic!("filler 缺席未報 Error:{diags:?}"));
    assert!(diag.contains("filler"), "訊息要指出是哪個 filler:{diag}");
    assert!(diag.contains('?'), "訊息要指路:{diag}");
}

/// **正向控制組**:filler 的宣告加上 `?` → 回到 `Unmatched`。
#[test]
fn the_marker_on_the_filler_restores_unmatched() {
    let diags = derive_diags("?");
    assert!(
        !diags.iter().any(|diag| diag.contains("syn.supplied")),
        "`?` 宣告的 filler feature 缺席不該是 Error:{diags:?}"
    );
}
