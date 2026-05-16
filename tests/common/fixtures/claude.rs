use std::fs;

use tempfile::TempDir;

use super::core::FixtureDir;
use super::util::{escape_json, iso_to_millis};

pub struct ClaudeFixtureBuilder {
    sessions: Vec<ClaudeSessionSpec>,
}

struct ClaudeSessionSpec {
    session_id: String,
    project_name: String,
    display_text: String,
    git_branch: Option<String>,
    cwd: Option<String>,
    start_time: String,
    messages: Vec<ClaudeMessageSpec>,
}

enum ClaudeMessageSpec {
    User {
        uuid: String,
        timestamp: String,
        content: String,
    },
    AssistantText {
        uuid: String,
        timestamp: String,
        text: String,
        model: String,
        input_tokens: u64,
        output_tokens: u64,
    },
    AssistantToolUse {
        uuid: String,
        timestamp: String,
        text: String,
        tool_name: String,
        tool_id: String,
        tool_input: String,
        model: String,
    },
    AssistantThinking {
        uuid: String,
        timestamp: String,
        thinking: String,
        text: String,
        model: String,
    },
    ToolResult {
        uuid: String,
        timestamp: String,
        tool_use_id: String,
        content: String,
    },
    Raw(String),
}

impl ClaudeFixtureBuilder {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn add_session(mut self, id: &str) -> ClaudeSessionBuilder {
        let spec = ClaudeSessionSpec {
            session_id: id.to_string(),
            project_name: "test-project".to_string(),
            display_text: format!("Session {id}"),
            git_branch: None,
            cwd: None,
            start_time: "2025-01-01T00:00:00Z".to_string(),
            messages: Vec::new(),
        };
        self.sessions.push(spec);
        let idx = self.sessions.len() - 1;
        ClaudeSessionBuilder {
            parent: self,
            idx,
            msg_counter: 0,
        }
    }

    pub fn build(self) -> FixtureDir {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join(".claude");
        fs::create_dir_all(&base).unwrap();

        let mut history_lines = Vec::new();
        for session in &self.sessions {
            let ts_millis = iso_to_millis(&session.start_time);
            history_lines.push(format!(
                r#"{{"display":"{}","timestamp":{},"project":"{}","sessionId":"{}"}}"#,
                escape_json(&session.display_text),
                ts_millis,
                escape_json(&session.project_name),
                escape_json(&session.session_id),
            ));

            let project_dir = base.join("projects").join(&session.project_name);
            fs::create_dir_all(&project_dir).unwrap();

            let mut session_lines = Vec::new();
            for msg in &session.messages {
                session_lines.push(render_claude_message(msg, session));
            }
            let session_file = project_dir.join(format!("{}.jsonl", session.session_id));
            fs::write(&session_file, session_lines.join("\n") + "\n").unwrap();
        }

        fs::write(base.join("history.jsonl"), history_lines.join("\n") + "\n").unwrap();

        FixtureDir {
            base_path: base,
            dir,
        }
    }
}

pub struct ClaudeSessionBuilder {
    parent: ClaudeFixtureBuilder,
    idx: usize,
    msg_counter: u32,
}

impl ClaudeSessionBuilder {
    fn session_mut(&mut self) -> &mut ClaudeSessionSpec {
        &mut self.parent.sessions[self.idx]
    }

    fn next_uuid(&mut self) -> String {
        self.msg_counter += 1;
        format!("msg-{:03}", self.msg_counter)
    }

    fn next_timestamp(&self) -> String {
        let offset = self.msg_counter * 5;
        format!("2025-01-01T00:00:{:02}Z", offset.min(59))
    }

    pub fn project(mut self, name: &str) -> Self {
        self.session_mut().project_name = name.to_string();
        self
    }

    pub fn display(mut self, text: &str) -> Self {
        self.session_mut().display_text = text.to_string();
        self
    }

    pub fn git_branch(mut self, branch: &str) -> Self {
        self.session_mut().git_branch = Some(branch.to_string());
        self
    }

    pub fn cwd(mut self, cwd: &str) -> Self {
        self.session_mut().cwd = Some(cwd.to_string());
        self
    }

    pub fn start_time(mut self, ts: &str) -> Self {
        self.session_mut().start_time = ts.to_string();
        self
    }

    pub fn user(mut self, text: &str) -> Self {
        let uuid = self.next_uuid();
        let timestamp = self.next_timestamp();
        self.session_mut().messages.push(ClaudeMessageSpec::User {
            uuid,
            timestamp,
            content: text.to_string(),
        });
        self
    }

    pub fn assistant(mut self, text: &str) -> Self {
        let uuid = self.next_uuid();
        let timestamp = self.next_timestamp();
        self.session_mut()
            .messages
            .push(ClaudeMessageSpec::AssistantText {
                uuid,
                timestamp,
                text: text.to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                input_tokens: 100,
                output_tokens: 50,
            });
        self
    }

    pub fn assistant_with_tool(mut self, text: &str, tool: &str, tool_input: &str) -> Self {
        let uuid = self.next_uuid();
        let timestamp = self.next_timestamp();
        let tool_id = format!("tool-{:03}", self.msg_counter);
        self.session_mut()
            .messages
            .push(ClaudeMessageSpec::AssistantToolUse {
                uuid,
                timestamp,
                text: text.to_string(),
                tool_name: tool.to_string(),
                tool_id,
                tool_input: tool_input.to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
            });
        self
    }

    pub fn thinking(mut self, thinking: &str, text: &str) -> Self {
        let uuid = self.next_uuid();
        let timestamp = self.next_timestamp();
        self.session_mut()
            .messages
            .push(ClaudeMessageSpec::AssistantThinking {
                uuid,
                timestamp,
                thinking: thinking.to_string(),
                text: text.to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
            });
        self
    }

    pub fn tool_result(mut self, tool_use_id: &str, content: &str) -> Self {
        let uuid = self.next_uuid();
        let timestamp = self.next_timestamp();
        self.session_mut()
            .messages
            .push(ClaudeMessageSpec::ToolResult {
                uuid,
                timestamp,
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
            });
        self
    }

    pub fn raw_line(mut self, line: &str) -> Self {
        self.session_mut()
            .messages
            .push(ClaudeMessageSpec::Raw(line.to_string()));
        self
    }

    pub fn done(self) -> ClaudeFixtureBuilder {
        self.parent
    }
}

fn render_claude_message(msg: &ClaudeMessageSpec, session: &ClaudeSessionSpec) -> String {
    let branch = session.git_branch.as_deref().map_or(String::new(), |b| {
        format!(r#","gitBranch":"{}""#, escape_json(b))
    });
    let cwd = session
        .cwd
        .as_deref()
        .map_or(String::new(), |c| format!(r#","cwd":"{}""#, escape_json(c)));

    match msg {
        ClaudeMessageSpec::User {
            uuid,
            timestamp,
            content,
        } => {
            format!(
                r#"{{"type":"user","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"user","content":"{}"}}{branch}{cwd}}}"#,
                escape_json(content),
            )
        }
        ClaudeMessageSpec::AssistantText {
            uuid,
            timestamp,
            text,
            model,
            input_tokens,
            output_tokens,
        } => {
            format!(
                r#"{{"type":"assistant","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","content":[{{"type":"text","text":"{}"}}],"model":"{model}","usage":{{"input_tokens":{input_tokens},"output_tokens":{output_tokens}}}}}}}"#,
                escape_json(text),
            )
        }
        ClaudeMessageSpec::AssistantToolUse {
            uuid,
            timestamp,
            text,
            tool_name,
            tool_id,
            tool_input,
            model,
        } => {
            format!(
                r#"{{"type":"assistant","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","content":[{{"type":"text","text":"{}"}},{{"type":"tool_use","id":"{tool_id}","name":"{tool_name}","input":{tool_input}}}],"model":"{model}","usage":{{"input_tokens":100,"output_tokens":50}}}}}}"#,
                escape_json(text),
            )
        }
        ClaudeMessageSpec::AssistantThinking {
            uuid,
            timestamp,
            thinking,
            text,
            model,
        } => {
            format!(
                r#"{{"type":"assistant","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","content":[{{"type":"thinking","thinking":"{}"}},{{"type":"text","text":"{}"}}],"model":"{model}","usage":{{"input_tokens":100,"output_tokens":50}}}}}}"#,
                escape_json(thinking),
                escape_json(text),
            )
        }
        ClaudeMessageSpec::ToolResult {
            uuid,
            timestamp,
            tool_use_id,
            content,
        } => {
            format!(
                r#"{{"type":"user","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{tool_use_id}","content":"{}"}}]}}}}"#,
                escape_json(content),
            )
        }
        ClaudeMessageSpec::Raw(line) => line.clone(),
    }
}

pub fn claude_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = ClaudeFixtureBuilder::new()
        .add_session("session-gen")
        .project("gen-project")
        .display("Generated session");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.assistant(&format!("Assistant response {i}"));
        }
    }
    builder.done().build()
}

pub fn claude_multi_session(n_sessions: usize, msgs_per_session: usize) -> FixtureDir {
    let mut builder = ClaudeFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = builder
            .add_session(&format!("session-multi-{s:03}"))
            .project(&format!("project-{s}"))
            .display(&format!("Multi session {s}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m} in session {s}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m} in session {s}"));
            }
        }
        builder = sb.done();
    }
    builder.build()
}
