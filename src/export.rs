use std::collections::HashMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::metadata::Note;
use crate::model::{ContentBlock, Message, Role, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
    Html,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Html => "html",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Markdown, Self::Json, Self::Html]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Json => "JSON",
            Self::Html => "HTML",
        }
    }
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl std::str::FromStr for ExportFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            "html" => Ok(Self::Html),
            _ => Err(format!("unknown format '{s}' (expected: md, json, html)")),
        }
    }
}

pub fn export(format: ExportFormat, session: &Session, messages: &[Message]) -> String {
    export_with_notes(format, session, messages, &[])
}

/// Same as [`export`], but with `notes` injected inline at their citation refs.
///
/// Notes are partitioned by `session_ref`: those without a `#turn` suffix are
/// session-level and render once near the metadata header; those with a turn
/// render right after the corresponding message (1-based turn = message index
/// + 1). Notes whose ref doesn't match this session are silently ignored.
pub fn export_with_notes(
    format: ExportFormat,
    session: &Session,
    messages: &[Message],
    notes: &[Note],
) -> String {
    match format {
        ExportFormat::Markdown => to_markdown_with_notes(session, messages, notes),
        ExportFormat::Json => to_json_with_notes(session, messages, notes),
        ExportFormat::Html => to_html_with_notes(session, messages, notes),
    }
}

/// Bucket of notes for a single session, partitioned by their citation ref.
struct NoteBuckets<'a> {
    session_level: Vec<&'a Note>,
    by_turn: HashMap<u32, Vec<&'a Note>>,
}

impl<'a> NoteBuckets<'a> {
    fn build(session: &Session, notes: &'a [Note]) -> Self {
        let session_ref = format!("{}/{}", session.provider.slug(), session.id.0);
        let turn_prefix = format!("{session_ref}#");
        let mut session_level = Vec::new();
        let mut by_turn: HashMap<u32, Vec<&Note>> = HashMap::new();
        for n in notes {
            if n.session_ref == session_ref {
                session_level.push(n);
            } else if let Some(rest) = n.session_ref.strip_prefix(&turn_prefix) {
                if let Ok(turn) = rest.parse::<u32>() {
                    if turn > 0 {
                        by_turn.entry(turn).or_default().push(n);
                    }
                }
            }
        }
        Self { session_level, by_turn }
    }
}

pub fn to_markdown(session: &Session, messages: &[Message]) -> String {
    to_markdown_with_notes(session, messages, &[])
}

pub fn to_markdown_with_notes(
    session: &Session,
    messages: &[Message],
    notes: &[Note],
) -> String {
    let mut out = String::new();

    let title = session.project_name.as_deref().unwrap_or("Conversation");
    let _ = writeln!(out, "# {title}\n");
    let _ = writeln!(out, "- **Provider**: {}", session.provider);
    let _ = writeln!(
        out,
        "- **Date**: {}",
        session.started_at.format("%Y-%m-%d %H:%M UTC")
    );
    if let Some(branch) = &session.git_branch {
        let _ = writeln!(out, "- **Branch**: {branch}");
    }
    if let Some(model) = &session.model {
        let _ = writeln!(out, "- **Model**: {model}");
    }
    out.push_str("\n---\n\n");

    let buckets = NoteBuckets::build(session, notes);
    if !buckets.session_level.is_empty() {
        out.push_str("## 📝 Private annotations\n\n");
        for n in &buckets.session_level {
            render_note_md(&mut out, n);
        }
    }

    for (idx, msg) in messages.iter().enumerate() {
        let _ = writeln!(out, "## {}\n", msg.role);
        render_content_md(&mut out, &msg.content);
        let turn = u32::try_from(idx).unwrap_or(u32::MAX).saturating_add(1);
        if let Some(turn_notes) = buckets.by_turn.get(&turn) {
            for n in turn_notes {
                render_note_md(&mut out, n);
            }
        }
    }

    out
}

fn render_note_md(out: &mut String, note: &Note) {
    let _ = writeln!(
        out,
        "> **📝 Private annotation** — {} (id {})\n>",
        note.created_at, note.id
    );
    for line in note.body.lines() {
        let _ = writeln!(out, "> {line}");
    }
    out.push('\n');
}

fn render_content_md(out: &mut String, blocks: &[ContentBlock]) {
    for block in blocks {
        match block {
            ContentBlock::Text(text) => {
                out.push_str(text);
                out.push_str("\n\n");
            }
            ContentBlock::CodeBlock { language, code } => {
                let lang = language.as_deref().unwrap_or("");
                let _ = writeln!(out, "```{lang}\n{code}\n```\n");
            }
            ContentBlock::ToolUse(tool) => {
                let _ = writeln!(out, "<details>\n<summary>Tool: {}</summary>\n", tool.name);
                let _ = writeln!(out, "```json\n{}\n```\n", tool.arguments);
                out.push_str("</details>\n\n");
            }
            ContentBlock::ToolResult(result) => {
                let status = if result.success { "Success" } else { "Error" };
                let _ = writeln!(out, "<details>\n<summary>Tool Result ({status})</summary>\n");
                let _ = writeln!(out, "```\n{}\n```\n", result.output);
                out.push_str("</details>\n\n");
            }
            ContentBlock::Thinking(text) => {
                out.push_str("<details>\n<summary>Thinking</summary>\n\n");
                out.push_str(text);
                out.push_str("\n\n</details>\n\n");
            }
            ContentBlock::Error(text) => {
                let _ = writeln!(out, "> **Error**: {text}\n");
            }
        }
    }
}

pub fn to_json(session: &Session, messages: &[Message]) -> String {
    to_json_with_notes(session, messages, &[])
}

pub fn to_json_with_notes(session: &Session, messages: &[Message], notes: &[Note]) -> String {
    #[derive(Serialize)]
    struct ExportData<'a> {
        session: &'a Session,
        messages: &'a [Message],
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<Vec<NoteWire<'a>>>,
    }

    /// JSON projection of [`Note`] with a `kind: "private-annotation"` tag
    /// so consumers don't conflate annotations with session content.
    #[derive(Serialize)]
    struct NoteWire<'a> {
        kind: &'static str,
        id: i64,
        session_ref: &'a str,
        body: &'a str,
        created_at: &'a str,
        updated_at: &'a str,
    }

    // Only emit notes that belong to this session (session-level or turn-level).
    let buckets = NoteBuckets::build(session, notes);
    let mut matched: Vec<&Note> = buckets.session_level.clone();
    for v in buckets.by_turn.values() {
        matched.extend(v.iter().copied());
    }
    matched.sort_by_key(|n| n.id);
    let wire_notes: Vec<NoteWire<'_>> = matched
        .into_iter()
        .map(|n| NoteWire {
            kind: "private-annotation",
            id: n.id,
            session_ref: &n.session_ref,
            body: &n.body,
            created_at: &n.created_at,
            updated_at: &n.updated_at,
        })
        .collect();
    let notes_field = if wire_notes.is_empty() {
        None
    } else {
        Some(wire_notes)
    };

    serde_json::to_string_pretty(&ExportData {
        session,
        messages,
        notes: notes_field,
    })
    .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

pub fn to_html(session: &Session, messages: &[Message]) -> String {
    to_html_with_notes(session, messages, &[])
}

pub fn to_html_with_notes(session: &Session, messages: &[Message], notes: &[Note]) -> String {
    let title = html_escape(session.project_name.as_deref().unwrap_or("Conversation"));
    let provider = html_escape(session.provider.as_str());
    let date = session.started_at.format("%Y-%m-%d %H:%M UTC").to_string();

    let mut meta = format!(
        "<strong>Provider:</strong> {provider}<br>\n  <strong>Date:</strong> {date}"
    );
    if let Some(branch) = &session.git_branch {
        let _ = write!(meta, "<br>\n  <strong>Branch:</strong> {}", html_escape(branch));
    }
    if let Some(model) = &session.model {
        let _ = write!(meta, "<br>\n  <strong>Model:</strong> {}", html_escape(model));
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
                let lang_attr = language
                    .as_deref()
                    .map_or(String::new(), |l| format!(" class=\"language-{}\"", html_escape(l)));
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
