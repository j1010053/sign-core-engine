//! 步驟 18 出口:**Need → Generator → Builder → 四原語 → Language′** 端到端。
//!
//! 釘住兩件事:
//!
//! 1. **C1 的形狀**:Builder 產出 `Vec<PrimitiveEdit>`,經既有 replay 落地。
//!    造出來的詞因此自動可 replay、進得了演化圖——步驟 13–17 的機器全部繼承。
//!    若 Builder 改成就地寫 `Language`,這裡的 replay 斷言會失去意義。
//! 2. **職責紅線**(本體論 §0):Generator 唯讀、Proposal 是幻影(不進 Language)、
//!    Builder 不內建語言學(阻擋由 Strategy 決定)。
//!
//! 折磨測試對應:**11**(thief 擋 stealer = blocking 委派)、
//! **12**(河 20 候選 = Proposal 帶評分、排序在提議側)。

use conlang_changeset::{
    change_set_prelude, ChangeInterpreter, PrimitiveEdit, ResolvedStatement, UnresolvedChangeSet,
};
use conlang_generate::{
    build, highest_scoring, ranked, sample_proposal, BlockingStrategy, Generator, Need, NeedOrigin,
    Proposal, Strategies, StrategyError,
};
use conlang_language::{compile_system, LanguageDocument, LibrarySpec, SignDef};

const BASE: &str = "Symbol k\nSymbol a\nSymbol t\nSymbol u\n\nClass vowel {a, u}\n\n\
sign thief:\n    belongs Noun\n    phon:\n        /kat/\n    \
sem:\n        senses:\n            core = THIEF\n";

fn document() -> LanguageDocument {
    LanguageDocument::import_new_root(BASE, "evo:root").expect("base parses")
}

/// 最小規則式提議者:依序列出候選並給分。**唯讀**。
#[derive(Debug)]
struct SyllableGenerator {
    shapes: Vec<(&'static str, f64)>,
}

impl Generator for SyllableGenerator {
    fn propose(
        &self,
        _need: &Need,
        _system: &conlang_language::CompiledSystem,
    ) -> Vec<Proposal> {
        self.shapes
            .iter()
            .map(|(phon, score)| Proposal {
                phon: (*phon).to_owned(),
                score: *score,
                rationale: format!("CV 音節模板 {phon}"),
            })
            .collect()
    }
}

fn need(name: &str) -> Need {
    Need {
        name: name.to_owned(),
        categories: vec!["Noun".to_owned()],
        gloss: Some("STEALER".to_owned()),
        origin: NeedOrigin::Coined,
    }
}

/// 把四原語經**既有 replay** 落地,回傳新的 `.lang` 原文。
fn apply(document: &LanguageDocument, edits: Vec<PrimitiveEdit>, ns: &str) -> String {
    let spec = LibrarySpec::default();
    let prelude = change_set_prelude(document, &spec, ns).expect("prelude");
    let mut resolved = UnresolvedChangeSet::parse(&prelude)
        .expect("prelude parses")
        .resolve(document, &spec)
        .expect("prelude resolves");
    resolved.statements = vec![ResolvedStatement { ordinal: 0, edits }];
    ChangeInterpreter::new(document.clone(), spec, ns.to_owned())
        .expect("interpreter")
        .run(&resolved)
        .expect("replay")
        .document
        .source()
        .to_owned()
}

// ── 🔑 端到端 ────────────────────────────────────────────────────────────

#[test]
fn a_need_becomes_a_stored_sign_through_the_four_primitives() {
    let document = document();
    let system = compile_system(document.language().clone()).expect("compiles");
    let generator = SyllableGenerator {
        shapes: vec![("/ka/", 0.4), ("/tu/", 0.9), ("/ku/", 0.7)],
    };

    let need = need("stealer");
    let proposals = generator.propose(&need, &system);
    let chosen = highest_scoring(&proposals).expect("有候選");
    assert_eq!(chosen.phon, "/tu/", "取最高分,不是列舉序第一個");

    let edits = build(&need, chosen, &document, &Strategies::default()).expect("build");
    // C1:Builder 的產物是四原語,不是就地寫入
    assert!(matches!(edits.as_slice(), [PrimitiveEdit::Insert { .. }]));

    let after = apply(&document, edits, "evo:coin");
    assert!(after.contains("sign stealer:"), "{after}");
    assert!(after.contains("/tu/"), "{after}");
    assert!(after.contains("core = STEALER"), "{after}");
    // 既有 sign 不受影響
    assert!(after.contains("sign thief:") && after.contains("/kat/"), "{after}");
}

/// Proposal 是**幻影**:提議本身不改變任何東西(本體論 §3)。
#[test]
fn proposing_alone_changes_nothing() {
    let document = document();
    let system = compile_system(document.language().clone()).expect("compiles");
    let before = document.source().to_owned();

    let generator = SyllableGenerator {
        shapes: vec![("/ka/", 0.4), ("/tu/", 0.9)],
    };
    let proposals = generator.propose(&need("stealer"), &system);
    assert_eq!(proposals.len(), 2, "提議確實產生了(否則本測試空轉)");
    assert_eq!(document.source(), before, "提議不得改動文件");
}

/// P70:**零候選是合法結果**,不是錯誤。
#[test]
fn no_candidates_is_a_legal_outcome() {
    let document = document();
    let system = compile_system(document.language().clone()).expect("compiles");
    let generator = SyllableGenerator { shapes: Vec::new() };
    let need = need("stealer");
    let proposals = generator.propose(&need, &system);
    // 兩個模式都必須把「零候選」當合法結果
    assert!(ranked(&proposals).is_empty());
    assert!(sample_proposal(&need, &proposals, 7).expect("不是錯誤").is_none());
    assert!(highest_scoring(&proposals).is_none());
}

// ── 折磨 12:20 個候選,排序在提議側 ──────────────────────────────────────

#[test]
fn many_candidates_are_ranked_by_the_proposer() {
    let document = document();
    let system = compile_system(document.language().clone()).expect("compiles");
    // 20 個候選,最高分刻意放在中間——若選擇層只取第一個就會挑錯
    let shapes: Vec<(&'static str, f64)> = ["/ka/", "/ku/", "/ta/", "/tu/", "/at/"]
        .into_iter()
        .cycle()
        .take(20)
        .enumerate()
        .map(|(index, phon)| (phon, if index == 11 { 0.99 } else { 0.1 }))
        .collect();
    let generator = SyllableGenerator { shapes };

    let need = need("river");
    let proposals = generator.propose(&need, &system);
    assert_eq!(proposals.len(), 20);

    // ── 手動 / 輔助模式:引擎只排序,不選 ──
    let ordered = ranked(&proposals);
    assert_eq!(ordered.len(), 20, "全部候選都交出去,不預先篩掉");
    assert_eq!(ordered[0].score, 0.99, "最高分排最前");
    // 同分保列舉序(穩定排序)→ 同輸入同輸出
    assert_eq!(ranked(&proposals), ordered);

    // ── 自動模式:seeded 加權抽樣,不是 argmax ──
    let trace = sample_proposal(&need, &proposals, 42)
        .expect("抽樣")
        .expect("有候選");
    assert_eq!(trace.algorithm, "rand_chacha/ChaCha20Rng@0.3", "演算法進 trace");
    assert_eq!(trace.ordered.len(), 20, "全部候選與權重都留在 trace 裡");
    assert_eq!(trace.selected, proposals[trace.selected_index]);
    // 同 seed 逐位元可重現(P26)
    assert_eq!(
        sample_proposal(&need, &proposals, 42).unwrap().unwrap(),
        trace
    );
}

/// **自動模式不是 argmax**——這是兩個模式真正的分野。
///
/// 少了這條,把 `sample_proposal` 實作成「取最高分」不會有任何一條紅,
/// 而那正是我初版寫錯的東西:同一個 Need 永遠得到同一個詞,
/// 自動造 200 個詞時拿不到變化。
#[test]
fn automatic_mode_samples_rather_than_taking_the_maximum() {
    let need = need("river");
    // 權重相近的兩個候選:argmax 恆選同一個,抽樣會隨 seed 換
    let proposals = vec![
        Proposal { phon: "/ka/".into(), score: 1.0, rationale: String::new() },
        Proposal { phon: "/tu/".into(), score: 1.0, rationale: String::new() },
    ];
    assert_eq!(highest_scoring(&proposals).unwrap().phon, "/ka/", "argmax 恆取前者");

    let picks: std::collections::BTreeSet<String> = (0..40)
        .map(|seed| {
            sample_proposal(&need, &proposals, seed)
                .unwrap()
                .unwrap()
                .selected
                .phon
        })
        .collect();
    assert_eq!(picks.len(), 2, "不同 seed 應抽到不同候選,而非恆取 argmax:{picks:?}");
}

/// 有候選但**權重全為零** → 自動模式無從抽起,回錯誤而非默默挑一個。
#[test]
fn all_zero_weights_is_an_error_not_a_silent_pick() {
    let need = need("river");
    let proposals = vec![
        Proposal { phon: "/ka/".into(), score: 0.0, rationale: String::new() },
        Proposal { phon: "/tu/".into(), score: 0.0, rationale: String::new() },
    ];
    assert!(sample_proposal(&need, &proposals, 1).is_err());
    // 判別性:手動模式照樣排得出來(零候選與零權重是兩回事)
    assert_eq!(ranked(&proposals).len(), 2);
}

// ── 折磨 11:thief 擋 stealer,而且是**委派**的判斷 ──────────────────────

/// 同義阻擋策略:已有 sign 佔住同一個 gloss 就擋下。
#[derive(Debug)]
struct BlockOnSameGloss;

impl BlockingStrategy for BlockOnSameGloss {
    fn check(
        &self,
        _need: &Need,
        sign: &SignDef,
        document: &LanguageDocument,
    ) -> Result<(), StrategyError> {
        let gloss_of = |sign: &SignDef| {
            sign.items.iter().find_map(|item| match item {
                conlang_language::SignItem::Sense(sense) if sense.name == "core" => {
                    Some(sense.gloss.clone())
                }
                _ => None,
            })
        };
        let Some(wanted) = gloss_of(sign) else {
            return Ok(());
        };
        for existing in &document.language().signs {
            if gloss_of(existing).as_deref() == Some(wanted.as_str()) {
                return Err(StrategyError::Blocked {
                    existing: existing.name.clone(),
                    reason: format!("已佔據義項 {wanted:?}"),
                });
            }
        }
        Ok(())
    }
}

#[test]
fn an_existing_synonym_blocks_the_new_coinage() {
    let document = document();
    let strategies = Strategies {
        blocking: Box::new(BlockOnSameGloss),
        ..Strategies::default()
    };
    // 想造一個 gloss 同為 THIEF 的詞 → 被既有的 thief 擋下
    let mut blocked = need("stealer");
    blocked.gloss = Some("THIEF".to_owned());
    let proposal = Proposal {
        phon: "/tu/".to_owned(),
        score: 1.0,
        rationale: String::new(),
    };
    let error = build(&blocked, &proposal, &document, &strategies).expect_err("應被擋");
    assert!(format!("{error}").contains("thief"), "{error}");

    // **判別性**:同一個策略下,不同義項的詞照樣造得出來
    build(&need("stealer"), &proposal, &document, &strategies)
        .expect("不同義項不該被擋");
}

/// Builder **不內建**阻擋——換掉策略,同一個輸入就通過。
///
/// 這條是紅線測試:若 Builder 自己實作了同義阻擋,拿掉策略也會被擋。
#[test]
fn blocking_is_delegated_not_built_in() {
    let document = document();
    let mut same_gloss = need("stealer");
    same_gloss.gloss = Some("THIEF".to_owned());
    let proposal = Proposal {
        phon: "/tu/".to_owned(),
        score: 1.0,
        rationale: String::new(),
    };

    // 預設策略不阻擋任何東西 → 造得出來
    build(&same_gloss, &proposal, &document, &Strategies::default())
        .expect("預設不阻擋,故同義詞可造");

    // 換上阻擋策略 → 同一個輸入被擋
    let strategies = Strategies {
        blocking: Box::new(BlockOnSameGloss),
        ..Strategies::default()
    };
    assert!(build(&same_gloss, &proposal, &document, &strategies).is_err());
}

/// 良構同樣是委派的:重名由 `ValidateStrategy` 擋。
#[test]
fn a_duplicate_name_is_rejected_by_the_validate_strategy() {
    let document = document();
    let proposal = Proposal {
        phon: "/tu/".to_owned(),
        score: 1.0,
        rationale: String::new(),
    };
    let error = build(&need("thief"), &proposal, &document, &Strategies::default())
        .expect_err("重名應被拒");
    assert!(format!("{error}").contains("thief"), "{error}");
}

/// 組合造詞留下 `origin`,借入留下 `provenance = loan`。
#[test]
fn origin_and_provenance_are_recorded() {
    let document = document();
    let proposal = Proposal {
        phon: "/tu/".to_owned(),
        score: 1.0,
        rationale: String::new(),
    };

    let mut composed = need("stealer");
    composed.origin = NeedOrigin::Composed {
        from: "thief".to_owned(),
    };
    let after = apply(
        &document,
        build(&composed, &proposal, &document, &Strategies::default()).unwrap(),
        "evo:composed",
    );
    assert!(after.contains("origin = sign(thief)"), "{after}");
    assert!(after.contains("provenance = derived"), "{after}");

    let mut borrowed = need("loanword");
    borrowed.origin = NeedOrigin::Borrowed;
    let after = apply(
        &document,
        build(&borrowed, &proposal, &document, &Strategies::default()).unwrap(),
        "evo:loan",
    );
    assert!(after.contains("provenance = loan"), "{after}");
}
