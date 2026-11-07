#!/usr/bin/env bash
set -euo pipefail

# Builds the Linux release inside a container so the resulting binaries only
# require a conservative glibc. Building on a current distribution (Fedora 44,
# Ubuntu 26.04, Arch) raises the GLIBC_* symbol floor and breaks older systems.
#
# Defaults to Ubuntu 20.04 (glibc 2.31), which covers Ubuntu 20.04+, Debian 11+,
# Fedora 32+, RHEL 9, openSUSE Leap 15.3+ and current Arch. Override the image
# with LINUX_BUILD_IMAGE when a different floor is needed.

cd "$(dirname "$0")/.."

runtime="${CONTAINER_RUNTIME:-}"
if [[ -z "$runtime" ]]; then
  if command -v podman >/dev/null 2>&1; then
    runtime=podman
  elif command -v docker >/dev/null 2>&1; then
    runtime=docker
  else
    echo "podman or docker is required to build the portable Linux packages." >&2
    exit 1
  fi
fi

image="${LINUX_BUILD_IMAGE:-ubuntu:20.04}"
# With rootless podman, container root is already the invoking user, so chown
# to 0:0 inside the container restores the host ownership. Docker is rootful,
# so hand the real host ids to the container.
host_uid="$(id -u)"
host_gid="$(id -g)"
runtime_flags=()
if [[ "$runtime" == podman ]]; then
  host_uid=0
  host_gid=0
  # Keep the checkout's SELinux labels intact.
  runtime_flags+=(--security-opt label=disable)
fi

"$runtime" run --rm \
  "${runtime_flags[@]}" \
  -v "$PWD:/src" \
  -w /src \
  -e HOST_UID="$host_uid" \
  -e HOST_GID="$host_gid" \
  -e VERSION="${VERSION:-}" \
  -e TARGET="${TARGET:-}" \
  -e ARCH="${ARCH:-}" \
  -e PACKAGES=1 \
  -e RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}" \
  "$image" \
  bash /src/scripts/package-linux-in-container.sh
