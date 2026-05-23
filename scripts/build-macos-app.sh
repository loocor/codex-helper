#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:-}"
ARCH="${2:-$(uname -m)}"
case "$ARCH" in
  arm64 | aarch64) TARGET="aarch64-apple-darwin" ;;
  x86_64) TARGET="x86_64-apple-darwin" ;;
  *) echo "unsupported architecture: ${ARCH}" >&2; exit 1 ;;
esac
export OUTPUT_DIR="${ROOT}/dist/macos"
export SKIP_NOTARIZE=1
# Always pass VERSION as $1 (may be empty). build-macos-dmg.sh falls back to
# package.json when $1 is empty; omitting $1 would shift TARGET into $1.
exec "${ROOT}/scripts/build-macos-dmg.sh" "${VERSION}" "$TARGET"
