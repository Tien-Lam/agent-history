//! Syntax highlighting for fenced code blocks in the message view.
//!
//! Wraps `syntect` with a tiny façade that returns ratatui `Span`s already
//! coloured for the current theme. The `SyntaxSet` and `Theme` are loaded
//! once and reused, so per-line cost is just the regex walk.

use std::sync::OnceLock;

use ratatui::style::{Color, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SynStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        let ts = ThemeSet::load_defaults();
        // Pick a dark theme that reads well over the app's dark background.
        // Fall back through a couple of common names so we tolerate any
        // future syntect theme set shuffles, then to whatever's first.
        for name in [
            "base16-eighties.dark",
            "base16-mocha.dark",
            "Solarized (dark)",
        ] {
            if let Some(t) = ts.themes.get(name) {
                return t.clone();
            }
        }
        ts.themes
            .values()
            .next()
            .cloned()
            .expect("syntect ships at least one default theme")
    })
}

/// Highlight a single line of source code as ratatui spans.
///
/// `language` is the fence info string (e.g. `rust`, `py`, `typescript`).
/// Looked up by token, then by extension; falls back to plain text when no
/// syntax matches. Trailing newlines on the input are stripped so callers
/// can wrap the result in a `Line` without doubled breaks.
pub fn highlight_line(language: Option<&str>, line: &str) -> Vec<Span<'static>> {
    let trimmed = line.strip_suffix('\n').unwrap_or(line);
    let ss = syntax_set();
    let syntax = language
        .and_then(|lang| {
            ss.find_syntax_by_token(lang)
                .or_else(|| ss.find_syntax_by_extension(lang))
        })
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let mut h = HighlightLines::new(syntax, theme());
    let Ok(ranges) = h.highlight_line(trimmed, ss) else {
        // Highlighter blew up on this line — degrade to a single plain span.
        return vec![Span::raw(trimmed.to_string())];
    };

    ranges
        .into_iter()
        .map(|(style, text)| Span::styled(text.to_string(), to_ratatui_style(style)))
        .collect()
}

fn to_ratatui_style(style: SynStyle) -> Style {
    let fg = style.foreground;
    Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b))
}
