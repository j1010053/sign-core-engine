//! 步驟 11:codegen(⑤)語意測試——排放格式 golden、P8 責任分離、
//! I17 顯式拒絕(else/Scan+stage/dsl 拒收)、決定性。

use conlang_language::codegen::{self, CodegenError};
use conlang_language::{Language, Stage};

/// dsl-valid 的最小綜合樣例:stage≠word 規則、`;` 多語句塊、Scan 塊、
/// 非 global trait 引用、sign 欄位與局部規則。
const SRC: &str = "\
Feature voice(+voice, -voice)

Symbol p [-voice]
Symbol b [+voice]
Symbol a

Class vowel {a}

Melody tone {H, L} anchor mora

global trait Core:
    phon:
        [+voice]&onset => [-voice] @stage stem
        insert H floating near mora / onset&[-voice] _ ; dock tone&floating strategy nearest
        Scan tone along mora within pword from left: H => L / H _

trait V:
    syn:
        provides = VERB

sign go:
    V[0]
    phon:
        /go/
        o => a / _ @stage phrase
";

fn artifacts() -> codegen::Artifacts {
    let l = Language::parse(SRC).expect("fixture parses");
    codegen::compile_full(&l).expect("①–⑤ succeed")
}

/// phon 側排放格式 golden(P20 §1.3 接口的文字形):宣告在前、合成標籤、
/// stage 僅於 ≠word 時輸出、`;` 塊展開、Scan 塊頭切分。
#[test]
fn phon_source_golden() {
    insta::assert_snapshot!("phon_source", artifacts().grammar.phon_source);
}

/// P8:Compiled Grammar 不保存 trait/sign/priority 痕跡;
/// sign 局部規則與 Def 不進 phon 側。
#[test]
fn p8_no_language_knowledge_leaks_into_grammar() {
    let a = artifacts();
    let src = &a.grammar.phon_source;
    assert!(!src.contains("trait"), "trait 痕跡洩入 phon 側:\n{src}");
    assert!(!src.contains("sign"), "sign 痕跡洩入 phon 側:\n{src}");
    assert!(!src.contains("o => a"), "sign 局部規則洩入 phon 側:\n{src}");
    assert!(!src.contains("phon ="), "sign Def 洩入 phon 側:\n{src}");
    assert!(!src.contains("VERB"), "syn Def 洩入 phon 側:\n{src}");
    // stage 標記僅於 ≠word 時輸出
    assert!(src.contains("stage: stem"));
    assert!(!src.contains("stage: word"));
}

/// Compiled Sign:③ 後者勝欄位 + 局部規則(含 trait 展開來的 Def),無 TraitUse。
#[test]
fn compiled_sign_carries_resolved_defs_and_local_rules() {
    let a = artifacts();
    assert_eq!(a.signs.len(), 1);
    let s = &a.signs[0];
    assert_eq!(s.name, "go");
    assert_eq!(
        s.defs,
        vec![
            ("syn.provides".to_owned(), "VERB".to_owned()),
            ("phon".to_owned(), "/go/".to_owned()),
        ]
    );
    assert_eq!(s.rules.len(), 1);
    assert_eq!(s.rules[0].body, "o => a / _");
    assert_eq!(s.rules[0].stage, Stage::Phrase);
}

/// 產出的 Program 可執行:對詞跑一遍(regression 深度由 dual_track 承擔)。
#[test]
fn program_is_runnable() {
    let a = artifacts();
    let p = &a.grammar.program;
    let w = tshiatun_dsl::build_word(p, "ba").expect("build");
    let steps = tshiatun_dsl::run_program(p, w).expect("run");
    assert!(!steps.is_empty());
}

/// 決定性:兩次 compile_full 產物逐位元相同;codegen 為純函數(對 ④ 冪等)。
#[test]
fn codegen_is_deterministic_and_pure() {
    let l = Language::parse(SRC).unwrap();
    let a = codegen::compile_full(&l).unwrap();
    let b = codegen::compile_full(&l).unwrap();
    assert_eq!(a.grammar.phon_source, b.grammar.phon_source);
    assert_eq!(a.signs, b.signs);
    let (g2, s2) = codegen::codegen(&a.pipeline.ordered).unwrap();
    assert_eq!(g2.phon_source, a.grammar.phon_source);
    assert_eq!(s2, a.signs);
}

/// 多 global trait:規則依 canonical 名稱序拼接(決定性;I17-c)。
#[test]
fn multiple_global_traits_emit_in_canonical_name_order() {
    let src = "\
Symbol a

Class vowel {a}

global trait Zeta:
    phon:
        a => a / _#

global trait Alpha:
    phon:
        a => a / #_
";
    let l = Language::parse(src).unwrap();
    let a = codegen::compile_full(&l).unwrap();
    let s = &a.grammar.phon_source;
    let i_alpha = s.find("a => a / #_").expect("Alpha rule present");
    let i_zeta = s.find("a => a / _#").expect("Zeta rule present");
    assert!(i_alpha < i_zeta, "canonical 名稱序:Alpha 先於 Zeta\n{s}");
}

// ── 顯式拒絕(I17-d):不默默近似 ──

fn run_text(program: &tshiatun_dsl::Program, input: &str) -> String {
    let word = tshiatun_dsl::build_word(program, input).unwrap();
    let fallback = word.clone();
    let steps = tshiatun_dsl::run_program(program, word).unwrap();
    let last = steps.last().map(|step| &step.word).unwrap_or(&fallback);
    last.skeleton
        .iter()
        .filter_map(|segment| program.env.syms.resolve(segment.sym))
        .collect()
}

/// phon Else lowers to Tshiatūn `Else:` and is differential-equivalent to a
/// directly authored `.qy` rule.
#[test]
fn else_chain_lowers_to_phon_dsl() {
    let src = "\
Symbol a
Symbol b
Symbol c

Class vowel {a, b, c}

global trait G:
    phon:
        a => b
        else c => a
";
    let l = Language::parse(src).unwrap();
    let generated = codegen::compile_full(&l).unwrap();
    assert!(generated.grammar.phon_source.contains("Else:"));
    let direct = tshiatun_dsl::compile(
        "Symbol a\nSymbol b\nSymbol c\nClass vowel {a, b, c}\nchoice:\n    a => b\n    Else: c => a\n",
    )
    .unwrap();
    for input in ["a", "c"] {
        assert_eq!(
            run_text(&generated.grammar.program, input),
            run_text(&direct, input)
        );
    }
}

/// phon Then lowers to Tshiatūn `Then:` and preserves feeding.
#[test]
fn then_chain_lowers_to_phon_dsl() {
    let src = "\
Symbol a
Symbol b
Symbol c

Class vowel {a, b, c}

global trait G:
    phon:
        a => b
        then b => c
";
    let l = Language::parse(src).unwrap();
    let generated = codegen::compile_full(&l).unwrap();
    assert!(generated.grammar.phon_source.contains("Then:"));
    let direct = tshiatun_dsl::compile(
        "Symbol a\nSymbol b\nSymbol c\nClass vowel {a, b, c}\nchain:\n    a => b\n    Then: b => c\n",
    )
    .unwrap();
    assert_eq!(run_text(&generated.grammar.program, "a"), "c");
    assert_eq!(
        run_text(&generated.grammar.program, "a"),
        run_text(&direct, "a")
    );
}

/// Scan 塊在 dsl 不承載 stage → 非 word stage 顯式拒絕。
#[test]
fn scan_with_non_word_stage_is_rejected() {
    let src = "\
Symbol a

global trait G:
    phon:
        Scan tone along mora within pword from left: H => L / H _ @stage stem
";
    let l = Language::parse(src).unwrap();
    let e = codegen::compile_full(&l).unwrap_err();
    assert!(
        matches!(e, CodegenError::ScanStageUnsupported { stage: "stem", .. }),
        "{e:?}"
    );
}

/// 語句原文有誤 → dsl 拒收,錯誤附完整產物原文(可定位)。
#[test]
fn invalid_statement_surfaces_dsl_error_with_generated_source() {
    let src = "\
Symbol a

global trait G:
    phon:
        bogus statement that is no dsl verb
";
    let l = Language::parse(src).unwrap();
    let e = codegen::compile_full(&l).unwrap_err();
    match e {
        CodegenError::Dsl { generated, .. } => {
            assert!(generated.contains("bogus statement"), "{generated}");
            assert!(generated.contains("r1:"), "{generated}");
        }
        other => panic!("expected Dsl error, got {other:?}"),
    }
}

/// 邊界:空 Language 與「僅宣告無規則」都能 codegen(零規則 Program)。
#[test]
fn empty_and_decls_only_languages_codegen() {
    let (g, signs) = codegen::codegen(&Language::new()).unwrap();
    assert_eq!(g.phon_source, "");
    assert!(g.program.rules.is_empty());
    assert!(signs.is_empty());

    let l = Language::parse("Symbol a\n\nClass vowel {a}\n\nprosody = μ σ\n").unwrap();
    let a = codegen::compile_full(&l).unwrap();
    assert!(a.grammar.program.rules.is_empty());
    assert!(a.grammar.phon_source.contains("Symbol a"));
}

/// P46 取徑 A(slice 1):phon `name:` 前綴 → 具名 rule;canonical dump 用前綴;
/// codegen 排放 Lexurgy `name:` 標籤(非合成 rN:)。
#[test]
fn phon_named_rule_uses_lexurgy_name_prefix_and_label() {
    let src = "\
Symbol a
Symbol b

global trait Core:
    phon:
        lenition: a => b
";
    let l = Language::parse(src).expect("named phon rule parses");
    let dumped = l.dump();
    assert!(dumped.contains("lenition: a => b"), "canonical 前綴:\n{dumped}");
    assert_eq!(
        Language::parse(&dumped).unwrap().dump(),
        dumped,
        "round-trip 穩定"
    );
    let a = codegen::compile_full(&l).unwrap();
    assert!(
        a.grammar.phon_source.contains("lenition:"),
        "phon_source 用 Lexurgy 名:\n{}",
        a.grammar.phon_source
    );
    assert!(
        !a.grammar.phon_source.contains("r0:"),
        "具名 rule 不用合成 rN 標籤"
    );
}

/// P46 S2:結構化 phon block(`name:` + 單層 `Then:`/`Else:`)→ round-trip → codegen
/// 排放可被引擎接受的 `.qy`。巢狀 Then/Else 需 upstream grouped-block parser,暫不測。
#[test]
fn phon_structured_block_then_and_else_codegen_flat() {
    let src = "\
Symbol a
Symbol b
Symbol c
Symbol d

global trait Core:
    phon:
        seq:
            a => b
            Then:
                c => d
        alt:
            a => a
            Else: a => b
";
    let l = Language::parse(src).expect("structured phon blocks parse");
    let dumped = l.dump();
    assert!(dumped.contains("seq:") && dumped.contains("Then:"), "Then block:\n{dumped}");
    assert!(dumped.contains("alt:") && dumped.contains("Else:"), "Else block:\n{dumped}");
    assert_eq!(Language::parse(&dumped).unwrap().dump(), dumped, "round-trip 穩定");
    let a = codegen::compile_full(&l).expect("flat blocks compile via engine");
    let s = &a.grammar.phon_source;
    assert!(s.contains("seq:") && s.contains("Then:") && s.contains("alt:") && s.contains("Else:"),
        "phon_source:\n{s}");
}
