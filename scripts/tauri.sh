#!/usr/bin/env bash
# Optional Unix helper. package.json uses scripts/tauri.mjs (cross-platform; works
# on Windows PowerShell without bash/WSL). Prefer: yarn tauri / yarn tauri:dev
# Ensure Rust/Cargo are on PATH for npm scripts (they often skip .bashrc).
set -euo pipefail

if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

export PATH="${HOME}/.cargo/bin:${PATH}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found."
  echo "Install Rust:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
  echo "Then open a new terminal (or: source \"\$HOME/.cargo/env\") and retry."
  exit 127
fi

if ! command -v pkg-config >/dev/null 2>&1; then
  echo "warning: pkg-config not found — Tauri may fail linking GTK/WebKit."
  echo "On Ubuntu/Debian install:"
  echo "  sudo apt update && sudo apt install -y \\"
  echo "    pkg-config build-essential libwebkit2gtk-4.1-dev libgtk-3-dev \\"
  echo "    libayatana-appindicator3-dev librsvg2-dev libssl-dev libxdo-dev"
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Linux WebKitGTK: avoid broken GPU/dmabuf paths that produce MESA/EGL spam
# and occasionally leave the webview at 1× scale on large monitors.
if [ "$(uname -s)" = "Linux" ]; then
  export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
  # Prefer a compact mouse cursor (large HiDPI cursors look huge in the webview).
  # Override with XCURSOR_SIZE=24 (etc.) if you want bigger.
  export XCURSOR_SIZE="${XCURSOR_SIZE:-16}"
  # Soft fallback when no DRM GPU is available (VMs / remote desktops).
  if [ ! -e /dev/dri/renderD128 ] && [ ! -e /dev/dri/card0 ]; then
    export LIBGL_ALWAYS_SOFTWARE="${LIBGL_ALWAYS_SOFTWARE:-1}"
  fi
fi

# Default to `dev` when npm runs bare `npm run tauri` with extra args via npm_config
# npm run tauri -- dev  → args: dev
# npm run tauri:dev     → we pass dev from package.json
if [ "$#" -eq 0 ]; then
  set -- dev
fi

exec npx tauri "$@"
