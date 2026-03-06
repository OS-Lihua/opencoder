#!/bin/bash
set -euo pipefail

# Build opencoder binary for the current or specified target platform.
# Usage: ./scripts/build.sh [--target TARGET] [--release]
#
# Examples:
#   ./scripts/build.sh                              # debug build for current platform
#   ./scripts/build.sh --release                     # release build for current platform
#   ./scripts/build.sh --target aarch64-apple-darwin  # cross-compile

TARGET=""
PROFILE="debug"
CARGO_FLAGS=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --release)
            PROFILE="release"
            CARGO_FLAGS="--release"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

if [ -n "$TARGET" ]; then
    CARGO_FLAGS="$CARGO_FLAGS --target $TARGET"
fi

echo "Building opencoder ($PROFILE)..."
cargo build $CARGO_FLAGS --bin opencoder

# Determine binary path
if [ -n "$TARGET" ]; then
    BIN_PATH="target/$TARGET/$PROFILE/opencoder"
else
    BIN_PATH="target/$PROFILE/opencoder"
fi

if [ ! -f "$BIN_PATH" ]; then
    echo "Error: binary not found at $BIN_PATH"
    exit 1
fi

# For release builds, strip and package
if [ "$PROFILE" = "release" ]; then
    echo "Stripping binary..."
    strip "$BIN_PATH" 2>/dev/null || true

    mkdir -p dist

    # Determine archive name
    OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64) ARCH="x64" ;;
        aarch64|arm64) ARCH="arm64" ;;
    esac

    if [ -n "$TARGET" ]; then
        SUFFIX="${TARGET}"
    else
        SUFFIX="${OS}-${ARCH}"
    fi

    VERSION="$(cargo metadata --format-version 1 --no-deps 2>/dev/null | \
        grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4)"

    ARCHIVE="dist/opencoder-${VERSION}-${SUFFIX}.tar.gz"
    tar -czf "$ARCHIVE" -C "$(dirname "$BIN_PATH")" "$(basename "$BIN_PATH")"
    echo "Package: $ARCHIVE ($(du -h "$ARCHIVE" | cut -f1))"
fi

echo "Done: $BIN_PATH"
