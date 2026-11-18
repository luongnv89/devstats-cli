//! btop-inspired terminal styling (see the design's "Visual style").
//!
//! The three decisions per stream (styled or plain, color tier, glyph set) are made
//! once, right after argument parsing. Render functions take an explicit [`Mode`], so
//! tests can render every path without a terminal.

pub mod components;
pub mod theme;

use std::io::IsTerminal;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyphs {
    Unicode,
    Ascii,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    /// False: plain records, no frames or escape codes.
    pub styled: bool,
    pub tier: Tier,
    pub glyphs: Glyphs,
    /// Terminal width in cells.
    pub width: usize,
}

impl Mode {
    pub const PLAIN: Mode = Mode { styled: false, tier: Tier::None, glyphs: Glyphs::Ascii, width: 80 };

    pub fn styled(tier: Tier, glyphs: Glyphs, width: usize) -> Mode {
        Mode { styled: true, tier, glyphs, width }
    }
    pub fn color(&self) -> bool {
        self.styled && self.tier != Tier::None
    }
    pub fn unicode(&self) -> bool {
        self.glyphs == Glyphs::Unicode
    }
    /// Pick the Unicode or ASCII form of a glyph.
    pub fn g<'a>(&self, unicode: &'a str, ascii: &'a str) -> &'a str {
        if self.unicode() {
            unicode
        } else {
            ascii
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// The parts of the environment the decisions depend on.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub term: Option<String>,
    pub force_color: Option<String>,
    pub no_color: Option<String>,
    pub colorterm: Option<String>,
    /// First set of LC_ALL, LC_CTYPE, LANG.
    pub locale: Option<String>,
    pub wt_session: bool,
    pub columns: Option<String>,
    pub is_tty: bool,
    pub tty_width: Option<usize>,
}

impl Env {
    pub fn capture(stream: Stream) -> Env {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let (is_tty, tty_width) = match stream {
            Stream::Stdout => {
                let s = std::io::stdout();
                (s.is_terminal(), terminal_size::terminal_size_of(&s).map(|(w, _)| w.0 as usize))
            }
            Stream::Stderr => {
                let s = std::io::stderr();
                (s.is_terminal(), terminal_size::terminal_size_of(&s).map(|(w, _)| w.0 as usize))
            }
        };
        Env {
            term: var("TERM"),
            force_color: var("FORCE_COLOR"),
            no_color: var("NO_COLOR"),
            colorterm: var("COLORTERM"),
            locale: var("LC_ALL").or_else(|| var("LC_CTYPE")).or_else(|| var("LANG")),
            wt_session: var("WT_SESSION").is_some(),
            columns: var("COLUMNS"),
            is_tty,
            tty_width,
        }
    }

    fn forced(&self) -> bool {
        self.force_color.as_deref().is_some_and(|v| v != "0")
    }
}

/// Decide the mode for one stream. `machine` is true when stdout carries a format a
/// program parses (json, tsv); it always wins, even over FORCE_COLOR.
pub fn decide(env: &Env, stream: Stream, machine: bool, no_color_flag: bool) -> Mode {
    let width = env
        .columns
        .as_deref()
        .and_then(|c| c.trim().parse().ok())
        .filter(|&w: &usize| w > 0)
        .or(env.tty_width)
        .unwrap_or(80)
        .clamp(40, 400);
    let plain = (stream == Stream::Stdout && machine)
        || env.term.as_deref() == Some("dumb")
        || (!env.is_tty && !env.forced());
    if plain {
        return Mode { width, ..Mode::PLAIN };
    }
    let tier = if no_color_flag || env.no_color.is_some() {
        Tier::None
    } else if matches!(env.colorterm.as_deref(), Some("truecolor") | Some("24bit")) {
        Tier::TrueColor
    } else if env.term.as_deref().is_some_and(|t| t.contains("256color")) {
        Tier::Ansi256
    } else {
        Tier::Ansi16
    };
    let utf8 = env.locale.as_deref().is_some_and(|l| {
        let l = l.to_ascii_lowercase();
        l.contains("utf-8") || l.contains("utf8")
    }) || (cfg!(windows) && env.wt_session);
    Mode::styled(tier, if utf8 { Glyphs::Unicode } else { Glyphs::Ascii }, width)
}

/// Replace control characters in user-supplied text with visible escapes.
/// With `plain`, also escape backslashes so one record stays one line with the same fields.
pub fn sanitize(s: &str, plain: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\\' if plain => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || (0x7f..=0x9f).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

/// Width in terminal cells (CJK and emoji count as two).
pub fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Shorten `s` to at most `max` cells, ending with an ellipsis when cut.
pub fn truncate(s: &str, max: usize, unicode: bool) -> String {
    if width(s) <= max {
        return s.to_string();
    }
    let ell = if unicode { "…" } else { "..." };
    let room = max.saturating_sub(width(ell));
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > room {
            break;
        }
        out.push(c);
        used += w;
    }
    if max >= width(ell) {
        out.push_str(ell);
    }
    out
}

/// Round half up, as btop's C++ does (Rust's `round` rounds half away from zero, which
/// matches for the non-negative values used here; this keeps the intent explicit).
pub fn round_half_up(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tty() -> Env {
        Env { is_tty: true, term: Some("xterm-256color".into()), locale: Some("C.UTF-8".into()), ..Default::default() }
    }

    #[test]
    fn piped_stdout_is_plain_unless_forced() {
        let env = Env { is_tty: false, ..tty() };
        assert!(!decide(&env, Stream::Stdout, false, false).styled);
        let forced = Env { force_color: Some("1".into()), ..env.clone() };
        assert!(decide(&forced, Stream::Stdout, false, false).styled);
        let zero = Env { force_color: Some("0".into()), ..env };
        assert!(!decide(&zero, Stream::Stdout, false, false).styled);
    }

    #[test]
    fn machine_format_wins_over_force_color_on_stdout_only() {
        let env = Env { force_color: Some("1".into()), ..tty() };
        assert!(!decide(&env, Stream::Stdout, true, false).styled);
        assert!(decide(&env, Stream::Stderr, true, false).styled);
    }

    #[test]
    fn dumb_terminal_is_plain() {
        let env = Env { term: Some("dumb".into()), ..tty() };
        assert!(!decide(&env, Stream::Stdout, false, false).styled);
    }

    #[test]
    fn color_tiers() {
        assert_eq!(decide(&tty(), Stream::Stdout, false, false).tier, Tier::Ansi256);
        assert_eq!(decide(&tty(), Stream::Stdout, false, true).tier, Tier::None);
        let nc = Env { no_color: Some("1".into()), force_color: Some("1".into()), ..tty() };
        assert_eq!(decide(&nc, Stream::Stdout, false, false).tier, Tier::None);
        let tc = Env { colorterm: Some("truecolor".into()), ..tty() };
        assert_eq!(decide(&tc, Stream::Stdout, false, false).tier, Tier::TrueColor);
        let basic = Env { term: Some("xterm".into()), ..tty() };
        assert_eq!(decide(&basic, Stream::Stdout, false, false).tier, Tier::Ansi16);
    }

    #[test]
    fn glyphs_follow_locale_and_width_follows_columns() {
        let m = decide(&tty(), Stream::Stdout, false, false);
        assert_eq!(m.glyphs, Glyphs::Unicode);
        assert_eq!(m.width, 80);
        let c = Env { locale: Some("C".into()), columns: Some("132".into()), ..tty() };
        let m = decide(&c, Stream::Stdout, false, false);
        assert_eq!((m.glyphs, m.width), (Glyphs::Ascii, 132));
    }

    #[test]
    fn sanitize_escapes_controls() {
        assert_eq!(sanitize("a\x1b[31mb\tc\\", false), "a\\x1b[31mb\\tc\\");
        assert_eq!(sanitize("a\\b\n", true), "a\\\\b\\n");
        assert_eq!(sanitize("\u{9b}x", true), "\\x9bx");
    }

    #[test]
    fn truncate_by_cells() {
        assert_eq!(truncate("contributor", 8, true), "contrib…");
        assert_eq!(truncate("contributor", 8, false), "contr...");
        assert_eq!(truncate("日本語テキスト", 7, true), "日本語…");
        assert_eq!(truncate("short", 8, true), "short");
    }
}
