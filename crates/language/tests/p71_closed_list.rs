//! P71 出口:**`Def` 路徑封閉清單** + `feature:` 分工 + §7 A1 規則目標同受約束。
//!
//! 這裡釘的是「關掉逃生口」本身。P71 §2.1 記載的兩個實測後果——`reanalyze` 寫入
//! 無人讀取的 `syn.category`、guard 欄位名打錯靜默 `false`——根因都是
//! `valid_dim` 只檢查路徑**長得像** `<dim>.<field>`,欄位名不查、值不查。
//! 寫入端由 §4.2 與 §7 A1 關上,讀取端由 **§10 增修 D(guard)**與
//! **§11 增修 E(值表達式)**關上,見本檔末兩段。
//!
//! 每一條否定斷言都配一條**正向控制組**:否則「因為路徑根本不存在而過」與
//! 「因為規則正確而過」在測試上無法區分(P71 §7.4 明列的假綠燈形態)。

use conlang_language::{check_language, Language, Severity};

/// 回傳所有 **Error** 級診斷的 `(code, message)`。
fn errors(src: &str) -> Vec<(String, String)> {
    let language = Language::parse(src).expect("fixture parses");
    check_language(&language)
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| (diagnostic.code.to_string(), diagnostic.message.clone()))
        .collect()
}

fn has(src: &str, code: &str) -> bool {
    errors(src).iter().any(|(actual, _)| actual == code)
}

// ── R1:Def 路徑封閉 ────────────────────────────────────────────────────────

#[test]
fn an_author_invented_def_path_is_rejected_and_points_at_feature() {
    let found = errors("sign s:\n    syn:\n        category = noun\n");
    let (_, message) = found
        .iter()
        .find(|(code, _)| code == "DEF_INVALID_PATH_OR_VALUE")
        .unwrap_or_else(|| panic!("自造 Def 路徑必須被拒:{found:?}"));
    // §4.2:訊息**必須指向 `feature:`**,否則作者只看到 invalid Definition 而不知正解。
    assert!(
        message.contains("feature:"),
        "訊息要指路到 `feature:`:{message}"
    );
    assert!(message.contains("syn.category"), "訊息要指出是哪條路徑:{message}");
}

/// 三個維度都關,不是只關 syn。
#[test]
fn the_closed_list_applies_to_every_authorable_dimension() {
    for src in [
        "sign s:\n    syn:\n        invented = x\n",
        "sign s:\n    sem:\n        invented = x\n",
        "sign s:\n    prag:\n        invented = x\n",
    ] {
        assert!(has(src, "DEF_INVALID_PATH_OR_VALUE"), "未擋:{src}");
    }
}

/// **正向控制組**:清單上的路徑必須照樣通過,否則上面的否定斷言可能只是「什麼都擋」。
#[test]
fn engine_owned_and_package_paths_stay_legal() {
    for src in [
        // 引擎自有
        "sign s:\n    phon:\n        /a/\n",
        // 套件座標(多段)
        "sign s:\n    syn:\n        tam.present = 1\n",
        "sign s:\n    sem:\n        time.past = 1\n",
        // 套件座標(單段)
        "sign s:\n    prag:\n        illocution = polar-question\n",
    ] {
        assert!(
            !has(src, "DEF_INVALID_PATH_OR_VALUE"),
            "清單上的路徑被誤擋:{src}\n{:?}",
            errors(src)
        );
    }
}

/// sign 層 meta 欄位(`valid_meta`)不受維度清單影響。
#[test]
fn sign_metadata_paths_are_unaffected() {
    assert!(!has(
        "sign s:\n    entrenchment = 0.5\n    lexicalized = true\n",
        "DEF_INVALID_PATH_OR_VALUE"
    ));
}

// ── §4.1 / A2:gloss 已退出 Def 與規則目標 ─────────────────────────────────

#[test]
fn gloss_is_no_longer_a_legal_def_path_or_rule_target() {
    assert!(
        has("sign s:\n    sem:\n        gloss = BOOK\n", "DEF_INVALID_PATH_OR_VALUE"),
        "gloss 已併入 senses,不得再當 Def"
    );
    assert!(
        has("sign s:\n    sem:\n        gloss => HOUND\n", "RULE_TARGET_NOT_ALLOWED"),
        "A2:gloss 不得作為規則目標"
    );
    // 正向控制組:義項的正解寫法必須通過
    assert!(!has(
        "sign s:\n    sem:\n        senses:\n            core = BOOK\n",
        "DEF_INVALID_PATH_OR_VALUE"
    ));
}

// ── §7 A1:規則目標同受封閉清單約束 ────────────────────────────────────────

/// 側門:規則寫的是同一個路徑空間。只關 `Def` 的話這條會靜默通過。
#[test]
fn a_rule_target_outside_the_closed_list_is_rejected() {
    let found = errors("sign s:\n    syn:\n        category => noun\n");
    let (_, message) = found
        .iter()
        .find(|(code, _)| code == "RULE_TARGET_NOT_ALLOWED")
        .unwrap_or_else(|| panic!("規則目標未受封閉清單約束:{found:?}"));
    assert!(message.contains("feature:"), "訊息要指路:{message}");
}

/// else / then 分支的目標也算數——否則把違規藏進第二個分支就能繞過。
#[test]
fn branch_targets_are_checked_too() {
    assert!(has(
        "sign s:\n    syn:\n        tam.past => a\n            else invented => b\n",
        "RULE_TARGET_NOT_ALLOWED"
    ));
}

/// **正向控制組**(§7.4 明列):目標是已宣告 feature 時必須通過。
///
/// 少了這條,上面兩條可能只是因為「任何規則都被擋」而綠。
#[test]
fn a_rule_targeting_a_declared_feature_is_accepted() {
    let src = "sign s:\n    syn:\n        feature:\n            category = enum(noun, verb)\n            category => noun\n";
    assert!(
        !has(src, "RULE_TARGET_NOT_ALLOWED"),
        "已宣告 feature 是 R2 的正解出口,不得被擋:{:?}",
        errors(src)
    );
    // 清單上的路徑作為普通規則目標亦合法
    assert!(!has(
        "sign s:\n    syn:\n        tam.present => 1\n",
        "RULE_TARGET_NOT_ALLOWED"
    ));
}

// ── §10 增修 D:guard 讀的路徑同受封閉清單約束 ────────────────────────────
//
// 讀取端與寫入端是同一個路徑空間,差別只在方向。A1 關掉寫入側門後,這裡是
// §2.1 記載的最後一個原狀:欄位名打錯回 `Unmatched`,不發診斷,規則永遠不觸發。
//
// 白名單多了一半(可見的 typed feature),所以每條否定斷言除了「清單上的路徑
// 仍通過」,還要配一條「宣告過的 feature 仍讀得到」——否則這個檢查可能只是把
// R2 的正解出口也一起關掉。

/// 沒宣告、也不在清單上 → 靜默 false 變成診斷。
#[test]
fn a_guard_reading_an_invented_path_is_rejected_and_points_at_feature() {
    let found = errors(
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n => sg / $self.syn.bogus == x\n",
    );
    let (_, message) = found
        .iter()
        .find(|(code, _)| code == "RULE_GUARD_NOT_ALLOWED")
        .unwrap_or_else(|| panic!("guard 路徑未受封閉清單約束:{found:?}"));
    assert!(message.contains("syn.bogus"), "訊息要指出是哪條路徑:{message}");
    assert!(message.contains("feature:"), "訊息要指路到 `feature:`:{message}");
}

/// 裸欄位守衛 = 本規則維度上的 `$self` 讀取,同樣受約束。
#[test]
fn a_bare_field_guard_is_rejected_too() {
    assert!(has(
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n => sg / bogus == x\n",
        "RULE_GUARD_NOT_ALLOWED"
    ));
}

/// **正向控制組(其一)**:R2 的正解出口——宣告過的 feature 必須讀得到。
/// 少了這條,上面兩條可能只是因為「任何 guard 都被擋」而綠。
#[test]
fn a_guard_reading_a_declared_feature_stays_legal() {
    for src in [
        // `$self` 形
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            mark = enum(yes)\n            mark => yes / $self.syn.n == sg\n",
        // 裸欄位形
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            mark = enum(yes)\n            mark => yes / n == sg\n",
        // 繼承來的宣告也算可見
        "trait T:\n    syn:\n        feature:\n            n = enum(sg, pl)\nsign s:\n    belongs T\n    syn:\n        feature:\n            mark = enum(yes)\n            mark => yes / n == sg\n",
    ] {
        assert!(
            !has(src, "RULE_GUARD_NOT_ALLOWED"),
            "宣告過的 feature 被誤擋:{src}\n{:?}",
            errors(src)
        );
    }
}

/// **正向控制組(其二)**:封閉清單上的路徑本來就讀得到。
#[test]
fn a_guard_reading_a_closed_list_path_stays_legal() {
    assert!(!has(
        "sign s:\n    syn:\n        tam.present = 1\n        feature:\n            mark = enum(yes)\n            mark => yes / tam.present == 1\n",
        "RULE_GUARD_NOT_ALLOWED"
    ));
}

/// **正向控制組(其三)**:範疇守衛讀的是本體樹不是路徑空間,不該被這條碰到
/// (它自有 unknown category 檢查)。
#[test]
fn category_guards_are_untouched() {
    let src = "trait T:\n    syn:\n        feature:\n            n = enum(sg)\nsign s:\n    belongs T\n    syn:\n        feature:\n            mark = enum(yes)\n            mark => yes / [T]\n";
    assert!(!has(src, "RULE_GUARD_NOT_ALLOWED"), "{:?}", errors(src));
}

/// `FeatureRule` 的**目標**有豁免(自有兩道檢查),它的 **guard 沒有**。
/// 上面幾條走的都是 `FeatureRule`,這裡釘住普通規則側也一樣。
#[test]
fn plain_rules_and_feature_rules_both_have_their_guards_checked() {
    assert!(
        has(
            "sign s:\n    syn:\n        tam.present => 1 / $self.syn.bogus == x\n",
            "RULE_GUARD_NOT_ALLOWED"
        ),
        "普通規則的 guard 未受約束"
    );
    assert!(
        has(
            "sign s:\n    syn:\n        feature:\n            n = enum(sg)\n            n => sg / $self.syn.bogus == x\n",
            "RULE_GUARD_NOT_ALLOWED"
        ),
        "FeatureRule 的 guard 未受約束"
    );
}

/// else / then 分支的 guard 也算數——否則把違規藏進第二個分支就能繞過。
#[test]
fn branch_guards_are_checked_too() {
    assert!(has(
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n => sg / n == pl\n                else n => pl / $self.syn.bogus == x\n",
        "RULE_GUARD_NOT_ALLOWED"
    ));
}

/// `$slot.NAME` 的主體是 filler,靜態未知,故白名單是**語言全域**宣告集:
/// 全語言沒人宣告過的名字擋下,別處宣告過的放行(filler 可能真的有)。
#[test]
fn slot_guards_use_the_language_wide_declaration_set() {
    let ghost = "sign c:\n    syn:\n        slots:\n            head [*]\n        feature:\n            mark = enum(yes)\n            mark => yes / $slot.head.syn.nowhere == x\n";
    assert!(has(ghost, "RULE_GUARD_NOT_ALLOWED"), "{:?}", errors(ghost));

    let declared_elsewhere = "sign filler:\n    syn:\n        feature:\n            animacy = enum(hi, lo)\nsign c:\n    syn:\n        slots:\n            head [*]\n        feature:\n            mark = enum(yes)\n            mark => yes / $slot.head.syn.animacy == hi\n";
    assert!(
        !has(declared_elsewhere, "RULE_GUARD_NOT_ALLOWED"),
        "別處宣告過的 feature 不該被擋:{:?}",
        errors(declared_elsewhere)
    );
}

/// trait 裡的 `$self` 同樣靜態未知(合成後帶什麼由 sign 決定)。菱形下兄弟
/// trait 的 feature 是合法讀取對象,不得因為「不在本 trait 的繼承視野裡」被擋。
#[test]
fn a_trait_may_guard_on_a_sibling_traits_feature() {
    let src = "trait L:\n    syn:\n        feature:\n            left = enum(yes)\ntrait R:\n    syn:\n        feature:\n            right = enum(yes)\n            right => yes / left == yes\nsign s:\n    belongs L\n    belongs R\n";
    assert!(!has(src, "RULE_GUARD_NOT_ALLOWED"), "{:?}", errors(src));
    // 但全語言都沒宣告過的名字,在 trait 裡照樣擋。
    assert!(has(
        "trait R:\n    syn:\n        feature:\n            right = enum(yes)\n            right => yes / nowhere == yes\n",
        "RULE_GUARD_NOT_ALLOWED"
    ));
}

/// typed `case:` 分支的 guard 走 `validate_realization_guard`(診斷碼
/// `CASE_INVALID_GUARD`),是另一個語法位置的同一個讀取通道。
#[test]
fn typed_case_branch_guards_are_checked() {
    let bogus = "Symbol x\nSymbol y\n\ntrait T:\n    syn:\n        feature:\n            n = enum(a, b)\n\nsign s:\n    belongs T\n    phon:\n        /x/\n        realization:\n            case:\n                $self.syn.bogus == a:\n                    /x/\n                else:\n                    /y/\n";
    let found = errors(bogus);
    let (_, message) = found
        .iter()
        .find(|(code, _)| code == "CASE_INVALID_GUARD")
        .unwrap_or_else(|| panic!("case guard 路徑未受約束:{found:?}"));
    assert!(message.contains("syn.bogus"), "{message}");

    // 正向控制組:宣告過的 feature 在 case guard 裡照樣讀得到。
    let declared = bogus.replace("syn.bogus", "syn.n");
    assert!(
        !has(&declared, "CASE_INVALID_GUARD"),
        "{:?}",
        errors(&declared)
    );
}

// ── §11 增修 E:值表達式讀的路徑同受封閉清單約束 ──────────────────────────
//
// 與 D 是同一個洞的兩半:`=>` 右端的 `$self.` / `$slot.` / `unify` / `require`
// 走同一組 `read_self`/`read_slot`,打錯同樣回 `Unmatched`。判準沿用 D2/D3。
//
// 差別在後果:guard 打錯是規則不觸發,值打錯會**落進 else 分支產出錯的值**。

#[test]
fn a_value_reading_an_invented_self_path_is_rejected_and_points_at_feature() {
    let found = errors(
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n => $self.syn.bogus\n",
    );
    let (_, message) = found
        .iter()
        .find(|(code, _)| code == "RULE_VALUE_NOT_ALLOWED")
        .unwrap_or_else(|| panic!("值表達式的讀取未受約束:{found:?}"));
    assert!(message.contains("syn.bogus"), "{message}");
    assert!(message.contains("feature:"), "訊息要指路到 `feature:`:{message}");
}

/// slot 讀取用語言全域宣告集(與 D3 同一檔)。
#[test]
fn a_value_reading_an_invented_slot_path_is_rejected() {
    assert!(has(
        "sign c:\n    syn:\n        slots:\n            head [*]\n        feature:\n            m = enum(y)\n            m => $slot.head.syn.nowhere\n",
        "RULE_VALUE_NOT_ALLOWED"
    ));
}

/// `unify` / `require` 的**每個**運算元都算數——只查第一個的話,把違規放第二個
/// 就能繞過。
#[test]
fn every_unify_and_require_operand_is_checked() {
    let both_bad = errors(
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n => unify($self.syn.bogusone, $self.syn.bogustwo)\n",
    );
    let violations = both_bad
        .iter()
        .filter(|(code, _)| code == "RULE_VALUE_NOT_ALLOWED")
        .count();
    assert_eq!(violations, 2, "兩個運算元都該回報:{both_bad:?}");

    assert!(
        has(
            "sign c:\n    syn:\n        slots:\n            a [*]\n            b [*]\n        feature:\n            m = enum(y)\n            m => require($slot.a.syn.number, $slot.b.syn.nowhere)\n",
            "RULE_VALUE_NOT_ALLOWED"
        ),
        "require 的第二個運算元未受檢"
    );
}

/// **正向控制組**:套件實際在用的三種寫法必須原樣通過,否則這條檢查只是把
/// `unify`/`require` 一律關掉。取自 `std:cxg` 與 `natural:en-standard`。
#[test]
fn package_shaped_value_reads_stay_legal() {
    for src in [
        // 清單上的路徑(cxg schema.lang 的 unify 形)
        "sign c:\n    syn:\n        slots:\n            controller [*]\n            target [*]\n        feature:\n            m = enum(y)\n            m => unify($slot.controller.syn.number, $slot.target.syn.number)\n",
        // 宣告過的 feature(en-standard 的 require 形)
        "sign filler:\n    syn:\n        feature:\n            case = enum(nominative, accusative)\n            subject_case = enum(nominative, accusative)\nsign c:\n    syn:\n        slots:\n            subject [*]\n            predicate [*]\n        feature:\n            m = enum(y)\n            m => require($slot.subject.syn.case, $slot.predicate.syn.subject_case)\n",
        // `$self` 讀自己宣告過的 feature
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            m = enum(sg, pl)\n            m => $self.syn.n\n",
        // 字面值:本來就不是讀取
        "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n => sg\n",
    ] {
        assert!(
            !has(src, "RULE_VALUE_NOT_ALLOWED"),
            "合法的值讀取被誤擋:{src}\n{:?}",
            errors(src)
        );
    }
}

/// 分支的值也算數,而且普通規則與 `FeatureRule` 兩側都查(理由同 D4)。
#[test]
fn branch_values_and_both_rule_kinds_are_checked() {
    assert!(
        has(
            "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n => sg / n == pl\n                else n => $self.syn.bogus\n",
            "RULE_VALUE_NOT_ALLOWED"
        ),
        "else 分支的值未受檢"
    );
    assert!(
        has(
            "sign s:\n    syn:\n        tam.present => $self.syn.bogus\n",
            "RULE_VALUE_NOT_ALLOWED"
        ),
        "普通規則的值未受檢"
    );
}

/// trait 的 `$self` 仍是全域上界(與 D3 同一檔,不因通道不同而改判)。
#[test]
fn trait_value_reads_use_the_language_wide_set() {
    let sibling = "trait L:\n    syn:\n        feature:\n            left = enum(yes)\ntrait R:\n    syn:\n        feature:\n            right = enum(yes)\n            right => $self.syn.left\nsign s:\n    belongs L\n    belongs R\n";
    assert!(!has(sibling, "RULE_VALUE_NOT_ALLOWED"), "{:?}", errors(sibling));
    assert!(has(
        "trait R:\n    syn:\n        feature:\n            right = enum(yes)\n            right => $self.syn.nowhere\n",
        "RULE_VALUE_NOT_ALLOWED"
    ));
}

// ── R2:`feature:` 的兩道檢查仍在(P71 §2.1 說它嚴格優於裸 def 的理由) ──

#[test]
fn feature_assignments_keep_their_two_checks() {
    assert!(
        has(
            "sign s:\n    syn:\n        feature:\n            ghost = yes\n",
            "FEATURE_UNDECLARED"
        ),
        "未宣告的 feature 賦值要報 FEATURE_UNDECLARED"
    );
    assert!(
        has(
            "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n = bogus\n",
            "FEATURE_VALUE_OUT_OF_DOMAIN"
        ),
        "值超出值域要報 FEATURE_VALUE_OUT_OF_DOMAIN"
    );
    // 正向控制組:宣告過、值在域內 → 乾淨
    let clean = "sign s:\n    syn:\n        feature:\n            n = enum(sg, pl)\n            n = pl\n";
    assert!(
        !has(clean, "FEATURE_UNDECLARED") && !has(clean, "FEATURE_VALUE_OUT_OF_DOMAIN"),
        "{:?}",
        errors(clean)
    );
}
