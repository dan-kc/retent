//! Stable, source-oriented diagnostics.

use std::fmt;
use std::path::PathBuf;

/// A validation problem with a stable machine-readable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub code: &'static str,
    pub message: String,
}

impl Diagnostic {
    /// Construct a diagnostic for `path`.
    pub fn new(
        path: impl Into<PathBuf>,
        line: Option<usize>,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            line,
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.path.display())?;
        if let Some(line) = self.line {
            write!(formatter, ":{line}")?;
        }
        write!(formatter, " [{}] {}", self.code, self.message)
    }
}
