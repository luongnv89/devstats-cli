//! Everything devstats writes to stderr: verbose logs, warnings, errors.

use std::io::Write;

use crate::error::AppError;

pub struct Reporter {
    pub verbose: bool,
    pub quiet: bool,
}

impl Reporter {
    pub fn new(verbose: bool, quiet: bool) -> Self {
        Reporter { verbose, quiet }
    }

    fn line(&self, s: &str) {
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "{s}");
    }

    pub fn debug(&self, msg: &str) {
        if self.verbose {
            self.line(&format!("· {msg}"));
        }
    }

    pub fn warn(&self, msg: &str) {
        if !self.quiet {
            self.line(&format!("! warning: {msg}"));
        }
    }

    pub fn info(&self, msg: &str) {
        if !self.quiet {
            self.line(msg);
        }
    }

    pub fn error(&self, e: &AppError) {
        self.line(&format!("x error: {}", e.msg));
        if let Some(h) = &e.hint {
            self.line(&format!("  hint: {h}"));
        }
    }
}
