//! `--format json`: the whole report, always plain.

use crate::model::Report;

pub fn render(report: &Report) -> String {
    let mut s = serde_json::to_string_pretty(report).expect("report serializes");
    s.push('\n');
    s
}
