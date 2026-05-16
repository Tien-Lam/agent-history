use std::fs;

use tempfile::TempDir;

use super::core::FixtureDir;
use super::util::escape_json;

pub struct CopilotFixtureBuilder {
    sessions: Vec<CopilotSessionSpec>,
}

struct CopilotSessionSpec {
    session_id: String,
    cwd: String,
    created_at: String,
    updated_at: String,
    events: Vec<CopilotEventSpec>,
}

enum CopilotEventSpec {
    UserMessage {
        id: String,
        timestamp: String,
        content: String,
    },
    AssistantMessage {
        id: String,
        timestamp: String,
        content: String,
        model: String,
    },
    ToolInvoke {
        id: String,
        timestamp: String,
        tool_name: String,
        tool_call_id: String,
    },
    Raw(String),
}

impl CopilotFixtureBuilder {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn add_session(mut self, id: &str) -> CopilotSessionBuilder {
        let spec = CopilotSessionSpec {
            session_id: id.to_string(),
            cwd: "/home/user/project".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:05:00Z".to_string(),
            events: Vec::new(),
        };
        self.sessions.push(spec);
        let idx = self.sessions.len() - 1;
        CopilotSessionBuilder {
            parent: self,
            idx,
            evt_counter: 0,
        }
    }

    pub fn build(self) -> FixtureDir {
        let dir = TempDir::new().unwrap();
        let base = dir.path().to_path_buf();

        for session in &self.sessions {
            let session_dir = base.join(&session.session_id);
            fs::create_dir_all(&session_dir).unwrap();

            let yaml = format!(
                "id: \"{}\"\ncwd: \"{}\"\ncreated_at: \"{}\"\nupdated_at: \"{}\"",
                session.session_id, session.cwd, session.created_at, session.updated_at,
            );
            fs::write(session_dir.join("workspace.yaml"), yaml).unwrap();

            let mut event_lines = Vec::new();
            for evt in &session.events {
                event_lines.push(render_copilot_event(evt));
            }
            if !event_lines.is_empty() {
                fs::write(
                    session_dir.join("events.jsonl"),
                    event_lines.join("\n") + "\n",
                )
                .unwrap();
            }
        }

        FixtureDir {
            base_path: base,
            dir,
        }
    }
}

pub struct CopilotSessionBuilder {
    parent: CopilotFixtureBuilder,
    idx: usize,
    evt_counter: u32,
}

impl CopilotSessionBuilder {
    fn session_mut(&mut self) -> &mut CopilotSessionSpec {
        &mut self.parent.sessions[self.idx]
    }

    fn next_id(&mut self) -> String {
        self.evt_counter += 1;
        format!("evt-{:03}", self.evt_counter)
    }

    fn next_timestamp(&self) -> String {
        let offset = self.evt_counter * 3;
        format!("2025-01-01T00:00:{:02}Z", offset.min(59))
    }

    pub fn cwd(mut self, cwd: &str) -> Self {
        self.session_mut().cwd = cwd.to_string();
        self
    }

    pub fn user(mut self, text: &str) -> Self {
        let id = self.next_id();
        let timestamp = self.next_timestamp();
        self.session_mut()
            .events
            .push(CopilotEventSpec::UserMessage {
                id,
                timestamp,
                content: text.to_string(),
            });
        self
    }

    pub fn assistant(mut self, text: &str) -> Self {
        let id = self.next_id();
        let timestamp = self.next_timestamp();
        self.session_mut()
            .events
            .push(CopilotEventSpec::AssistantMessage {
                id,
                timestamp,
                content: text.to_string(),
                model: "gpt-4o".to_string(),
            });
        self
    }

    pub fn raw_line(mut self, line: &str) -> Self {
        self.session_mut()
            .events
            .push(CopilotEventSpec::Raw(line.to_string()));
        self
    }

    pub fn done(self) -> CopilotFixtureBuilder {
        self.parent
    }
}

fn render_copilot_event(evt: &CopilotEventSpec) -> String {
    match evt {
        CopilotEventSpec::UserMessage {
            id,
            timestamp,
            content,
        } => {
            format!(
                r#"{{"id":"{id}","type":"user.message","timestamp":"{timestamp}","content":"{}"}}"#,
                escape_json(content),
            )
        }
        CopilotEventSpec::AssistantMessage {
            id,
            timestamp,
            content,
            model,
        } => {
            format!(
                r#"{{"id":"{id}","type":"assistant.message","timestamp":"{timestamp}","content":"{}","model":"{model}","usage":{{"inputTokens":50,"outputTokens":80}}}}"#,
                escape_json(content),
            )
        }
        CopilotEventSpec::ToolInvoke {
            id,
            timestamp,
            tool_name,
            tool_call_id,
        } => {
            format!(
                r#"{{"id":"{id}","type":"tool.invoke","timestamp":"{timestamp}","toolName":"{tool_name}","toolCallId":"{tool_call_id}","content":""}}"#,
            )
        }
        CopilotEventSpec::Raw(line) => line.clone(),
    }
}

pub fn copilot_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = CopilotFixtureBuilder::new().add_session("copilot-gen-001");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.assistant(&format!("Assistant response {i}"));
        }
    }
    builder.done().build()
}
