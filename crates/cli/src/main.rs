//! conlang CLI(M0 步驟 4):`conlang <rules.dsl> <words.txt>`。
//!
//! 讀規則檔 + 詞表(每行一詞)→ 對每詞跑完整規則序列 → 印出推導表(trace)。
//! IO/println 只住殼層;core/dsl 維持 WASM-safe(M0 §1.2)。

use std::process::ExitCode;

use conlang_core::repr::notation;
use conlang_dsl::{build_word, compile, run_program, surface};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (rules_path, words_path) = match (args.get(1), args.get(2)) {
        (Some(r), Some(w)) => (r.clone(), w.clone()),
        _ => {
            eprintln!("usage: conlang <rules.dsl> <words.txt>");
            return ExitCode::from(2);
        }
    };

    let src = match std::fs::read_to_string(&rules_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {rules_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let words = match std::fs::read_to_string(&words_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {words_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let program = match compile(&src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    for line in words.lines() {
        let word_text = line.trim();
        if word_text.is_empty() || word_text.starts_with("/*") {
            continue;
        }
        let w = match build_word(&program, word_text) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        println!("*{word_text}");
        println!(
            "  {:<14} {}",
            "input",
            render(&program, &w)
        );
        match run_program(&program, w) {
            Ok(steps) => {
                for s in &steps {
                    let issues = if s.issues.is_empty() {
                        String::new()
                    } else {
                        format!("   [{} issue(s)]", s.issues.len())
                    };
                    println!("  {:<14} {}{}", s.rule, render(&program, &s.word), issues);
                }
                if let Some(last) = steps.last() {
                    match surface(&program, &last.word) {
                        Some(Ok(sf)) => println!("  {:<14} {}", "spell-out", sf),
                        Some(Err(e)) => println!("  {:<14} error: {e}", "spell-out"),
                        None => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
        println!();
    }
    ExitCode::SUCCESS
}

fn render(p: &conlang_dsl::Program, w: &conlang_core::repr::word::Word) -> String {
    let mut s = notation::render_skeleton(w, &p.env.syms);
    for t in &w.melodies {
        let tier = notation::render_tier(t, &p.env.vals);
        if !tier.is_empty() {
            s.push_str("  |  ");
            s.push_str(&tier);
        }
    }
    s
}
