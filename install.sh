#!/usr/bin/env bash
# Install the `xeon` package manager from source.
#
# Detects and checks the host architecture, builds a release binary with
# cargo, and installs it. Supports an optional --prefix (default ~/.local).
#
# Usage:
#   ./install.sh [--prefix DIR] [--no-sudo]
set -euo pipefail

#-------------- host detection / arch check --------------#
case "$(uname -s)" in
  Linux)  PLATFORM="linux" ;;
  Darwin) PLATFORM="macos" ;;
  CYGWIN*|MINGW*|MSYS*) PLATFORM="windows" ;;
  *)      PLATFORM="unknown" ;;
esac

case "$(uname -m)" in
  x86_64|amd64)  ARCH="x86_64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *)             ARCH="$(uname -m)" ;;
esac

echo "==> host: ${ARCH}-${PLATFORM}"

case "$ARCH" in
  x86_64|arm64) ;; # supported — continue
  *)
    echo "!! arch '${ARCH}' is not a supported build target" >&2
    echo "   supported arches: x86_64, arm64" >&2
    exit 1
    ;;
esac

#-------------- options --------------#
PREFIX="${PREFIX:-$HOME/.local}"
SUDO=
for arg in "$@"; do
  case "$arg" in
    --prefix=*) PREFIX="${arg#--prefix=}" ;;
    --no-sudo)  SUDO="" ;;
  esac
done

#-------------- locate rust --------------#
if ! command -v cargo >/dev/null 2>&1; then
  echo "!! rust/cargo not found" >&2
  echo "   install it with:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
  exit 1
fi

#-------------- build --------------#
REPO_DIR="$(cd "$(dirname "$0")" && pwd)"
echo "==> building xeon from $REPO_DIR"
( cd "$REPO_DIR" && cargo build --release )

#-------------- install --------------#
BIN_SRC="$REPO_DIR/target/release/xeon"
if [ ! -f "$BIN_SRC" ]; then
  echo "!! build did not produce $BIN_SRC" >&2
  exit 1
fi

mkdir -p "$PREFIX/bin"
if [ -w "$PREFIX/bin" ]; then
  cp "$BIN_SRC" "$PREFIX/bin/xeon"
else
  if [ -n "$SUDO" ] || [ "${EUID:-$(id -u)}" -eq 0 ]; then
    sudo cp "$BIN_SRC" "$PREFIX/bin/xeon" 2>/dev/null \
      || { echo "!! could not write to $PREFIX/bin (no sudo either)" >&2; exit 1; }
  else
    echo "!! no write permission for $PREFIX/bin" >&2
    echo "   rerun as root, or pass --prefix=<dir>:" >&2
    echo "     ./install.sh --prefix=~/.local" >&2
    exit 1
  fi
fi
chmod +x "$PREFIX/bin/xeon"

VER="$("$REPO_DIR/target/release/xeon" --version 2>/dev/null || true)"
echo "==> installed $VER"
echo "   binary: $PREFIX/bin/xeon"
case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *) echo "   note: add $PREFIX/bin to your PATH to use xeon" ;;
esac
