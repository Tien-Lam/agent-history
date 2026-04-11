# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.3] - 2026-04-11

### Changed

- Migrated tests to shared helpers, added search and export workflow tests
- Strengthened tests to assert real behavior instead of just "didn't crash"
- Removed mocked dispatch calls in favor of exercising the real event loop

## [0.1.2] - 2026-04-10

### Added

- Resume command with clipboard copy for all providers (`y` keybinding)
- Cross-platform release workflow (Linux x86_64, Windows x86_64, macOS aarch64)
- Demo tape for recording GIF with VHS

### Fixed

- Windows key handling and UI rendering
- Filter editing, project name display, config loading, and assorted UX issues

## [0.1.0] - 2026-04-10

### Added

- TUI with session browsing, message viewing, and vim-style keybindings
- Five provider parsers: Claude Code, Copilot CLI, Gemini CLI, Codex CLI, OpenCode
- Full-text search powered by Tantivy with incremental indexing
- Filter panel (provider, project, date range)
- Export to Markdown, JSON, and HTML
- Configuration via TOML (`~/.config/aghist/config.toml`)
- LRU message cache
- GitHub Actions CI (clippy, tests, build)
- Snapshot tests with insta

[0.1.3]: https://github.com/Tien-Lam/agent-history/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Tien-Lam/agent-history/compare/v0.1.0...v0.1.2
[0.1.0]: https://github.com/Tien-Lam/agent-history/releases/tag/v0.1.0
