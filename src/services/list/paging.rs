use crate::cursor::ListCursor;

use super::ListedSession;

pub(super) fn compare_listed_sessions(a: &ListedSession, b: &ListedSession) -> std::cmp::Ordering {
    b.session
        .started_at
        .cmp(&a.session.started_at)
        .then_with(|| a.session.id.0.cmp(&b.session.id.0))
        .then_with(|| a.session.identity_key().cmp(&b.session.identity_key()))
}

pub(super) fn listed_session_is_after_cursor(listed: &ListedSession, cursor: &ListCursor) -> bool {
    if listed.session.started_at != cursor.started_at {
        return listed.session.started_at < cursor.started_at;
    }
    if listed.session.id.0 != cursor.session_id {
        return listed.session.id.0 > cursor.session_id;
    }

    if cursor.session_key.is_empty() {
        return false;
    }
    listed.session.identity_key().as_str() > cursor.session_key.as_str()
}
