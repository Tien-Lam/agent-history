use std::path::Path;

use crate::model::{ContentBlock, Message, Provider, Role, Session, SessionId};

use super::blocks::split_sessions;
use super::messages::parse_messages;
use super::ProviderError;

pub(crate) fn parse_sessions_in_file(path: &Path) -> Result<Vec<Session>, ProviderError> {
    let content = std::fs::read_to_string(path).map_err(|e| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let project_dir = path.parent().map(Path::to_path_buf);
    let project_name = project_dir
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .map(str::to_string);

    // Stable per-file prefix so session IDs differ across projects even
    // when timestamps collide.
    let file_prefix = short_hash(path);

    let mut sessions = Vec::new();
    for block in split_sessions(&content) {
        let id = format!(
            "{file_prefix}:{}",
            block.started_at.format("%Y%m%dT%H%M%SZ")
        );
        let messages = parse_messages(&block, &id);
        if messages.is_empty() {
            // Empty section (no `####` and no body lines) — skip rather
            // than emit a phantom session.
            continue;
        }
        let ended_at = messages.last().map(|m| m.timestamp);
        sessions.push(Session {
            id: SessionId(id),
            provider: Provider::Aider,
            project_path: project_dir.clone(),
            project_name: project_name.clone(),
            git_branch: None,
            started_at: block.started_at,
            ended_at,
            summary: first_user_line(&messages),
            model: None,
            token_usage: None,
            message_count: messages.len(),
            source_path: path.to_path_buf(),
        });
    }
    Ok(sessions)
}

fn first_user_line(messages: &[Message]) -> Option<String> {
    for msg in messages {
        if msg.role != Role::User {
            continue;
        }
        for block in &msg.content {
            if let ContentBlock::Text(t) = block {
                let line = t.lines().next().unwrap_or("").trim();
                if !line.is_empty() {
                    return Some(truncate(line, 120));
                }
            }
        }
    }
    None
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// 8-hex-digit `FNV1a` of a path. Used as a session-ID prefix; not security-
/// sensitive — collisions are tolerated, the timestamp is the real key.
fn short_hash(path: &Path) -> String {
    let bytes = path.to_string_lossy();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:08x}", (h ^ (h >> 32)) & 0xffff_ffff)
}
