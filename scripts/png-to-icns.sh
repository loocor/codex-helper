#!/usr/bin/env bash
# Build a macOS .icns file from a square PNG (1024x1024 recommended).
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <source.png> <output.icns>" >&2
  exit 1
fi

SOURCE_PNG="$1"
OUTPUT_ICNS="$2"

if [[ ! -f "${SOURCE_PNG}" ]]; then
  echo "Icon source not found: ${SOURCE_PNG}" >&2
  exit 1
fi

ICONSET_DIR="$(mktemp -d).iconset"
cleanup() {
  rm -rf "${ICONSET_DIR}"
}
trap cleanup EXIT
mkdir -p "${ICONSET_DIR}"

make_icon() {
  local size="$1"
  local name="$2"
  sips -z "${size}" "${size}" "${SOURCE_PNG}" --out "${ICONSET_DIR}/${name}" >/dev/null
}

make_icon 16 icon_16x16.png
make_icon 32 icon_16x16@2x.png
make_icon 32 icon_32x32.png
make_icon 64 icon_32x32@2x.png
make_icon 128 icon_128x128.png
make_icon 256 icon_128x128@2x.png
make_icon 256 icon_256x256.png
make_icon 512 icon_256x256@2x.png
make_icon 512 icon_512x512.png
make_icon 1024 icon_512x512@2x.png

if iconutil -c icns "${ICONSET_DIR}" -o "${OUTPUT_ICNS}"; then
  exit 0
fi

echo "iconutil failed; writing PNG-compressed ICNS directly" >&2
python3 - "${ICONSET_DIR}" "${OUTPUT_ICNS}" <<'PY'
import struct
import sys
from pathlib import Path

iconset = Path(sys.argv[1])
output = Path(sys.argv[2])
entries = [
    ("icp4", "icon_16x16.png"),
    ("icp5", "icon_32x32.png"),
    ("icp6", "icon_32x32@2x.png"),
    ("ic07", "icon_128x128.png"),
    ("ic08", "icon_256x256.png"),
    ("ic09", "icon_512x512.png"),
    ("ic10", "icon_512x512@2x.png"),
]

chunks = []
for chunk_type, file_name in entries:
    data = (iconset / file_name).read_bytes()
    chunks.append(chunk_type.encode("ascii") + struct.pack(">I", len(data) + 8) + data)

body = b"".join(chunks)
output.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)
PY
