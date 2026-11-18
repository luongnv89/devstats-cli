//! Everything devstats writes to stderr: progress, verbose logs, warnings, errors.

use std::io::{IsTerminal, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::AppError;
use crate::style::components::{meter, status_symbol, Line};
use crate::style::theme::{paint, paint_bold, Role};
use crate::style::{truncate, Mode};

/// Progress shows only after this long, so quick runs print nothing extra.
const PROGRESS_DELAY: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Progress {
    label: String,
    done: usize,
    total: Option<usize>,
    /// Shown instead of the meter, e.g. "counting PRs and issues".
    note: Option<String>,
    /// Visible width of the progress line currently on screen (0 = none).
    shown: usize,
}

pub struct Reporter {
    pub verbose: bool,
    pub quiet: bool,
    mode: Mode,
    progress_on: bool,
    start: Instant,
    state: Mutex<Progress>,
}

impl Reporter {
    pub fn new(mode: Mode, verbose: bool, quiet: bool) -> Self {
        // A real TTY only, whatever FORCE_COLOR says.
        let progress_on = !quiet && std::io::stderr().is_terminal();
        Reporter {
            verbose,
            quiet,
            mode,
            progress_on,
            start: Instant::now(),
            state: Mutex::new(Progress::default()),
        }
    }

    /// Print one line on stderr, keeping the progress line below it.
    fn line(&self, s: &str) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut err = std::io::stderr().lock();
        erase(&mut err, &mut st);
        let _ = writeln!(err, "{s}");
        self.draw(&mut err, &mut st);
    }

    pub fn debug(&self, msg: &str) {
        if self.verbose {
            self.line(&paint(
                &self.mode,
                Role::Dim,
                &format!("{} {msg}", self.mode.g("·", "-")),
            ));
        }
    }

    pub fn warn(&self, msg: &str) {
        if !self.quiet {
            self.line(&format!(
                "{} {} {msg}",
                status_symbol(&self.mode, Role::Mid),
                paint(&self.mode, Role::Title, "warning:")
            ));
        }
    }

    pub fn info(&self, msg: &str) {
        if !self.quiet {
            self.line(&format!(
                "{} {msg}",
                paint(&self.mode, Role::Low, self.mode.g("✓", "+"))
            ));
        }
    }

    pub fn error(&self, e: &AppError) {
        self.clear();
        let label = paint_bold(
            &self.mode,
            Role::High,
            &format!("{} error:", self.mode.g("✗", "x")),
        );
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "{label} {}", e.msg);
        if let Some(h) = &e.hint {
            let _ = writeln!(
                err,
                "  {}",
                paint(&self.mode, Role::Dim, &format!("hint: {h}"))
            );
        }
    }

    /// Update the progress line (`label  ■■■···  12/30 repos · 4.1s`).
    pub fn progress(&self, label: &str, done: usize, total: Option<usize>) {
        if !self.progress_on {
            return;
        }
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.label = label.to_string();
        st.done = done;
        st.total = total;
        st.note = None;
        let mut err = std::io::stderr().lock();
        self.draw(&mut err, &mut st);
    }

    /// Replace the meter with a short note (`label  counting PRs and issues · 31.0s`).
    pub fn progress_note(&self, label: &str, note: &str) {
        if !self.progress_on {
            return;
        }
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.label = label.to_string();
        st.note = Some(note.to_string());
        let mut err = std::io::stderr().lock();
        self.draw(&mut err, &mut st);
    }

    /// Redraw with the current elapsed time (called by a ticker thread).
    pub fn tick(&self) {
        if !self.progress_on {
            return;
        }
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !st.label.is_empty() {
            let mut err = std::io::stderr().lock();
            self.draw(&mut err, &mut st);
        }
    }

    /// Remove the progress line before the result prints.
    pub fn clear(&self) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut err = std::io::stderr().lock();
        erase(&mut err, &mut st);
        st.label.clear();
    }

    fn draw(&self, err: &mut impl Write, st: &mut Progress) {
        if !self.progress_on || st.label.is_empty() || self.start.elapsed() < PROGRESS_DELAY {
            return;
        }
        let m = &self.mode;
        let secs = format!("{:.1}s", self.start.elapsed().as_secs_f64());
        let mut l = Line::new();
        l.push(m, Role::Title, &truncate(&st.label, 30, m.unicode()))
            .pad(2);
        match (&st.note, st.total) {
            (Some(note), _) => {
                l.push(m, Role::Text, note);
            }
            (None, Some(t)) if t > 0 => {
                l.append(&meter(m, st.done as f64 * 100.0 / t as f64, 12))
                    .pad(2);
                l.push(m, Role::Text, &format!("{}/{t} repos", st.done));
            }
            (None, _) => {
                l.push(m, Role::Text, &format!("{} repos", st.done));
            }
        }
        l.push(m, Role::Dim, &format!(" {} {secs}", m.g("·", "-")));
        if l.width + 1 >= m.width {
            // Too narrow for the full line: fall back to the short form.
            l = Line::of(
                m,
                Role::Text,
                &truncate(
                    &format!("{} {}/{} {secs}", st.label, st.done, st.total.unwrap_or(0)),
                    m.width.saturating_sub(1),
                    m.unicode(),
                ),
            );
        }
        let pad = st.shown.saturating_sub(l.width);
        let _ = write!(err, "\r{}{}", l.text, " ".repeat(pad));
        let _ = err.flush();
        st.shown = l.width;
    }
}

fn erase(err: &mut impl Write, st: &mut Progress) {
    if st.shown > 0 {
        let _ = write!(err, "\r{}\r", " ".repeat(st.shown));
        let _ = err.flush();
        st.shown = 0;
    }
}
