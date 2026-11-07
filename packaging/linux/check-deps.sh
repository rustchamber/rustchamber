#!/bin/sh
# Reports desktop libraries that RustChamber needs but the system is missing.
#
# egui/winit load X11, Wayland, OpenGL and xkbcommon through dlopen, so the
# dynamic linker cannot report these before the first frame. This script reads
# the loader cache instead and prints the install command for the detected
# distribution. It is advisory: the app may still start when an optional
# backend (for example X11 on a Wayland-only machine) is absent.
set -u

libs="libxkbcommon.so.0 libxkbcommon-x11.so.0 libxcb.so.1 libGL.so.1 libEGL.so.1 libfontconfig.so.1 libwayland-client.so.0 libX11.so.6 libXcursor.so.1 libXrandr.so.2 libXi.so.6"

libc="$(ldd --version 2>&1 | head -1)"
case "$libc" in
  *musl* | *Musl*)
    echo "This build is linked against glibc; Alpine and other musl distributions need a source build." >&2
    exit 1
    ;;
esac
glibc="$(printf '%s\n' "$libc" | awk '{print $NF}')"
if [ -n "$glibc" ] && ! printf '%s\n' "$glibc" | awk -F. '{ if ($1 > 2 || ($1 == 2 && $2 >= 31)) exit 0; exit 1 }'; then
  echo "This build needs glibc 2.31 or newer; this system has $glibc." >&2
  echo "Use the package for this distribution or build from source." >&2
  exit 1
fi

if ! command -v ldconfig >/dev/null 2>&1; then
  echo "Could not inspect the loader cache; skipping the desktop library check." >&2
  exit 0
fi

cache="$(ldconfig -p 2>/dev/null || true)"
missing=""
for lib in $libs; do
  case "$cache" in
    *"$lib"*) ;;
    *) missing="$missing $lib" ;;
  esac
done

if [ -z "$missing" ]; then
  if [ "${RCHAMBER_DEPS_SILENT:-0}" != 1 ]; then
    echo "All required desktop libraries are present."
  fi
  exit 0
fi

distro=""
if [ -r /etc/os-release ]; then
  # shellcheck disable=SC1091
  . /etc/os-release
  distro="${ID:-}"
fi

echo "RustChamber is missing these desktop libraries:$missing" >&2
echo >&2
echo "Install them with:" >&2
case "$distro" in
  ubuntu | debian | linuxmint | pop | elementary | zorin | kali)
    echo "  sudo apt install libxkbcommon0 libxkbcommon-x11-0 libxcb1 libgl1 libegl1 libfontconfig1 libwayland-client0 libx11-6 libxcursor1 libxrandr2 libxi6" >&2
    ;;
  fedora | rhel | centos | rocky | almalinux | ol)
    echo "  sudo dnf install libxkbcommon libxkbcommon-x11 libxcb mesa-libGL mesa-libEGL fontconfig libwayland-client libX11 libXcursor libXrandr libXi" >&2
    ;;
  arch | archarm | manjaro | endeavouros | garuda | cachyos)
    echo "  sudo pacman -S --needed libxkbcommon libxkbcommon-x11 libxcb mesa libglvnd fontconfig wayland libx11 libxcursor libxrandr libxi" >&2
    ;;
  opensuse* | suse | sled)
    echo "  sudo zypper install libxkbcommon0 libxkbcommon-x11-0 libxcb1 libGL1 libEGL1 fontconfig libwayland-client0 libX11-6 libXcursor1 libXrandr2 libXi6" >&2
    ;;
  alpine)
    echo "  Alpine uses musl; the prebuilt glibc Linux package is not compatible." >&2
    ;;
  *)
    echo "  Use your distribution package manager to install the X11/Wayland, OpenGL, xkbcommon and fontconfig libraries." >&2
    echo "  See https://github.com/rustchamber/rustchamber#linux" >&2
    ;;
esac

exit 0
