//! `$` 引用在 `.lang` 表面的契約:**主體一律顯式**。
//!
//! 曾經有三種裸寫法(constraint 運算元、case scrutinee 讀 slot、case
//! scrutinee 讀自己),主體靠「首段是不是維度關鍵字」猜。本檔釘住那些寫法
//! 現在都被擋在入口,而顯式形照常運作。

use conlang_language::{compile_system, Language};

/// 兩個 slot、一個 phon 模板。`phon_tail` 縮排在 `phon:` 之下
/// (realization case 住那裡),`sign_tail` 在 sign 層(constraints 住那裡)。
fn source(phon_tail: &str, sign_tail: &str) -> String {
    format!(
        r#"Symbol a
Symbol b

trait RefEntity:
    syn:
        feature:
            number = enum(singular, plural)

sign one:
    belongs RefEntity
    phon:
        /a/

sign two:
    belongs RefEntity
    phon:
        /b/

sign pair:
    belongs RefEntity
    syn:
        slots:
            head [RefEntity]
            tail [RefEntity]
    phon:
        /{{$slot.head}}{{$slot.tail}}/
{phon_tail}{sign_tail}"#
    )
}

fn compiles(phon_tail: &str, sign_tail: &str) -> Result<(), String> {
    let language =
        Language::parse(&source(phon_tail, sign_tail)).map_err(|error| error.to_string())?;
    compile_system(language)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// `realization:` 底下一個以 `scrutinee` 為判準的 phon case。
/// scrutinee + `== VALUE` 一律是**純量**比對(範疇比對走 guard 形)。
fn realization(scrutinee: &str) -> String {
    format!(
        "        realization:\n            case {scrutinee}:\n                == singular:\n                    /{{$slot.head}}/\n                else:\n                    /{{$slot.head}}{{$slot.tail}}/\n"
    )
}

/// 範疇比對的正解:guard 形 case,`[Trait]` 是既有的範疇記法。
fn category_realization(guard: &str) -> String {
    format!(
        "        realization:\n            case:\n                {guard}:\n                    /{{$slot.head}}/\n                else:\n                    /{{$slot.head}}{{$slot.tail}}/\n"
    )
}

// ── constraint 運算元 ────────────────────────────────────────────────────

#[test]
fn an_explicit_constraint_operand_compiles() {
    compiles(
        "",
        "    constraints:\n        before($slot.head, $slot.tail)\n",
    )
    .expect("顯式形合法");
    compiles(
        "",
        "    constraints:\n        equal($slot.head.syn.number, $slot.tail.syn.number)\n",
    )
    .expect("顯式欄位運算元合法");
}

#[test]
fn a_bare_constraint_operand_is_rejected() {
    compiles("", "    constraints:\n        before(head, tail)\n").expect_err("裸運算元不得再通過");
}

#[test]
fn a_bare_field_constraint_operand_is_rejected() {
    compiles(
        "",
        "    constraints:\n        equal(head.syn.number, tail.syn.number)\n",
    )
    .expect_err("裸欄位運算元不得再通過");
}

// ── case scrutinee ──────────────────────────────────────────────────────

#[test]
fn an_explicit_scrutinee_compiles() {
    compiles(&realization("$slot.head.syn.number"), "")
        .expect("`$slot.NAME.DIM.FIELD` 是合法 scrutinee");
    compiles(&realization("$self.syn.number"), "").expect("`$self.DIM.FIELD` 是合法 scrutinee");
}

/// P78(修訂):scrutinee 搭配的是 `== VALUE` **純量**比對,故欄位必填。
/// `$slot.NAME.phon` 不再兼差當範疇比對——`.phon` 就是維度,缺欄位即錯誤,
/// 且錯在 **compile 期**而非求值期。
#[test]
fn a_dimension_without_a_field_is_not_a_category_test() {
    compiles(&realization("$slot.head.phon"), "")
        .expect_err("`$slot.NAME.phon` 缺欄位,不得再被解讀成範疇比對");
}

/// 範疇比對的正解:`[Trait]` guard。這是全庫既有的範疇記法
/// (slot 約束、function 參數、`$self == [Trait]` 都用它)。
#[test]
fn a_category_test_uses_the_bracket_guard_form() {
    compiles(&category_realization("$slot.head == [RefEntity]"), "")
        .expect("`$slot.NAME == [Trait]` 是範疇比對的記法");
}

/// scrutinee 讀 slot 的**非 phon 欄位**——與 `$self.<dim>.<field>` 對稱。
///
/// 這一形先前不成立,但那是實作債而非規格:`FillerSnapshot` 一直帶著各維的
/// 純量欄位,只是 scrutinee 沒去讀。沒有任何規格規定 scrutinee 只收 `.phon`。
#[test]
fn a_slot_scrutinee_may_read_a_scalar_field_not_only_phon() {
    let source = source(
        "",
        "    syn:\n        feature:\n            outcome = enum(hit, miss)\n            outcome =>\n",
    );
    // 直接組一份 construction:讀 filler 的 syn.number 決定自己的 outcome。
    let source = source.replace(
        "            outcome =>\n",
        "            outcome =>\n                case $slot.head.syn.number:\n                    == singular:\n                        hit\n                    else:\n                        miss\n",
    );
    let language = Language::parse(&source).expect("parse");
    compile_system(language).expect("`$slot.NAME.syn.FIELD` 應為合法 scrutinee");
}

#[test]
fn a_bare_slot_scrutinee_is_rejected() {
    compiles(&realization("head.phon"), "").expect_err("裸 slot scrutinee 不得再通過");
}

/// 首段猜測消失的直接後果:一個**叫 `syn` 的 slot** 不再被靜默讀成
/// 自己的 syn 維——因為根本沒有「省略主體」這回事了。
#[test]
fn a_slot_named_after_a_dimension_is_addressable() {
    let source = source(&realization("$slot.syn.syn.number"), "")
        .replace(
            "            head [RefEntity]",
            "            syn [RefEntity]",
        )
        .replace("{$slot.head}", "{$slot.syn}");
    let language = Language::parse(&source).expect("parse");
    compile_system(language).expect("`$slot.syn` 就是那個 slot,不是自己的 syn 維");
}

// ── canonical ───────────────────────────────────────────────────────────

#[test]
fn the_explicit_spelling_survives_a_round_trip() {
    let source = source(
        "",
        "    constraints:\n        before($slot.head, $slot.tail)\n        equal($slot.head.syn.number, $slot.tail.syn.number)\n",
    );
    let language = Language::parse(&source).expect("parse");
    let canonical = language.dump();
    assert!(
        canonical.contains("before($slot.head, $slot.tail)"),
        "{canonical}"
    );
    assert!(
        canonical.contains("equal($slot.head.syn.number, $slot.tail.syn.number)"),
        "{canonical}"
    );
    assert_eq!(
        Language::parse(&canonical).expect("re-parse").dump(),
        canonical,
        "不動點"
    );
}

// ── P75 增修 A:構式內部不回指構式本身 ────────────────────────────────

/// phon 模板**就是**這個 sign 的形式,`{$self}` 等於把自己的 surface 嵌進
/// 自己的 surface——無條件遞迴。訊息必須講這個,不能落到 unknown-slot
/// (那會讓作者去找一個叫 `$self` 的 slot)。
#[test]
fn a_self_interpolation_in_a_phon_template_names_the_recursion() {
    let error = compiles("", "").err().map(|e| e).unwrap_or_default();
    assert!(error.is_empty(), "基準案例應可編譯:{error}");

    let source = source("", "").replace("/{$slot.head}{$slot.tail}/", "/{$self}/");
    let language = Language::parse(&source).expect("parse");
    let error = format!(
        "{}",
        compile_system(language).expect_err("`{$self}` 不得合法")
    );
    assert!(
        error.contains("TEMPLATE_INVALID") || error.contains("M1++"),
        "{error}"
    );
}

/// role 的填充者是某個 slot,不會是構式自己。
#[test]
fn a_role_cannot_be_filled_by_the_construction_itself() {
    let source = source(
        "",
        "    sem:\n        roles:\n            agent [RefEntity]\n            agent = {$self}\n",
    );
    let error = Language::parse(&source).expect_err("`{$self}` 不得是 role 的填充者");
    assert!(
        format!("{error}").contains("cannot be filled by the construction itself"),
        "訊息要講回指,不能只講形狀:{error}"
    );
}

/// **不誤傷反身**:兩個 role 綁**同一個 slot** 是正常的反身構式,
/// 與「回指構式本身」無關。
#[test]
fn two_roles_sharing_one_slot_is_still_legal() {
    compiles(
        "",
        "    sem:\n        roles:\n            agent [RefEntity]\n            patient [RefEntity]\n            agent = {$slot.head}\n            patient = {$slot.head}\n",
    )
    .expect("反身:兩個 role 綁同一 slot");
}

/// **不誤傷把自己傳給別的構式**:`f({$self})` 不是內部回指,是往外傳。
/// 傳給自己才是環,由 `APPLICATION_CYCLE` 擋。
#[test]
fn passing_self_as_an_argument_to_another_construction_is_legal() {
    let source = r#"Symbol a
Class vowel {a}

trait RefThing:

sign wrapper:
    belongs RefThing
    syn:
        slots:
            stem [RefThing]
    phon:
        /{$slot.stem}a/
    sem:
        senses:
            core = W

sign word:
    belongs RefThing
    phon:
        /a/
    case:
        else:
            wrapper({$self})
"#;
    let language = Language::parse(source).expect("parse");
    compile_system(language).expect("把自己傳給別的 construction 是合法的");
}

/// 傳給**自己**才是回指:環偵測擋下。
#[test]
fn passing_self_to_itself_is_a_cycle() {
    let source = r#"Symbol a
Class vowel {a}

trait RefThing:

sign word:
    belongs RefThing
    syn:
        slots:
            stem [RefThing]
    phon:
        /{$slot.stem}a/
    sem:
        senses:
            core = W
    case:
        else:
            word({$self})
"#;
    let language = Language::parse(source).expect("parse");
    let error = format!("{}", compile_system(language).expect_err("自我套用是環"));
    assert!(
        error.contains("APPLICATION_CYCLE") || error.contains("M1++"),
        "{error}"
    );
}
