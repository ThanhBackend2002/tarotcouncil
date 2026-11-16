#!/usr/bin/env bash
# Build + chạy TarotCouncil trong WSL (Ubuntu) với frontend nhúng sẵn từ ../dist.
#
# Dùng khi node_modules được cài từ Windows (binary native của Vite/Tauri CLI là bản win32,
# nên `npm run tauri dev` trong WSL không chạy được). Quy trình:
#   1. Windows (PowerShell):  npm run build          → tạo dist/
#   2. WSL:                   bash scripts/wsl-run.sh
#
# Yêu cầu một lần: sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev \
#   libayatana-appindicator3-dev libgtk-3-dev libxdo-dev build-essential pkg-config
set -euo pipefail

source "$HOME/.cargo/env" 2>/dev/null || true
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ ! -f "$ROOT/dist/index.html" ]]; then
  echo "Chưa có dist/. Chạy 'npm run build' bên Windows trước." >&2
  exit 1
fi

# Target trên ext4 của WSL: build nhanh hơn nhiều so với /mnt/c.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/tarotcouncil-target}"
# WSLg: WebKitGTK hay ra cửa sổ trắng với DMABUF renderer trên GPU ảo.
export WEBKIT_DISABLE_DMABUF_RENDERER=1

cd "$ROOT/src-tauri"
# tauri/custom-protocol = nhúng dist/ vào binary thay vì trỏ tới devUrl (localhost:1420).
cargo run --features tauri/custom-protocol "$@"
