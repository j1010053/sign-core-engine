//! `ONTOLOGY_DEF_CONFLICT_RESOLVED` 報的 winner 必須等於引擎**實際採用**的值。
//!
//! 這條原本會紅:值的合併改走逐包解析(每個掛載的 trait 先在自己那層解完)之後,
//! 診斷卻還在用閉包攤平序自己算一次贏家——兩套算法在菱形上會給出不同答案,於是
//! 警告說「選了 1」而引擎實際用 0。診斷各算各的,就會變成誤導而不是幫助。

use conlang_language::{compile_system, Language, SignItem};

/// 回傳所有「警告報的值 ≠ 實際值」的路徑。
fn mismatches(src: &str) -> Vec<String> {
    let language = Language::parse(src).expect("parse");
    let system = compile_system(language.clone()).expect("compiles");
    let mut out = Vec::new();
    for diagnostic in system.validation.diagnostics() {
        if diagnostic.code != "ONTOLOGY_DEF_CONFLICT_RESOLVED" {
            continue;
        }
        // "{sign} {path} conflict resolved to {value:?} from {owner:?}"
        let mut head = diagnostic.message.splitn(3, ' ');
        let sign_name = head.next().unwrap_or_default();
        let path = head.next().unwrap_or_default();
        let reported = diagnostic
            .message
            .split("resolved to ")
            .nth(1)
            .and_then(|rest| rest.split(" from ").next())
            .map(|value| value.trim_matches('"').to_owned())
            .expect("訊息必須帶 winner");
        let Some(sign) = language.sign_named(sign_name) else {
            continue;
        };
        let actual = system
            .ontology
            .effective_sign(sign)
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                SignItem::Def(def) if def.path == path => Some(def.value.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "<無>".to_owned());
        if actual != reported {
            out.push(format!(
                "{sign_name}.{path}:報 {reported:?},實得 {actual:?}"
            ));
        }
    }
    out
}

const DIAMOND: &str = "\
trait A:
    syn:
        tam.present = 0

trait B:
    belongs A
    syn:
        tam.present = 1

trait C:
    belongs A

sign s:
    belongs B
    belongs C
";

/// 判準案例:菱形——共祖 `A` 帶值,`B` 覆寫,`C` 只是繼承。
#[test]
fn the_reported_winner_matches_the_effective_value_in_a_diamond() {
    assert!(mismatches(DIAMOND).is_empty(), "{:?}", mismatches(DIAMOND));
}

/// 換邊寫也必須一致——這條在的理由是:診斷若改成「照抄某個固定位置」也能讓
/// 上一條變綠,唯有兩種掛載順序都對得上,才證明它真的讀了合併結果。
#[test]
fn it_still_matches_when_the_mounts_are_written_the_other_way_round() {
    let flipped = DIAMOND.replace(
        "    belongs B\n    belongs C\n",
        "    belongs C\n    belongs B\n",
    );
    assert!(
        mismatches(&flipped).is_empty(),
        "{:?}",
        mismatches(&flipped)
    );
}

/// 深一點的鏈 + 並列。
#[test]
fn it_matches_for_a_deeper_chain_mounted_alongside_a_sibling() {
    let src = "\
trait A:
    syn:
        tam.present = 0

trait B:
    belongs A
    syn:
        tam.present = 1

trait C:
    belongs B

trait D:
    belongs A
    syn:
        tam.present = 2

sign s:
    belongs C
    belongs D
";
    assert!(mismatches(src).is_empty(), "{:?}", mismatches(src));
}

/// 正向控制組:兩個平行 trait 直接對撞時,警告本來就該出現——不能靠「不報任何
/// 警告」讓上面三條變綠。
#[test]
fn a_plain_two_trait_conflict_still_raises_the_warning() {
    let src = "\
trait Earlier:
    syn:
        tam.present = 0

trait Later:
    syn:
        tam.present = 1

sign s:
    belongs Earlier
    belongs Later
";
    let system = compile_system(Language::parse(src).expect("parse")).expect("compiles");
    assert!(
        system
            .validation
            .diagnostics()
            .iter()
            .any(|d| d.code == "ONTOLOGY_DEF_CONFLICT_RESOLVED"),
        "並列衝突必須有警告"
    );
    assert!(mismatches(src).is_empty());
}
