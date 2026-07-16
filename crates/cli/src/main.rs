//! conlang CLI — 獨立音變 DSL(P20 路徑 A)。
//!
//! 契約:`(規則檔, 詞表) → 詞表′`(修補05 §1.1)。
//! 預設輸出**純詞表′**(一行一詞,管線友善);`--trace` 印逐規則推導表(含 spell-out)。
//! IO/println 只住殼層;core/dsl 維持 WASM-safe(M0 §1.2)。

use std::process::ExitCode;

use conlang_core::repr::notation;
use conlang_dsl::{build_word, compile, run_program, surface, Program};

const USAGE: &str = "usage: conlang [--trace] <rules.dsl> <words.txt>
       conlang --help | --version

  (規則檔, 詞表) → 詞表′:對詞表逐詞套用規則檔,輸出演化後詞形。
  預設一行一詞;--trace 印逐規則推導表(每 commit 一行)與 spell-out。
  詞表:一行一詞;空行與 /* 開頭行略過。";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut trace = false;
    let mut paths: Vec<&str> = Vec::new();
    for a in &args {
        match a.as_str() {
            "--trace" => trace = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" | "-V" => {
                println!("conlang {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("unknown option: {other}\n{USAGE}");
                return ExitCode::from(2);
            }
            other => paths.push(other),
        }
    }
    let [rules_path, words_path] = paths.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let src = match std::fs::read_to_string(rules_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {rules_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let words = match std::fs::read_to_string(words_path) {
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
        if trace {
            println!("*{word_text}");
            println!("  {:<14} {}", "input", render_trace(&program, &w));
        }
        let steps = match run_program(&program, w) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        let last = steps.last().map(|s| s.word.clone()).unwrap_or_default();
        if trace {
            for s in &steps {
                let issues = if s.issues.is_empty() {
                    String::new()
                } else {
                    format!("   [{} issue(s)]", s.issues.len())
                };
                println!("  {:<14} {}{}", s.rule, render_trace(&program, &s.word), issues);
            }
        }
        // 詞表′:Spell-out 宣告過 → 表層;否則末狀態骨架(緊排)
        let out = match surface(&program, &last) {
            Some(Ok(sf)) => sf.replace(' ', ""),
            Some(Err(e)) => {
                eprintln!("error: spell-out: {e}");
                return ExitCode::FAILURE;
            }
            None => last
                .skeleton
                .iter()
                .filter_map(|s| program.env.syms.resolve(s.sym))
                .collect(),
        };
        if trace {
            println!("  {:<14} {out}\n", "⇒");
        } else {
            println!("{out}");
        }
    }
    ExitCode::SUCCESS
}

fn render_trace(p: &Program, w: &conlang_core::repr::word::Word) -> String {
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
