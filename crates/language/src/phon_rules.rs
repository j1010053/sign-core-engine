//! Cyclic realization rules: domain declarations plus branch rules only.
//! Final grammar rules are intentionally excluded at this boundary.
use crate::codegen;

#[derive(Debug, thiserror::Error)]
pub(crate) enum RuleError {
    #[error("phon compile: {0}")]
    Compile(String),
    #[error("phon runtime: {0}")]
    Runtime(String),
}

pub(crate) fn apply(
    declarations: &[String],
    input: &str,
    rules: &[&crate::Rule],
) -> Result<String, RuleError> {
    let mut source = String::new();
    for line in declarations {
        source.push_str(line);
        source.push('\n');
    }
    source.push('\n');
    let mut number = 1u32;
    for rule in rules {
        codegen::emit_rule(&mut source, &mut number, rule)
            .map_err(|error| RuleError::Compile(error.to_string()))?;
    }
    let program =
        tshiatun_dsl::compile(&source).map_err(|error| RuleError::Compile(error.to_string()))?;
    let word = tshiatun_dsl::build_phrase(&program, input)
        .map_err(|error| RuleError::Runtime(error.to_string()))?;
    let fallback = word.clone();
    let steps = tshiatun_dsl::run_program(&program, word)
        .map_err(|error| RuleError::Runtime(error.to_string()))?;
    let last = steps.last().map(|step| &step.word).unwrap_or(&fallback);
    tshiatun_dsl::surface_phrase(&program, last)
        .map_err(|error| RuleError::Runtime(error.to_string()))
}

/// A realized phon string must no longer contain DSL references or delimiters.
/// Keep the caller's error type and input untouched; this is only a predicate.
pub(crate) fn is_pure_input(input: &str) -> bool {
    !input.contains("$self") && !input.contains("$slot") && !input.contains(['{', '}', '/'])
}
