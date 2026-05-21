#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-0.1.0}"
ARCH="${2:-$(uname -m)}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="CodexHelper"
EXECUTABLE_NAME="codex-helper"
DIST_DIR="${ROOT_DIR}/dist/macos"
STAGE_DIR="${DIST_DIR}/stage"
APP_DIR="${STAGE_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
CODEX_APP_PATH="${CODEX_APP_PATH:-/Applications/Codex.app}"
CODEX_ICON_SOURCE="${CODEX_APP_PATH}/Contents/Resources/icon.icns"
ICON_NAME="codex.icns"
DMG_PATH="${DIST_DIR}/${APP_NAME}-${VERSION}-macos-${ARCH}.dmg"

cd "${ROOT_DIR}"

if [[ ! -f "${CODEX_ICON_SOURCE}" ]]; then
  echo "Codex icon not found: ${CODEX_ICON_SOURCE}" >&2
  exit 1
fi

RUSTC_WRAPPER="${RUSTC_WRAPPER:-}" cargo build --manifest-path src-tauri/Cargo.toml --release

rm -rf "${DIST_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

cp "${ROOT_DIR}/src-tauri/target/release/${EXECUTABLE_NAME}" "${MACOS_DIR}/${EXECUTABLE_NAME}"
cp "${CODEX_ICON_SOURCE}" "${RESOURCES_DIR}/${ICON_NAME}"
chmod +x "${MACOS_DIR}/${EXECUTABLE_NAME}"

cat > "${CONTENTS_DIR}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>codex-helper</string>
  <key>CFBundleIconFile</key>
  <string>codex.icns</string>
  <key>CFBundleIdentifier</key>
  <string>ai.codexhelper.launcher</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>CodexHelper</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

printf 'APPL????' > "${CONTENTS_DIR}/PkgInfo"

codesign --force --deep --sign - "${APP_DIR}" >/dev/null
ln -s /Applications "${STAGE_DIR}/Applications"
hdiutil create -volname "${APP_NAME}" -srcfolder "${STAGE_DIR}" -ov -format UDZO "${DMG_PATH}" >/dev/null

echo "${APP_DIR}"
echo "${DMG_PATH}"
