use std::fmt::Write as _;

use crate::metadata::Note;
use crate::model::ContentBlock;

pub(super) fn render_content_html(out: &mut String, blocks: &[ContentBlock]) {
    for block in blocks {
        match block {
            ContentBlock::Text(text) => {
                let _ = writeln!(out, "<p>{}</p>", html_escape(text));
            }
            ContentBlock::CodeBlock { language, code } => {
                let lang_attr = language.as_deref().map_or(String::new(), |l| {
                    format!(" class=\"language-{}\"", html_escape(l))
                });
                let _ = writeln!(
                    out,
                    "<pre><code{lang_attr}>{}</code></pre>",
                    html_escape(code)
                );
            }
            ContentBlock::ToolUse(tool) => {
                let _ = writeln!(
                    out,
                    "<details>\n<summary>Tool: {}</summary>\n<pre><code>{}</code></pre>\n</details>",
                    html_escape(&tool.name),
                    html_escape(&tool.arguments)
                );
            }
            ContentBlock::ToolResult(result) => {
                let label = if result.success {
                    "Tool Result"
                } else {
                    "Tool Error"
                };
                let _ = writeln!(
                    out,
                    "<details>\n<summary>{label}</summary>\n<pre><code>{}</code></pre>\n</details>",
                    html_escape(&result.output)
                );
            }
            ContentBlock::Thinking(text) => {
                let _ = writeln!(
                    out,
                    "<details>\n<summary>Thinking</summary>\n<p class=\"thinking\">{}</p>\n</details>",
                    html_escape(text)
                );
            }
            ContentBlock::Error(text) => {
                let _ = writeln!(out, "<p class=\"error\">{}</p>", html_escape(text));
            }
        }
    }
}

pub(super) fn render_note_html(out: &mut String, note: &Note) {
    let _ = writeln!(
        out,
        "<aside class=\"note\" data-kind=\"private-annotation\">\n  \
         <div class=\"note-label\">📝 Private annotation — {} (id {})</div>\n  \
         <div class=\"note-body\">{}</div>\n\
         </aside>",
        html_escape(&note.created_at),
        note.id,
        html_escape(&note.body),
    );
}

pub(super) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
