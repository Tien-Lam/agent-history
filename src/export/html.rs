use std::fmt::Write as _;

use crate::metadata::Note;
use crate::model::{ContentBlock, Message, Role, Session};

use super::notes::NoteBuckets;

pub fn to_html(session: &Session, messages: &[Message]) -> String {
    to_html_with_notes(session, messages, &[])
}

pub fn to_html_with_notes(session: &Session, messages: &[Message], notes: &[Note]) -> String {
    let title = html_escape(session.project_name.as_deref().unwrap_or("Conversation"));
    let provider = html_escape(session.provider.as_str());
    let date = session.started_at.format("%Y-%m-%d %H:%M UTC").to_string();

    let mut meta =
        format!("<strong>Provider:</strong> {provider}<br>\n  <strong>Date:</strong> {date}");
    if let Some(branch) = &session.git_branch {
        let _ = write!(
            meta,
            "<br>\n  <strong>Branch:</strong> {}",
            html_escape(branch)
        );
    }
    if let Some(model) = &session.model {
        let _ = write!(
            meta,
            "<br>\n  <strong>Model:</strong> {}",
            html_escape(model)
        );
    }

    let buckets = NoteBuckets::build(session, notes);
    let mut body = String::new();
    if !buckets.session_level.is_empty() {
        body.push_str("<section class=\"session-notes\">\n");
        body.push_str("<h2>📝 Private annotations</h2>\n");
        for n in &buckets.session_level {
            render_note_html(&mut body, n);
        }
        body.push_str("</section>\n");
    }
    for (idx, msg) in messages.iter().enumerate() {
        let role_class = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        };
        let _ = writeln!(
            body,
            "<div class=\"message {role_class}\">\n<div class=\"role\">{}</div>",
            html_escape(msg.role.as_str())
        );
        render_content_html(&mut body, &msg.content);
        let turn = u32::try_from(idx).unwrap_or(u32::MAX).saturating_add(1);
        if let Some(turn_notes) = buckets.by_turn.get(&turn) {
            for n in turn_notes {
                render_note_html(&mut body, n);
            }
        }
        body.push_str("</div>\n");
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — aghist export</title>
<style>
:root {{
  --bg:#fff; --text:#1a1a1a;
  --user-bg:#e3f2fd; --asst-bg:#f5f5f5; --sys-bg:#fff3e0; --tool-bg:#e8f5e9;
  --code-bg:#263238; --code-fg:#eeffff; --border:#e0e0e0; --meta:#666;
}}
@media(prefers-color-scheme:dark){{:root{{
  --bg:#1e1e1e; --text:#ddd;
  --user-bg:#1a3a5c; --asst-bg:#2d2d2d; --sys-bg:#3e2723; --tool-bg:#1b3320;
  --code-bg:#0d1117; --code-fg:#e6edf3; --border:#444; --meta:#aaa;
}}}}
*{{box-sizing:border-box}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;max-width:860px;margin:0 auto;padding:2rem 1rem;background:var(--bg);color:var(--text);line-height:1.6}}
h1{{border-bottom:2px solid var(--border);padding-bottom:.5rem}}
.meta{{color:var(--meta);font-size:.9rem;margin-bottom:2rem}}
.message{{margin:1.5rem 0;padding:1rem 1.25rem;border-radius:8px;border-left:4px solid transparent}}
.message.user{{background:var(--user-bg);border-left-color:#1976d2}}
.message.assistant{{background:var(--asst-bg);border-left-color:#616161}}
.message.system{{background:var(--sys-bg);border-left-color:#f57c00}}
.message.tool{{background:var(--tool-bg);border-left-color:#388e3c}}
.role{{font-weight:700;font-size:.85rem;text-transform:uppercase;letter-spacing:.05em;margin-bottom:.5rem}}
pre{{background:var(--code-bg);color:var(--code-fg);padding:1rem;border-radius:6px;overflow-x:auto;font-size:.875rem}}
code{{font-family:'Cascadia Code','Fira Code','SF Mono',monospace}}
details{{margin:.75rem 0;border:1px solid var(--border);border-radius:6px;padding:.5rem .75rem}}
summary{{cursor:pointer;font-weight:600}}
.error{{color:#d32f2f;padding:.5rem;border:1px solid #d32f2f;border-radius:4px}}
.thinking{{font-style:italic;color:var(--meta)}}
aside.note{{margin:.75rem 0;padding:.75rem 1rem;border-left:4px solid #ffb300;background:rgba(255,179,0,.08);border-radius:4px}}
aside.note .note-label{{font-size:.8rem;font-weight:700;text-transform:uppercase;letter-spacing:.05em;color:#b26500;margin-bottom:.25rem}}
aside.note .note-body{{white-space:pre-wrap}}
section.session-notes{{margin:1.5rem 0}}
section.session-notes h2{{font-size:1rem;margin:0 0 .5rem 0}}
</style>
</head>
<body>
<h1>{title}</h1>
<div class="meta">
  {meta}
</div>
{body}
</body>
</html>"#
    )
}

fn render_content_html(out: &mut String, blocks: &[ContentBlock]) {
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

fn render_note_html(out: &mut String, note: &Note) {
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

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
