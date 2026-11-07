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
  x86_64 | amd64)
    ARCH=x86_64
    NFPM_ARCH=amd64
    ;;
  aarch64 | arm64)
    ARCH=aarch64
    NFPM_ARCH=arm64
    ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

export RCHAMBER_VERSION="$VERSION"
NAME="RustChamber-${VERSION}-linux-${ARCH}"
STAGE="dist/$NAME"
ARCHIVE="dist/$NAME.tar.gz"
BUILD_DIR="${CARGO_TARGET_DIR:-target}${TARGET:+/$TARGET}/release"
PACKAGES="${PACKAGES:-auto}"
BUILD="${BUILD:-1}"

find_nfpm() {
  if [[ -n "${NFPM:-}" ]]; then
    if [[ -x "$NFPM" ]]; then
      printf '%s\n' "$NFPM"
      return 0
    fi
    echo "NFPM points to a file that is not executable: $NFPM" >&2
    return 1
  fi
  if command -v nfpm >/dev/null 2>&1; then
    command -v nfpm
    return 0
  fi
  return 1
}

sed_escape() {
  printf '%s' "$1" | sed -e 's/[\\/&|]/\\&/g'
}

render_nfpm() {
  local version="$1"
  local output="$2"
  local schema="${3:-}"
  sed \
    -e "s|\$[{]VERSION[}]|$(sed_escape "$version")|g" \
    -e "s|\$[{]NFPM_ARCH[}]|$(sed_escape "$NFPM_ARCH")|g" \
    -e "s|\$[{]STAGE[}]|$(sed_escape "$PWD/$STAGE")|g" \
    -e "s|\$[{]ROOT[}]|$(sed_escape "$(pwd)")|g" \
    packaging/linux/nfpm.yaml >"$output"
  if [[ -n "$schema" ]]; then
    printf '\nversion_schema: %s\n' "$schema" >>"$output"
  fi
}

if [[ "$BUILD" != 0 ]]; then
  if [[ -n "$TARGET" ]]; then
    cargo build --release -p rchamber --target "$TARGET"
  else
    cargo build --release -p rchamber
  fi
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
install -m 755 packaging/linux/check-deps.sh "$STAGE/check-deps.sh"
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

packages=()
if [[ "$PACKAGES" != 0 && "$PACKAGES" != false ]]; then
  if nfpm_bin="$(find_nfpm)"; then
    rendered="dist/nfpm.yaml"
    arch_rendered="dist/nfpm-arch.yaml"
    render_nfpm "$VERSION" "$rendered"
    # Arch pkgver cannot contain a hyphen. Keep the prerelease suffix instead
    # of letting nfpm silently drop a prerelease suffix.
    render_nfpm "${VERSION//-/}" "$arch_rendered" none

    for packager in deb rpm archlinux; do
      case "$packager" in
        deb) extension=deb ;;
        rpm) extension=rpm ;;
        archlinux) extension=pkg.tar.zst ;;
      esac
      config="$rendered"
      if [[ "$packager" == archlinux ]]; then
        config="$arch_rendered"
      fi
      output="dist/${NAME}.${extension}"
      rm -f "$output"
      "$nfpm_bin" package -f "$config" -p "$packager" -t "$output" >/dev/null
      packages+=("$output")
      echo "$output"
    done
    rm -f "$rendered" "$arch_rendered"
  else
    if [[ "$PACKAGES" == 1 || "$PACKAGES" == true || "$PACKAGES" == required ]]; then
      echo "nfpm is required to build the deb, rpm and Arch packages." >&2
      echo "Install nfpm (https://nfpm.goreleaser.com) or set NFPM=/path/to/nfpm." >&2
      exit 1
    fi
    echo "nfpm not found; skipped deb, rpm and Arch packages." >&2
  fi
fi

if command -v sha256sum >/dev/null 2>&1; then
  checksums="dist/${NAME}.sha256"
  (
    cd dist
    files=("$(basename "$ARCHIVE")")
    for package in ${packages[@]+"${packages[@]}"}; do
      files+=("$(basename "$package")")
    done
    sha256sum "${files[@]}" >"$(basename "$checksums")"
  )
  echo "$checksums"
fi
