//! btop components: titled box, meter, braille sparkline, table, status/error lines.
//! Every width is measured in terminal cells, never in bytes or chars.

use super::theme::{paint, Role};
use super::{round_half_up, truncate, width, Mode};

/// A rendered line plus its visible width (escape codes excluded).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    pub text: String,
    pub width: usize,
}

impl Line {
    pub fn new() -> Self {
        Line::default()
    }
    pub fn of(mode: &Mode, role: Role, s: &str) -> Self {
        let mut l = Line::new();
        l.push(mode, role, s);
        l
    }
    pub fn push(&mut self, mode: &Mode, role: Role, s: &str) -> &mut Self {
        self.text.push_str(&paint(mode, role, s));
        self.width += width(s);
        self
    }
    pub fn append(&mut self, other: &Line) -> &mut Self {
        self.text.push_str(&other.text);
        self.width += other.width;
        self
    }
    pub fn pad(&mut self, n: usize) -> &mut Self {
        self.text.push_str(&" ".repeat(n));
        self.width += n;
        self
    }
}

/// `╭─┐title┌────╮` (ASCII `+-[title]----+`), exactly `w` cells wide.
pub fn box_top(mode: &Mode, title: &str, w: usize, border: u8) -> String {
    notch_line(
        mode,
        title,
        w,
        border,
        mode.g("╭", "+"),
        mode.g("╮", "+"),
        mode.g("┐", "["),
        mode.g("┌", "]"),
    )
}

/// `╰─┘footer└────╯`, or a plain bottom line when `footer` is empty.
pub fn box_bottom(mode: &Mode, footer: &str, w: usize, border: u8) -> String {
    notch_line(
        mode,
        footer,
        w,
        border,
        mode.g("╰", "+"),
        mode.g("╯", "+"),
        mode.g("┘", "["),
        mode.g("└", "]"),
    )
}

#[allow(clippy::too_many_arguments)]
fn notch_line(
    mode: &Mode,
    label: &str,
    w: usize,
    border: u8,
    left: &str,
    right: &str,
    open: &str,
    close: &str,
) -> String {
    let b = Role::Border(border);
    let h = mode.g("─", "-");
    if label.is_empty() {
        return format!(
            "{}{}{}",
            paint(mode, b, left),
            paint(mode, b, &h.repeat(w.saturating_sub(2))),
            paint(mode, b, right)
        );
    }
    let label = truncate(label, w.saturating_sub(6), mode.unicode());
    let fill = w.saturating_sub(5 + width(&label));
    format!(
        "{}{}{}{}{}",
        paint(mode, b, &format!("{left}{h}{open}")),
        paint(mode, Role::Title, &label),
        paint(mode, b, close),
        paint(mode, b, &h.repeat(fill)),
        paint(mode, b, right)
    )
}

/// `│ content    │`, padded (content must already fit in `w - 4` cells).
pub fn box_row(mode: &Mode, content: &Line, w: usize, border: u8) -> String {
    let side = paint(mode, Role::Border(border), mode.g("│", "|"));
    let pad = w.saturating_sub(4 + content.width);
    format!("{side} {}{} {side}", content.text, " ".repeat(pad))
}

/// A whole box: top with title, rows, bottom with footer.
pub fn boxed(
    mode: &Mode,
    title: &str,
    rows: &[Line],
    footer: &str,
    w: usize,
    border: u8,
) -> String {
    let mut out = box_top(mode, title, w, border);
    out.push('\n');
    for r in rows {
        out.push_str(&box_row(mode, r, w, border));
        out.push('\n');
    }
    out.push_str(&box_bottom(mode, footer, w, border));
    out.push('\n');
    out
}

/// Meter of `cells` cells followed by the right-aligned percentage (` 72%`).
pub fn meter(mode: &Mode, value: f64, cells: usize) -> Line {
    let mut l = Line::new();
    for i in 1..=cells {
        let pos = round_half_up(i as f64 * 100.0 / cells as f64);
        if value >= pos as f64 {
            l.push(mode, Role::Grad(pos.clamp(0, 100) as u8), mode.g("■", "#"));
        } else if !mode.unicode() {
            l.push(mode, Role::Dim, ".");
        } else if mode.color() {
            l.push(mode, Role::Dim, "■");
        } else {
            l.push(mode, Role::Dim, "·");
        }
    }
    // Always one space before the value: ` 50%`, `  5%`, ` 100%`.
    let pct = format!(
        " {:>3}",
        format!("{}%", round_half_up(value.clamp(0.0, 999.0)))
    );
    l.push(mode, Role::Text, &pct);
    l
}

const BRAILLE_UP: [[char; 5]; 5] = [
    [' ', '⢀', '⢠', '⢰', '⢸'],
    ['⡀', '⣀', '⣠', '⣰', '⣸'],
    ['⡄', '⣄', '⣤', '⣴', '⣼'],
    ['⡆', '⣆', '⣦', '⣶', '⣾'],
    ['⡇', '⣇', '⣧', '⣷', '⣿'],
];

/// Braille sparkline, two samples per cell, scaled to `max` (or the series maximum).
/// Returns only the graph; the caller prints the numbers next to it.
/// In ASCII mode the graph is empty (the numbers carry the data).
pub fn sparkline(mode: &Mode, samples: &[u64], max: Option<u64>) -> Line {
    let mut l = Line::new();
    if !mode.unicode() || samples.is_empty() {
        return l;
    }
    let max = max.unwrap_or_else(|| samples.iter().copied().max().unwrap_or(0));
    let level = |s: u64| {
        if max == 0 {
            0
        } else {
            round_half_up(s as f64 / max as f64 * 4.0).clamp(0, 4) as usize
        }
    };
    for pair in samples.chunks(2) {
        let (a, b) = (pair[0], *pair.get(1).unwrap_or(&pair[0]));
        let cell = BRAILLE_UP[level(a)][level(b)];
        let pos = if max == 0 {
            0
        } else {
            (a.max(b) as f64 / max as f64 * 100.0).min(100.0) as u8
        };
        l.push(mode, Role::Grad(pos), &cell.to_string());
    }
    l
}

/// Status symbol in its level color: `● ok`, `▲ warning`, `✗ error`.
pub fn status_symbol(mode: &Mode, level: Role) -> String {
    let (u, a) = match level {
        Role::Low => ("●", "+"),
        Role::Mid => ("▲", "!"),
        _ => ("✗", "x"),
    };
    paint(mode, level, mode.g(u, a))
}

#[derive(Debug, Clone)]
pub struct Col {
    pub header: String,
    pub right: bool,
    /// Columns with a higher number are dropped first when the table is too wide; 0 = never.
    pub drop: u8,
    /// Truncate this column (instead of dropping others) as the last resort.
    pub shrink: bool,
}

impl Col {
    pub fn new(header: &str, right: bool, drop: u8) -> Self {
        Col {
            header: header.to_string(),
            right,
            drop,
            shrink: false,
        }
    }
    pub fn shrinkable(mut self) -> Self {
        self.shrink = true;
        self
    }
}

/// Lay out a table in at most `max_w` cells: header row in `title`, no rule under it.
/// Cells are pre-rendered [`Line`]s; shrinkable cells are re-rendered via `shrink`.
pub fn table(
    mode: &Mode,
    cols: &[Col],
    rows: &[Vec<Line>],
    max_w: usize,
    shrink: &dyn Fn(usize, usize, usize) -> Line,
) -> Vec<Line> {
    let gap = 2;
    let mut keep: Vec<bool> = vec![true; cols.len()];
    let natural: Vec<usize> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            rows.iter()
                .map(|r| r[i].width)
                .max()
                .unwrap_or(0)
                .max(width(&c.header))
        })
        .collect();
    let total = |keep: &[bool], widths: &[usize]| -> usize {
        let n = keep.iter().filter(|k| **k).count();
        widths
            .iter()
            .zip(keep)
            .filter(|(_, k)| **k)
            .map(|(w, _)| *w)
            .sum::<usize>()
            + gap * n.saturating_sub(1)
    };
    let mut widths = natural.clone();
    while total(&keep, &widths) > max_w {
        let victim = cols
            .iter()
            .enumerate()
            .filter(|(i, c)| keep[*i] && c.drop > 0)
            .max_by_key(|(i, c)| (c.drop, *i))
            .map(|(i, _)| i);
        match victim {
            Some(i) => keep[i] = false,
            None => break,
        }
    }
    let over = total(&keep, &widths).saturating_sub(max_w);
    if over > 0 {
        if let Some(i) = cols.iter().position(|c| c.shrink) {
            widths[i] = widths[i].saturating_sub(over).max(4);
        }
    }

    let mut out = Vec::new();
    let mut header = Line::new();
    let mut first = true;
    for (i, c) in cols.iter().enumerate().filter(|(i, _)| keep[*i]) {
        if !first {
            header.pad(gap);
        }
        first = false;
        let h = truncate(&c.header, widths[i], mode.unicode());
        let pad = widths[i].saturating_sub(width(&h));
        if c.right {
            header.pad(pad).push(mode, Role::Title, &h);
        } else {
            header.push(mode, Role::Title, &h).pad(pad);
        }
    }
    out.push(header);
    for (r, row) in rows.iter().enumerate() {
        let mut line = Line::new();
        let mut first = true;
        for (i, c) in cols.iter().enumerate().filter(|(i, _)| keep[*i]) {
            if !first {
                line.pad(gap);
            }
            first = false;
            let cell = if row[i].width > widths[i] {
                shrink(r, i, widths[i])
            } else {
                row[i].clone()
            };
            let pad = widths[i].saturating_sub(cell.width);
            if c.right {
                line.pad(pad).append(&cell);
            } else {
                line.append(&cell).pad(pad);
            }
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Glyphs, Tier};

    fn uni(tier: Tier) -> Mode {
        Mode::styled(tier, Glyphs::Unicode, 80)
    }

    #[test]
    fn styled_16_unicode_box_has_escapes_and_notch() {
        let s = boxed(
            &uni(Tier::Ansi16),
            "devstats",
            &[Line::of(&uni(Tier::Ansi16), Role::Text, "hi")],
            "",
            30,
            0,
        );
        assert!(s.contains("\x1b["));
        assert!(s.contains("╭─┐"));
    }

    #[test]
    fn box_lines_have_exact_width() {
        let m = uni(Tier::None);
        let s = boxed(
            &m,
            "title",
            &[Line::of(&m, Role::Text, "row")],
            "foot",
            24,
            0,
        );
        for line in s.lines() {
            assert_eq!(width(line), 24, "line: {line}");
        }
        assert!(s.starts_with("╭─┐title┌"));
        assert!(s.lines().last().unwrap().starts_with("╰─┘foot└"));
    }

    #[test]
    fn ascii_mode_renders_only_ascii_bytes() {
        let m = Mode::styled(Tier::None, Glyphs::Ascii, 80);
        let mut s = boxed(&m, "devstats", &[meter(&m, 42.0, 10)], "3 orgs", 40, 1);
        s.push_str(&sparkline(&m, &[1, 2, 3], None).text);
        s.push_str(&status_symbol(&m, Role::Mid));
        assert!(s.chars().all(|c| (c as u32) < 0x80), "{s}");
    }

    #[test]
    fn meter_50_of_10_without_color() {
        let l = meter(&uni(Tier::None), 50.0, 10);
        assert_eq!(l.text, "■■■■■····· 50%");
        assert_eq!(l.width, 14);
    }

    #[test]
    fn meter_value_never_touches_the_cells() {
        let m = uni(Tier::None);
        assert_eq!(meter(&m, 100.0, 4).text, "■■■■ 100%");
        assert_eq!(meter(&m, 5.0, 4).text, "····  5%");
    }

    #[test]
    fn meter_with_color_uses_dim_squares() {
        let l = meter(&uni(Tier::Ansi16), 0.0, 3);
        assert_eq!(l.text.matches('■').count(), 3);
        assert!(!l.text.contains('·'));
    }

    #[test]
    fn sparkline_levels() {
        let m = uni(Tier::None);
        assert_eq!(sparkline(&m, &[0, 0, 4, 4, 2], Some(4)).text, " ⣿⣤");
        assert_eq!(sparkline(&m, &[0, 0, 0, 0], None).text, "  ");
        assert_eq!(sparkline(&m, &[], None).width, 0);
    }

    #[test]
    fn table_drops_columns_by_priority_then_shrinks() {
        let m = uni(Tier::None);
        let cols = vec![
            Col::new("name", false, 0).shrinkable(),
            Col::new("a", true, 0),
            Col::new("extra", true, 2),
            Col::new("more", true, 1),
        ];
        let cell = |s: &str| Line::of(&m, Role::Text, s);
        let rows = vec![vec![cell("alexander"), cell("1"), cell("12345"), cell("9")]];
        let shrink = |_r: usize, _c: usize, w: usize| cell(&truncate("alexander", w, true));
        let wide = table(&m, &cols, &rows, 80, &shrink);
        assert_eq!(wide[0].text, "name       a  extra  more");
        let narrow = table(&m, &cols, &rows, 18, &shrink);
        assert_eq!(
            narrow[0].text, "name       a  more",
            "extra (drop 2) goes first"
        );
        let tiny = table(&m, &cols, &rows, 8, &shrink);
        assert_eq!(tiny[1].text, "alex…  1");
    }
}
