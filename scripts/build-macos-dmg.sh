#!/usr/bin/env bash
# Build CodexHelper.app and a DMG.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAURI="${ROOT}/src-tauri"
VERSION="${1:-$(jq -r '.version' "${ROOT}/package.json")}"
TARGET="${2:-}"
OUTPUT_DIR="${OUTPUT_DIR:-${ROOT}/dist/macos}"
APP_NAME="CodexHelper"
NOTARY_WAIT_TIMEOUT="${NOTARY_WAIT_TIMEOUT:-10m}"
NOTARY_MAX_ATTEMPTS="${NOTARY_MAX_ATTEMPTS:-3}"
NOTARY_RETRY_DELAY_SECONDS="${NOTARY_RETRY_DELAY_SECONDS:-30}"

if [[ -z "$TARGET" ]]; then
  case "$(uname -m)" in
    arm64) TARGET="aarch64-apple-darwin" ;;
    x86_64) TARGET="x86_64-apple-darwin" ;;
    *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
  esac
fi

case "$TARGET" in
  aarch64-apple-darwin) ARCH_SUFFIX="aarch64" ;;
  x86_64-apple-darwin) ARCH_SUFFIX="x86_64" ;;
  *) echo "unsupported target: $TARGET" >&2; exit 1 ;;
esac

PROFILE="${CARGO_PROFILE:-release}"
case "$PROFILE" in
  debug | release) ;;
  *) echo "unsupported CARGO_PROFILE: ${PROFILE}" >&2; exit 1 ;;
esac
DIST="${ROOT}/dist/macos/${TARGET}"
STAGE="${DIST}/stage"
APP="${STAGE}/${APP_NAME}.app"
if [[ "$PROFILE" == "debug" ]]; then
  DMG="${OUTPUT_DIR}/${APP_NAME}-${VERSION}-macos-${ARCH_SUFFIX}-debug.dmg"
else
  DMG="${OUTPUT_DIR}/${APP_NAME}-${VERSION}-macos-${ARCH_SUFFIX}.dmg"
fi
UPDATER_ARCHIVE="${OUTPUT_DIR}/${APP_NAME}-${VERSION}-macos-${ARCH_SUFFIX}.app.tar.gz"
ENTITLEMENTS="${TAURI}/assets/entitlements.plist"
ICON="${TAURI}/icons/icon.png"

require_release_signing() {
  if [[ "${REQUIRE_SIGNING:-}" == "1" && ( -z "${APPLE_SIGNING_IDENTITY:-}" || "${APPLE_SIGNING_IDENTITY}" == "-" ) ]]; then
    echo "APPLE_SIGNING_IDENTITY is required when REQUIRE_SIGNING=1" >&2
    exit 1
  fi
}

has_notary_credentials() {
  [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY_PATH:-}" ]] && return 0
  [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]] && return 0
  return 1
}

require_updater_signing() {
  if [[ "${REQUIRE_UPDATER_SIGNING:-}" != "1" ]]; then
    return
  fi
  if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
    echo "TAURI_SIGNING_PRIVATE_KEY is required when REQUIRE_UPDATER_SIGNING=1" >&2
    exit 1
  fi
}

require_notarization_credentials() {
  if [[ "${SKIP_NOTARIZE:-}" == "1" ]]; then
    return
  fi
  if [[ "${REQUIRE_NOTARIZE:-}" != "1" ]]; then
    return
  fi
  if ! command -v xcrun >/dev/null 2>&1; then
    echo "xcrun is required for notarization; set SKIP_NOTARIZE=1 only for local unsigned builds" >&2
    exit 1
  fi
  if ! has_notary_credentials; then
    echo "notarization credentials are required unless SKIP_NOTARIZE=1" >&2
    exit 1
  fi
}

require_release_signing
require_notarization_credentials
require_updater_signing

if ! [[ "$NOTARY_MAX_ATTEMPTS" =~ ^[1-9][0-9]*$ ]]; then
  echo "NOTARY_MAX_ATTEMPTS must be a positive integer" >&2
  exit 1
fi

submit_notarization() {
  local -a args=("$@")
  local attempt=1
  local status=0

  while (( attempt <= NOTARY_MAX_ATTEMPTS )); do
    echo "Notarization attempt ${attempt}/${NOTARY_MAX_ATTEMPTS} with timeout ${NOTARY_WAIT_TIMEOUT}"
    if xcrun notarytool submit "$DMG" "${args[@]}" --wait --timeout "$NOTARY_WAIT_TIMEOUT"; then
      return 0
    else
      status=$?
    fi
    if (( attempt >= NOTARY_MAX_ATTEMPTS )); then
      echo "Notarization failed after ${NOTARY_MAX_ATTEMPTS} attempts" >&2
      return "$status"
    fi
    echo "Notarization attempt ${attempt} failed; retrying in ${NOTARY_RETRY_DELAY_SECONDS}s" >&2
    sleep "$NOTARY_RETRY_DELAY_SECONDS"
    attempt=$((attempt + 1))
  done
}

mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
DMG="${OUTPUT_DIR}/$(basename "$DMG")"
UPDATER_ARCHIVE="${OUTPUT_DIR}/$(basename "$UPDATER_ARCHIVE")"

echo "build ${TARGET} v${VERSION} (${PROFILE})"
CARGO_ARGS=(--manifest-path "${TAURI}/Cargo.toml" --target "$TARGET" --bin codex-helper)
if [[ "$PROFILE" != "debug" ]]; then
  CARGO_ARGS+=(--release)
fi
RUSTC_WRAPPER="${RUSTC_WRAPPER:-}" cargo build "${CARGO_ARGS[@]}"

rm -rf "$DIST"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources"
cp "${TAURI}/target/${TARGET}/${PROFILE}/codex-helper" "${APP}/Contents/MacOS/codex-helper"
chmod +x "${APP}/Contents/MacOS/codex-helper"
"${ROOT}/scripts/png-to-icns.sh" "$ICON" "${APP}/Contents/Resources/app.icns"

cat > "${APP}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>codex-helper</string>
  <key>CFBundleIconFile</key><string>app</string>
  <key>CFBundleIdentifier</key><string>ai.codexhelper.launcher</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>CodexHelper</string>
  <key>CFBundleDisplayName</key><string>CodexHelper</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
printf 'APPL????' > "${APP}/Contents/PkgInfo"

create_updater_archive() {
  if [[ "${REQUIRE_UPDATER_SIGNING:-}" != "1" && -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
    return
  fi
  if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
    echo "TAURI_SIGNING_PRIVATE_KEY is required to sign updater archives" >&2
    exit 1
  fi
  if ! command -v bun >/dev/null 2>&1; then
    echo "bun is required to sign updater archives" >&2
    exit 1
  fi
  if [[ "${SKIP_NOTARIZE:-}" != "1" ]]; then
    xcrun stapler staple "$APP"
  fi
  tar -C "$STAGE" -czf "$UPDATER_ARCHIVE" "${APP_NAME}.app"
  bunx --bun @tauri-apps/cli signer sign "$UPDATER_ARCHIVE"
}

sign_app() {
  local identity="${APPLE_SIGNING_IDENTITY:--}"
  if [[ "$identity" == "-" ]]; then
    codesign --force --deep --sign - "$APP"
    return
  fi
  local -a args=(--force --options runtime --timestamp --sign "$identity")
  [[ -f "$ENTITLEMENTS" ]] && args+=(--entitlements "$ENTITLEMENTS")
  codesign "${args[@]}" "${APP}/Contents/MacOS/codex-helper"
  codesign "${args[@]}" "$APP"
}

sign_app
rm -f "$DMG"
ln -sfn /Applications "${STAGE}/Applications"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -f "${STAGE}/Applications"

if [[ -n "${APPLE_SIGNING_IDENTITY:-}" && "${APPLE_SIGNING_IDENTITY}" != "-" ]]; then
  codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$DMG"
fi

if [[ "${SKIP_NOTARIZE:-}" != "1" ]]; then
  if [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY_PATH:-}" ]]; then
    submit_notarization --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER"
    xcrun stapler staple "$DMG"
  elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
    submit_notarization --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_PASSWORD"
    xcrun stapler staple "$DMG"
  elif [[ "${REQUIRE_NOTARIZE:-}" == "1" ]]; then
    echo "notarization credentials are required unless SKIP_NOTARIZE=1" >&2
    exit 1
  fi
fi

create_updater_archive

echo "$APP"
echo "$DMG"
if [[ -f "$UPDATER_ARCHIVE" ]]; then
  echo "$UPDATER_ARCHIVE"
fi
