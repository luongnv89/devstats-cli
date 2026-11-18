//! Output styling. Phase 1 holds only text sanitizing; the btop look lands in Phase 2.

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
