//! Structured validation diagnostics shared by compile, ontology,
//! constructions, patches, and synchronous rule execution.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceLocation {
    /// One-based physical source line. Zero means a value constructed through
    /// the Rust API without a source file location.
    pub line: usize,
    /// One-based column when known. The M1++ parser currently records line
    /// granularity and leaves this at zero.
    pub column: usize,
}

impl SourceLocation {
    pub const fn unknown() -> SourceLocation {
        SourceLocation { line: 0, column: 0 }
    }

    pub const fn line(line: usize) -> SourceLocation {
        SourceLocation { line, column: 0 }
    }

    pub const fn is_known(self) -> bool {
        self.line != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticSource {
    /// Owning trait, sign, construction, or generated artifact.
    pub owner: String,
    /// Field/slot/rule path within the owner when applicable.
    pub path: Option<String>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable machine-readable code (for example `ONTOLOGY_UNKNOWN_TRAIT`).
    pub code: &'static str,
    pub message: String,
    /// Winner first for resolved conflicts, followed by shadowed sources.
    pub sources: Vec<DiagnosticSource>,
}

impl Diagnostic {
    pub fn new(severity: Severity, code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity,
            code,
            message: message.into(),
            sources: Vec::new(),
        }
    }

    pub fn with_sources(mut self, sources: Vec<DiagnosticSource>) -> Diagnostic {
        self.sources = sources;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub fn new() -> ValidationReport {
        ValidationReport::default()
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }
}
