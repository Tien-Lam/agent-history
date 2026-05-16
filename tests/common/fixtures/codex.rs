use std::fs;

use tempfile::TempDir;

use super::core::FixtureDir;
use super::util::escape_json;

pub struct CodexFixtureBuilder {
    sessions: Vec<CodexSessionSpec>,
}

struct CodexSessionSpec {
    rollout_id: String,
    date: String, // YYYY/MM/DD
    entries: Vec<CodexEntrySpec>,
}

enum CodexEntrySpec {
    User { content: String, timestamp: String },
    Assistant { content: String, timestamp: String },
    ToolUse { content: String, timestamp: String },
    Error { error: String, timestamp: String },
    Raw(String),
}

impl CodexFixtureBuilder {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn add_session(mut self, id: &str) -> CodexSessionBuilder {
        let spec = CodexSessionSpec {
            rollout_id: id.to_string(),
            date: "2025/01/01".to_string(),
            entries: Vec::new(),
        };
        self.sessions.push(spec);
        let idx = self.sessions.len() - 1;
        CodexSessionBuilder {
            parent: self,
            idx,
            entry_counter: 0,
        }
    }

    pub fn build(self) -> FixtureDir {
        let dir = TempDir::new().unwrap();
        let base = dir.path().to_path_buf();

        for session in &self.sessions {
            let date_dir = base.join(&session.date);
            fs::create_dir_all(&date_dir).unwrap();

            let mut lines = Vec::new();
            for entry in &session.entries {
                lines.push(render_codex_entry(entry));
            }
            let filename = format!("rollout-{}.jsonl", session.rollout_id);
            fs::write(date_dir.join(filename), lines.join("\n") + "\n").unwrap();
        }

        FixtureDir {
            base_path: base,
            dir,
        }
    }
}

pub struct CodexSessionBuilder {
    parent: CodexFixtureBuilder,
    idx: usize,
    entry_counter: u32,
}

impl CodexSessionBuilder {
    fn session_mut(&mut self) -> &mut CodexSessionSpec {
        &mut self.parent.sessions[self.idx]
    }

    fn next_timestamp(&mut self) -> String {
        self.entry_counter += 1;
        let offset = self.entry_counter * 5;
        format!("2025-01-01T00:00:{:02}Z", offset.min(59))
    }

    pub fn date(mut self, date: &str) -> Self {
        self.session_mut().date = date.to_string();
        self
    }

    pub fn user(mut self, text: &str) -> Self {
        let timestamp = self.next_timestamp();
        self.session_mut().entries.push(CodexEntrySpec::User {
            content: text.to_string(),
            timestamp,
        });
        self
    }

    pub fn assistant(mut self, text: &str) -> Self {
        let timestamp = self.next_timestamp();
        self.session_mut().entries.push(CodexEntrySpec::Assistant {
            content: text.to_string(),
            timestamp,
        });
        self
    }

    pub fn error(mut self, error: &str) -> Self {
        let timestamp = self.next_timestamp();
        self.session_mut().entries.push(CodexEntrySpec::Error {
            error: error.to_string(),
            timestamp,
        });
        self
    }

    pub fn raw_line(mut self, line: &str) -> Self {
        self.session_mut()
            .entries
            .push(CodexEntrySpec::Raw(line.to_string()));
        self
    }

    pub fn done(self) -> CodexFixtureBuilder {
        self.parent
    }
}

fn render_codex_entry(entry: &CodexEntrySpec) -> String {
    match entry {
        CodexEntrySpec::User { content, timestamp } => {
            format!(
                r#"{{"type":"user","content":"{}","timestamp":"{timestamp}"}}"#,
                escape_json(content),
            )
        }
        CodexEntrySpec::Assistant { content, timestamp } => {
            format!(
                r#"{{"type":"assistant","content":"{}","timestamp":"{timestamp}"}}"#,
                escape_json(content),
            )
        }
        CodexEntrySpec::ToolUse { content, timestamp } => {
            format!(
                r#"{{"type":"tool_use","content":"{}","tool_calls":{{}},"timestamp":"{timestamp}"}}"#,
                escape_json(content),
            )
        }
        CodexEntrySpec::Error { error, timestamp } => {
            format!(
                r#"{{"type":"error","error":"{}","timestamp":"{timestamp}"}}"#,
                escape_json(error),
            )
        }
        CodexEntrySpec::Raw(line) => line.clone(),
    }
}

pub fn codex_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = CodexFixtureBuilder::new().add_session("codex-gen");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.assistant(&format!("Assistant response {i}"));
        }
    }
    builder.done().build()
}
