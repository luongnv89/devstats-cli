//! `--html`: one self-contained HTML file (inline CSS and JS; the data is embedded JSON).

use std::path::Path;

use crate::error::{AppError, Result};
use crate::model::Report;

const TEMPLATE: &str = include_str!("../../assets/report.html");
const PLACEHOLDER: &str = "__DEVSTATS_DATA__";

/// JSON that is safe inside `<script type="application/json">`: nothing in it can close
/// the tag, open a comment, or break a JS string (`<`, `>`, `&`, U+2028, U+2029 escaped).
pub fn embed_json(report: &Report) -> String {
    let json = serde_json::to_string(report).expect("report serializes");
    let mut out = String::with_capacity(json.len() + 64);
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}

pub fn render(report: &Report) -> String {
    TEMPLATE.replacen(PLACEHOLDER, &embed_json(report), 1)
}

pub fn write(report: &Report, path: &Path) -> Result<()> {
    std::fs::write(path, render(report)).map_err(|e| {
        AppError::runtime(format!(
            "cannot write the HTML report to {}: {e}",
            path.display()
        ))
        .hint("check that the folder exists and is writable, or pass another path with --html=PATH")
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn template_has_one_placeholder_and_no_external_scripts() {
        assert_eq!(super::TEMPLATE.matches(super::PLACEHOLDER).count(), 1);
        assert!(!super::TEMPLATE.contains("<script src"));
        assert!(!super::TEMPLATE.contains("<link rel=\"stylesheet\""));
    }
}
