//! 步驟 15 的**規格出口**:《總鳥瞰》line 222「每個 rewrite 的**展開 golden**
//! (不執行)」+ line 171「展開依賴 Language 當前狀態 → 固定 Language fixture」。
//!
//! `atomic_rewrite.rs` 驗的是**行為**(展開後實跑、結果 `.lang` 正確);本檔補的是
//! **展開序列本身**——把 `(rewrite, fixture) → Vec<PrimitiveEdit>` 逐項釘成快照。
//! 兩者互補:行為測試抓「結果錯」,golden 抓「展開的形狀變了」(例如 §4.3 說 split
//! 是 insert+delete,若哪天悄悄變成 update 就會在此顯形)。

use conlang_changeset::rewrite::{
    expand, AdoptSource, AtomicRewrite, ReanalysisTarget, RuleHome, ServiceContext,
};
use conlang_changeset::{apply_edit, PrimitiveEdit};
use conlang_language::{
    DerivationKind, LanguageDocument, LibrarySpec, SignDef, SignId, SignItem, SourceLocation,
};

/// 固定 Language fixture(《總鳥瞰》line 171)。
const FIXTURE: &str = r#"Symbol b
Symbol o
Symbol k

trait LocalNoun:

global trait Core:

sign book:
    belongs LocalNoun
    entrenchment = 0.5
    phon:
        /bo/
    syn:
        category = noun
    sem:
        senses:
            core = BOOK
            log = LOGBOOK
        edges:
            log from core metonymy

sign kobo:
    belongs LocalNoun
    phon:
        /ko/
    sem:
        senses:
            only = KOBO
"#;

fn base() -> LanguageDocument {
    LanguageDocument::import_new_root(FIXTURE, "evo:root").expect("fixture parses")
}

/// 把展開序列轉成穩定、可讀的形狀。**刻意不含 NodeId**——id 是決定性配發的,
/// 但把它寫進 golden 會讓任何不相干的 fixture 變動都造成假失敗;
/// 這裡要釘的是「**用了哪個原語、作用在哪種節點、放哪個位置**」。
fn shape(edits: &[PrimitiveEdit]) -> String {
    edits
        .iter()
        .map(|edit| match edit {
            PrimitiveEdit::Insert {
                parent,
                anchor,
                subtree,
            } => format!(
                "insert {:?} into {:?} at {:?}",
                subtree.kind(),
                parent.expected,
                anchor
            ),
            PrimitiveEdit::Delete { node } => format!("delete {:?}", node.expected),
            PrimitiveEdit::Update { node, change } => format!(
                "update {:?}.{}",
                node.expected,
                match change {
                    conlang_changeset::NodeUpdate::SenseGloss(_) => "gloss",
                    conlang_changeset::NodeUpdate::SenseEdgeTransparency(_) => "transparency",
                    conlang_changeset::NodeUpdate::SenseEdgeKind(_) => "kind",
                    conlang_changeset::NodeUpdate::DefinitionValue(_) => "value",
                    _ => "other",
                }
            ),
            PrimitiveEdit::Move {
                node,
                new_parent,
                anchor,
            } => format!(
                "move {:?} to {:?} at {:?}",
                node.expected, new_parent.expected, anchor
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn golden(name: &str, rewrite: &AtomicRewrite) {
    let edits = expand(rewrite, &base(), &ServiceContext::offline())
        .unwrap_or_else(|error| panic!("{name} must expand: {error}"));
    insta::assert_snapshot!(name, shape(&edits));
}

fn new_sign(name: &str, gloss: &str) -> SignDef {
    SignDef {
        id: SignId::synthetic(),
        name: name.to_owned(),
        items: vec![
            SignItem::Belongs("LocalNoun".to_owned()),
            SignItem::Sense(conlang_language::Sense {
                name: "core".to_owned(),
                gloss: gloss.to_owned(),
                source: SourceLocation::unknown(),
            }),
        ],
    }
}

// ── 12 列各一份展開 golden(《修補05》§4.3 檢核表)─────────────────────────

#[test]
fn golden_sound_change() {
    golden(
        "sound_change",
        &AtomicRewrite::SoundChange {
            home: RuleHome::Global("Core".to_owned()),
            body: "b => k".to_owned(),
        },
    );
}

#[test]
fn golden_drift() {
    golden(
        "drift",
        &AtomicRewrite::Drift {
            sign: "book".to_owned(),
            sense: "log".to_owned(),
            gloss: "JOURNAL".to_owned(),
        },
    );
}

#[test]
fn golden_derive_sense() {
    golden(
        "derive_sense",
        &AtomicRewrite::DeriveSense {
            sign: "book".to_owned(),
            from: "core".to_owned(),
            name: "fig".to_owned(),
            gloss: "FIGURATIVE".to_owned(),
            kind: DerivationKind::Metaphor,
        },
    );
}

#[test]
fn golden_lexicalize_sense() {
    golden(
        "lexicalize_sense",
        &AtomicRewrite::LexicalizeSense {
            sign: "book".to_owned(),
            edge: 0,
        },
    );
}

#[test]
fn golden_reanalyze_update_and_insert() {
    // 同一項的兩條路:欄位已存在 → update;不存在 → insert(upsert)。
    golden(
        "reanalyze_update",
        &AtomicRewrite::Reanalyze {
            sign: "book".to_owned(),
            target: ReanalysisTarget::Category,
            to: "aux".to_owned(),
        },
    );
    golden(
        "reanalyze_insert",
        &AtomicRewrite::Reanalyze {
            sign: "kobo".to_owned(),
            target: ReanalysisTarget::Valence,
            to: "intransitive".to_owned(),
        },
    );
}

#[test]
fn golden_entrench_and_attrit() {
    golden(
        "entrench",
        &AtomicRewrite::Entrench {
            sign: "book".to_owned(),
            delta: 0.25,
        },
    );
    golden(
        "attrit",
        &AtomicRewrite::Attrit {
            sign: "book".to_owned(),
            delta: 0.25,
        },
    );
}

#[test]
fn golden_lexicalize() {
    golden(
        "lexicalize",
        &AtomicRewrite::Lexicalize {
            sign: new_sign("booklet", "SMALL_BOOK"),
        },
    );
}

#[test]
fn golden_create_and_delete() {
    golden(
        "create",
        &AtomicRewrite::Create {
            sign: new_sign("tome", "TOME"),
        },
    );
    golden(
        "delete",
        &AtomicRewrite::Delete {
            selector: "sign(\"kobo\")".to_owned(),
        },
    );
}

#[test]
fn golden_split() {
    // §4.3:split = insert(新 sign + origin) + delete(來源義項)。
    golden(
        "split",
        &AtomicRewrite::Split {
            sign: "kobo".to_owned(),
            new_name: "kobolet".to_owned(),
            senses: vec!["only".to_owned()],
        },
    );
}

#[test]
fn golden_merge_and_fuse() {
    // §4.3:merge = move + delete;fuse = insert(**不刪**來源)。
    golden(
        "merge",
        &AtomicRewrite::Merge {
            into: "book".to_owned(),
            from: "kobo".to_owned(),
        },
    );
    golden(
        "fuse",
        &AtomicRewrite::Fuse {
            left: "book".to_owned(),
            right: "kobo".to_owned(),
            name: "bokobo".to_owned(),
            gloss: "FUSED".to_owned(),
        },
    );
}

#[test]
fn golden_adopt() {
    golden(
        "adopt",
        &AtomicRewrite::Adopt {
            sign: new_sign("kaffe", "COFFEE"),
            source: AdoptSource::Loan,
        },
    );
}

// ── 居所階梯(P14):三級 Global↔Trait↔Sign,兩個方向都要有正例 ─────────────

/// 把一條規則放進指定居所,回傳文件(居所搬移的起點)。
fn with_rule_in(home: RuleHome) -> LanguageDocument {
    let mut document = base();
    let spec = LibrarySpec::default();
    for edit in expand(
        &AtomicRewrite::SoundChange {
            home,
            body: "b => k".to_owned(),
        },
        &document,
        &ServiceContext::offline(),
    )
    .expect("sound_change expands")
    {
        document = apply_edit(&document, edit, &spec)
            .expect("sound_change applies")
            .document;
    }
    document
}

fn move_rule(document: &LanguageDocument, rewrite: &AtomicRewrite) -> String {
    let edits = expand(rewrite, document, &ServiceContext::offline())
        .unwrap_or_else(|error| panic!("must expand: {error}"));
    // §4.3:居所搬移用的是 **move**,不是 delete+insert。
    assert!(
        matches!(edits.as_slice(), [PrimitiveEdit::Move { .. }]),
        "expected a single Move, got {edits:?}"
    );
    let spec = LibrarySpec::default();
    let mut document = document.clone();
    for edit in edits {
        document = apply_edit(&document, edit, &spec)
            .expect("the move applies")
            .document;
    }
    document.source().to_owned()
}

/// **迴歸**:`global trait X` 與 `trait X` 的 selector 都是 `trait("X")`。
/// 早期 `home_of` 只看前綴,把三級階梯塌成兩級,導致這個合法的下移**被誤拒**。
#[test]
fn fossilize_moves_a_rule_from_a_global_trait_down_to_a_trait() {
    let document = with_rule_in(RuleHome::Global("Core".to_owned()));
    let lang = move_rule(
        &document,
        &AtomicRewrite::Fossilize {
            rule: "trait(\"Core\").block[0].rule[0]".to_owned(),
            to: RuleHome::Trait("LocalNoun".to_owned()),
        },
    );
    let local = lang.find("trait LocalNoun:").expect("trait present");
    assert!(
        lang.find("b => k").expect("rule present") > local,
        "rule now lives under the plain trait:\n{lang}"
    );
}

/// **`generalize` 的正例**——先前只有「方向錯被拒」的負例,往上搬這條正路
/// 從未被證明可行。
#[test]
fn generalize_moves_a_rule_from_a_sign_up_to_a_trait() {
    let document = with_rule_in(RuleHome::Sign("book".to_owned()));
    let lang = move_rule(
        &document,
        &AtomicRewrite::Generalize {
            rule: "sign(\"book\").rule[0]".to_owned(),
            to: RuleHome::Trait("LocalNoun".to_owned()),
        },
    );
    let sign_at = lang.find("sign book:").expect("sign present");
    assert!(
        lang.find("b => k").expect("rule present") < sign_at,
        "rule moved up out of the sign:\n{lang}"
    );
}

#[test]
fn generalize_moves_a_rule_from_a_trait_up_to_a_global_trait() {
    // 階梯的第三級:trait → global trait。
    let document = with_rule_in(RuleHome::Trait("LocalNoun".to_owned()));
    let lang = move_rule(
        &document,
        &AtomicRewrite::Generalize {
            rule: "trait(\"LocalNoun\").block[0].rule[0]".to_owned(),
            to: RuleHome::Global("Core".to_owned()),
        },
    );
    assert!(lang.contains("b => k"), "{lang}");
    let core = lang
        .find("global trait Core:")
        .expect("global trait present");
    let local = lang.find("trait LocalNoun:").expect("trait present");
    let rule = lang.find("b => k").expect("rule present");
    assert!(
        rule > core && rule < local,
        "rule now sits inside the global trait:\n{lang}"
    );
}

#[test]
fn fossilize_may_not_move_a_rule_upward() {
    // 對稱負例:方向守衛兩邊都要擋。
    let document = with_rule_in(RuleHome::Sign("book".to_owned()));
    let err = expand(
        &AtomicRewrite::Fossilize {
            rule: "sign(\"book\").rule[0]".to_owned(),
            to: RuleHome::Trait("LocalNoun".to_owned()),
        },
        &document,
        &ServiceContext::offline(),
    )
    .expect_err("fossilize must not move upward");
    assert!(format!("{err}").contains("downward"), "{err}");
}

// ── 已知模型缺口(誠實標記,不默默近似)────────────────────────────────────

/// 《修補05》§4.3 要求 `fuse` 帶「component 引用」,但 `origin` 只收**單一**
/// `SignRef`,模型沒有記錄多成分的欄位。此測試**釘住現況**:`right` 只被驗證
/// 存在、不被記錄,故 `fuse(a,b)` 與 `fuse(a,c)` 產出相同。
///
/// 這不是可以靜默的近似——補 components 欄位屬架構層(P 系列)。此測試存在的
/// 目的就是讓缺口在補上時**主動失敗**,提醒更新。
#[test]
fn fuse_does_not_yet_record_its_second_component() {
    let with_kobo = expand(
        &AtomicRewrite::Fuse {
            left: "book".to_owned(),
            right: "kobo".to_owned(),
            name: "fused".to_owned(),
            gloss: "F".to_owned(),
        },
        &base(),
        &ServiceContext::offline(),
    )
    .unwrap();
    // 換一個 right(同樣存在於 fixture),展開結果目前**完全相同**。
    let with_book = expand(
        &AtomicRewrite::Fuse {
            left: "kobo".to_owned(),
            right: "book".to_owned(),
            name: "fused".to_owned(),
            gloss: "F".to_owned(),
        },
        &base(),
        &ServiceContext::offline(),
    )
    .unwrap();
    let subtree = |edits: &[PrimitiveEdit]| match &edits[0] {
        PrimitiveEdit::Insert { subtree, .. } => format!("{subtree:?}"),
        other => panic!("expected an insert, got {other:?}"),
    };
    // 兩者的 origin 不同(left 有記),但**第二成分完全沒出現在任何一邊**。
    assert!(subtree(&with_kobo).contains("book"), "left 有被記錄");
    assert!(
        !subtree(&with_kobo).contains("kobo"),
        "已知缺口:right 未被記錄。若此斷言開始失敗,表示模型補上了 components——\
         請更新本測試與 rewrite.rs 的註記"
    );
    assert!(
        !subtree(&with_book).contains("book"),
        "對稱地,right 未被記錄"
    );
}
