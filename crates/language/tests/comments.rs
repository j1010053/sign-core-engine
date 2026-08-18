//! `/* … */` 註解可出現於任意位置(檔首/檔中/行尾/跨行);錯誤行號不因註解漂移。

use conlang_language::{Dim, Language, SignItem};

/// 檔中(非檔首)整行註解 + 行尾註解 + 容器內註解:皆合法解析。
#[test]
fn comments_anywhere_parse() {
    let src = "\
/* 檔首 */
trait Verb:

/* 檔中整行註解(舊版會 parse error) */
sign go:
    belongs Verb   /* 行尾註解 */
    phon:
        /go/
";
    let l = Language::parse(src).expect("comments anywhere must parse");
    assert!(l.trait_named("Verb").is_some());
    let go = l.sign_named("go").unwrap();
    // 行尾註解不污染 belongs;phon Def 乾淨
    assert!(matches!(&go.items[0], SignItem::TraitMount { name: n, kind: conlang_language::TraitMountKind::Declaration, .. } if n == "Verb"));
    assert!(go
        .items
        .iter()
        .any(|i| matches!(i, SignItem::Def(d) if d.path == "phon" && d.value == "/go/")));
}

/// 跨行區塊註解:不破壞後續解析。
#[test]
fn multiline_block_comment() {
    let src = "\
sign a:
    /* 這是
       跨越多行
       的註解 */
    phon:
        /a/
";
    let l = Language::parse(src).unwrap();
    assert!(l.sign_named("a").is_some());
}

/// 錯誤行號在(跨行)註解之後仍正確(strip 保留換行)。
#[test]
fn error_line_numbers_survive_comments() {
    let src = "\
/* line 1
   line 2 */
sign x:
    ??garbage
";
    let e = Language::parse(src).unwrap_err();
    assert_eq!(e.line, 4, "?? 在第 4 行,註解不使行號漂移:{e:?}");
}

/// 行尾註解不影響 dim 規則 body / 縮排。
#[test]
fn trailing_comment_does_not_corrupt_rule() {
    let src = "\
sign r:
    syn:
        x => a   /* set x */
";
    let l = Language::parse(src).unwrap();
    let r = l.sign_named("r").unwrap();
    let rule = r
        .items
        .iter()
        .find_map(|i| match i {
            SignItem::Rule(rr) => Some(rr),
            _ => None,
        })
        .unwrap();
    assert_eq!(rule.body, "x => a", "行尾註解已剝除,body 乾淨");
    assert_eq!(rule.dim, Dim::Syn);
}
