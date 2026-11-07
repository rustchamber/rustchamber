#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

VERSION="${VERSION:-$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "rchamber") | .version')}"
TARGET="${TARGET:-}"
if [[ -z "${ARCH:-}" && -n "$TARGET" ]]; then
  case "$TARGET" in
    x86_64-*) ARCH=x86_64 ;;
    aarch64-*) ARCH=aarch64 ;;
  esac
fi
ARCH="${ARCH:-$(uname -m)}"
case "$ARCH" in
  x86_64 | amd64) ARCH=x86_64 ;;
  aarch64 | arm64) ARCH=aarch64 ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

export RCHAMBER_VERSION="$VERSION"
NAME="RustChamber-${VERSION}-linux-${ARCH}"
STAGE="dist/$NAME"
ARCHIVE="dist/$NAME.tar.gz"
BUILD_DIR="target${TARGET:+/$TARGET}/release"

if [[ -n "$TARGET" ]]; then
  cargo build --release -p rchamber --target "$TARGET"
else
  cargo build --release -p rchamber
fi

BIN="$BUILD_DIR/rchamber"
if [[ ! -x "$BIN" ]]; then
  echo "The release build is missing: $BIN" >&2
  exit 1
fi

rm -rf "$STAGE" "$ARCHIVE"
mkdir -p \
  "$STAGE/share/applications" \
  "$STAGE/share/metainfo" \
  "$STAGE/share/icons/hicolor"

install -m 755 "$BIN" "$STAGE/rchamber"
install -m 755 packaging/linux/install.sh "$STAGE/install.sh"
install -m 644 packaging/linux/dev.rustchamber.RustChamber.desktop \
  "$STAGE/share/applications/dev.rustchamber.RustChamber.desktop"
install -m 644 packaging/linux/dev.rustchamber.RustChamber.metainfo.xml \
  "$STAGE/share/metainfo/dev.rustchamber.RustChamber.metainfo.xml"
for size in 128 256 512; do
  install -Dm644 \
    "packaging/linux/icons/hicolor/${size}x${size}/apps/dev.rustchamber.RustChamber.png" \
    "$STAGE/share/icons/hicolor/${size}x${size}/apps/dev.rustchamber.RustChamber.png"
done
install -m 644 README.md LICENSE NOTICE "$STAGE/"

tar -C dist -czf "$ARCHIVE" "$NAME"

echo "$ARCHIVE"
