//! Minimal ANSI color helper for task-output project-name prefixes.
//!
//! Colors are assigned **by a project's position in the current run**, not by
//! hashing its name — so within one `gitm x` / `gitm sync` every project gets a
//! distinct color until the palette is exhausted (far beyond any realistic
//! workspace size). Honors the de-facto `NO_COLOR` env var and the
//! `CLICOLOR_FORCE` override; otherwise colors only when stdout is a TTY.
//! The decision is cached once per process.

use std::io::IsTerminal;
use std::sync::OnceLock;

/// Curated 256-color foreground codes — bright enough to read on dark
/// backgrounds, distinct enough from each other to tell apart at a glance.
/// Order rotates through hue families so adjacent indices contrast strongly.
const PALETTE: &[&str] = &[
    "38;5;46",  // bright green
    "38;5;196", // red
    "38;5;51",  // cyan
    "38;5;208", // orange
    "38;5;129", // purple
    "38;5;226", // yellow
    "38;5;75",  // light blue
    "38;5;202", // orange-red
    "38;5;201", // magenta
    "38;5;154", // chartreuse
    "38;5;39",  // dodger blue
    "38;5;214", // light orange
    "38;5;171", // pink
    "38;5;118", // green
    "38;5;81",  // sky
    "38;5;220", // gold
    "38;5;135", // violet
    "38;5;48",  // spring green
    "38;5;177", // light pink
    "38;5;190", // yellow-green
    "38;5;33",  // blue
    "38;5;205", // hot pink
    "38;5;69",  // medium blue
    "38;5;141", // medium purple
];

fn enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| {
        // https://no-color.org/: presence (any value) disables color.
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        // CLICOLOR_FORCE=1 forces color on even when piped/redirected.
        if std::env::var_os("CLICOLOR_FORCE")
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            return true;
        }
        std::io::stdout().is_terminal()
    })
}

/// Colorize a project name for a `[name]` prefix, picking the color from the
/// run-wide position `index` so each project in a run is distinct. Plain when
/// color is disabled.
pub fn name_at(index: usize, n: &str) -> String {
    paint(enabled(), index, n)
}

/// Like `name_at` but left-pads the result to `width` visible columns, so the
/// color escapes don't break fixed-width table alignment. Plain (and padded)
/// when color is disabled.
pub fn name_at_padded(index: usize, n: &str, width: usize) -> String {
    paint_padded(enabled(), index, n, width)
}

/// Pure coloring core (testable without touching the process-global flag).
fn paint(color_on: bool, index: usize, n: &str) -> String {
    if !color_on {
        return n.to_string();
    }
    let code = PALETTE[index % PALETTE.len()];
    format!("\x1b[{code}m{n}\x1b[0m")
}

/// Pure padded-coloring core: pads to `width` *visible* columns regardless of
/// the (zero-width) ANSI escapes.
fn paint_padded(color_on: bool, index: usize, n: &str, width: usize) -> String {
    let visible = n.chars().count();
    let pad = " ".repeat(width.saturating_sub(visible));
    if !color_on {
        // Defer to Rust's formatter for the plain padded path.
        return format!("{n:width$}");
    }
    let code = PALETTE[index % PALETTE.len()];
    format!("\x1b[{code}m{n}\x1b[0m{pad}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_within_palette_size() {
        // The first PALETTE.len() indices must all map to different codes.
        let codes: Vec<&str> = (0..PALETTE.len())
            .map(|i| PALETTE[i % PALETTE.len()])
            .collect();
        let mut sorted = codes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "palette has duplicate codes");
    }

    #[test]
    fn paint_padded_preserves_visible_width_when_colored() {
        let width = 24usize;
        // Names shorter than the column pad out to exactly `width`.
        for n in ["a", "alpha", "medium-name"] {
            let s = paint_padded(true, 0, n, width);
            assert_eq!(strip_csi(&s).chars().count(), width, "name {n:?}");
        }
        // A name longer than the column overflows (no truncation) — the cell
        // is just as wide as the name, with zero pad.
        let long = "some-much-longer-project-name-than-usual";
        let s = paint_padded(true, 0, long, width);
        assert_eq!(strip_csi(&s).chars().count(), long.chars().count());
    }

    #[test]
    fn paint_padded_plain_when_disabled() {
        let s = paint_padded(false, 0, "alpha", 24);
        assert_eq!(s, format!("{:<24}", "alpha"));
    }

    #[test]
    fn paint_plain_when_disabled() {
        assert_eq!(paint(false, 0, "svc"), "svc");
        assert!(paint(true, 0, "svc").contains('\x1b'));
    }

    fn strip_csi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for cc in chars.by_ref() {
                    if cc.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }
}
