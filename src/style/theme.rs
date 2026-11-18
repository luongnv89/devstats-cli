//! Theme roles → escape codes for the active color tier (btop default theme).
//! Command code names a role; it never contains an escape code or a hex value.

use anstyle::{Ansi256Color, AnsiColor, Color, Effects, RgbColor, Style};

use super::{round_half_up, Mode, Tier};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Body text: the terminal's default color.
    Text,
    /// Box titles, table headers.
    Title,
    /// Flags and command names, key letters.
    Accent,
    /// Units, hints, defaults, empty meter cells.
    Dim,
    /// Box lines; 0-3 picks cpu/mem/net/proc box colors to tell sections apart.
    Border(u8),
    Low,
    Mid,
    High,
    /// Gradient position 0-100 (low → mid → high).
    Grad(u8),
    /// The highest value in a table column (bold cyan, a color no other cell uses).
    Lead,
}

const BORDERS: [(u8, u8, u8); 4] = [
    (0x55, 0x6d, 0x59),
    (0x6c, 0x6c, 0x4b),
    (0x5c, 0x58, 0x8d),
    (0x80, 0x52, 0x52),
];
const BORDERS16: [AnsiColor; 4] = [
    AnsiColor::Green,
    AnsiColor::Yellow,
    AnsiColor::Magenta,
    AnsiColor::Red,
];
const ACCENT: (u8, u8, u8) = (0xb5, 0x40, 0x40);
const LOW: (u8, u8, u8) = (0x77, 0xca, 0x9b);
const MID: (u8, u8, u8) = (0xcb, 0xc0, 0x6c);
const HIGH: (u8, u8, u8) = (0xdc, 0x4c, 0x4c);
const LEAD: (u8, u8, u8) = (0x56, 0xc8, 0xe8);

/// Gradient color at position 0-100: linear low→mid over 0-50, mid→high over 50-100.
pub fn gradient(pos: u8) -> (u8, u8, u8) {
    let p = pos.min(100) as f64;
    let (a, b, t) = if p <= 50.0 {
        (LOW, MID, p / 50.0)
    } else {
        (MID, HIGH, (p - 50.0) / 50.0)
    };
    let mix =
        |x: u8, y: u8| round_half_up(x as f64 + (y as f64 - x as f64) * t).clamp(0, 255) as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// btop's 24-bit → 256-color conversion.
pub fn to_256((r, g, b): (u8, u8, u8)) -> u8 {
    let (r11, g11, b11) = (
        round_half_up(r as f64 / 11.0),
        round_half_up(g as f64 / 11.0),
        round_half_up(b as f64 / 11.0),
    );
    if r11 == g11 && g11 == b11 {
        (232 + r11).min(255) as u8
    } else {
        (16 + 36 * round_half_up(r as f64 / 51.0)
            + 6 * round_half_up(g as f64 / 51.0)
            + round_half_up(b as f64 / 51.0)) as u8
    }
}

fn rgb(tier: Tier, c: (u8, u8, u8), basic: AnsiColor) -> Option<Color> {
    match tier {
        Tier::None => None,
        Tier::Ansi16 => Some(Color::Ansi(basic)),
        Tier::Ansi256 => Some(Color::Ansi256(Ansi256Color(to_256(c)))),
        Tier::TrueColor => Some(Color::Rgb(RgbColor(c.0, c.1, c.2))),
    }
}

fn grad16(pos: u8) -> AnsiColor {
    match pos {
        0..=33 => AnsiColor::BrightGreen,
        34..=66 => AnsiColor::BrightYellow,
        _ => AnsiColor::BrightRed,
    }
}

pub fn style(role: Role, tier: Tier) -> Style {
    if tier == Tier::None {
        return Style::new();
    }
    let s = Style::new();
    match role {
        Role::Text => s,
        Role::Title => s.effects(Effects::BOLD),
        Role::Accent => s
            .fg_color(rgb(tier, ACCENT, AnsiColor::BrightRed))
            .effects(Effects::BOLD),
        Role::Dim => s.effects(Effects::DIMMED),
        Role::Border(i) => {
            let i = (i % 4) as usize;
            s.fg_color(rgb(tier, BORDERS[i], BORDERS16[i]))
        }
        Role::Low => style(Role::Grad(0), tier),
        Role::Mid => style(Role::Grad(50), tier),
        Role::High => style(Role::Grad(100), tier),
        Role::Grad(p) => s.fg_color(rgb(tier, gradient(p), grad16(p.min(100)))),
        Role::Lead => s
            .fg_color(rgb(tier, LEAD, AnsiColor::BrightCyan))
            .effects(Effects::BOLD),
    }
}

/// Paint `text` in `role`. Plain modes and tier none get the text unchanged.
pub fn paint(mode: &Mode, role: Role, text: &str) -> String {
    if !mode.color() || text.is_empty() || role == Role::Text {
        return text.to_string();
    }
    let st = style(role, mode.tier);
    format!("{}{}{}", st.render(), text, st.render_reset())
}

/// Same as [`paint`] with bold added (used for the error label).
pub fn paint_bold(mode: &Mode, role: Role, text: &str) -> String {
    if !mode.color() {
        return text.to_string();
    }
    let st = style(role, mode.tier).effects(Effects::BOLD);
    format!("{}{}{}", st.render(), text, st.render_reset())
}

/// clap help styles: `accent` for flags and commands, `title` for headings.
pub fn clap_styles() -> clap::builder::Styles {
    let tier = Tier::TrueColor;
    let accent = style(Role::Accent, tier);
    let title = style(Role::Title, tier);
    clap::builder::Styles::styled()
        .header(title)
        .usage(title)
        .literal(accent)
        .placeholder(style(Role::Dim, tier))
        .valid(accent)
        .invalid(style(Role::High, tier).effects(Effects::BOLD))
        .error(style(Role::High, tier).effects(Effects::BOLD))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_endpoints() {
        assert_eq!(gradient(0), LOW);
        assert_eq!(gradient(50), MID);
        assert_eq!(gradient(100), HIGH);
        assert_eq!(gradient(200), HIGH);
    }

    #[test]
    fn btop_256_conversion() {
        assert_eq!(to_256((0x30, 0x30, 0x30)), 232 + 4);
        assert_eq!(to_256((0xdc, 0x4c, 0x4c)), 16 + 36 * 4 + 6 + 1);
    }

    #[test]
    fn tier_none_has_no_escapes() {
        let m = Mode::styled(Tier::None, crate::style::Glyphs::Unicode, 80);
        assert_eq!(paint(&m, Role::Accent, "x"), "x");
        assert_eq!(paint_bold(&m, Role::High, "x"), "x");
    }

    #[test]
    fn sixteen_color_codes() {
        let m = Mode::styled(Tier::Ansi16, crate::style::Glyphs::Unicode, 80);
        assert_eq!(paint(&m, Role::Border(0), "─"), "\x1b[32m─\x1b[0m");
        assert!(paint(&m, Role::Grad(80), "■").starts_with("\x1b[91m"));
        assert!(paint(&m, Role::Dim, "x").starts_with("\x1b[2m"));
    }

    #[test]
    fn lead_has_its_own_color_in_every_tier() {
        use crate::style::Glyphs;
        for tier in [Tier::Ansi16, Tier::Ansi256, Tier::TrueColor] {
            let lead = style(Role::Lead, tier);
            assert!(lead.get_fg_color().is_some(), "{tier:?}");
            assert!(lead.get_effects().contains(Effects::BOLD), "{tier:?}");
            let others = [Role::Title, Role::Accent, Role::Low, Role::Mid, Role::High]
                .into_iter()
                .chain((0..=100).map(Role::Grad))
                .chain((0..4).map(Role::Border));
            for other in others {
                assert_ne!(
                    style(other, tier).get_fg_color(),
                    lead.get_fg_color(),
                    "{tier:?} {other:?}"
                );
            }
        }
        let m = Mode::styled(Tier::Ansi16, Glyphs::Unicode, 80);
        assert!(paint(&m, Role::Lead, "9").contains("\x1b[96m"));
        let none = Mode::styled(Tier::None, Glyphs::Unicode, 80);
        assert_eq!(paint(&none, Role::Lead, "9"), "9");
    }
}
