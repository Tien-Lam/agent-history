use std::fs;

use tempfile::TempDir;

use super::core::FixtureDir;

pub struct CursorFixtureBuilder {
    sessions: Vec<CursorSessionSpec>,
}

struct CursorSessionSpec {
    composer_id: String,
    name: String,
    workspace: String,
    created_at_ms: i64,
    updated_at_ms: i64,
    messages: Vec<CursorMessageSpec>,
}

enum CursorMessageSpec {
    User {
        bubble_id: String,
        text: String,
        ts_ms: i64,
    },
    Assistant {
        bubble_id: String,
        text: String,
        ts_ms: i64,
    },
}

pub struct CursorSessionBuilder {
    parent: CursorFixtureBuilder,
    idx: usize,
    msg_counter: usize,
}

impl Default for CursorFixtureBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorFixtureBuilder {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn add_session(mut self, id: &str) -> CursorSessionBuilder {
        let spec = CursorSessionSpec {
            composer_id: id.to_string(),
            name: format!("Session {id}"),
            workspace: "/home/user/myapp".to_string(),
            created_at_ms: 1_767_225_600_000,
            updated_at_ms: 1_767_225_900_000,
            messages: Vec::new(),
        };
        self.sessions.push(spec);
        let idx = self.sessions.len() - 1;
        CursorSessionBuilder {
            parent: self,
            idx,
            msg_counter: 0,
        }
    }

    pub fn build(self) -> FixtureDir {
        use rusqlite::Connection;

        let dir = TempDir::new().unwrap();
        let base = dir.path().to_path_buf();
        let db_path = base.join("User").join("globalStorage").join("state.vscdb");
        fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value BLOB)",
            [],
        )
        .unwrap();

        for session in &self.sessions {
            let headers: Vec<serde_json::Value> = session
                .messages
                .iter()
                .map(|m| {
                    let (bid, ty) = match m {
                        CursorMessageSpec::User { bubble_id, .. } => (bubble_id, 1),
                        CursorMessageSpec::Assistant { bubble_id, .. } => (bubble_id, 2),
                    };
                    serde_json::json!({"bubbleId": bid, "type": ty})
                })
                .collect();

            let composer = serde_json::json!({
                "composerId": session.composer_id,
                "name": session.name,
                "createdAt": session.created_at_ms,
                "lastUpdatedAt": session.updated_at_ms,
                "currentWorkspaceFolder": session.workspace,
                "fullConversationHeadersOnly": headers,
            });
            let bytes = serde_json::to_vec(&composer).unwrap();
            conn.execute(
                "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
                rusqlite::params![format!("composerData:{}", session.composer_id), bytes],
            )
            .unwrap();

            for msg in &session.messages {
                let (key, json) = match msg {
                    CursorMessageSpec::User {
                        bubble_id,
                        text,
                        ts_ms,
                    } => (
                        format!("bubbleId:{}:{}", session.composer_id, bubble_id),
                        serde_json::json!({"type": 1, "text": text, "createdAt": ts_ms}),
                    ),
                    CursorMessageSpec::Assistant {
                        bubble_id,
                        text,
                        ts_ms,
                    } => (
                        format!("bubbleId:{}:{}", session.composer_id, bubble_id),
                        serde_json::json!({"type": 2, "text": text, "createdAt": ts_ms}),
                    ),
                };
                let bytes = serde_json::to_vec(&json).unwrap();
                conn.execute(
                    "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
                    rusqlite::params![key, bytes],
                )
                .unwrap();
            }
        }
        drop(conn);

        FixtureDir {
            base_path: base,
            dir,
        }
    }
}

impl CursorSessionBuilder {
    pub fn user(mut self, text: &str) -> Self {
        let session = &mut self.parent.sessions[self.idx];
        let bubble_id = format!("b-{}-{}", session.composer_id, self.msg_counter);
        let ts_ms = session.created_at_ms + i64::try_from(self.msg_counter).unwrap_or(0) * 1000;
        session.messages.push(CursorMessageSpec::User {
            bubble_id,
            text: text.to_string(),
            ts_ms,
        });
        self.msg_counter += 1;
        self
    }

    pub fn assistant(mut self, text: &str) -> Self {
        let session = &mut self.parent.sessions[self.idx];
        let bubble_id = format!("b-{}-{}", session.composer_id, self.msg_counter);
        let ts_ms = session.created_at_ms + i64::try_from(self.msg_counter).unwrap_or(0) * 1000;
        session.messages.push(CursorMessageSpec::Assistant {
            bubble_id,
            text: text.to_string(),
            ts_ms,
        });
        self.msg_counter += 1;
        self
    }

    pub fn done(self) -> CursorFixtureBuilder {
        self.parent
    }
}

pub fn cursor_single_session(n_messages: usize) -> FixtureDir {
    let mut builder = CursorFixtureBuilder::new().add_session("comp-gen-001");
    for i in 0..n_messages {
        if i % 2 == 0 {
            builder = builder.user(&format!("User message {i}"));
        } else {
            builder = builder.assistant(&format!("Assistant response {i}"));
        }
    }
    builder.done().build()
}
