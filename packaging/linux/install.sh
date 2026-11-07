#!/bin/sh
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix="${PREFIX:-$HOME/.local}"
bindir="$prefix/bin"
appdir="$prefix/share/applications"
metadir="$prefix/share/metainfo"
icondir="$prefix/share/icons/hicolor"

install -Dm755 "$here/rchamber" "$bindir/rchamber"
install -Dm644 \
  "$here/share/applications/dev.rustchamber.RustChamber.desktop" \
  "$appdir/dev.rustchamber.RustChamber.desktop"
sed -i "s|^Exec=rchamber|Exec=\"$bindir/rchamber\"|" \
  "$appdir/dev.rustchamber.RustChamber.desktop"

for size in 128 256 512; do
  install -Dm644 \
    "$here/share/icons/hicolor/${size}x${size}/apps/dev.rustchamber.RustChamber.png" \
    "$icondir/${size}x${size}/apps/dev.rustchamber.RustChamber.png"
done

install -Dm644 \
  "$here/share/metainfo/dev.rustchamber.RustChamber.metainfo.xml" \
  "$metadir/dev.rustchamber.RustChamber.metainfo.xml"

command -v update-desktop-database >/dev/null 2>&1 &&
  update-desktop-database "$appdir" >/dev/null 2>&1 || true
command -v gtk-update-icon-cache >/dev/null 2>&1 &&
  gtk-update-icon-cache -f -t "$icondir" >/dev/null 2>&1 || true

echo "Installed RustChamber to $bindir/rchamber"
case ":$PATH:" in
  *":$bindir:"*) ;;
  *)
    echo "Add $bindir to PATH, or open RustChamber from your application menu."
    ;;
esac
