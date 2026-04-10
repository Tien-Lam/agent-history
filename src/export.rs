use std::fmt::Write as _;

use serde::Serialize;

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
    match format {
        ExportFormat::Markdown => to_markdown(session, messages),
        ExportFormat::Json => to_json(session, messages),
        ExportFormat::Html => to_html(session, messages),
    }
}

pub fn to_markdown(session: &Session, messages: &[Message]) -> String {
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

    for msg in messages {
        let _ = writeln!(out, "## {}\n", msg.role);
        render_content_md(&mut out, &msg.content);
    }

    out
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
    #[derive(Serialize)]
    struct ExportData<'a> {
        session: &'a Session,
        messages: &'a [Message],
    }

    serde_json::to_string_pretty(&ExportData { session, messages })
        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

pub fn to_html(session: &Session, messages: &[Message]) -> String {
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

    let mut body = String::new();
    for msg in messages {
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
                    .map_or(String::new(), |l| format!(" class=\"language-{l}\""));
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

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
