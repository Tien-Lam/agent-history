# Agent Instructions

This project is **bd + gt driven**. All task tracking lives in beads (`bd`); Gas Town (`gt`) dispatches polecat agents to work tasks.

- **HQ:** `~/gt/` (shared with other rigs)
- **Rig:** `aghist` (this project registered under HQ)
- **Issue prefix:** `ahist-` (lowercase — uppercase prefixes break gt routing)
- **bd db:** dolt server at port pinned in `.beads/config.yaml`, db name `ahist`

## Quick Reference

```bash
bd ready                     # Available work in this rig
bd show <id>                 # View an issue
bd update <id> --claim       # Claim work atomically
bd create -t task -p 2 --title "..."   # File new work
gt ready                     # Ready work across all rigs
gt bead show ahist-<id>      # Resolve a bead via routes (works from anywhere)
gt sling ahist-<id> aghist --merge=direct   # Dispatch a polecat to work it
```

## Rules

- **Use `bd` for ALL task tracking.** Do NOT use TodoWrite, TaskCreate, or markdown TODO lists.
- **Bead IDs must stay lowercase.** `ahist-abc` is valid, `AHIST-ABC` will break gt routing.
- **Use `bd remember` for persistent agent knowledge** — do NOT use MEMORY.md files.
- **Run `bd prime`** for the full bd command reference.

## Citation Refs

Every message in a session has a stable, human-quotable reference of the form:

```
<provider-slug>/<session-id>#<turn>
```

Examples:

- `claude-code/abc-123-def#7` — turn 7 of a Claude Code session
- `codex-cli/rollout-2024-03-15T10-30-00-a1b2c3d4-...#1` — first turn of a Codex CLI rollout
- `opencode/ses_abc123#42` — turn 42 of an OpenCode session

Rules:

- **Provider slug** is the kebab-case name from `Provider::slug()` — exactly one of `claude-code`, `copilot-cli`, `gemini-cli`, `codex-cli`, `opencode`. These slugs are stable contract; do not rename without a migration plan.
- **Session id** is whatever the provider returns as `Session.id`, used verbatim. It may contain dashes, underscores, dots, etc.
- **Turn** is the 1-based index of the message within the session in the order the provider yields it from `load_messages`. Turn `0` is invalid.
- **Refs are opaque-stable across reindex.** Rebuilding the search index does not change a ref. As long as the source files are unchanged, the same `(provider, session-id, turn)` always points at the same message.

In code: parse with `CitationRef::from_str` (returns `CitationParseError` on malformed input), build with `CitationRef::new` (validates non-zero turn, non-empty session id), render with `Display`. See `src/model/citation.rs`.

## Non-Interactive Shell Commands

Polecat sessions and CI cannot answer interactive prompts. Always use non-interactive flags:

```bash
cp -f / mv -f / rm -f         # NOT cp/mv/rm (may be aliased to -i)
rm -rf / cp -rf               # recursive forms
scp -o BatchMode=yes
ssh -o BatchMode=yes
apt-get -y
HOMEBREW_NO_AUTO_UPDATE=1 brew ...
```

## Session Completion

The exit path differs depending on whether you are a gt polecat or a standalone session. Pick the right one — getting this wrong leaves work stranded.

### If you are a gt polecat session

Check first: does `gt hook` show work assigned to you? If yes, you are a polecat — follow this path.

**Path A — task produced git changes (the common case):**

1. **Commit all changes locally:** `git add <files> && git commit -m "type: desc (ahist-<id>)"`
2. **Do NOT run `bd close`** — the Refinery closes issues after merge. Closing early causes the witness to respawn you in a loop.
3. **Do NOT run `git push`** — `gt done` handles push + MR creation.
4. **Run `gt done`** as your only exit:
   ```bash
   gt done --pre-verified --target main
   ```
5. **If `gt done` fails on uncommitted `.beads/metadata.json`** — known drift issue:
   ```bash
   git restore .beads/metadata.json && gt done --pre-verified --target main
   ```

**Path B — task produced no git changes (config-only, dolt-only, external API calls, etc.):**

If the work lives entirely outside the git tree (e.g., `bd dolt remote add`, modifying live state), there is no MR for the Refinery to merge — so it will never close the bead. In that case you MUST close manually:

1. **Run `bd close ahist-<id> --reason "..."`** with a clear reason that names what was done.
2. **Run `gt done --status DEFERRED --cleanup-status clean`** — tells the rig "no merge needed, polecat exiting cleanly".

Decide between Path A and Path B by `git status` after your work: if there are no changes to commit, you are on Path B.

### If you are a standalone session (no gt)

```bash
# 1. Quality gates — run if code changed
cargo test
cargo clippy

# 2. Update bd state
bd close <id> --reason "..."   # for finished work
bd update <id> ...             # for in-progress work

# 3. Push everything
git pull --rebase
bd dolt push                   # push beads data (if remote configured)
git push
git status                     # MUST show "up to date with origin"
```

**Standalone rules:**
- Work is NOT complete until `git push` succeeds.
- Never say "ready to push when you are" — YOU push.
- If push fails, resolve and retry until it succeeds.

## CLI error envelope and exit codes

All `aghist` subcommands emit errors as a single line of JSON to **stderr** in
this shape (see `src/cli_error.rs`):

```json
{"error":{"kind":"<kebab>","message":"...","hint":"..."}}
```

`hint` is omitted when there is no actionable suggestion. Successful command
output goes to stdout and is never wrapped in this envelope.

### Semantic exit codes

| Code | Meaning                               | Examples |
|------|---------------------------------------|----------|
| 0    | success with results                  | `--list` returned ≥1 session, `export` wrote a session |
| 1    | runtime error (envelope on stderr)    | session not found, provider failure, IO error |
| 2    | usage error (envelope on stderr)      | unknown flag, invalid `--format` value, missing required arg |
| 3    | success but empty (no envelope)       | `--list` found zero sessions, future: search with zero hits |

Exit-code 3 is a **success** signal — agents should treat it as "the query ran
fine and the answer is the empty set," not as failure.

### Stable `kind` values

New code MUST reuse one of these kinds when it fits; if a genuinely new
condition needs its own kind, add it here in the same PR. Kinds are
kebab-case, lowercase, no underscores.

| Kind                  | When to use |
|-----------------------|-------------|
| `usage`               | Argument parsing failed (clap error). Always paired with exit 2. |
| `session-not-found`   | Caller named a session ID/prefix that didn't match any session. |
| `provider-unavailable`| Session refers to a provider that isn't enabled in config. |
| `provider-error`      | A provider failed while loading messages or discovering sessions. |
| `io-error`            | Filesystem or terminal IO failure (read/write/permissions). |
| `index-error`         | Tantivy search index could not be opened, written, or queried. |
| `update-failed`       | `aghist update` self-update flow failed. |
| `rsync-failed`        | `aghist sources pull` invoked rsync, which exited non-zero. |
| `aborted`             | User declined a confirmation prompt (e.g. `uninstall`). |
| `internal-error`      | Unexpected error from the TUI or another component. Treat as a bug. |

## bd ↔ gt Architecture (this project)

```
~/gt/                                  # Gas Town HQ
  .beads/                              # HQ beads (prefix: hq-, port 3307)
    routes.jsonl                       # ahist- → aghist/mayor/rig
  aghist/                              # This project's rig
    .beads/redirect → mayor/rig/.beads
    mayor/rig/.beads/                  # Rig beads — points at project dolt
      metadata.json                    #   dolt_database: ahist
      dolt-server.port                 #   matches project's port
    mayor/rig/, refinery/rig/          # gt's working clones of this repo
    polecats/                          # Spawned worker dirs
    crew/<name>/                       # Human workspace(s)

/home/tien/projects/agent-history/     # The project repo (this dir)
  .beads/                              # Project beads (prefix: ahist, db: ahist)
    dolt/ahist/.dolt/                  # Dolt data
    dolt-server.port                   # Source of truth for the port
    metadata.json                      # dolt_mode: server
```

If `gt` can't find beads (`database "ahist" not found`), the rig's `dolt-server.port` has drifted from the project's. Re-sync:

```bash
cp /home/tien/projects/agent-history/.beads/dolt-server.port \
   ~/gt/aghist/mayor/rig/.beads/dolt-server.port
```

Pinning the port in `/home/tien/projects/agent-history/.beads/config.yaml` (`dolt.port: <N>`) avoids this drift on restarts.

### Polecats: bd can't find the database

**If you are a polecat** and `bd <anything>` fails with `database "ahist" not found on Dolt server`, you are hitting a known issue (ahist-9vx). Polecat worktrees don't have `.beads/dolt-server.port` (it's a runtime file, gitignored), and bd's port resolution doesn't honor the `dolt.port` pin in `config.yaml` for clones. Workaround:

```bash
export BEADS_DOLT_PORT=$(cat /home/tien/projects/agent-history/.beads/dolt-server.port)
# Or hardcode if the project port is pinned:
export BEADS_DOLT_PORT=33141
```

Set this once at the start of your session — every subsequent `bd` call inherits it. Do NOT spend time diagnosing; this is the fix.

## Recovery references

When polecats get stuck, agent beads conflict, or Dolt complains about case-twin dirs (`AHIST/` + `ahist/`), see the runbooks in `~/projects/runbooks/gastown/`:

- `gt-bd-setup.md` — full setup + troubleshooting matrix
- `polecat-recovery.md` — recovery procedures
- `polecat-monitoring.md` — live tmux monitoring from a parent Claude session
- `prefix-case-migration.md` — fixing uppercase prefixes
