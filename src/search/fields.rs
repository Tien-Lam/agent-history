use tantivy::schema::{Field, Schema, INDEXED, STORED, STRING, TEXT};

#[derive(Clone, Copy)]
pub(super) struct SearchFields {
    pub(super) session_key: Field,
    pub(super) session_id: Field,
    pub(super) message_key: Field,
    pub(super) message_id: Field,
    pub(super) provider: Field,
    pub(super) project: Field,
    pub(super) project_raw: Field,
    pub(super) role: Field,
    pub(super) content: Field,
    pub(super) tool_output: Field,
    pub(super) timestamp: Field,
    pub(super) has_tool_call: Field,
    pub(super) kind: Field,
    pub(super) note_id: Field,
    pub(super) note_session_ref: Field,
}

impl SearchFields {
    pub(super) fn build_schema() -> (Schema, Self) {
        let mut builder = Schema::builder();
        let fields = Self {
            session_key: builder.add_text_field("session_key", STRING | STORED),
            session_id: builder.add_text_field("session_id", STRING | STORED),
            message_key: builder.add_text_field("message_key", STRING | STORED),
            message_id: builder.add_text_field("message_id", STRING | STORED),
            provider: builder.add_text_field("provider", STRING | STORED),
            project: builder.add_text_field("project", TEXT | STORED),
            project_raw: builder.add_text_field("project_raw", STRING | STORED),
            role: builder.add_text_field("role", STRING | STORED),
            content: builder.add_text_field("content", TEXT | STORED),
            tool_output: builder.add_text_field("tool_output", TEXT | STORED),
            timestamp: builder.add_i64_field("timestamp", INDEXED | STORED),
            has_tool_call: builder.add_i64_field("has_tool_call", INDEXED | STORED),
            kind: builder.add_text_field("kind", STRING | STORED),
            note_id: builder.add_i64_field("note_id", INDEXED | STORED),
            note_session_ref: builder.add_text_field("note_session_ref", STRING | STORED),
        };
        (builder.build(), fields)
    }
}
