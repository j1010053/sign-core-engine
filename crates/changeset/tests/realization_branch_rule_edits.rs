//! C2:分支內的規則要能被四原語編輯 —— 否則它有身分卻改不動,`.chg` 無從承載
//! 「把這條 ablaut 規則的環境改一下」這種最基本的歷時操作。
//!
//! 與 C1/C3 同理,這一層也是 P93「不新增 `Expression` variant」的回報:分支
//! 的 items 走的是既有的 fragment 路徑,`SignFragment` 怎麼編輯,phon fragment
//! 就怎麼編輯。本檔把它釘成契約。

use conlang_changeset::{apply_edit, Anchor, DetachedNode, NodeUpdate, PrimitiveEdit};
use conlang_language::{Dim, RuleId};
use conlang_language::{LanguageDocument, LibrarySpec, NodeKind, NodeRef, Rule, SignItem, Stage};

const SOURCE: &str = r#"Feature Type(*cons, vowel)
Symbol a [vowel]
Symbol i [vowel]
Symbol n [cons]
Symbol g [cons]
Symbol s [cons]
Class vowel {a, i}

trait Ablauting:
    syn:
        feature:
            k = enum(one)

sign sing:
    belongs Ablauting
    phon:
        /sing/
        realization:
            case:
                $self == [Ablauting]:
                    i => a / _ n g
                else:
                    /sing/
"#;

fn child(document: &LanguageDocument, parent: &NodeRef, kind: NodeKind, ordinal: usize) -> NodeRef {
    let node = document
        .identities()
        .nodes
        .iter()
        .filter(|node| node.parent.as_ref() == Some(&parent.id) && node.kind == kind)
        .nth(ordinal)
        .expect("child exists");
    NodeRef::new(node.id.clone(), kind)
}

fn realization_branch(document: &LanguageDocument, sign: &str, index: usize) -> NodeRef {
    let sign = document.ref_for_sign(sign).expect("sign");
    let case = child(document, &sign, NodeKind::Case, 0);
    child(document, &case, NodeKind::CaseBranch, index)
}

/// 🔑 改分支裡那條規則的本體,身分不變。
#[test]
fn a_branch_rule_body_can_be_updated_in_place() {
    let before = LanguageDocument::import_new_root(SOURCE, "evo:branch-rule").expect("import");
    let branch = realization_branch(&before, "sing", 0);
    let rule = child(&before, &branch, NodeKind::Rule, 0);

    let after = apply_edit(
        &before,
        PrimitiveEdit::Update {
            node: rule.clone(),
            change: NodeUpdate::RuleBody("i => u / _ n g".to_owned()),
        },
        &LibrarySpec::default(),
    )
    .expect("update applies")
    .document;

    assert!(
        after.source().contains("i => u / _ n g"),
        "規則本體應已改:\n{}",
        after.source()
    );
    assert_eq!(
        child(&after, &branch, NodeKind::Rule, 0).id,
        rule.id,
        "改本體不該換身分"
    );
}

/// 往同一個分支再加一條規則 —— 一個分支帶多條規則是德語 umlaut+加綴的形狀。
#[test]
fn a_second_rule_can_be_inserted_into_a_branch() {
    let before = LanguageDocument::import_new_root(SOURCE, "evo:branch-rule").expect("import");
    let branch = realization_branch(&before, "sing", 0);

    let after = apply_edit(
        &before,
        PrimitiveEdit::Insert {
            parent: branch.clone(),
            anchor: Anchor::End,
            subtree: DetachedNode::Item(SignItem::Rule(Rule {
                id: RuleId::local(0),
                name: None,
                phon_block: None,
                propagate: false,
                body: "* => er / _ #".to_owned(),
                stage: Stage::Word,
                dim: Dim::Phon,
                else_chain: Vec::new(),
                then_chain: Vec::new(),
                source: Default::default(),
                branch_sources: Vec::new(),
            })),
        },
        &LibrarySpec::default(),
    )
    .expect("insert applies")
    .document;

    assert!(
        after.source().contains("* => er / _ #"),
        "第二條規則應已插入:\n{}",
        after.source()
    );
}

/// 刪掉分支**唯一**的內容會留下空分支 —— 拒絕。
///
/// 注意這道不變式是靠**重新解析**守住的(訊息來自 parser 的「case branch
/// requires a result expression」),不是編輯期的專門檢查。契約成立,但錯誤
/// 訊息對呼叫端偏間接;若日後要改善,是在 `apply_edit` 加前置檢查,而不是
/// 放寬這裡。
#[test]
fn deleting_the_only_content_of_a_branch_is_refused() {
    let before = LanguageDocument::import_new_root(SOURCE, "evo:branch-rule").expect("import");
    let branch = realization_branch(&before, "sing", 0);
    let rule = child(&before, &branch, NodeKind::Rule, 0);

    let error = apply_edit(
        &before,
        PrimitiveEdit::Delete { node: rule },
        &LibrarySpec::default(),
    )
    .expect_err("空分支不合法");
    assert!(
        error.to_string().contains("result expression"),
        "應說明分支不能沒有結果: {error}"
    );
}

/// 分支是 `(模板, [規則])` 時刪掉規則,退回 `(模板, [])` —— 模板留著、身分不變。
///
/// 這曾是 `ShapeMismatch`:單行 `/…/` 分支解析成純量 `PhonTemplate`(不產生節點)、
/// 多行解析成 `DimFragment`(items 各是節點),刪除後 canonical 塌陷成純量而 manifest
/// 還記著 `Items(0)`。修法是**統一表示**:phon 模板一律存成 fragment,不再塌陷。
/// 結構化的 phon 表達式(interpolation / projection)不在此列——它們帶著
/// `SignApplication`,壓成 Def 字串會讓遞迴套用失去結構。
#[test]
fn a_rule_can_be_deleted_when_the_branch_keeps_its_template() {
    let source = SOURCE.replace(
        "                $self == [Ablauting]:\n                    i => a / _ n g",
        "                $self == [Ablauting]:\n                    /singing/\n                    i => a / _ n g",
    );
    let before = LanguageDocument::import_new_root(&source, "evo:branch-rule").expect("import");
    let branch = realization_branch(&before, "sing", 0);
    let rule = child(&before, &branch, NodeKind::Rule, 0);

    let after = apply_edit(
        &before,
        PrimitiveEdit::Delete { node: rule },
        &LibrarySpec::default(),
    )
    .expect("delete applies")
    .document;

    assert!(
        !after.source().contains("i => a / _ n g"),
        "規則應已刪除:\n{}",
        after.source()
    );
    assert!(
        after.source().contains("/singing/"),
        "模板應留著:\n{}",
        after.source()
    );
}
