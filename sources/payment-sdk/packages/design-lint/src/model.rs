use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    pub subject: String,
    pub message: String,
    pub help: String,
    pub location: Location,
    pub related: Vec<Related>,
    pub review: Option<Review>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Related {
    pub label: String,
    pub location: Location,
}

/// Evidence for human review; attaching it never suppresses a finding.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Review {
    pub metadata: Vec<(String, String)>,
    pub dependencies: Vec<String>,
    pub questions: Vec<String>,
}

impl Finding {
    pub fn error(rule: &'static str, subject: impl Into<String>, location: Location) -> Self {
        Self {
            rule,
            severity: Severity::Error,
            subject: subject.into(),
            message: String::new(),
            help: String::new(),
            location,
            related: Vec::new(),
            review: None,
        }
    }

    pub fn warning(rule: &'static str, subject: impl Into<String>, location: Location) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(rule, subject, location)
        }
    }

    #[must_use]
    pub const fn is_violation(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Summary {
    pub rule: &'static str,
    pub severity: Severity,
    pub findings: usize,
}
