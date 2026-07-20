//! 🔑 步驟 11 出口:**雙軌迴歸**(P20 §1.4)——範例 8.1–8.6 的同一組音變,
//! 路徑 A(純 .qy → dsl 引擎)與路徑 B(Language 檔 → Compile ①–⑤ →
//! Compiled Grammar phon 側 → 同一 dsl 引擎)必須:
//! 1. **表層逐字相同**(P20 §1.4 出口定義;重現 CLI 的詞表′輸出邏輯);
//! 2. **末狀態結構相同**(Word 全狀態 Debug 比對:骨架/韻律/旋律 links/stale
//!    ——防「表層對、結構錯」的假綠);
//! 3. **步數相同**(規則粒度未漂移:`;` 塊 ↔ .qy 多語句塊一一對應,B5 保全);
//! 4. **逐步狀態相同**(規則名除外——路徑 B 為合成標籤 rN)。
//!
//! 一測三證(修補05 §1.4):Compile 無語意漂移、DSL 兩路徑行為一致、
//! M0 引擎核心存活。

use conlang_language::codegen::compile_full;
use conlang_language::Language;
use tshiatun_core::repr::word::{Bracket, MorphUnit, Word};
use tshiatun_dsl::{build_word, run_program, surface, Program};

struct Case {
    name: &'static str,
    qy: &'static str,
    lang: &'static str,
    words: &'static [&'static str],
    /// 型態括號(詞條載入層;8.5):`(lo, hi)` 的 stem 括號,兩路徑同樣注入。
    stem: Option<(u32, u32)>,
}

const CASES: &[Case] = &[
    Case {
        name: "8.1 tonogenesis",
        qy: include_str!("../../../tshiatun/examples/8_1_tonogenesis.qy"),
        lang: include_str!("fixtures/8_1_tonogenesis.lang"),
        words: &["pa", "ba", "baba", "a"],
        stem: None,
    },
    Case {
        name: "8.2 nasal harmony",
        qy: include_str!("../../../tshiatun/examples/8_2_nasal_harmony.qy"),
        lang: include_str!("fixtures/8_2_nasal_harmony.lang"),
        words: &["mata"],
        stem: None,
    },
    Case {
        name: "8.3 stability",
        qy: include_str!("../../../tshiatun/examples/8_3_stability.qy"),
        lang: include_str!("fixtures/8_3_stability.lang"),
        words: &["ada"],
        stem: None,
    },
    Case {
        name: "8.4 compensatory",
        qy: include_str!("../../../tshiatun/examples/8_4_compensatory.qy"),
        lang: include_str!("fixtures/8_4_compensatory.lang"),
        words: &["ak"],
        stem: None,
    },
    Case {
        name: "8.5 atr",
        qy: include_str!("../../../tshiatun/examples/8_5_atr.qy"),
        lang: include_str!("fixtures/8_5_atr.lang"),
        words: &["papipa"],
        stem: Some((0, 4)),
    },
    Case {
        name: "8.6 bantu scan",
        qy: include_str!("../../../tshiatun/examples/8_6_bantu.qy"),
        lang: include_str!("fixtures/8_6_bantu.lang"),
        words: &["bapa", "bababa"],
        stem: None,
    },
];

/// 路徑 B:Language 檔 → ①–⑤ → Program。
fn program_b(lang_src: &str) -> Program {
    let l = Language::parse(lang_src).expect("path B: .lang parses");
    compile_full(&l).expect("path B: compile ①–⑤").grammar.program
}

/// CLI 詞表′輸出邏輯的重現:Spell-out 過 → 表層(緊排);否則骨架緊排。
fn surface_line(p: &Program, w: &Word) -> String {
    match surface(p, w) {
        Some(Ok(sf)) => sf.replace(' ', ""),
        Some(Err(e)) => panic!("spell-out: {e}"),
        None => w
            .skeleton
            .iter()
            .filter_map(|s| p.env.syms.resolve(s.sym).map(str::to_owned))
            .collect(),
    }
}

fn build(p: &Program, word: &str, stem: Option<(u32, u32)>) -> Word {
    let mut w = build_word(p, word).expect("build_word");
    if let Some((lo, hi)) = stem {
        w.morph.push(Bracket {
            unit: MorphUnit::Stem,
            lo,
            hi,
        });
    }
    w
}

#[test]
fn dual_track_8_1_to_8_6_paths_agree_verbatim() {
    for case in CASES {
        let pa = tshiatun_dsl::compile(case.qy).expect("path A: .qy compiles");
        let pb = program_b(case.lang);
        for word in case.words {
            let wa = build(&pa, word, case.stem);
            let wb = build(&pb, word, case.stem);
            // 入口一致(同 build_word、同宣告 → 同初始狀態)
            assert_eq!(
                format!("{wa:?}"),
                format!("{wb:?}"),
                "{}/{word}: 初始 Word 分歧",
                case.name
            );
            let sa = run_program(&pa, wa).expect("path A run");
            let sb = run_program(&pb, wb).expect("path B run");
            // 3. 規則粒度未漂移
            assert_eq!(
                sa.len(),
                sb.len(),
                "{}/{word}: 步數漂移(A 規則塊 ↔ B `;` 塊必須一一對應)",
                case.name
            );
            // 4. 逐步狀態相同(rule 名除外)+ 診斷數一致
            for (i, (a, b)) in sa.iter().zip(&sb).enumerate() {
                assert_eq!(
                    format!("{:?}", a.word),
                    format!("{:?}", b.word),
                    "{}/{word}: 第 {i} 步後狀態分歧(A rule={}, B rule={})",
                    case.name,
                    a.rule,
                    b.rule
                );
                assert_eq!(
                    a.issues.len(),
                    b.issues.len(),
                    "{}/{word}: 第 {i} 步診斷數分歧",
                    case.name
                );
            }
            // 1.+2. 表層逐字相同 + 末狀態已由逐步比對涵蓋;表層另測(出口定義原文)
            let la = &sa.last().expect("A has steps").word;
            let lb = &sb.last().expect("B has steps").word;
            assert_eq!(
                surface_line(&pa, la),
                surface_line(&pb, lb),
                "{}/{word}: 表層不逐字相同(P20 §1.4 出口)",
                case.name
            );
        }
    }
}

/// .lang fixture 本身必須守步驟 9 的正規化不動點(parser 擴充 I17-a 不得破壞)。
#[test]
fn lang_fixtures_normalize_to_fixpoint() {
    for case in CASES {
        let d1 = Language::parse(case.lang).expect("parse").dump();
        let d2 = Language::parse(&d1).expect("re-parse").dump();
        assert_eq!(d1, d2, "{}: dump 不是不動點", case.name);
    }
}
