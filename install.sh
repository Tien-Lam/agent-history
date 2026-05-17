#!/usr/bin/env bash
set -euo pipefail

REPO="Tien-Lam/agent-history"
BINARY="aghist"
INSTALL_DIR=""
TAG=""
ARCHIVE=""

usage() {
    cat <<EOF
Install aghist from GitHub releases.

Usage: install.sh [OPTIONS]

Options:
    --to DIR       Install directory (default: ~/.local/bin)
    --tag TAG      Install a specific version (default: latest)
    --archive FILE Install from a local release archive instead of GitHub
    -h, --help     Show this help
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --to)
            if [ $# -lt 2 ] || [ -z "$2" ]; then
                echo "Error: --to requires a non-empty DIR"
                usage
                exit 1
            fi
            INSTALL_DIR="$2"
            shift 2
            ;;
        --tag)
            if [ $# -lt 2 ] || [ -z "$2" ]; then
                echo "Error: --tag requires a non-empty TAG"
                usage
                exit 1
            fi
            TAG="$2"
            shift 2
            ;;
        --archive)
            if [ $# -lt 2 ] || [ -z "$2" ]; then
                echo "Error: --archive requires a non-empty FILE"
                usage
                exit 1
            fi
            ARCHIVE="$2"
            shift 2
            ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    MINGW*|MSYS*|CYGWIN*) os="pc-windows-msvc" ;;
    *) echo "Error: unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${arch}-${os}"

# Verify the target has a release artifact
case "$TARGET" in
    x86_64-unknown-linux-gnu|x86_64-pc-windows-msvc|aarch64-apple-darwin) ;;
    *) echo "Error: no prebuilt binary for $TARGET"; echo "Install from source: cargo install --git https://github.com/$REPO"; exit 1 ;;
esac

# Resolve version tag
if [ -z "$TAG" ] && [ -z "$ARCHIVE" ]; then
    TAG="$(curl -sSfI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")"
    TAG="${TAG##*/}"
    if [ -z "$TAG" ]; then
        echo "Error: could not determine latest release"; exit 1
    fi
elif [ -z "$TAG" ]; then
    TAG="local"
fi

# Download and extract
case "$TARGET" in
    *-windows-*) EXT="zip" ;;
    *)           EXT="tar.gz" ;;
esac

URL="https://github.com/$REPO/releases/download/$TAG/$BINARY-$TAG-$TARGET.$EXT"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

if [ -n "$ARCHIVE" ]; then
    if [ ! -r "$ARCHIVE" ]; then
        echo "Error: local archive is not readable: $ARCHIVE"
        exit 1
    fi
    echo "Installing $BINARY $TAG for $TARGET from $ARCHIVE..."
    cp "$ARCHIVE" "$TMPDIR/archive"
else
    echo "Downloading $BINARY $TAG for $TARGET..."

    if ! curl -sSfL "$URL" -o "$TMPDIR/archive"; then
        echo "Error: download failed"
        echo "  URL: $URL"
        echo "  Is $TAG a valid release?"
        exit 1
    fi
fi

case "$EXT" in
    tar.gz) tar xzf "$TMPDIR/archive" -C "$TMPDIR" ;;
    zip)    unzip -qo "$TMPDIR/archive" -d "$TMPDIR" ;;
esac

# Install
case "$TARGET" in
    *-windows-*) BIN_FILE="$BINARY.exe" ;;
    *)           BIN_FILE="$BINARY" ;;
esac

MARKER_TMP="$TMPDIR/$BINARY.install"
cat > "$MARKER_TMP" <<EOF
method=github-release
repo=$REPO
target=$TARGET
tag=$TAG
EOF

if ! mkdir -p "$INSTALL_DIR"; then
    echo "Error: could not create install directory: $INSTALL_DIR"
    echo "Choose a writable directory with --to DIR."
    exit 1
fi
if [ ! -w "$INSTALL_DIR" ]; then
    echo "Error: install directory is not writable: $INSTALL_DIR"
    echo "Choose a user-writable directory with --to DIR, or run the installer with the permissions you intend to own the binary."
    exit 1
fi
cp -f "$TMPDIR/$BIN_FILE" "$INSTALL_DIR/$BIN_FILE"
chmod 0755 "$INSTALL_DIR/$BIN_FILE"
cp -f "$MARKER_TMP" "$INSTALL_DIR/$BINARY.install"
chmod 0644 "$INSTALL_DIR/$BINARY.install"

echo "Installed $BINARY $TAG to $INSTALL_DIR/$BIN_FILE"

# Check PATH
case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
        echo ""
        echo "Warning: $INSTALL_DIR is not in your PATH."
        echo "Add it with: export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac
