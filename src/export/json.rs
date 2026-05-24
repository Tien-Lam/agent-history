use serde::Serialize;

use crate::metadata::Note;
use crate::model::{Message, Session};

use super::notes::NoteBuckets;

pub fn to_json(session: &Session, messages: &[Message]) -> String {
    to_json_with_notes(session, messages, &[])
}

pub fn to_json_with_notes(session: &Session, messages: &[Message], notes: &[Note]) -> String {
    let session_ref = session.session_ref().to_string();
    to_json_with_notes_for_session_ref(session, messages, notes, &session_ref)
}

pub(crate) fn to_json_with_notes_for_session_ref(
    session: &Session,
    messages: &[Message],
    notes: &[Note],
    session_ref: &str,
) -> String {
    #[derive(Serialize)]
    struct ExportData<'a> {
        session: &'a Session,
        messages: &'a [Message],
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<Vec<NoteWire<'a>>>,
    }

    /// JSON projection of [`Note`] with a `kind: "private-annotation"` tag
    /// so consumers don't conflate annotations with session content.
    #[derive(Serialize)]
    struct NoteWire<'a> {
        kind: &'static str,
        id: i64,
        session_ref: &'a str,
        body: &'a str,
        created_at: &'a str,
        updated_at: &'a str,
    }

    let buckets = NoteBuckets::build_for_session_ref(session_ref, notes);
    let mut matched: Vec<&Note> = buckets.session_level.clone();
    for v in buckets.by_turn.values() {
        matched.extend(v.iter().copied());
    }
    matched.sort_by_key(|n| n.id);
    let wire_notes: Vec<NoteWire<'_>> = matched
        .into_iter()
        .map(|n| NoteWire {
            kind: "private-annotation",
            id: n.id,
            session_ref: &n.session_ref,
            body: &n.body,
            created_at: &n.created_at,
            updated_at: &n.updated_at,
        })
        .collect();
    let notes_field = if wire_notes.is_empty() {
        None
    } else {
        Some(wire_notes)
    };

    serde_json::to_string_pretty(&ExportData {
        session,
        messages,
        notes: notes_field,
    })
    .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string())
}
