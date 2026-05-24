#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<EOF
Build a local release archive smoke test:
  1. package the current release binary
  2. verify the archive contains the binary and install marker
  3. install from the local archive
  4. run aghist --version
  5. uninstall the installed binary non-interactively

Usage: scripts/smoke-release-install.sh [OPTIONS]

Options:
    --target TARGET  Rust target triple (default: current host)
    --tag TAG        Synthetic release tag (default: v0.0.0-smoke)
    -h, --help       Show this help
EOF
}

TARGET=""
TAG="v0.0.0-smoke"

require_value() {
    local opt="$1"
    local name="$2"
    local value="${3:-}"
    if [ -z "$value" ]; then
        echo "Error: $opt requires a non-empty $name" >&2
        usage >&2
        exit 1
    fi
}

while [ $# -gt 0 ]; do
    case "$1" in
        --target)
            require_value "$1" TARGET "${2:-}"
            TARGET="${2:-}"
            shift 2
            ;;
        --tag)
            require_value "$1" TAG "${2:-}"
            TAG="${2:-}"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [ -z "$TARGET" ]; then
    TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

ARCHIVE="$(cd "$ROOT" && scripts/package-release.sh --target "$TARGET" --tag "$TAG" --out-dir "$TMPDIR/dist")"

case "$ARCHIVE" in
    *.tar.gz)
        tar tzf "$ARCHIVE" | sort > "$TMPDIR/archive-files"
        mkdir -p "$TMPDIR/archive-marker"
        tar xzf "$ARCHIVE" -C "$TMPDIR/archive-marker" aghist.install
        ;;
    *.zip)
        if command -v unzip >/dev/null 2>&1; then
            unzip -Z1 "$ARCHIVE" | sort > "$TMPDIR/archive-files"
            unzip -p "$ARCHIVE" aghist.install > "$TMPDIR/aghist.install"
        elif command -v 7z >/dev/null 2>&1; then
            7z l -ba "$ARCHIVE" | awk '{ print $NF }' | sort > "$TMPDIR/archive-files"
            7z x -so "$ARCHIVE" aghist.install > "$TMPDIR/aghist.install"
        else
            echo "Error: inspecting zip archives requires unzip or 7z" >&2
            exit 1
        fi
        ;;
    *)
        echo "Error: unsupported archive type: $ARCHIVE" >&2
        exit 1
        ;;
esac

case "$TARGET" in
    *-windows-*) BIN_NAME="aghist.exe" ;;
    *)           BIN_NAME="aghist" ;;
esac

printf '%s\n%s\n' "$BIN_NAME" aghist.install | sort > "$TMPDIR/expected-files"
diff -u "$TMPDIR/expected-files" "$TMPDIR/archive-files"

if [ -f "$TMPDIR/archive-marker/aghist.install" ]; then
    ARCHIVE_MARKER="$TMPDIR/archive-marker/aghist.install"
else
    ARCHIVE_MARKER="$TMPDIR/aghist.install"
fi
check_marker() {
    local marker="$1"
    grep -qx 'method=github-release' "$marker"
    grep -qx "repo=Tien-Lam/agent-history" "$marker"
    grep -qx "target=$TARGET" "$marker"
    grep -qx "tag=$TAG" "$marker"
}

check_marker "$ARCHIVE_MARKER"

INSTALL_DIR="$TMPDIR/bin"
bash "$ROOT/install.sh" --to "$INSTALL_DIR" --tag "$TAG" --archive "$ARCHIVE"
"$INSTALL_DIR/$BIN_NAME" --version

check_marker "$INSTALL_DIR/aghist.install"

printf 'y\n' | AGHIST_INDEX_DIR="$TMPDIR/index" AGHIST_CONFIG="$TMPDIR/config/config.toml" "$INSTALL_DIR/$BIN_NAME" uninstall
test ! -e "$INSTALL_DIR/$BIN_NAME"
test ! -e "$INSTALL_DIR/aghist.install"
