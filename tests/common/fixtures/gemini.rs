use std::fs;

use tempfile::TempDir;

use super::core::FixtureDir;
use super::util::{escape_json, serde_json_string};

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
    User {
        id: String,
        timestamp: String,
        content: String,
    },
    Gemini {
        id: String,
        timestamp: String,
        text: String,
        model: String,
        input_tokens: u32,
        output_tokens: u32,
    },
    Raw(String),
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

        let chats_dir = base.join("tmp").join(&self.project_slug).join("chats");
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
        self.session_mut().messages.push(GeminiMessageSpec::User {
            id,
            timestamp,
            content: text.to_string(),
        });
        self
    }

    pub fn gemini(mut self, text: &str) -> Self {
        let id = self.next_id();
        let timestamp = self.next_timestamp();
        self.session_mut().messages.push(GeminiMessageSpec::Gemini {
            id,
            timestamp,
            text: text.to_string(),
            model: "gemini-2.5-pro".to_string(),
            input_tokens: 30,
            output_tokens: 150,
        });
        self
    }

    pub fn raw_message(mut self, raw: &str) -> Self {
        self.session_mut()
            .messages
            .push(GeminiMessageSpec::Raw(raw.to_string()));
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
        GeminiMessageSpec::Raw(raw) => raw.clone(),
    }
}

pub fn gemini_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = GeminiFixtureBuilder::new().add_session("gemini-gen-001");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.gemini(&format!("Gemini response {i}"));
        }
    }
    builder.done().build()
}
