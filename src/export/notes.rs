use std::collections::HashMap;

use crate::metadata::Note;
use crate::model::Session;

/// Bucket of notes for a single session, partitioned by their citation ref.
pub(super) struct NoteBuckets<'a> {
    pub(super) session_level: Vec<&'a Note>,
    pub(super) by_turn: HashMap<u32, Vec<&'a Note>>,
}

impl<'a> NoteBuckets<'a> {
    pub(super) fn build(session: &Session, notes: &'a [Note]) -> Self {
        let session_ref = session.session_ref().to_string();
        let turn_prefix = format!("{session_ref}#");
        let mut session_level = Vec::new();
        let mut by_turn: HashMap<u32, Vec<&Note>> = HashMap::new();
        for n in notes {
            if n.session_ref == session_ref {
                session_level.push(n);
            } else if let Some(rest) = n.session_ref.strip_prefix(&turn_prefix) {
                if let Ok(turn) = rest.parse::<u32>() {
                    if turn > 0 {
                        by_turn.entry(turn).or_default().push(n);
                    }
                }
            }
        }
        Self {
            session_level,
            by_turn,
        }
    }
}
