//! conlang-dsl — Tier 音變 DSL 前端(M0 步驟 4)。
//!
//! 管線:`parse_str`(logos+chumsky,I6)→ [`ast::FileAst`] → [`lower::lower`] →
//! [`lower::Program`] → [`exec::run_program`](verbs+lifecycle)。
//! 造詞:[`build::build_word`](暫定 CV 音節化;`Parse` 宣告於步驟 5+ 取代)。
//!
//! 音段規則語法貼合 Lexurgy(`Feature`/`Symbol`/`Class` 宣告、`A => B / C _ D`);
//! 沿用其**文法形式**,不引用其實作(I5 哨兵規則)。

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod ast;
pub mod build;
pub mod exec;
pub mod lexer;
pub mod lower;
pub mod parser;

pub use build::build_word;
pub use exec::{run_program, StepRecord};
pub use lower::{lower, LowerError, Program};

use conlang_core::lifecycle::EngineError;

/// DSL 前端統一錯誤。
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DslError {
    #[error(transparent)]
    Lex(#[from] lexer::LexError),
    #[error(transparent)]
    Parse(#[from] parser::ParseError),
    #[error(transparent)]
    Lower(#[from] LowerError),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("word {word:?}: no declared symbol matches at byte {at}")]
    UnknownSegment { word: String, at: usize },
    #[error("word {word:?}: no vowel nucleus (declare `Class vowel {{…}}`)")]
    NoNucleus { word: String },
}

/// 一步到位:原始碼 → 可執行 Program。
pub fn compile(src: &str) -> Result<Program, DslError> {
    let lines = lexer::lex_lines(src)?;
    let file = parser::parse_lines(&lines)?;
    Ok(lower(&file)?)
}
