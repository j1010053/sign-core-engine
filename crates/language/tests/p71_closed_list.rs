//! P71 出口:**`Def` 路徑封閉清單** + `feature:` 分工 + §7 A1 規則目標同受約束。
//!
//! 這裡釘的是「關掉逃生口」本身。P71 §2.1 記載的兩個實測後果——`reanalyze` 寫入
//! 無人讀取的 `syn.category`、guard 欄位名打錯靜默 `false`——根因都是
//! `valid_dim` 只檢查路徑**長得像** `<dim>.<field>`,欄位名不查、值不查。
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
