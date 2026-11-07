#!/bin/sh
# Shared host-environment discovery for Flatpak wrappers.
#
# Inside the sandbox $HOME is the app's private home. The OpenCode CLI, git and
# the user's shell must run on the host with the host home and PATH, otherwise
# they lose credentials, SSH keys and the toolchain the user actually uses.

if ! command -v flatpak-spawn >/dev/null 2>&1; then
  echo "RustChamber: flatpak-spawn is not available in this sandbox." >&2
  exit 127
fi

HOST_HOME="$(flatpak-spawn --host sh -c 'getent passwd "$(id -u)" 2>/dev/null | cut -d: -f6' 2>/dev/null || true)"
if [ -z "$HOST_HOME" ]; then
  HOST_HOME="$HOME"
fi

HOST_PATH="$HOST_HOME/.opencode/bin:$HOST_HOME/.local/bin:/usr/local/bin:/usr/bin:/bin"

HOST_SHELL="$(flatpak-spawn --host sh -c 'getent passwd "$(id -u)" 2>/dev/null | cut -d: -f7' 2>/dev/null || true)"
if [ -z "$HOST_SHELL" ]; then
  HOST_SHELL="/bin/bash"
fi
