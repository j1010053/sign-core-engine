//! 步驟 15b — **Atomic Rewrite 12 項**(P16 定案;展開檢核表 =《修補05》§4.3)。
//!
//! 契約:`(rewrite, Language) → Vec<PrimitiveEdit>`,**純展開、不執行**。
//! 規格出口只要求「展開 golden」,但只驗展開序列**無法證明它套得上去**(假綠燈),
//! 故每一項都再把序列餵給 `apply_edit` 實跑,斷言結果 `.lang`。

use conlang_changeset::rewrite::{
    expand, AdoptSource, AtomicRewrite, DonorScope, ReanalysisTarget, RuleHome, ServiceContext,
};
use conlang_changeset::{apply_edit, PrimitiveEdit};
use conlang_language::{
    DerivationKind, Language, LanguageDocument, LibrarySpec, SignDef, SignId, SignItem,
};

const SOURCE: &str = r#"Symbol b
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
        feature:
            category = enum(noun, verb, adj, aux, bound, case, conjunct, inner, lexical, new, particle)
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
    LanguageDocument::import_new_root(SOURCE, "evo:root").expect("base parses")
}

/// 展開 → 依序實跑 → 回傳結果 `.lang`。
fn run(rewrite: &AtomicRewrite) -> String {
    let mut document = base();
    let spec = LibrarySpec::default();
    let edits = expand_off(rewrite, &document).expect("expand");
    assert!(!edits.is_empty(), "a rewrite must expand to ≥1 primitive");
    for edit in edits {
        document = apply_edit(&document, edit, &spec)
            .expect("each expanded primitive applies")
            .document;
    }
    let lang = document.source().to_owned();
    Language::parse(&lang).expect("result .lang re-parses");
    lang
}

/// P53:測試一律離線展開(無 live 外部服務)。
fn expand_off(
    rewrite: &AtomicRewrite,
    document: &LanguageDocument,
) -> Result<Vec<PrimitiveEdit>, conlang_changeset::rewrite::RewriteError> {
    expand(
        rewrite,
        document,
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
}

fn expand_only(rewrite: &AtomicRewrite) -> Vec<PrimitiveEdit> {
    expand(
        rewrite,
        &base(),
        &ServiceContext::offline(),
        &DonorScope::new(),
    )
    .expect("expand")
}

/// 某個 sign 目前宣告的義項名(逐 sign 檢查,避免被別的 sign 的同名義項騙過)。
fn sense_names(language: &Language, sign: &str) -> Vec<String> {
    language
        .signs
        .iter()
        .find(|candidate| candidate.name == sign)
        .map(|candidate| {
            candidate
                .items
                .iter()
                .filter_map(|item| match item {
                    SignItem::Sense(sense) => Some(sense.name.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn new_sign(name: &str, gloss: &str) -> SignDef {
    SignDef {
        id: SignId::synthetic(),
        name: name.to_owned(),
        items: vec![
            SignItem::TraitMount {
                name: "LocalNoun".to_owned(),
                kind: conlang_language::TraitMountKind::Declaration,
                args: vec![],
            },
            SignItem::Sense(conlang_language::Sense {
                name: "core".to_owned(),
                gloss: gloss.to_owned(),
                source: conlang_language::SourceLocation::unknown(),
            }),
        ],
    }
}

// ── ① form ────────────────────────────────────────────────────────────────

#[test]
fn sound_change_inserts_a_rule_into_its_home() {
    let lang = run(&AtomicRewrite::SoundChange {
        home: RuleHome::Global("Core".to_owned()),
        body: "b => k".to_owned(),
    });
    assert!(lang.contains("b => k"), "rule landed in the trait:\n{lang}");
}

// ── ② sem ─────────────────────────────────────────────────────────────────

#[test]
fn drift_rewrites_one_sense_only() {
    let lang = run(&AtomicRewrite::Drift {
        sign: "book".to_owned(),
        sense: "log".to_owned(),
        gloss: "JOURNAL".to_owned(),
    });
    assert!(lang.contains("log = JOURNAL"), "{lang}");
    assert!(lang.contains("core = BOOK"), "sibling untouched:\n{lang}");
}

#[test]
fn derive_sense_adds_a_sense_and_its_edge() {
    let rewrite = AtomicRewrite::DeriveSense {
        sign: "book".to_owned(),
        from: "core".to_owned(),
        name: "fig".to_owned(),
        gloss: "FIGURATIVE".to_owned(),
        kind: DerivationKind::Metaphor,
    };
    // §4.3:derive_sense = insert(sense) + insert(edge) —— 兩個原語。
    assert_eq!(expand_only(&rewrite).len(), 2);
    let lang = run(&rewrite);
    assert!(lang.contains("fig = FIGURATIVE"), "{lang}");
    assert!(lang.contains("fig from core metaphor"), "{lang}");
}

#[test]
fn lexicalize_sense_makes_a_derivation_opaque() {
    let lang = run(&AtomicRewrite::LexicalizeSense {
        sign: "book".to_owned(),
        edge: 0,
    });
    assert!(lang.contains("log from core metonymy opaque"), "{lang}");
}

// ── ③ syn ─────────────────────────────────────────────────────────────────

/// 重分析**搬動 `belongs`**——範疇在本系統是本體樹的成員關係
/// (`std/core/code/ontology.lang`:「category membership—not a mutable `syn.class`」)。
///
/// ## 誌誤
///
/// 這裡原本斷言 `category = aux`,也就是一個裸 `syn.category` def。那條路徑走的是
/// `Def` 驗證裡唯一不檢查內容的一支,**沒有任何語意層讀它**:實測 reanalyze 之後
/// `belongs` 原封不動、`category_is_a(…, Verb)` 仍為真,參數約束 `[Verb]` 照樣通過
/// ——語法化最核心的動作是空操作,而測試因為只看那個 def 而全綠。
#[test]
fn reanalyze_moves_the_sign_into_another_category() {
    let lang = run(&AtomicRewrite::Reanalyze {
        sign: "book".to_owned(),
        target: ReanalysisTarget::Category,
        from: None,
        to: "Aux".to_owned(),
    });
    // 逐 sign 檢查:`kobo` 也 belongs LocalNoun,全文比對會被它騙過。
    let book = lang
        .split("sign book:")
        .nth(1)
        .and_then(|rest| rest.split("\nsign ").next())
        .expect("book block");
    assert!(book.contains("belongs Aux"), "{lang}");
    assert!(!book.contains("belongs LocalNoun"), "{lang}");
    // 來源不動:`kobo` 的歸屬不得被波及。
    assert!(lang.contains("belongs LocalNoun"), "{lang}");
}

/// 目標範疇不存在 ⇒ 由 compile 層的 `ONTOLOGY_UNKNOWN_TRAIT` 擋下。
///
/// 展開層**刻意不檢查**:`expand` 拿不到 `LibrarySpec`,而範疇多半來自套件
/// (`Aux` 住 std:core,文件自身的 `traits` 是空的)。在展開層檢查會把合法的
/// 套件範疇也一起拒絕。
#[test]
fn reanalyze_into_an_unknown_category_is_rejected_at_compile() {
    let mut document = base();
    let spec = LibrarySpec::default();
    let edits = expand_off(
        &AtomicRewrite::Reanalyze {
            sign: "book".to_owned(),
            target: ReanalysisTarget::Category,
            from: None,
            to: "no_such_category".to_owned(),
        },
        &document,
    )
    .expect("展開本身不查本體樹");
    let mut failed = false;
    for edit in edits {
        match apply_edit(&document, edit, &spec) {
            Ok(outcome) => document = outcome.document,
            Err(_) => failed = true,
        }
    }
    assert!(failed, "懸空的 belongs 必須被擋下:\n{}", document.source());
}

/// `Valence`/`Slot` 兩個 target **顯式拒絕**。
///
/// 它們原本也寫裸 def(`syn.valence`/`syn.slot`),與 `category` 同樣無人讀取。
/// valence 該是宣告過的 syn feature、slot 該動 `SignItem::Slot`,兩者都還沒裁定
/// ——**寧可硬錯,不留一個看起來會動其實不動的操作**(同 `Boundary` 的處理)。
#[test]
fn reanalyze_valence_and_slot_are_explicitly_unsupported() {
    for target in [ReanalysisTarget::Valence, ReanalysisTarget::Slot] {
        let err = expand_off(
            &AtomicRewrite::Reanalyze {
                sign: "kobo".to_owned(),
                target,
                from: None,
                to: "intransitive".to_owned(),
            },
            &base(),
        )
        .expect_err("must be rejected");
        assert!(
            format!("{err}").contains("UNSUPPORTED"),
            "{target:?}: {err}"
        );
    }
}

/// 多 belongs 時不給 `from` 仍拒絕——不猜。
#[test]
fn reanalyze_refuses_multi_belongs_without_from() {
    let document = LanguageDocument::import_new_root(
        "trait A:\n\ntrait B:\n\nsign both:\n    belongs A\n    belongs B\n",
        "evo:multi",
    )
    .expect("fixture parses");
    let err = expand_off(
        &AtomicRewrite::Reanalyze {
            sign: "both".to_owned(),
            target: ReanalysisTarget::Category,
            from: None,
            to: "Aux".to_owned(),
        },
        &document,
    )
    .expect_err("ambiguous");
    assert!(format!("{err}").contains("UNSUPPORTED"), "{err}");
    assert!(
        format!("{err}").contains("from:"),
        "should suggest from: {err}"
    );
}

/// 多 belongs + `from` 指定 → 只換指定的那一條。
#[test]
fn reanalyze_with_from_replaces_the_specified_belongs() {
    let document = LanguageDocument::import_new_root(
        "trait A:\n\ntrait B:\n\ntrait C:\n\nsign both:\n    belongs A\n    belongs B\n",
        "evo:multi",
    )
    .expect("fixture parses");
    let spec = LibrarySpec::default();
    let edits = expand_off(
        &AtomicRewrite::Reanalyze {
            sign: "both".to_owned(),
            target: ReanalysisTarget::Category,
            from: Some("A".to_owned()),
            to: "C".to_owned(),
        },
        &document,
    )
    .expect("from disambiguates");
    let mut doc = document;
    for edit in edits {
        doc = apply_edit(&doc, edit, &spec).expect("applies").document;
    }
    let lang = doc.source();
    let both = lang.split("sign both:").nth(1).expect("sign block");
    assert!(both.contains("belongs C"), "A should become C:\n{lang}");
    assert!(both.contains("belongs B"), "B should be untouched:\n{lang}");
    assert!(!both.contains("belongs A"), "A should be gone:\n{lang}");
}

/// `from` 指定的範疇不存在 → 報錯。
#[test]
fn reanalyze_with_wrong_from_is_rejected() {
    let document = LanguageDocument::import_new_root(
        "trait A:\n\ntrait B:\n\nsign both:\n    belongs A\n    belongs B\n",
        "evo:multi",
    )
    .expect("fixture parses");
    let err = expand_off(
        &AtomicRewrite::Reanalyze {
            sign: "both".to_owned(),
            target: ReanalysisTarget::Category,
            from: Some("NoSuch".to_owned()),
            to: "C".to_owned(),
        },
        &document,
    )
    .expect_err("from doesn't match");
    assert!(format!("{err}").contains("does not"), "{err}");
}

#[test]
fn reanalyze_boundary_is_explicitly_unsupported() {
    // 不默默近似:成分重新切分尚未支援,必須明確拒絕。
    let err = expand_off(
        &AtomicRewrite::Reanalyze {
            sign: "book".to_owned(),
            target: ReanalysisTarget::Boundary,
            from: None,
            to: "x".to_owned(),
        },
        &base(),
    )
    .expect_err("Boundary must be rejected");
    assert!(format!("{err}").contains("UNSUPPORTED"), "{err}");
}

// ── ④ usage ───────────────────────────────────────────────────────────────

#[test]
fn entrench_and_attrit_move_the_same_field_in_opposite_directions() {
    let up = run(&AtomicRewrite::Entrench {
        sign: "book".to_owned(),
        delta: 0.25,
    });
    assert!(up.contains("entrenchment = 0.75"), "{up}");

    let down = run(&AtomicRewrite::Attrit {
        sign: "book".to_owned(),
        delta: 0.25,
    });
    assert!(down.contains("entrenchment = 0.25"), "{down}");
}

#[test]
fn attrit_clamps_at_zero() {
    let lang = run(&AtomicRewrite::Attrit {
        sign: "book".to_owned(),
        delta: 10.0,
    });
    assert!(
        lang.contains("entrenchment = 0"),
        "no negative entrenchment:\n{lang}"
    );
}

#[test]
fn lexicalize_inserts_the_new_type() {
    let lang = run(&AtomicRewrite::Lexicalize {
        sign: new_sign("booklet", "SMALL_BOOK"),
    });
    assert!(lang.contains("sign booklet:"), "{lang}");
}

// ── ⑤ 結構 ────────────────────────────────────────────────────────────────

#[test]
fn create_and_delete_are_inverse_at_the_sign_level() {
    let created = run(&AtomicRewrite::Create {
        sign: new_sign("tome", "TOME"),
    });
    assert!(created.contains("sign tome:"), "{created}");

    let deleted = run(&AtomicRewrite::Delete {
        selector: "sign(\"kobo\")".to_owned(),
    });
    assert!(!deleted.contains("sign kobo:"), "{deleted}");
}

#[test]
fn split_moves_named_senses_into_a_new_sign_with_origin() {
    let rewrite = AtomicRewrite::Split {
        sign: "book".to_owned(),
        new_name: "logbook".to_owned(),
        senses: vec!["log".to_owned()],
    };
    // §4.3:split = insert(新 sign 帶著義項 + origin)+ delete(來源那些義項)。
    // kobo 只有一個無邊義項,是最乾淨的案例。
    let lang = run(&AtomicRewrite::Split {
        sign: "kobo".to_owned(),
        new_name: "kobolet".to_owned(),
        senses: vec!["only".to_owned()],
    });
    assert!(lang.contains("sign kobolet:"), "{lang}");
    assert!(
        lang.contains("origin = sign(kobo)"),
        "origin points back:\n{lang}"
    );
    // **搬移**不是複製:新 sign 拿到義項,來源必須失去它。
    // (只看全檔字串會被新 sign 的那份騙過去 —— 要逐 sign 檢查。)
    let parsed = Language::parse(&lang).expect("result .lang re-parses");
    assert_eq!(sense_names(&parsed, "kobolet"), vec!["only".to_owned()]);
    assert!(
        sense_names(&parsed, "kobo").is_empty(),
        "the source must no longer declare the moved sense:\n{lang}"
    );
    // book 的 split 會拆散 `log from core` 這條邊 → 顯式拒絕(見下)。
    assert!(expand(
        &rewrite,
        &base(),
        &ServiceContext::offline(),
        &DonorScope::new()
    )
    .is_err());
}

#[test]
fn split_refuses_to_strand_a_derivation_edge() {
    // `log from core`:只搬 log 會讓邊的另一端留在來源 → 懸空。不默默近似。
    let err = expand_off(
        &AtomicRewrite::Split {
            sign: "book".to_owned(),
            new_name: "logbook".to_owned(),
            senses: vec!["log".to_owned()],
        },
        &base(),
    )
    .expect_err("stranding must be rejected");
    assert!(format!("{err}").contains("strand"), "{err}");
}

#[test]
fn merge_moves_senses_then_deletes_the_source() {
    let lang = run(&AtomicRewrite::Merge {
        into: "book".to_owned(),
        from: "kobo".to_owned(),
    });
    assert!(
        lang.contains("only = KOBO"),
        "sense collapsed into book:\n{lang}"
    );
    assert!(!lang.contains("sign kobo:"), "source removed:\n{lang}");
}

#[test]
fn merge_needs_two_different_signs() {
    let err = expand_off(
        &AtomicRewrite::Merge {
            into: "book".to_owned(),
            from: "book".to_owned(),
        },
        &base(),
    )
    .expect_err("self-merge must be rejected");
    assert!(format!("{err}").contains("different"), "{err}");
}

#[test]
fn fuse_builds_a_new_sign_from_two_components() {
    let lang = run(&AtomicRewrite::Fuse {
        left: "book".to_owned(),
        right: "kobo".to_owned(),
        name: "bokobo".to_owned(),
        gloss: "FUSED".to_owned(),
    });
    assert!(lang.contains("sign bokobo:"), "{lang}");
    assert!(lang.contains("core = FUSED"), "{lang}");
    // fuse ≠ merge:來源**不刪**(線性組合,不是範疇塌縮)。
    assert!(
        lang.contains("sign book:") && lang.contains("sign kobo:"),
        "{lang}"
    );
}

// ── ⑥ 接觸 ────────────────────────────────────────────────────────────────

#[test]
fn adopt_marks_the_borrowed_sign_as_a_loan() {
    // v0.3:指名借入。**「怎麼挑」現在在引擎裡**——先前是呼叫端把整個 sign 挑好遞進來,
    // 那段決定不被記錄、不被測試、也不能 replay(P62 §7.1)。
    let donor = LanguageDocument::import_new_root(
        "sign kaffe:\n    belongs LocalNoun\n    sem:\n        senses:\n            core = COFFEE\n",
        "fr",
    )
    .expect("donor parses");
    let mut donors = DonorScope::new();
    donors.insert("fr", &donor);
    let mut document = base();
    let spec = LibrarySpec::default();
    for edit in expand(
        &AtomicRewrite::Adopt {
            donor: "fr".to_owned(),
            sign: "kaffe".to_owned(),
            source: AdoptSource::Loan,
        },
        &document,
        &ServiceContext::offline(),
        &donors,
    )
    .expect("expand")
    {
        document = apply_edit(&document, edit, &spec)
            .expect("applies")
            .document;
    }
    let lang = document.source().to_owned();
    assert!(lang.contains("sign kaffe:"), "{lang}");
    assert!(
        lang.contains("provenance = loan"),
        "provenance recorded:\n{lang}"
    );
}

// ── ⑦ 居所(P14 Life Cycle 軸)────────────────────────────────────────────

/// 一份 trait `Core` 底下已有一條規則的文件(居所搬移的起點)。
fn with_trait_rule() -> LanguageDocument {
    let mut document = base();
    let spec = LibrarySpec::default();
    for edit in expand_off(
        &AtomicRewrite::SoundChange {
            home: RuleHome::Global("Core".to_owned()),
            body: "b => k".to_owned(),
        },
        &document,
    )
    .unwrap()
    {
        document = apply_edit(&document, edit, &spec).unwrap().document;
    }
    document
}

#[test]
fn fossilize_moves_a_rule_down_the_ladder() {
    let mut document = with_trait_rule();
    let spec = LibrarySpec::default();
    let edits = expand_off(
        &AtomicRewrite::Fossilize {
            rule: "trait(\"Core\").block[0].rule[0]".to_owned(),
            to: RuleHome::Sign("book".to_owned()),
        },
        &document,
    )
    .expect("fossilize expands");
    // §4.3:居所搬移用的是 **move**,不是 delete+insert。
    assert!(matches!(edits.as_slice(), [PrimitiveEdit::Move { .. }]));
    for edit in edits {
        document = apply_edit(&document, edit, &spec)
            .expect("fossilize applies")
            .document;
    }
    let lang = document.source();
    let sign_at = lang.find("sign book:").unwrap();
    assert!(
        lang.find("b => k").unwrap() > sign_at,
        "rule now lives under the sign:\n{lang}"
    );
}

#[test]
fn a_wrong_direction_move_is_rejected() {
    // generalize 必須往上;指定一個更低的居所要明確報錯,不默默當成 fossilize。
    // 先真的把規則放進 trait,才是在驗**方向**而不是「位址不存在」。
    let document = with_trait_rule();
    let err = expand_off(
        &AtomicRewrite::Generalize {
            rule: "trait(\"Core\").block[0].rule[0]".to_owned(),
            to: RuleHome::Sign("book".to_owned()),
        },
        &document,
    )
    .expect_err("wrong direction must be rejected");
    assert!(
        format!("{err}").contains("upward"),
        "expected the direction guard, got {err}"
    );
}

// ── 不變式 ────────────────────────────────────────────────────────────────

#[test]
fn expansion_is_deterministic_and_does_not_mutate_the_document() {
    let document = base();
    let before = document.source().to_owned();
    let rewrite = AtomicRewrite::DeriveSense {
        sign: "book".to_owned(),
        from: "core".to_owned(),
        name: "fig".to_owned(),
        gloss: "FIGURATIVE".to_owned(),
        kind: DerivationKind::Metaphor,
    };
    let first = expand_off(&rewrite, &document).unwrap();
    let second = expand_off(&rewrite, &document).unwrap();
    assert_eq!(first, second, "same input → same expansion");
    assert_eq!(document.source(), before, "expansion never mutates(純函數)");
}

#[test]
fn an_unknown_target_is_rejected_by_every_rewrite_that_addresses_one() {
    for rewrite in [
        AtomicRewrite::Drift {
            sign: "ghost".to_owned(),
            sense: "core".to_owned(),
            gloss: "X".to_owned(),
        },
        AtomicRewrite::DeriveSense {
            sign: "book".to_owned(),
            from: "ghost".to_owned(),
            name: "n".to_owned(),
            gloss: "G".to_owned(),
            kind: DerivationKind::Broaden,
        },
        AtomicRewrite::Entrench {
            sign: "ghost".to_owned(),
            delta: 0.1,
        },
        AtomicRewrite::Merge {
            into: "book".to_owned(),
            from: "ghost".to_owned(),
        },
    ] {
        assert!(
            expand(
                &rewrite,
                &base(),
                &ServiceContext::offline(),
                &DonorScope::new()
            )
            .is_err(),
            "expected rejection for {rewrite:?}"
        );
    }
}
