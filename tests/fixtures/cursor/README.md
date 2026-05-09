# Cursor fixtures

Cursor stores chat history in a SQLite database at:

- Linux:   `~/.config/Cursor/User/globalStorage/state.vscdb`
- macOS:   `~/Library/Application Support/Cursor/User/globalStorage/state.vscdb`
- Windows: `%APPDATA%\Cursor\User\globalStorage\state.vscdb`

The relevant table is `cursorDiskKV (key TEXT PRIMARY KEY, value BLOB)`,
where the BLOB is JSON. Keys we care about:

- `composerData:<composerId>`           → session header
- `bubbleId:<composerId>:<bubbleId>`    → individual message bubble

Bubble `type` field encodes the role (`1` = user, `2` = assistant).

## Static fixture

`User/globalStorage/state.vscdb` here is a small SQLite database checked
in for parser-stability tests. Regenerate it via `tests/cursor_fixture_gen.rs`
(see top-level `cursor_fixture` test).

For tests that need a controlled fresh fixture, prefer the programmatic
builder: `tests::common::fixtures::cursor_single_session(n)` /
`CursorFixtureBuilder::new()`.
