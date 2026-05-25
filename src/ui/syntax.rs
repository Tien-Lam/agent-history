//! Lightweight syntax highlighting for fenced code blocks in the message view.

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use super::palette;

fn plain_span(line: &str) -> Vec<Span<'static>> {
    vec![Span::raw(line.to_string())]
}

fn split_at_checked(value: &str, index: usize) -> (&str, &str) {
    match (value.get(..index), value.get(index..)) {
        (Some(left), Some(right)) => (left, right),
        _ => (value, ""),
    }
}

fn take_while_len(value: &str, predicate: fn(char) -> bool) -> usize {
    let mut len = 0;
    for (idx, ch) in value.char_indices() {
        if !predicate(ch) {
            break;
        }
        len = idx + ch.len_utf8();
    }
    len
}

fn quoted_len(value: &str, quote: char) -> usize {
    let mut escaped = false;
    let mut len = 0;
    for (idx, ch) in value.char_indices() {
        len = idx + ch.len_utf8();
        if idx == 0 {
            continue;
        }
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            break;
        }
    }
    len
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn is_number_continue(ch: char) -> bool {
    ch == '_' || ch == '.' || ch.is_ascii_hexdigit()
}

fn normalized_language(language: Option<&str>) -> String {
    language.unwrap_or("").trim().to_ascii_lowercase()
}

fn comment_markers(language: &str) -> &'static [&'static str] {
    match language {
        "bash" | "conf" | "fish" | "ini" | "py" | "python" | "sh" | "shell" | "toml" | "yaml"
        | "yml" => &["#"],
        "html" | "md" | "markdown" | "xml" => &["<!--"],
        _ => &["//"],
    }
}

fn is_keyword(language: &str, token: &str) -> bool {
    match language {
        "rs" | "rust" => matches!(
            token,
            "as" | "async"
                | "await"
                | "const"
                | "crate"
                | "else"
                | "enum"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "let"
                | "match"
                | "mod"
                | "mut"
                | "pub"
                | "return"
                | "self"
                | "static"
                | "struct"
                | "true"
                | "type"
                | "use"
                | "where"
                | "while"
        ),
        "js" | "jsx" | "ts" | "tsx" | "javascript" | "typescript" => matches!(
            token,
            "async"
                | "await"
                | "class"
                | "const"
                | "else"
                | "export"
                | "false"
                | "function"
                | "if"
                | "import"
                | "let"
                | "new"
                | "null"
                | "return"
                | "true"
                | "type"
                | "undefined"
        ),
        "py" | "python" => matches!(
            token,
            "and"
                | "as"
                | "class"
                | "def"
                | "elif"
                | "else"
                | "False"
                | "for"
                | "from"
                | "if"
                | "import"
                | "in"
                | "None"
                | "or"
                | "return"
                | "True"
                | "while"
        ),
        "bash" | "fish" | "sh" | "shell" => matches!(
            token,
            "case"
                | "do"
                | "done"
                | "elif"
                | "else"
                | "esac"
                | "fi"
                | "for"
                | "function"
                | "if"
                | "in"
                | "then"
                | "while"
        ),
        _ => matches!(token, "false" | "False" | "null" | "None" | "true" | "True"),
    }
}

fn push_token(spans: &mut Vec<Span<'static>>, token: &str, style: Style) {
    spans.push(Span::styled(token.to_string(), style));
}

fn highlight_known_line(language: &str, line: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let markers = comment_markers(language);
    let keyword_style = Style::default()
        .fg(palette::MAUVE)
        .add_modifier(Modifier::BOLD);
    let string_style = Style::default().fg(palette::GREEN);
    let number_style = Style::default().fg(palette::PEACH);
    let comment_style = Style::default().fg(palette::TEXT_DIM);
    let plain_style = Style::default().fg(palette::TEXT);

    let mut rest = line;
    while !rest.is_empty() {
        if markers.iter().any(|marker| rest.starts_with(*marker)) {
            push_token(&mut spans, rest, comment_style);
            break;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        if ch == '"' || ch == '`' || (ch == '\'' && !matches!(language, "rs" | "rust")) {
            let len = quoted_len(rest, ch);
            let (token, next) = split_at_checked(rest, len);
            push_token(&mut spans, token, string_style);
            rest = next;
        } else if is_ident_start(ch) {
            let len = take_while_len(rest, is_ident_continue);
            let (token, next) = split_at_checked(rest, len);
            if is_keyword(language, token) {
                push_token(&mut spans, token, keyword_style);
            } else {
                push_token(&mut spans, token, plain_style);
            }
            rest = next;
        } else if ch.is_ascii_digit() {
            let len = take_while_len(rest, is_number_continue);
            let (token, next) = split_at_checked(rest, len);
            push_token(&mut spans, token, number_style);
            rest = next;
        } else {
            let len = ch.len_utf8();
            let (token, next) = split_at_checked(rest, len);
            push_token(&mut spans, token, plain_style);
            rest = next;
        }
    }

    if spans.is_empty() {
        plain_span(line)
    } else {
        spans
    }
}

/// Highlight a single line of source code as ratatui spans.
///
/// `language` is the fence info string (for example `rust`, `py`, or
/// `typescript`). Unknown languages still get conservative treatment for
/// strings, comments, numbers, and common literals. Trailing newlines on the
/// input are stripped so callers can wrap the result in a `Line` without
/// doubled breaks.
pub fn highlight_line(language: Option<&str>, line: &str) -> Vec<Span<'static>> {
    let trimmed = line.strip_suffix('\n').unwrap_or(line);
    highlight_known_line(&normalized_language(language), trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(spans: &[Span<'static>]) -> String {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn strips_trailing_newline() {
        let spans = highlight_line(None, "hello\n");

        assert_eq!(rendered(&spans), "hello");
    }

    #[test]
    fn rust_keywords_are_styled_without_changing_text() {
        let spans = highlight_line(Some("rust"), "let count = 42;");

        assert_eq!(rendered(&spans), "let count = 42;");
        assert_eq!(spans[0].content.as_ref(), "let");
        assert_eq!(spans[0].style.fg, Some(palette::MAUVE));
    }

    #[test]
    fn comment_markers_inside_strings_do_not_start_comments() {
        let spans = highlight_line(Some("rust"), r#"let url = "https://example"; // ok"#);

        assert_eq!(rendered(&spans), r#"let url = "https://example"; // ok"#);
        let comment = spans
            .iter()
            .find(|span| span.content.as_ref() == "// ok")
            .expect("comment span");
        assert_eq!(comment.style.fg, Some(palette::TEXT_DIM));
    }
}
