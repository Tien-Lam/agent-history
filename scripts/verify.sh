#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/verify.sh [--all-features] [--supply-chain]

Runs the local checks expected before opening or merging feature work.

Options:
  --all-features   Include optional feature checks for embeddings/self-update paths.
  --supply-chain   Include cargo-deny, cargo-machete, and actionlint checks.
  -h, --help       Show this help.
EOF
}

all_features=0
supply_chain=0

while (($#)); do
  case "$1" in
    --all-features)
      all_features=1
      ;;
    --supply-chain)
      supply_chain=1
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

run() {
  printf '\n+'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "Error: required command not found: $name" >&2
    exit 127
  fi
}

run cargo fmt --check
run cargo clippy --locked --all-targets -- -D warnings
run cargo clippy --locked --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::indexing_slicing -D clippy::panic
RUSTDOCFLAGS="-D warnings" run cargo doc --locked --no-deps
run cargo test --locked

if ((all_features)); then
  run cargo clippy --locked --all-targets --all-features -- -D warnings
  run cargo clippy --locked --lib --bins --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::indexing_slicing -D clippy::panic
  RUSTDOCFLAGS="-D warnings" run cargo doc --locked --no-deps --all-features
  run cargo test --locked --all-features --lib
fi

if ((supply_chain)); then
  require_command cargo-deny
  require_command cargo-machete
  require_command actionlint

  mkdir -p target/deny
  printf '\n+ cargo metadata --locked --format-version 1 > target/deny/default.json\n'
  cargo metadata --locked --format-version 1 > target/deny/default.json
  run cargo deny check --metadata-path target/deny/default.json --hide-inclusion-graph -A advisory-not-detected -A unmatched-skip

  printf '\n+ cargo metadata --locked --format-version 1 --all-features > target/deny/all-features.json\n'
  cargo metadata --locked --format-version 1 --all-features > target/deny/all-features.json
  run cargo deny check --metadata-path target/deny/all-features.json --hide-inclusion-graph

  run cargo machete --with-metadata
  run actionlint
fi
