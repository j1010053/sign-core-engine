//! Lexurgy 社群黃金測試(M0 步驟 7 執行級確認)。
//!
//! 自 corpus 的 core spec 測試(`TestRules/TestEnvironment/TestBoundaries.kt`)萃取
//! inline「規則 + 輸入 + 期望」三元組——**只取測試資料**(I5 語料用途;哨兵規則:
//! 不讀實作)。過濾出 M0 子集(單符號改寫 `x => y|* [/ P _ S]`,`$`→`#`,D19),
//! 經本引擎實跑並與 Lexurgy 期望輸出比對。超出子集者計入 skipped(M2 匯入器範圍,
//! 見 corpus/whitelist.md)。
//!
//! 語意對齊點:Lexurgy「compound rules 同時執行」= 本引擎 B5(同規則語句共享凍結
//! match、一次 commit);規則間循序 = 每規則一 commit。

use conlang_core::repr::word::{Seg, Word};
use conlang_core::repr::FeatBits;
use conlang_dsl::{compile, run_program, Program};

const KT_FILES: &[&str] = &[
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/lexurgy/core/src/test/kotlin/com/meamoria/lexurgy/sc/TestRules.kt"
    ),
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/lexurgy/core/src/test/kotlin/com/meamoria/lexurgy/sc/TestEnvironment.kt"
    ),
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/lexurgy/core/src/test/kotlin/com/meamoria/lexurgy/sc/TestBoundaries.kt"
    ),
];

/// 一個萃取案例:規則原文 + (輸入, 期望) 對。
struct Case {
    rules: String,
    pairs: Vec<(String, String)>,
}

/// 從 .kt 原始碼萃取 `lsc("""…""")` + `ch("in") shouldBe "out"`(純字串掃描)。
fn extract(src: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let chunks: Vec<&str> = src.split("lsc(").skip(1).collect();
    for chunk in chunks {
        let Some(q1) = chunk.find("\"\"\"") else { continue };
        let body = &chunk[q1 + 3..];
        let Some(q2) = body.find("\"\"\"") else { continue };
        let rules = body[..q2]
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n");
        let rest = &body[q2 + 3..];
        let mut pairs = Vec::new();
        for line in rest.lines() {
            let Some(sb) = line.find(") shouldBe ") else { continue };
            let head = &line[..sb];
            let Some(op) = head.find("(\"") else { continue };
            let input = &head[op + 2..];
            let Some(input) = input.strip_suffix('"') else { continue };
            let tail = &line[sb + ") shouldBe ".len()..].trim();
            let Some(expected) = tail
                .strip_prefix('"')
                .and_then(|t| t.strip_suffix('"'))
            else {
                continue;
            };
            if input.contains(['"', ' ']) {
                continue; // 多詞/複雜輸入不取
            }
            pairs.push((input.to_owned(), expected.to_owned()));
        }
        if !pairs.is_empty() {
            cases.push(Case { rules, pairs });
        }
    }
    cases
}

/// M0 子集判定 + 轉換($→#);超出子集回 None。
fn convert(rules: &str) -> Option<String> {
    let mut out_lines = Vec::new();
    for l in rules.lines() {
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        if l.contains(['{', '[', '(', '!', '+', ',']) || l.contains("//") {
            return None; // 類/矩陣/量詞/unless:M2 匯入器範圍
        }
        if let Some(name) = l.strip_suffix(':') {
            if name.chars().all(|c| c.is_alphanumeric() || c == '-') && !name.is_empty() {
                out_lines.push(l.to_owned());
                continue;
            }
            return None; // 帶 filter 的規則頭等
        }
        let (lhs, rest) = l.split_once("=>")?;
        let lhs = lhs.trim();
        if lhs.chars().count() != 1 || !lhs.chars().next().unwrap().is_alphabetic() {
            return None; // 多符號/插入規則
        }
        let (rhs, env) = match rest.split_once('/') {
            None => (rest.trim(), None),
            Some((r, e)) => (r.trim(), Some(e.trim())),
        };
        let rhs_ok = rhs == "*"
            || (rhs.chars().count() == 1 && rhs.chars().next().unwrap().is_alphabetic());
        if !rhs_ok {
            return None;
        }
        let mut env_out = String::new();
        if let Some(e) = env {
            let parts: Vec<&str> = e.split_whitespace().collect();
            if parts.iter().filter(|p| **p == "_").count() != 1 {
                return None;
            }
            for p in &parts {
                let ok = *p == "_"
                    || *p == "$"
                    || (p.chars().count() == 1 && p.chars().next().unwrap().is_alphabetic());
                if !ok {
                    return None;
                }
            }
            // 環境至多 前一項 _ 後一項(M0 骨架相鄰一格)
            let ui = parts.iter().position(|p| *p == "_").unwrap();
            if ui > 1 || parts.len() - ui > 2 {
                return None;
            }
            env_out = format!(" / {}", e.replace('$', "#"));
        }
        out_lines.push(format!("{lhs} => {rhs}{env_out}"));
    }
    Some(out_lines.join("\n"))
}

/// 收集規則+詞中的全部字母,合成 Symbol 宣告。
fn synth_symbols(rules: &str, pairs: &[(String, String)]) -> String {
    let mut chars: Vec<char> = Vec::new();
    let mut push = |c: char| {
        if c.is_alphabetic() && !chars.contains(&c) {
            chars.push(c);
        }
    };
    for c in rules.chars() {
        push(c);
    }
    for (i, o) in pairs {
        for c in i.chars().chain(o.chars()) {
            push(c);
        }
    }
    chars
        .iter()
        .map(|c| format!("Symbol {c}\n"))
        .collect::<String>()
}

fn build_plain_word(p: &Program, text: &str) -> Option<Word> {
    let mut w = Word::new();
    for ch in text.chars() {
        let sym = (0..p.env.syms.len() as u32)
            .map(conlang_core::repr::intern::SymId)
            .find(|&s| p.env.syms.resolve(s) == Some(ch.to_string().as_str()))?;
        w.skeleton.push(Seg::new(sym, FeatBits::EMPTY));
    }
    Some(w)
}

fn final_form(p: &Program, w: &Word) -> String {
    w.skeleton
        .iter()
        .filter_map(|s| p.env.syms.resolve(s.sym))
        .collect()
}

#[test]
fn lexurgy_golden_subset_matches_expected() {
    let mut attempted = 0usize;
    let mut passed = 0usize;
    let mut skipped_cases = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut report = String::new();

    for path in KT_FILES {
        let Ok(src) = std::fs::read_to_string(path) else {
            eprintln!("corpus not checked out; skipping {path}");
            return;
        };
        for case in extract(&src) {
            let Some(rules) = convert(&case.rules) else {
                skipped_cases += 1;
                continue;
            };
            let program_src = format!("{}\n{}\n", synth_symbols(&rules, &case.pairs), rules);
            let p = match compile(&program_src) {
                Ok(p) => p,
                Err(e) => {
                    failures.push(format!("compile error: {e}\n---\n{rules}"));
                    continue;
                }
            };
            for (input, expected) in &case.pairs {
                attempted += 1;
                let Some(w) = build_plain_word(&p, input) else {
                    failures.push(format!("unknown segment in {input:?}"));
                    continue;
                };
                match run_program(&p, w) {
                    Ok(steps) => {
                        let got = steps
                            .last()
                            .map(|s| final_form(&p, &s.word))
                            .unwrap_or_else(|| input.clone());
                        if &got == expected {
                            passed += 1;
                            report.push_str(&format!("PASS {input} -> {got}\n"));
                        } else {
                            failures.push(format!(
                                "MISMATCH {input}: got {got}, lexurgy expects {expected}\n---\n{rules}"
                            ));
                        }
                    }
                    Err(e) => failures.push(format!("engine error on {input:?}: {e}")),
                }
            }
        }
    }

    report.push_str(&format!(
        "\nattempted={attempted} passed={passed} skipped_cases={skipped_cases}(M2)\n"
    ));
    insta::assert_snapshot!("lexurgy_golden", report);
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(attempted >= 8, "expected a meaningful subset, got {attempted}");
}
