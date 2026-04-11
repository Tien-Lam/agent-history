use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

/// Holds a temp directory and the base path to pass to a provider constructor.
/// The `TempDir` must be kept alive for the duration of the test.
pub struct FixtureDir {
    pub dir: TempDir,
    pub base_path: PathBuf,
}

// ─── Claude Code ────────────────────────────────────────────────────────────

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

            let project_dir = base
                .join("projects")
                .join(&session.project_name);
            fs::create_dir_all(&project_dir).unwrap();

            let mut session_lines = Vec::new();
            for msg in &session.messages {
                session_lines.push(render_claude_message(msg, session));
            }
            let session_file = project_dir.join(format!("{}.jsonl", session.session_id));
            fs::write(&session_file, session_lines.join("\n") + "\n").unwrap();
        }

        fs::write(
            base.join("history.jsonl"),
            history_lines.join("\n") + "\n",
        )
        .unwrap();

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
    let branch = session
        .git_branch
        .as_deref()
        .map_or(String::new(), |b| format!(r#","gitBranch":"{}""#, escape_json(b)));
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

// ─── Copilot CLI ────────────────────────────────────────────────────────────

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
    UserMessage { id: String, timestamp: String, content: String },
    AssistantMessage { id: String, timestamp: String, content: String, model: String },
    ToolInvoke { id: String, timestamp: String, tool_name: String, tool_call_id: String },
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

// ─── Gemini CLI ─────────────────────────────────────────────────────────────

pub struct GeminiFixtureBuilder {
    project_path: String,
    project_slug: String,
    sessions: Vec<GeminiSessionSpec>,
}

struct GeminiSessionSpec {
    session_id: String,
    start_time: String,
    last_updated: String,
    messages: Vec<GeminiMessageSpec>,
}

enum GeminiMessageSpec {
    User { id: String, timestamp: String, content: String },
    Gemini {
        id: String,
        timestamp: String,
        text: String,
        model: String,
        input_tokens: u32,
        output_tokens: u32,
    },
}

impl GeminiFixtureBuilder {
    pub fn new() -> Self {
        Self {
            project_path: "/home/user/webapp".to_string(),
            project_slug: "test-project".to_string(),
            sessions: Vec::new(),
        }
    }

    pub fn project(mut self, path: &str, slug: &str) -> Self {
        self.project_path = path.to_string();
        self.project_slug = slug.to_string();
        self
    }

    pub fn add_session(mut self, id: &str) -> GeminiSessionBuilder {
        let spec = GeminiSessionSpec {
            session_id: id.to_string(),
            start_time: "2025-01-01T00:00:00Z".to_string(),
            last_updated: "2025-01-01T00:10:00Z".to_string(),
            messages: Vec::new(),
        };
        self.sessions.push(spec);
        let idx = self.sessions.len() - 1;
        GeminiSessionBuilder {
            parent: self,
            idx,
            msg_counter: 0,
        }
    }

    pub fn build(self) -> FixtureDir {
        let dir = TempDir::new().unwrap();
        let base = dir.path().to_path_buf();

        let projects_json = format!(
            r#"{{"projects":{{"{}":{}}}}}"#,
            escape_json(&self.project_path),
            serde_json_string(&self.project_slug),
        );
        fs::write(base.join("projects.json"), projects_json).unwrap();

        let chats_dir = base
            .join("tmp")
            .join(&self.project_slug)
            .join("chats");
        fs::create_dir_all(&chats_dir).unwrap();

        for session in &self.sessions {
            let mut msg_json_parts = Vec::new();
            for msg in &session.messages {
                msg_json_parts.push(render_gemini_message(msg));
            }

            let session_json = format!(
                r#"{{"sessionId":"{}","startTime":"{}","lastUpdated":"{}","messages":[{}]}}"#,
                escape_json(&session.session_id),
                session.start_time,
                session.last_updated,
                msg_json_parts.join(","),
            );

            let name = &session.session_id;
            let filename = if name.starts_with("session-") {
                format!("{name}.json")
            } else {
                format!("session-{name}.json")
            };
            fs::write(chats_dir.join(filename), session_json).unwrap();
        }

        FixtureDir {
            base_path: base,
            dir,
        }
    }
}

pub struct GeminiSessionBuilder {
    parent: GeminiFixtureBuilder,
    idx: usize,
    msg_counter: u32,
}

impl GeminiSessionBuilder {
    fn session_mut(&mut self) -> &mut GeminiSessionSpec {
        &mut self.parent.sessions[self.idx]
    }

    fn next_id(&mut self) -> String {
        self.msg_counter += 1;
        format!("gm-{:03}", self.msg_counter)
    }

    fn next_timestamp(&self) -> String {
        let offset = self.msg_counter * 5;
        format!("2025-01-01T00:00:{:02}Z", offset.min(59))
    }

    pub fn user(mut self, text: &str) -> Self {
        let id = self.next_id();
        let timestamp = self.next_timestamp();
        self.session_mut()
            .messages
            .push(GeminiMessageSpec::User {
                id,
                timestamp,
                content: text.to_string(),
            });
        self
    }

    pub fn gemini(mut self, text: &str) -> Self {
        let id = self.next_id();
        let timestamp = self.next_timestamp();
        self.session_mut()
            .messages
            .push(GeminiMessageSpec::Gemini {
                id,
                timestamp,
                text: text.to_string(),
                model: "gemini-2.5-pro".to_string(),
                input_tokens: 30,
                output_tokens: 150,
            });
        self
    }

    pub fn done(self) -> GeminiFixtureBuilder {
        self.parent
    }
}

fn render_gemini_message(msg: &GeminiMessageSpec) -> String {
    match msg {
        GeminiMessageSpec::User {
            id,
            timestamp,
            content,
        } => {
            format!(
                r#"{{"id":"{id}","timestamp":"{timestamp}","type":"user","content":"{}","tokens":{{"input":30,"output":0}}}}"#,
                escape_json(content),
            )
        }
        GeminiMessageSpec::Gemini {
            id,
            timestamp,
            text,
            model,
            input_tokens,
            output_tokens,
        } => {
            format!(
                r#"{{"id":"{id}","timestamp":"{timestamp}","type":"gemini","content":[{{"text":"{}"}}],"model":"{model}","tokens":{{"input":{input_tokens},"output":{output_tokens}}}}}"#,
                escape_json(text),
            )
        }
    }
}

// ─── Codex CLI ──────────────────────────────────────────────────────────────

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
        self.session_mut()
            .entries
            .push(CodexEntrySpec::User {
                content: text.to_string(),
                timestamp,
            });
        self
    }

    pub fn assistant(mut self, text: &str) -> Self {
        let timestamp = self.next_timestamp();
        self.session_mut()
            .entries
            .push(CodexEntrySpec::Assistant {
                content: text.to_string(),
                timestamp,
            });
        self
    }

    pub fn error(mut self, error: &str) -> Self {
        let timestamp = self.next_timestamp();
        self.session_mut()
            .entries
            .push(CodexEntrySpec::Error {
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

// ─── OpenCode ───────────────────────────────────────────────────────────────

pub struct OpenCodeFixtureBuilder {
    sessions: Vec<OpenCodeSessionSpec>,
}

struct OpenCodeSessionSpec {
    session_id: String,
    project_hash: String,
    title: String,
    cwd: String,
    created_at: String,
    updated_at: String,
    messages: Vec<OpenCodeMessageSpec>,
}

enum OpenCodeMessageSpec {
    User { id: String, timestamp: String, content: String },
    Assistant { id: String, timestamp: String, content: String },
}

impl OpenCodeFixtureBuilder {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn add_session(mut self, id: &str) -> OpenCodeSessionBuilder {
        let spec = OpenCodeSessionSpec {
            session_id: id.to_string(),
            project_hash: format!("proj-{id}"),
            title: format!("Session {id}"),
            cwd: "/home/user/project".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:30:00Z".to_string(),
            messages: Vec::new(),
        };
        self.sessions.push(spec);
        let idx = self.sessions.len() - 1;
        OpenCodeSessionBuilder {
            parent: self,
            idx,
            msg_counter: 0,
        }
    }

    pub fn build(self) -> FixtureDir {
        let dir = TempDir::new().unwrap();
        let base = dir.path().to_path_buf();

        for session in &self.sessions {
            let session_dir = base
                .join("session")
                .join(&session.project_hash);
            fs::create_dir_all(&session_dir).unwrap();

            let session_json = format!(
                r#"{{"id":"{}","title":"{}","createdAt":"{}","updatedAt":"{}","cwd":"{}"}}"#,
                escape_json(&session.session_id),
                escape_json(&session.title),
                session.created_at,
                session.updated_at,
                escape_json(&session.cwd),
            );
            let session_file = session_dir.join(format!("{}.json", session.session_id));
            fs::write(session_file, session_json).unwrap();

            let msg_dir = base.join("message").join(&session.session_id);
            fs::create_dir_all(&msg_dir).unwrap();

            for msg in &session.messages {
                let (id, json) = render_opencode_message(msg);
                let msg_file = msg_dir.join(format!("{id}.json"));
                fs::write(msg_file, json).unwrap();
            }
        }

        FixtureDir {
            base_path: base,
            dir,
        }
    }
}

pub struct OpenCodeSessionBuilder {
    parent: OpenCodeFixtureBuilder,
    idx: usize,
    msg_counter: u32,
}

impl OpenCodeSessionBuilder {
    fn session_mut(&mut self) -> &mut OpenCodeSessionSpec {
        &mut self.parent.sessions[self.idx]
    }

    fn next_id(&mut self) -> String {
        self.msg_counter += 1;
        format!("msg-{:03}", self.msg_counter)
    }

    fn next_timestamp(&self) -> String {
        let offset = self.msg_counter * 10;
        format!("2025-01-01T00:00:{:02}Z", offset.min(59))
    }

    pub fn title(mut self, title: &str) -> Self {
        self.session_mut().title = title.to_string();
        self
    }

    pub fn cwd(mut self, cwd: &str) -> Self {
        self.session_mut().cwd = cwd.to_string();
        self
    }

    pub fn user(mut self, text: &str) -> Self {
        let id = self.next_id();
        let timestamp = self.next_timestamp();
        self.session_mut()
            .messages
            .push(OpenCodeMessageSpec::User {
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
            .messages
            .push(OpenCodeMessageSpec::Assistant {
                id,
                timestamp,
                content: text.to_string(),
            });
        self
    }

    pub fn done(self) -> OpenCodeFixtureBuilder {
        self.parent
    }
}

fn render_opencode_message(msg: &OpenCodeMessageSpec) -> (String, String) {
    match msg {
        OpenCodeMessageSpec::User {
            id,
            timestamp,
            content,
        } => {
            let json = format!(
                r#"{{"id":"{id}","role":"user","timestamp":"{timestamp}","content":"{}"}}"#,
                escape_json(content),
            );
            (id.clone(), json)
        }
        OpenCodeMessageSpec::Assistant {
            id,
            timestamp,
            content,
        } => {
            let json = format!(
                r#"{{"id":"{id}","role":"assistant","timestamp":"{timestamp}","content":"{}"}}"#,
                escape_json(content),
            );
            (id.clone(), json)
        }
    }
}

// ─── Convenience factories ──────────────────────────────────────────────────

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

pub fn copilot_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = CopilotFixtureBuilder::new()
        .add_session("copilot-gen-001");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.assistant(&format!("Assistant response {i}"));
        }
    }
    builder.done().build()
}

pub fn gemini_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = GeminiFixtureBuilder::new()
        .add_session("gemini-gen-001");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.gemini(&format!("Gemini response {i}"));
        }
    }
    builder.done().build()
}

pub fn codex_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = CodexFixtureBuilder::new()
        .add_session("codex-gen");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.assistant(&format!("Assistant response {i}"));
        }
    }
    builder.done().build()
}

pub fn opencode_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = OpenCodeFixtureBuilder::new()
        .add_session("oc-gen-001");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.assistant(&format!("Assistant response {i}"));
        }
    }
    builder.done().build()
}

pub fn all_generated_providers(
    n_sessions: usize,
    msgs_per_session: usize,
) -> (Vec<TempDir>, Vec<Box<dyn aghist::provider::HistoryProvider>>) {
    use aghist::provider::claude_code::ClaudeCodeProvider;
    use aghist::provider::codex_cli::CodexCliProvider;
    use aghist::provider::copilot_cli::CopilotCliProvider;
    use aghist::provider::gemini_cli::GeminiCliProvider;
    use aghist::provider::opencode::OpenCodeProvider;

    let mut dirs = Vec::new();
    let mut providers: Vec<Box<dyn aghist::provider::HistoryProvider>> = Vec::new();

    // Claude
    let mut claude = ClaudeFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = claude.add_session(&format!("session-{s:03}"))
            .project(&format!("project-{s}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m} in session {s}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m} in session {s}"));
            }
        }
        claude = sb.done();
    }
    let cf = claude.build();
    providers.push(Box::new(ClaudeCodeProvider::new(vec![cf.base_path.clone()])));
    dirs.push(cf.dir);

    // Copilot
    let mut copilot = CopilotFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = copilot.add_session(&format!("copilot-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m}"));
            }
        }
        copilot = sb.done();
    }
    let cpf = copilot.build();
    providers.push(Box::new(CopilotCliProvider::new(vec![cpf.base_path.clone()])));
    dirs.push(cpf.dir);

    // Gemini
    let mut gemini = GeminiFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = gemini.add_session(&format!("gemini-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.gemini(&format!("Gemini msg {m}"));
            }
        }
        gemini = sb.done();
    }
    let gf = gemini.build();
    providers.push(Box::new(GeminiCliProvider::new(vec![gf.base_path.clone()])));
    dirs.push(gf.dir);

    // Codex
    let mut codex = CodexFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = codex.add_session(&format!("codex-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m}"));
            }
        }
        codex = sb.done();
    }
    let cxf = codex.build();
    providers.push(Box::new(CodexCliProvider::new(vec![cxf.base_path.clone()])));
    dirs.push(cxf.dir);

    // OpenCode
    let mut opencode = OpenCodeFixtureBuilder::new();
    for s in 0..n_sessions {
        let mut sb = opencode.add_session(&format!("oc-{s:03}"));
        for m in 0..msgs_per_session {
            if m % 2 == 0 {
                sb = sb.user(&format!("User msg {m}"));
            } else {
                sb = sb.assistant(&format!("Assistant msg {m}"));
            }
        }
        opencode = sb.done();
    }
    let ocf = opencode.build();
    providers.push(Box::new(OpenCodeProvider::new(vec![ocf.base_path.clone()])));
    dirs.push(ocf.dir);

    (dirs, providers)
}

// ─── Utilities ──────────────────────────────────────────────────────────────

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn serde_json_string(s: &str) -> String {
    format!("\"{}\"", escape_json(s))
}

fn iso_to_millis(iso: &str) -> u64 {
    use chrono::DateTime;
    DateTime::parse_from_rfc3339(iso)
        .map(|dt| u64::try_from(dt.timestamp_millis()).unwrap_or(0))
        .unwrap_or(0)
}
