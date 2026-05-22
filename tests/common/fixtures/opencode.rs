use std::fs;

use tempfile::TempDir;

use super::core::FixtureDir;
use super::util::escape_json;

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
    User {
        id: String,
        timestamp: String,
        content: String,
    },
    Assistant {
        id: String,
        timestamp: String,
        content: String,
    },
    Raw {
        id: String,
        json: String,
    },
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
            let session_dir = base.join("session").join(&session.project_hash);
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
        self.session_mut().messages.push(OpenCodeMessageSpec::User {
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

    pub fn raw_message(mut self, id: &str, json: &str) -> Self {
        self.session_mut().messages.push(OpenCodeMessageSpec::Raw {
            id: id.to_string(),
            json: json.to_string(),
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
        OpenCodeMessageSpec::Raw { id, json } => (id.clone(), json.clone()),
    }
}

pub fn opencode_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = OpenCodeFixtureBuilder::new().add_session("oc-gen-001");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.assistant(&format!("Assistant response {i}"));
        }
    }
    builder.done().build()
}
