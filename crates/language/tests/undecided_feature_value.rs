//! **未定案值域**:`FeatureValue` 的值是集合,`len >= 2` = 此主體在該維度尚未收斂。
//!
//! 語言學動機:單複同形的 *fish*、形式單數卻取複數一致的 *police*——同一個 sign
//! 在不同構式中收斂到不同的值是語言事實,不是建模錯誤。靠優先序挑一個贏家會把
//! 這個事實刪掉,所以這裡改成保留候選、把決議推遲到構式。
//!
//! 與宣告域的分工:`FeatureDecl.values` 說「哪些值**合法**」(型別),這裡的
//! `values` 說「這個主體**實際是**哪幾個」,恆為前者的子集。

use conlang_language::system::CompileSystemError;
use conlang_language::{compile_system, Language, SignItem};

fn parse(src: &str) -> Language {
    Language::parse(src).expect("parse")
}

fn feature_values(src: &str, sign: &str, name: &str) -> Vec<String> {
    parse(src)
        .sign_named(sign)
        .expect("sign")
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::FeatureValue(value) if value.name == name => Some(value.values.clone()),
            _ => None,
        })
        .expect("feature value")
}

const FISH: &str = "\
sign fish:
    syn:
        feature:
            number = enum(singular, plural)
            number = singular | plural
";

// ── 表面語法 ──────────────────────────────────────────────────────────────

#[test]
fn an_undecided_value_set_parses_and_round_trips() {
    let dump = parse(FISH).dump();
    assert_eq!(parse(&dump).dump(), dump, "多值賦值必須 round-trip 恆等");
    assert_eq!(feature_values(FISH, "fish", "number"), ["singular", "plural"]);
}

/// 控制組:已定案的單值 canonical form **逐位元不變**。這條在的理由是 library
/// lock digest——若單值也開始印 `|` 或改寫排版,沒用到多值的套件其 digest 會平白
/// 變動,P75 §3 b 保住的東西就破了。
#[test]
fn a_decided_value_prints_exactly_as_before() {
    let src = "sign john:\n    syn:\n        feature:\n            number = enum(singular, plural)\n            number = singular\n";
    assert_eq!(parse(src).dump(), src);
}

#[test]
fn duplicate_alternatives_are_rejected() {
    let src = "sign x:\n    syn:\n        feature:\n            number = enum(singular, plural)\n            number = singular | singular\n";
    assert!(Language::parse(src).is_err(), "重複候選必須被拒");
}

// ── 值域 ⊆ 宣告域 ─────────────────────────────────────────────────────────

/// 未定案時**每一個**候選都要落在宣告域內,不能只驗第一個——否則非法值會一路
/// 帶到構式層才爆,那時已經離宣告處很遠了。
#[test]
fn every_candidate_must_lie_inside_the_declared_domain() {
    let src = "sign x:\n    syn:\n        feature:\n            number = enum(singular, plural)\n            number = singular | dual\n";
    // Error 級診斷會讓 `compile_system` 直接回 `Err`,報告在錯誤裡。
    let error = compile_system(parse(src))
        .err()
        .expect("域外候選必須讓編譯失敗");
    let CompileSystemError::Validation(report) = error else {
        panic!("預期驗證失敗,實得 {error:?}");
    };
    let hit = report
        .diagnostics()
        .iter()
        .find(|d| d.code == "FEATURE_VALUE_OUT_OF_DOMAIN")
        .expect("必須有 FEATURE_VALUE_OUT_OF_DOMAIN");
    assert!(
        hit.message.contains("dual"),
        "訊息要點出是哪個候選出界:{}",
        hit.message
    );
}

/// 正向控制組:全部候選合法時不得誤報。
#[test]
fn a_legal_candidate_set_raises_nothing() {
    let system = compile_system(parse(FISH)).expect("compiles");
    let diagnostics = system.validation.diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == "FEATURE_VALUE_OUT_OF_DOMAIN"),
        "合法值域不得誤報:{diagnostics:?}"
    );
}

// ── 投影:並列聯集 vs 鏈上覆寫 ────────────────────────────────────────────

fn effective_number(src: &str, sign: &str) -> Vec<String> {
    let language = parse(src);
    let (registry, _) = conlang_language::ontology::OntologyRegistry::build(&[&language]);
    registry
        .effective_sign(language.sign_named(sign).expect("sign"))
        .items
        .iter()
        .find_map(|item| match item {
            SignItem::FeatureValue(value) if value.name == "number" => Some(value.values.clone()),
            _ => None,
        })
        .expect("number 必須有值")
}

const DECL: &str = "\
trait Agreeing:
    syn:
        feature:
            number = enum(singular, plural)
";

/// 兩個**並列**掛載的 trait 各說各話 → 未定案值域,而不是靜默挑一個。
#[test]
fn two_sibling_traits_that_disagree_leave_the_sign_undecided() {
    let src = format!(
        "{DECL}\
trait Sg:
    belongs Agreeing
    syn:
        feature:
            number = singular

trait Pl:
    belongs Agreeing
    syn:
        feature:
            number = plural

sign fish:
    belongs Sg
    belongs Pl
"
    );
    assert_eq!(effective_number(&src, "fish"), ["singular", "plural"]);
}

/// **順序無關**——這是整套設計不需要任何優先序機制(距離、書寫序、套件
/// priority)的直接證據:聯集可交換,換邊寫結果一樣。
#[test]
fn the_undecided_set_does_not_depend_on_the_order_of_the_mounts() {
    let one = format!(
        "{DECL}\
trait Sg:
    belongs Agreeing
    syn:
        feature:
            number = singular

trait Pl:
    belongs Agreeing
    syn:
        feature:
            number = plural

sign fish:
    belongs Sg
    belongs Pl
"
    );
    let other = one.replace(
        "    belongs Sg\n    belongs Pl\n",
        "    belongs Pl\n    belongs Sg\n",
    );
    let mut a = effective_number(&one, "fish");
    let mut b = effective_number(&other, "fish");
    a.sort();
    b.sort();
    assert_eq!(a, b, "候選集合不得隨掛載順序改變");
}

/// 反向控制組:**鏈上**的後代覆寫祖先是特化,不是分歧——不得聯集。
///
/// 少了這條,「聯集」會把「沒有覆寫」當成一次斷言,幾乎每個菱形都會憑空變成
/// 未定案,而全庫掃描顯示菱形絕大多數是無害的。
#[test]
fn a_descendant_overriding_an_ancestor_is_specialisation_not_disagreement() {
    let src = format!(
        "{DECL}\
trait Base:
    belongs Agreeing
    syn:
        feature:
            number = singular

trait Special:
    belongs Base
    syn:
        feature:
            number = plural

sign x:
    belongs Special
"
    );
    assert_eq!(effective_number(&src, "x"), ["plural"], "近者特化,不聯集");
}

/// **逐包解析的判準測試**:菱形——`CountNoun` 對 number 一個字都沒說(承繼
/// `Noun` 的 sg),`Collective` 覆寫成 pl。sign 掛兩者。
///
/// 正解是**未定案** `{singular, plural}`,因為 `CountNoun` 作為一個類確實承諾了
/// sg;而語言事實正是如此:*the police **are** coming* 對 *the police **is** ...*、
/// *the committee has* 對 *the committee have*,是英美與語域的真實分歧。
///
/// 攤平閉包 + 距離的舊模型會判 `plural`(距離 0 壓過距離 1),**把這個分歧無聲
/// 刪掉**。這條測試就是釘住兩個模型的差別。
#[test]
fn a_class_that_merely_inherits_still_counts_as_asserting_it() {
    let src = format!(
        "{DECL}\
trait Noun:
    belongs Agreeing
    syn:
        feature:
            number = singular

trait CountNoun:
    belongs Noun

trait Collective:
    belongs Noun
    syn:
        feature:
            number = plural

sign police:
    belongs CountNoun
    belongs Collective
"
    );
    let mut values = effective_number(&src, "police");
    values.sort();
    assert_eq!(
        values,
        ["plural", "singular"],
        "CountNoun 承繼的 sg 與 Collective 覆寫的 pl 並列 → 未定案"
    );
}

/// 反向控制組:同一棵鏈上單獨掛 `Collective` 時**必須定案**為 pl——覆寫在包內部
/// 就結束了,不得冒到 sign 這層假裝成分歧。
#[test]
fn a_single_mount_resolves_inside_its_own_bundle() {
    let src = format!(
        "{DECL}\
trait Noun:
    belongs Agreeing
    syn:
        feature:
            number = singular

trait Collective:
    belongs Noun
    syn:
        feature:
            number = plural

sign committee:
    belongs Collective
"
    );
    assert_eq!(effective_number(&src, "committee"), ["plural"]);
}

// ── 診斷 ──────────────────────────────────────────────────────────────────

fn diagnostics_of(src: &str) -> Vec<(String, String)> {
    let system = conlang_language::compile_system(parse(src)).expect("compiles");
    system
        .validation
        .diagnostics()
        .iter()
        .map(|d| (d.code.to_owned(), format!("{:?}", d.severity)))
        .collect()
}

const SIBLINGS: &str = "\
trait Agreeing:
    syn:
        feature:
            number = enum(singular, plural)

trait Sg:
    belongs Agreeing
    syn:
        feature:
            number = singular

trait Pl:
    belongs Agreeing
    syn:
        feature:
            number = plural

sign fish:
    belongs Sg
    belongs Pl
";

/// 並列分歧產生的未定案要被**報告**——作者多半沒意識到自己掛出了一個未定案。
///
/// 但級別是 **Info 不是 Warning**:未定案是合法狀態(*fish*、*police*),它報告
/// 的是一個推導出來的事實,不是要作者去修的缺陷。
#[test]
fn an_inferred_undecided_value_is_reported_as_info_not_a_defect() {
    let hit = diagnostics_of(SIBLINGS)
        .into_iter()
        .find(|(code, _)| code == "FEATURE_UNRESOLVED_ACROSS_TRAITS")
        .expect("並列分歧必須被報告");
    assert_eq!(hit.1, "Info", "未定案是合法狀態,不得升級成缺陷");
}

/// 反向控制組:sign 自己定案之後,就不該再報——P6 最高階已經回答了這個問題。
#[test]
fn deciding_it_on_the_sign_silences_the_report() {
    let decided = format!("{SIBLINGS}    syn:\n        feature:\n            number = singular\n");
    assert!(
        !diagnostics_of(&decided)
            .iter()
            .any(|(code, _)| code == "FEATURE_UNRESOLVED_ACROSS_TRAITS"),
        "sign 顯式定案後不得再報未定案"
    );
}
