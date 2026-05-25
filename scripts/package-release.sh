#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<EOF
Package an aghist release artifact from an already-built release binary.

Usage: scripts/package-release.sh [OPTIONS]

Options:
    --target TARGET  Rust target triple (default: current host)
    --tag TAG        Release tag written into the install marker (default: v<crate-version>)
    --repo REPO      GitHub repository owner/name (default: Tien-Lam/agent-history)
    --out-dir DIR    Directory for the archive (default: current directory)
    --bin-dir DIR    Directory containing aghist or aghist.exe (default: target/<target>/release)
    -h, --help       Show this help
EOF
}

TARGET=""
TAG=""
REPO="${GITHUB_REPOSITORY:-Tien-Lam/agent-history}"
OUT_DIR="$PWD"
BIN_DIR=""
BIN_DIR_WAS_SET=0

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

validate_artifact_component() {
    local label="$1"
    local value="$2"
    case "$value" in
        ""|*/*|*\\*|*..*|*[!A-Za-z0-9._+-]*)
            echo "Error: $label contains unsafe archive filename characters: $value" >&2
            echo "Allowed characters: ASCII letters, digits, '.', '_', '+', and '-'; no path separators or '..'." >&2
            exit 1
            ;;
    esac
}

validate_repo() {
    local value="$1"
    case "$value" in
        ""|/*|*/|*/*/*|*\\*|*..*|*[!A-Za-z0-9._/-]*)
            echo "Error: --repo must be a safe GitHub owner/name, got: $value" >&2
            echo "Allowed characters: ASCII letters, digits, '.', '_', and '-'; exactly one '/' separator; no '..'." >&2
            exit 1
            ;;
        */*) ;;
        *)
            echo "Error: --repo must be a safe GitHub owner/name, got: $value" >&2
            echo "Allowed characters: ASCII letters, digits, '.', '_', and '-'; exactly one '/' separator; no '..'." >&2
            exit 1
            ;;
    esac
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
        --repo)
            require_value "$1" REPO "${2:-}"
            REPO="${2:-}"
            shift 2
            ;;
        --out-dir)
            require_value "$1" DIR "${2:-}"
            OUT_DIR="${2:-}"
            shift 2
            ;;
        --bin-dir)
            require_value "$1" DIR "${2:-}"
            BIN_DIR="${2:-}"
            BIN_DIR_WAS_SET=1
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
if [ -z "$TAG" ]; then
    version="$(cargo metadata --locked --no-deps --format-version 1 | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')"
    TAG="v$version"
fi
if [ -z "$REPO" ]; then
    echo "Error: --repo must not be empty" >&2
    exit 1
fi
validate_repo "$REPO"
if [ -z "$OUT_DIR" ]; then
    echo "Error: --out-dir must not be empty" >&2
    exit 1
fi
if [ -z "$BIN_DIR" ]; then
    BIN_DIR="target/$TARGET/release"
fi
validate_artifact_component "target" "$TARGET"
validate_artifact_component "tag" "$TAG"

case "$TARGET" in
    *-windows-*) BIN_NAME="aghist.exe"; EXT="zip" ;;
    *)           BIN_NAME="aghist"; EXT="tar.gz" ;;
esac

BIN_PATH="$BIN_DIR/$BIN_NAME"
HOST_TARGET="$(rustc -vV | awk '/^host:/ { print $2 }')"
if [ ! -f "$BIN_PATH" ] && [ "$BIN_DIR_WAS_SET" -eq 0 ] && [ "$TARGET" = "$HOST_TARGET" ] && [ -f "target/release/$BIN_NAME" ]; then
    BIN_DIR="target/release"
    BIN_PATH="$BIN_DIR/$BIN_NAME"
fi
if [ ! -f "$BIN_PATH" ]; then
    echo "Error: release binary not found: $BIN_PATH" >&2
    echo "Build it first, for example: cargo build --release --features self-update --target $TARGET" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

cp "$BIN_PATH" "$WORK_DIR/$BIN_NAME"
cat > "$WORK_DIR/aghist.install" <<EOF
method=github-release
repo=$REPO
target=$TARGET
tag=$TAG
EOF

ARCHIVE="$OUT_DIR/aghist-$TAG-$TARGET.$EXT"
rm -f "$ARCHIVE"

case "$EXT" in
    tar.gz)
        (cd "$WORK_DIR" && tar czf "$ARCHIVE" "$BIN_NAME" aghist.install)
        ;;
    zip)
        if command -v 7z >/dev/null 2>&1; then
            (cd "$WORK_DIR" && 7z a "$ARCHIVE" "$BIN_NAME" aghist.install >/dev/null)
        elif command -v zip >/dev/null 2>&1; then
            (cd "$WORK_DIR" && zip -q "$ARCHIVE" "$BIN_NAME" aghist.install)
        else
            echo "Error: packaging Windows artifacts requires 7z or zip" >&2
            exit 1
        fi
        ;;
esac

printf '%s\n' "$ARCHIVE"
