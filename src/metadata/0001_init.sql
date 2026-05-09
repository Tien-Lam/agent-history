-- Per-user annotations on agent sessions/turns. session_ref is a citation ref
-- of the form "<provider>/<session-id>" or "<provider>/<session-id>#<turn>".

CREATE TABLE notes (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_ref  TEXT NOT NULL,
    body         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX notes_session_ref_idx ON notes(session_ref);

CREATE TABLE tags (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_ref  TEXT NOT NULL,
    tag          TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(session_ref, tag)
);

CREATE INDEX tags_session_ref_idx ON tags(session_ref);
CREATE INDEX tags_tag_idx ON tags(tag);

CREATE TABLE stars (
    session_ref  TEXT NOT NULL UNIQUE,
    starred_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
