//! 步驟 21-1 出口:衍生家族是**兩張圖的接合**(流 D 框架 §1.1)。
//!
//! 這一組的判別性全在「兩張圖不是同一張」上:
//!
//! - `origin` 跨 sign,端點是 sign 名;
//! - `SenseEdge` 只在單一 sign 之內,端點是義項名。
//!
//! 若把兩者寫成一次遍歷,會得到「同 sign 內看得到、跨 sign 斷掉」的假象。

use conlang_language::{compile_system, CompiledSystem, DerivationKind, Language};
use conlang_query::derivation_family;

/// `bake` → `baker`、`bakery`;`bakery` 再生 `bakeries`。
/// `bake` 自己有兩個義項與一條義項邊。`cloud` 是無關的另一支。
const SOURCE: &str = r#"
sign bake:
    belongs Verb
    phon:
        /bejk/
    sem:
        senses:
            core = BAKE
            figurative = ENDURE_HEAT
        edges:
            figurative from core metaphor

sign baker:
    belongs Noun
    origin = sign(bake)
    phon:
        /bejkr/
    sem:
        senses:
            core = BAKER

sign bakery:
    belongs Noun
    origin = sign(bake)
    phon:
        /bejkri/
    sem:
        senses:
            core = BAKERY

sign bakeries:
    belongs Noun
    origin = sign(bakery)
    phon:
        /bejkriz/
    sem:
        senses:
            core = BAKERIES

sign cloud:
    belongs Noun
    phon:
        /klawd/
    sem:
        senses:
            core = CLOUD
"#;

fn system() -> CompiledSystem {
    compile_system(Language::parse(SOURCE).expect("parses")).expect("compiles")
}

fn members(system: &CompiledSystem, from: &str) -> Vec<String> {
    derivation_family(system, from)
        .nodes
        .into_iter()
        .map(|node| node.name)
        .collect()
}

// ── 圖一:跨 sign 的 origin 世系 ──────────────────────────────────────────

/// 🔑 家族是**雙向**的:從派生詞查得到兄弟,不只查得到祖先。
///
/// 判別性:只往上走的實作會回 `["baker", "bake"]`,看不到 `bakery`/`bakeries`。
#[test]
fn the_family_reaches_siblings_and_descendants_not_only_ancestors() {
    let system = system();
    assert_eq!(
        members(&system, "baker"),
        vec!["bake", "baker", "bakeries", "bakery"],
        "自 baker 出發應收齊整支家族"
    );
    // 自根出發結果相同——家族與從哪個成員問無關
    assert_eq!(members(&system, "baker"), members(&system, "bake"));
    assert_eq!(members(&system, "baker"), members(&system, "bakeries"));
}

/// 無關的支系不得混進來。
#[test]
fn an_unrelated_sign_forms_its_own_family() {
    let system = system();
    assert_eq!(members(&system, "cloud"), vec!["cloud"]);
    assert!(!members(&system, "bake").contains(&"cloud".to_owned()));
}

/// 查一個不存在的 sign 回空家族,**不是錯誤**——UI 端這是正常操作。
#[test]
fn querying_an_unknown_sign_yields_an_empty_family() {
    let family = derivation_family(&system(), "nonexistent");
    assert!(family.nodes.is_empty());
    assert_eq!(family.root, "nonexistent");
    assert!(family.dangling_origins.is_empty());
}

// ── 圖二:sign 內部的義項邊 ──────────────────────────────────────────────

/// 🔑 **義項邊掛在自己那個 sign 上,不會跨 sign 擴散**。
///
/// 判別性:若把 `SenseEdge` 當成跨 sign 的邊,`baker` 也會冒出 `figurative`
/// 這條邊——而它其實是 `bake` 內部的。
#[test]
fn sense_edges_stay_inside_the_sign_that_declares_them() {
    let family = derivation_family(&system(), "bake");

    let bake = family.nodes.iter().find(|n| n.name == "bake").unwrap();
    assert_eq!(bake.senses.len(), 1, "bake 有一條義項邊");
    assert_eq!(bake.senses[0].to, "figurative");
    assert_eq!(bake.senses[0].from, "core");
    assert_eq!(bake.senses[0].kind, DerivationKind::Metaphor);

    for other in family.nodes.iter().filter(|n| n.name != "bake") {
        assert!(
            other.senses.is_empty(),
            "{} 不該有義項邊——那是 bake 內部的",
            other.name
        );
    }
}

/// 兩張圖同時可見:世系有 origin、節點內有義項邊,兩者並存不互相取代。
#[test]
fn both_graphs_are_visible_on_the_same_result() {
    let family = derivation_family(&system(), "bake");
    let baker = family.nodes.iter().find(|n| n.name == "baker").unwrap();

    assert_eq!(baker.origin.as_deref(), Some("bake"), "跨 sign 那張圖");
    assert!(baker.senses.is_empty(), "sign 內那張圖是空的");
    assert_eq!(baker.gloss.as_deref(), Some("BAKER"));
    assert_eq!(baker.underlying_form.as_deref(), Some("bejkr"));

    let bake = family.nodes.iter().find(|n| n.name == "bake").unwrap();
    assert_eq!(bake.origin, None, "根沒有 origin");
    assert_eq!(bake.senses.len(), 1, "但它有義項邊");
}

// ── 壞資料不得靜默 ───────────────────────────────────────────────────────

/// **跨語言的 origin 現形**,不靜默丟棄。
///
/// `validate_origin_graph` 只對**非限定**名字要求本地存在
/// (`!origin.0.contains("::") && !names.contains(..)` → `META_ORIGIN_UNKNOWN`);
/// 帶 `::` 的限定名是刻意的逃生口——借詞的來源常是本專案外的語言。
/// 那種 origin 在本語言裡查無,家族因此會斷在這裡;吞掉它會讓斷掉的世系看起來完整。
#[test]
fn a_cross_language_origin_is_reported_rather_than_dropped() {
    let language = Language::parse(
        "sign window:\n    belongs Noun\n    origin = sign(oldnorse::vindauga)\n    phon:\n        /window/\n",
    )
    .expect("parses");
    let system = compile_system(language).expect("限定名不受本地存在檢查");

    let family = derivation_family(&system, "window");
    assert_eq!(family.dangling_origins, vec!["oldnorse::vindauga"]);
    assert_eq!(family.nodes.len(), 1, "本語言查無的祖先不算家族成員");
    assert_eq!(family.nodes[0].origin.as_deref(), Some("oldnorse::vindauga"));
}

/// 反向控制組:**非**限定的查無 origin 在編譯期就被擋下,到不了 Query 層。
///
/// 少了這條,上面那個測試無從說明「為什麼要留 `dangling_origins`」——
/// 若兩種形式都被擋,那個欄位就是造了一個產生不出來的狀態。
#[test]
fn an_unqualified_unknown_origin_is_rejected_at_compile_time() {
    let language = Language::parse(
        "sign loan:\n    belongs Noun\n    origin = sign(from_elsewhere)\n    phon:\n        /lon/\n",
    )
    .expect("parses");
    let error = compile_system(language).expect_err("非限定名必須存在於本地");
    assert!(
        format!("{error:?}").contains("META_ORIGIN_UNKNOWN"),
        "{error:?}"
    );
}

/// `origin` 成環在**編譯期**就被擋下(`META_ORIGIN_CYCLE`),進不到 Query 層。
///
/// `derivation_family` 內仍有環偵測(`members` 的 insert 回傳值),但那是**第二道
/// 防線**:公開 API 只吃 `CompiledSystem`,故它在正常路徑上不可達。
/// 這裡釘住第一道防線,而不是假裝 Query 層在擋——與 `role 約束的 reg.has()`
/// 那則誌誤同型:先確認既有把關在哪,再決定要不要補。
#[test]
fn a_cyclic_origin_chain_is_rejected_at_compile_time() {
    let language = Language::parse(
        "sign a:\n    belongs Noun\n    origin = sign(b)\n\
         sign b:\n    belongs Noun\n    origin = sign(a)\n",
    )
    .expect("parses");

    let error = compile_system(language).expect_err("成環必須擋下");
    assert!(
        format!("{error:?}").contains("META_ORIGIN_CYCLE"),
        "要指名是 origin 成環:{error:?}"
    );
}

/// 決定性:同輸入兩次逐欄位相同。
#[test]
fn the_same_input_yields_the_same_family() {
    let system = system();
    assert_eq!(
        derivation_family(&system, "baker"),
        derivation_family(&system, "baker")
    );
}
