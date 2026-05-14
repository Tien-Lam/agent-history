# Zed AI fixtures

Zed (<https://zed.dev>) stores assistant-panel conversations as JSON files
under its user-data directory:

- Linux:   `~/.local/share/zed/conversations/*.json` (also `~/.config/zed/conversations/`)
- macOS:   `~/Library/Application Support/Zed/conversations/*.json`
- Windows: `%APPDATA%\Zed\conversations\*.json`

Each `*.json` file holds a single conversation. The schema is tolerant —
fields are optional, and unknown shapes are skipped rather than fatal. See
the module docstring in `src/provider/zed_ai.rs` for the supported shape.

## Static fixture

`conversations/sample.json` is a small, hand-written conversation kept
for parser-stability tests. It exercises:

- ISO RFC3339 timestamps on the session and per-message
- A fenced code block in an assistant message (must be promoted to a
  `ContentBlock::CodeBlock` by the shared parser)
- A `workspace` field that yields a non-`None` `project_name`

For tests that need a controlled fresh fixture, prefer the unit-test
helpers in `src/provider/zed_ai.rs` (`TempDir` + JSON writer) rather than
mutating this checked-in file.
