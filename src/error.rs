//! Application errors and the canonical exit-code mapping.
//!
//! | Code | Meaning       |
//! |------|---------------|
//! | 0    | success       |
//! | 1    | runtime error (network, API, rate limit exhausted) |
//! | 2    | usage error (bad flags or flag values)             |
//! | 3    | input error (bad org URL, no org found)            |
//! | 130  | interrupted (Ctrl+C, reported by the shell)        |

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Runtime,
    Usage,
    Input,
}

#[derive(Debug, Clone)]
pub struct AppError {
    pub kind: Kind,
    pub msg: String,
    pub hint: Option<String>,
}

impl AppError {
    fn new(kind: Kind, msg: impl Into<String>) -> Self {
        AppError {
            kind,
            msg: msg.into(),
            hint: None,
        }
    }
    pub fn runtime(msg: impl Into<String>) -> Self {
        Self::new(Kind::Runtime, msg)
    }
    pub fn usage(msg: impl Into<String>) -> Self {
        Self::new(Kind::Usage, msg)
    }
    pub fn input(msg: impl Into<String>) -> Self {
        Self::new(Kind::Input, msg)
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    pub fn exit_code(&self) -> i32 {
        match self.kind {
            Kind::Runtime => 1,
            Kind::Usage => 2,
            Kind::Input => 3,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for AppError {}

pub type Result<T> = std::result::Result<T, AppError>;
