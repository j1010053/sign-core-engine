//! 步驟 15 的**規格出口**:《總鳥瞰》line 222「每個 rewrite 的**展開 golden**
//! (不執行)」+ line 171「展開依賴 Language 當前狀態 → 固定 Language fixture」。
//!
//! `atomic_rewrite.rs` 驗的是**行為**(展開後實跑、結果 `.lang` 正確);本檔補的是
//! **展開序列本身**——把 `(rewrite, fixture) → Vec<PrimitiveEdit>` 逐項釘成快照。
//! 兩者互補:行為測試抓「結果錯」,golden 抓「展開的形狀變了」(例如 §4.3 說 split
//! 是 insert+delete,若哪天悄悄變成 update 就會在此顯形)。

use conlang_changeset::rewrite::{
    expand, AdoptSource, AtomicRewrite, DonorScope, ReanalysisTarget, RuleHome, ServiceContext,
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
    golden_with(name, rewrite, &DonorScope::new());
}

fn golden_with(name: &str, rewrite: &AtomicRewrite, donors: &DonorScope<'_>) {
    let edits = expand(rewrite, &base(), &ServiceContext::offline(), donors)
        .unwrap_or_else(|error| panic!("{name} must expand: {error}"));
    insta::assert_snapshot!(name, shape(&edits));
}

/// 一份外部語言,供 `adopt` 指名取材。
fn donor_language() -> LanguageDocument {
    LanguageDocument::import_new_root(
        "sign kaffe:\n    belongs LocalNoun\n    sem:\n        senses:\n            core = COFFEE\n",
        "fr",
    )
    .expect("donor parses")
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

/// `reanalyze{Category}` 的展開形狀:**單一 `Update`,目標是 `Belongs` 節點**。
///
/// 先前這裡有兩個 golden(update/insert 兩條 upsert 路徑),記的是對裸 `syn.category`
/// / `syn.valence` def 的 upsert。那兩條路現在都不存在:`Category` 搬 `belongs`
/// (目標必存在,故恆為 Update),`Valence` 顯式拒絕。
#[test]
fn golden_reanalyze_moves_belongs() {
    golden(
        "reanalyze_belongs",
        &AtomicRewrite::Reanalyze {
            sign: "book".to_owned(),
            target: ReanalysisTarget::Category,
            to: "Aux".to_owned(),
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
    // v0.3:`adopt` 由「呼叫端遞現成的 SignDef」改為**指名**——donor 別名 + sign 名字,
    // 選取在展開時發生(P62 §7)。golden 因此變動:內容改由 donor 提供。
    let donor = donor_language();
    let mut donors = DonorScope::new();
    donors.insert("fr", &donor);
    golden_with(
        "adopt",
        &AtomicRewrite::Adopt {
            donor: "fr".to_owned(),
            sign: "kaffe".to_owned(),
            source: AdoptSource::Loan,
        },
        &donors,
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
        &DonorScope::new(),
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
    let edits = expand(
        rewrite,
        document,
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
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
        &DonorScope::new(),
    )
    .expect_err("fossilize must not move upward");
    assert!(format!("{err}").contains("downward"), "{err}");
}

// ── P54:fuse 記錄兩個成分 ────────────────────────────────────────────────

/// 《修補05》§4.3 要求 `fuse` 帶「component 引用」。`origin` 只收**單一**
/// `SignRef`(衍生自誰),故 P54 另立 `components`(線性組合的各成分)。
///
/// 沒有它,`fuse(a,b)` 與 `fuse(a,c)` 會產出**完全相同**的結果——第二個成分
/// 只被驗證存在就丟掉了。本測試就是釘住這件事不會回頭。
#[test]
fn fuse_records_both_components() {
    let subtree_of = |left: &str, right: &str| {
        let edits = expand(
            &AtomicRewrite::Fuse {
                left: left.to_owned(),
                right: right.to_owned(),
                name: "fused".to_owned(),
                gloss: "F".to_owned(),
            },
            &base(),
            &ServiceContext::offline(),
            &DonorScope::new(),
        )
        .expect("fuse expands");
        match &edits[0] {
            PrimitiveEdit::Insert { subtree, .. } => format!("{subtree:?}"),
            other => panic!("expected an insert, got {other:?}"),
        }
    };

    let both = subtree_of("book", "kobo");
    assert!(both.contains("book") && both.contains("kobo"), "{both}");

    // 換掉第二個成分,結果**必須不同**——這正是缺口存在時測不出來的那件事。
    let swapped = subtree_of("book", "book");
    assert_ne!(both, swapped, "第二個成分必須影響結果,否則等於沒被記錄");
}

/// 套用後,`components` 真的落到 `.lang` 上且能被讀回(不是只存在於展開序列)。
#[test]
fn the_fused_sign_carries_its_components_in_the_language() {
    let mut document = base();
    let spec = LibrarySpec::default();
    for edit in expand(
        &AtomicRewrite::Fuse {
            left: "book".to_owned(),
            right: "kobo".to_owned(),
            name: "bokobo".to_owned(),
            gloss: "FUSED".to_owned(),
        },
        &document,
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .unwrap()
    {
        document = apply_edit(&document, edit, &spec)
            .expect("fuse applies")
            .document;
    }
    let language = conlang_language::Language::parse(&document.source()).expect("re-parses");
    let fused = language
        .signs
        .iter()
        .find(|sign| sign.name == "bokobo")
        .expect("fused sign present");
    let components = fused.components().expect("components recorded");
    assert_eq!(
        components.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        ["book", "kobo"]
    );
    // `origin` 與 `components` 是**不同**的東西:前者單一來源,後者各成分。
    assert_eq!(fused.origin().map(|r| r.0), Some("book".to_owned()));
}

/// 近似負例:只有一個成分不是 fusion——`components` 至少要兩個,否則該用 `origin`。
#[test]
fn a_single_component_list_is_rejected() {
    let source = "Symbol a\n\ntrait LocalNoun:\n\nsign x:\n    belongs LocalNoun\n    components = sign(a)\n    phon:\n        /a/\n";
    let language = conlang_language::Language::parse(source).expect("parses");
    let report = conlang_language::check_language(&language);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|d| d.severity == conlang_language::Severity::Error),
        "單一成分的 components 必須被診斷:{:?}",
        report.diagnostics()
    );
}
