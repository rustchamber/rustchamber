#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

VERSION="${VERSION:-$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "rchamber") | .version')}"
BUILD="${BUILD:-1}"
ARM_TARGET="aarch64-apple-darwin"
INTEL_TARGET="x86_64-apple-darwin"
APP="dist/RustChamber.app"
DMG="dist/RustChamber-${VERSION}-macos-universal.dmg"
ZIP="dist/RustChamber-${VERSION}-macos-universal.zip"

rustup target add "$ARM_TARGET" "$INTEL_TARGET"
cargo build --release -p rchamber --target "$ARM_TARGET"
cargo build --release -p rchamber --target "$INTEL_TARGET"

rm -rf "$APP" "$DMG" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

lipo -create \
  "target/$ARM_TARGET/release/rchamber" \
  "target/$INTEL_TARGET/release/rchamber" \
  -output "$APP/Contents/MacOS/rchamber"

cp packaging/macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
sed -e "s/__VERSION__/$VERSION/g" -e "s/__BUILD__/$BUILD/g" \
  packaging/macos/Info.plist > "$APP/Contents/Info.plist"
chmod +x "$APP/Contents/MacOS/rchamber"

if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$APP"
else
  codesign --force --sign - "$APP"
fi

if [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  NOTARIZE_ZIP="$(mktemp -d)/RustChamber.zip"
  ditto -c -k --keepParent "$APP" "$NOTARIZE_ZIP"
  xcrun notarytool submit "$NOTARIZE_ZIP" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait
  xcrun stapler staple "$APP"
  rm -rf "$(dirname "$NOTARIZE_ZIP")"
fi

ditto -c -k --keepParent "$APP" "$ZIP"

STAGING="$(mktemp -d)"
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"
hdiutil create -volname "RustChamber" -srcfolder "$STAGING" -ov -format UDZO "$DMG"
rm -rf "$STAGING"

if [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  xcrun notarytool submit "$DMG" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait
  xcrun stapler staple "$DMG"
fi

echo "$DMG"
echo "$ZIP"
