//! P71-B 出口:`.chg` 的具名 selector **`feature[<dim>.<name>]`**。
//!
//! P71 R2 把作者自造欄位全數趕進 `feature:`,而 `feature:` 下的賦值在 AST 是
//! `SignItem::FeatureValue` 而非 `Def`——遷移前 `.chg` 只有 `def[path]`,
//! 於是那些欄位在歷時層變成只能用 `node(feature_value, @id)` 的機器形式定址。
//! 本檔釘住補上的具名入口:定位、改值、負例,以及**與 `def[…]` 不互相冒認**。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, ReplayError, ResolvedChangeSet, UnresolvedChangeSet,
};
use conlang_language::{LanguageDocument, LibrarySpec};

/// `category` 是宣告過的 syn 特徵;`phon` 是引擎自有的 Def。兩者並存,
/// 才能證明 `feature[…]` 與 `def[…]` 各自只認自己那類節點。
const SOURCE: &str = r#"Symbol b
Symbol o

trait LocalNoun:

sign book:
    belongs LocalNoun
    phon:
        /bo/
    syn:
        feature:
            category = enum(noun, verb)
            category = noun
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("base parses")
}

fn resolve(chg: &str, ns: &str) -> Result<ResolvedChangeSet, ReplayError> {
    let base = base();
    let spec = LibrarySpec::default();
    let mut source = change_set_prelude(&base, &spec, ns).unwrap();
    source.push_str(chg);
    UnresolvedChangeSet::parse(&source)
        .unwrap()
        .resolve(&base, &spec)
}

fn apply(chg: &str, ns: &str) -> String {
    let base = base();
    let spec = LibrarySpec::default();
    let resolved = resolve(chg, ns).expect("resolve");
    ChangeInterpreter::new(base, spec, ns)
        .unwrap()
        .run(&resolved)
        .expect("replay")
        .document
        .source()
        .to_owned()
}

#[test]
fn feature_selector_addresses_a_feature_value_by_name() {
    let lang = apply(
        "\n    statement 0:\n        update sign(\"book\").feature[syn.category].value = verb\n",
        "evo:feature-update",
    );
    assert!(lang.contains("category = verb"), "改值生效:\n{lang}");
    assert!(!lang.contains("category = noun"), "舊值不留:\n{lang}");
    // 判別性:改的是賦值不是宣告——值域那行必須原封不動
    assert!(
        lang.contains("category = enum(noun, verb)"),
        "宣告未被波及:\n{lang}"
    );
}

/// 維度限定(B2):鍵是 `syn.category`,裸 `category` 不該命中。
#[test]
fn the_selector_key_is_dimension_qualified() {
    let err = resolve(
        "\n    statement 0:\n        update sign(\"book\").feature[category].value = verb\n",
        "evo:feature-bare",
    )
    .expect_err("裸 name 不是合法鍵");
    let text = format!("{err}");
    assert!(
        text.contains("category"),
        "錯誤要指出定位不到的鍵:{text}"
    );
}

/// `feature[…]` 與 `def[…]` 不互相冒認:各自只認自己那類節點。
///
/// 這是本檔最具判別性的一條——若 selector 實作忘了比對 `NodeKind`,
/// 兩者會在同一個 sign 上互相命中而測不出來。
#[test]
fn feature_and_def_selectors_do_not_impersonate_each_other() {
    // ① `def[syn.category]` 找不到:category 是 FeatureValue,不是 Def
    let err = resolve(
        "\n    statement 0:\n        update sign(\"book\").def[syn.category].value = verb\n",
        "evo:def-on-feature",
    )
    .expect_err("def[…] 不得命中 FeatureValue");
    assert!(format!("{err}").contains("syn.category"), "{err}");

    // ② `feature[phon]` 找不到:phon 是引擎自有 Def,不是 FeatureValue
    let err = resolve(
        "\n    statement 0:\n        update sign(\"book\").feature[phon].value = /ba/\n",
        "evo:feature-on-def",
    )
    .expect_err("feature[…] 不得命中 Def");
    assert!(format!("{err}").contains("phon"), "{err}");

    // ③ 正向控制組:各自用對了都成功(否則上面兩條可能只是「什麼都定位不到」)
    let lang = apply(
        "\n    statement 0:\n        update sign(\"book\").feature[syn.category].value = verb\n",
        "evo:control-feature",
    );
    assert!(lang.contains("category = verb"), "{lang}");
    let lang = apply(
        "\n    statement 0:\n        update sign(\"book\").def[phon].value = /ba/\n",
        "evo:control-def",
    );
    assert!(lang.contains("/ba/"), "{lang}");
}

/// 不存在的特徵 → 定位失敗,不得默默無視。
#[test]
fn an_unknown_feature_key_is_rejected() {
    let err = resolve(
        "\n    statement 0:\n        update sign(\"book\").feature[syn.ghost].value = x\n",
        "evo:feature-ghost",
    )
    .expect_err("未知特徵不得靜默通過");
    assert!(format!("{err}").contains("syn.ghost"), "{err}");
}
