#!/usr/bin/env bash
# Generate Tauri updater latest.json from signed macOS archives.
set -euo pipefail

VERSION=""
NOTES_FILE=""
PUB_DATE=""
ASSET_BASE_URL=""
AARCH64_ARCHIVE=""
AARCH64_SIG=""
X86_64_ARCHIVE=""
X86_64_SIG=""
OUTPUT=""

usage() {
  echo "Usage: generate-latest-json.sh --version VERSION --notes-file FILE --pub-date RFC3339 --asset-base-url URL --aarch64-archive FILE --aarch64-sig FILE --x86_64-archive FILE --x86_64-sig FILE --output FILE" >&2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --notes-file) NOTES_FILE="$2"; shift 2 ;;
    --pub-date) PUB_DATE="$2"; shift 2 ;;
    --asset-base-url) ASSET_BASE_URL="$2"; shift 2 ;;
    --aarch64-archive) AARCH64_ARCHIVE="$2"; shift 2 ;;
    --aarch64-sig) AARCH64_SIG="$2"; shift 2 ;;
    --x86_64-archive) X86_64_ARCHIVE="$2"; shift 2 ;;
    --x86_64-sig) X86_64_SIG="$2"; shift 2 ;;
    --output) OUTPUT="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$VERSION" || -z "$NOTES_FILE" || -z "$PUB_DATE" || -z "$ASSET_BASE_URL" || -z "$AARCH64_ARCHIVE" || -z "$AARCH64_SIG" || -z "$X86_64_ARCHIVE" || -z "$X86_64_SIG" || -z "$OUTPUT" ]]; then
  usage
  exit 1
fi

for path in "$NOTES_FILE" "$AARCH64_ARCHIVE" "$AARCH64_SIG" "$X86_64_ARCHIVE" "$X86_64_SIG"; do
  if [[ ! -f "$path" ]]; then
    echo "missing file: $path" >&2
    exit 1
  fi
done

if [[ ! -s "$AARCH64_SIG" || ! -s "$X86_64_SIG" ]]; then
  echo "updater signature files must not be empty" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required to generate latest.json" >&2
  exit 1
fi

aarch64_url="${ASSET_BASE_URL%/}/$(basename "$AARCH64_ARCHIVE")"
x86_64_url="${ASSET_BASE_URL%/}/$(basename "$X86_64_ARCHIVE")"

jq -n \
  --arg version "$VERSION" \
  --rawfile notes "$NOTES_FILE" \
  --arg pub_date "$PUB_DATE" \
  --arg aarch64_url "$aarch64_url" \
  --rawfile aarch64_sig "$AARCH64_SIG" \
  --arg x86_64_url "$x86_64_url" \
  --rawfile x86_64_sig "$X86_64_SIG" \
  '{
    version: $version,
    notes: ($notes | sub("\n$"; "")),
    pub_date: $pub_date,
    platforms: {
      "darwin-aarch64": {
        url: $aarch64_url,
        signature: ($aarch64_sig | gsub("(^\\s+)|(\\s+$)"; ""))
      },
      "darwin-x86_64": {
        url: $x86_64_url,
        signature: ($x86_64_sig | gsub("(^\\s+)|(\\s+$)"; ""))
      }
    }
  }' > "$OUTPUT"

if [[ ! -s "$OUTPUT" ]]; then
  echo "failed to write $OUTPUT" >&2
  exit 1
fi

echo "$OUTPUT"
