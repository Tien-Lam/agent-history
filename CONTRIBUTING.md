# Contributing

This project is intentionally contract-heavy: CLI JSON, MCP responses, citation
refs, provider normalization, and release packaging are all user-facing
interfaces. Feature work should preserve those contracts unless the changelog
explicitly calls out a breaking change.

## Local Verification

Run the default check set before committing feature work:

```sh
scripts/verify.sh
```

For changes that touch optional embeddings, release/update code, dependencies,
or GitHub workflows, run the broader set:

```sh
scripts/verify.sh --all-features --supply-chain
```

The `--supply-chain` mode expects `cargo-deny`, `cargo-machete`, and
`actionlint` to be installed locally. CI installs pinned versions of those tools.

## Adding A Feature

Prefer adding behavior through the existing service layer instead of wiring
providers directly into commands or MCP handlers. The main boundaries are:

- `src/provider/`: discovery and parser normalization for provider-owned files.
- `src/services/`: shared lookup/list/search/index behavior used by CLI and MCP.
- `src/commands/`: argument decoding, service calls, and stdout/stderr rendering.
- `src/mcp/`: JSON-RPC transport, tool/resource dispatch, and MCP payload shapes.
- `src/schema/` and `src/schema_fragments/`: machine-readable command contracts.

When a feature changes machine output, update or add JSON/MCP contract tests and
snapshots in the same commit as the behavior. When it changes user-facing
behavior, update `README.md` or `docs/ARCHITECTURE.md` as part of the same stage.

## Adding A Provider

Start with the provider checklist in [Architecture](docs/ARCHITECTURE.md#adding-a-provider).
At minimum, provider additions should include:

- focused parser fixtures,
- generated fixture support under `tests/common/fixtures/`,
- provider conformance coverage,
- README Supported Providers documentation,
- schema or MCP updates only when the public surface changes.

Provider parsers read untrusted local history files. Keep reads bounded, reject
symlinked session files, tolerate malformed records where possible, and surface
partial failures through warnings instead of aborting unrelated providers.

## Commit Shape

Keep commits small enough that a failure points to one concern: behavior,
contracts, docs, or tooling. Update `CHANGELOG.md` under `Unreleased` for
user-visible changes and new project guardrails.
