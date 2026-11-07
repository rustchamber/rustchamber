#!/usr/bin/env bash
set -euo pipefail

# Runs inside the container started by scripts/package-linux-container.sh.
# It is not meant to be executed directly on the host.

export DEBIAN_FRONTEND=noninteractive
export PATH="/root/.cargo/bin:/tmp/nfpm:$PATH"

apt-get update
apt-get install -y --no-install-recommends \
  build-essential \
  ca-certificates \
  curl \
  jq \
  pkg-config \
  xz-utils \
  libxkbcommon-dev \
  libxkbcommon-x11-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  libgl1-mesa-dev \
  libegl1-mesa-dev \
  libfontconfig1-dev \
  libwayland-dev \
  libx11-dev \
  libxcursor-dev \
  libxrandr-dev \
  libxi-dev

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --profile minimal --default-toolchain "${RUSTUP_TOOLCHAIN:-stable}"
fi
if [[ -f /root/.cargo/env ]]; then
  # shellcheck disable=SC1091
  . /root/.cargo/env
fi

case "$(uname -m)" in
  x86_64 | amd64)
    nfpm_arch=x86_64
    nfpm_sha256=6d7f9a28b2f397934403950fcbba83f598b08d02edbe79a0815b3b4897341750
    ;;
  aarch64 | arm64)
    nfpm_arch=arm64
    nfpm_sha256=a9a75213763783b7669719e505474c748c5ad7dc66591a195619deb50f85c2f4
    ;;
  *)
    echo "Unsupported nfpm host architecture: $(uname -m)" >&2
    exit 1
    ;;
esac
NFPM_VERSION="${NFPM_VERSION:-2.48.0}"
NFPM_SHA256="${NFPM_SHA256:-$nfpm_sha256}"
if [[ ! -x /tmp/nfpm/nfpm ]]; then
  curl -fsSL -o /tmp/nfpm.tar.gz \
    "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_${nfpm_arch}.tar.gz"
  echo "${NFPM_SHA256}  /tmp/nfpm.tar.gz" | sha256sum -c -
  mkdir -p /tmp/nfpm
  tar -xzf /tmp/nfpm.tar.gz -C /tmp/nfpm nfpm
fi

cd /src
export CARGO_TARGET_DIR=/tmp/rchamber-target
PACKAGES=1 bash scripts/package-linux.sh

# Refuse to publish a binary newer than the documented glibc floor.
version="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) arch="$(uname -m)" ;;
esac
staged="dist/RustChamber-${version}-linux-${arch}/rchamber"
highest="$(objdump -T "$staged" | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1 | cut -d_ -f2)"
echo "Highest glibc symbol: $highest"
if ! printf '%s\n' "$highest" | awk -F. '{ if ($1 > 2 || ($1 == 2 && $2 > 31)) exit 1 }'; then
  echo "The Linux build needs glibc $highest, above the 2.31 floor." >&2
  exit 1
fi

# The container runs as root; hand the published artifacts back to the user.
if [[ -n "${HOST_UID:-}" && -n "${HOST_GID:-}" ]]; then
  chown -R "${HOST_UID}:${HOST_GID}" dist
fi
