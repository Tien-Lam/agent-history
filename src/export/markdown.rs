use std::fmt::Write as _;

use crate::metadata::Note;
use crate::model::{ContentBlock, Message, Session};

use super::notes::NoteBuckets;

pub fn to_markdown(session: &Session, messages: &[Message]) -> String {
    to_markdown_with_notes(session, messages, &[])
}

pub fn to_markdown_with_notes(session: &Session, messages: &[Message], notes: &[Note]) -> String {
    let session_ref = session.session_ref().to_string();
    to_markdown_with_notes_for_session_ref(session, messages, notes, &session_ref)
}

pub(crate) fn to_markdown_with_notes_for_session_ref(
    session: &Session,
    messages: &[Message],
    notes: &[Note],
    session_ref: &str,
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

    let buckets = NoteBuckets::build_for_session_ref(session_ref, notes);
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
                render_fenced_code(out, language.as_deref(), code);
            }
            ContentBlock::ToolUse(tool) => {
                let _ = writeln!(out, "<details>\n<summary>Tool: {}</summary>\n", tool.name);
                render_fenced_code(out, Some("json"), &tool.arguments);
                out.push_str("</details>\n\n");
            }
            ContentBlock::ToolResult(result) => {
                let status = if result.success { "Success" } else { "Error" };
                let _ = writeln!(
                    out,
                    "<details>\n<summary>Tool Result ({status})</summary>\n"
                );
                render_fenced_code(out, None, &result.output);
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

fn render_fenced_code(out: &mut String, language: Option<&str>, code: &str) {
    let fence = markdown_fence_for(code);
    let info = language.map_or_else(String::new, sanitize_fence_info);
    if info.is_empty() {
        let _ = writeln!(out, "{fence}\n{code}\n{fence}\n");
    } else {
        let _ = writeln!(out, "{fence}{info}\n{code}\n{fence}\n");
    }
}

fn markdown_fence_for(code: &str) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in code.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

fn sanitize_fence_info(language: &str) -> String {
    language
        .chars()
        .filter(|ch| *ch != '`' && !ch.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}
