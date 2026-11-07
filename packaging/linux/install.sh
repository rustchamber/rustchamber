#!/bin/sh
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix="${PREFIX:-$HOME/.local}"
if [ -z "$prefix" ]; then
  echo "PREFIX must not be empty." >&2
  exit 1
fi
case "$prefix" in
  /usr | /usr/* | /opt | /opt/*)
    if [ "$(id -u)" -ne 0 ]; then
      echo "$prefix needs root. Run with sudo, or use PREFIX=\$HOME/.local." >&2
      exit 1
    fi
    ;;
esac
bindir="$prefix/bin"
appdir="$prefix/share/applications"
metadir="$prefix/share/metainfo"
icondir="$prefix/share/icons/hicolor"
desktop="$appdir/dev.rustchamber.RustChamber.desktop"
metainfo="$metadir/dev.rustchamber.RustChamber.metainfo.xml"

refresh_caches() {
  command -v update-desktop-database >/dev/null 2>&1 &&
    update-desktop-database "$appdir" >/dev/null 2>&1 || true
  command -v gtk-update-icon-cache >/dev/null 2>&1 &&
    gtk-update-icon-cache -f -t "$icondir" >/dev/null 2>&1 || true
  command -v kbuildsycoca6 >/dev/null 2>&1 &&
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
  command -v kbuildsycoca5 >/dev/null 2>&1 &&
    kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
}

uninstall() {
  rm -f "$bindir/rchamber" "$desktop" "$metainfo"
  for size in 128 256 512; do
    rm -f "$icondir/${size}x${size}/apps/dev.rustchamber.RustChamber.png"
  done
  refresh_caches
  echo "Removed RustChamber from $prefix."
  echo "Settings and project data in ~/.config/rustchamber were kept."
}

case "${1:-}" in
  -u | --uninstall)
    uninstall
    exit 0
    ;;
  -h | --help)
    echo "Usage: ./install.sh [--uninstall]"
    echo "Installs to \$PREFIX (default: \$HOME/.local)."
    exit 0
    ;;
esac

if [ ! -x "$here/rchamber" ]; then
  echo "rchamber is missing from $here; run this script from the extracted tarball." >&2
  exit 1
fi

if ! RCHAMBER_DEPS_SILENT=1 sh "$here/check-deps.sh"; then
  echo "Installation stopped: this system cannot run the prebuilt binary." >&2
  echo "Build from source or use the package for your distribution." >&2
  exit 1
fi

install -Dm755 "$here/rchamber" "$bindir/rchamber"
install -Dm644 "$here/share/applications/dev.rustchamber.RustChamber.desktop" "$desktop"
# The menu must work when ~/.local/bin is not on the session PATH.
escaped_bindir=$(printf '%s' "$bindir" | sed -e 's/[\\/&|]/\\&/g')
sed -i "s|^Exec=rchamber|Exec=\"$escaped_bindir/rchamber\"|" "$desktop"

for size in 128 256 512; do
  install -Dm644 \
    "$here/share/icons/hicolor/${size}x${size}/apps/dev.rustchamber.RustChamber.png" \
    "$icondir/${size}x${size}/apps/dev.rustchamber.RustChamber.png"
done

install -Dm644 \
  "$here/share/metainfo/dev.rustchamber.RustChamber.metainfo.xml" \
  "$metainfo"

refresh_caches

echo "Installed RustChamber to $bindir/rchamber"
case ":$PATH:" in
  *":$bindir:"*) ;;
  *)
    echo "Add $bindir to PATH, or open RustChamber from your application menu."
    ;;
esac
