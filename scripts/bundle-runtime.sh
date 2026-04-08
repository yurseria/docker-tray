#!/usr/bin/env bash
# Bundle Colima + Lima binaries into src-tauri/runtime/
# Run before `tauri build` to include runtime in the app bundle.
set -euo pipefail

RUNTIME_DIR="$(cd "$(dirname "$0")/../src-tauri/runtime" && pwd)"
ARCH="$(uname -m)"

info()  { printf '\033[1;34m%s\033[0m\n' "$*"; }
error() { printf '\033[1;31mError: %s\033[0m\n' "$*" >&2; exit 1; }

# Check if Colima and Lima are installed
command -v colima >/dev/null 2>&1 || error "colima not found. Install with: brew install colima"
command -v limactl >/dev/null 2>&1 || error "limactl not found. Should be installed with colima."

COLIMA_BIN="$(readlink -f "$(which colima)")"
LIMACTL_BIN="$(readlink -f "$(which limactl)")"
LIMA_DIR="$(dirname "$(dirname "$LIMACTL_BIN")")"

info "Bundling Colima runtime..."
info "  colima: $COLIMA_BIN"
info "  limactl: $LIMACTL_BIN"
info "  lima dir: $LIMA_DIR"

# Clean and copy
rm -rf "$RUNTIME_DIR/colima" "$RUNTIME_DIR/lima"
mkdir -p "$RUNTIME_DIR/colima/bin" "$RUNTIME_DIR/lima/bin" "$RUNTIME_DIR/lima/share/lima" "$RUNTIME_DIR/lima/libexec"

# Colima binary
cp "$COLIMA_BIN" "$RUNTIME_DIR/colima/bin/colima"

# Lima binaries and data
cp "$LIMACTL_BIN" "$RUNTIME_DIR/lima/bin/limactl"
cp -R "$LIMA_DIR/libexec/"* "$RUNTIME_DIR/lima/libexec/"
cp -R "$LIMA_DIR/share/lima/"* "$RUNTIME_DIR/lima/share/lima/"

# Ensure executables
chmod +x "$RUNTIME_DIR/colima/bin/colima"
chmod +x "$RUNTIME_DIR/lima/bin/limactl"
chmod +x "$RUNTIME_DIR/lima/libexec/"*

TOTAL="$(du -sh "$RUNTIME_DIR" | awk '{print $1}')"
info "Done! Runtime bundled to $RUNTIME_DIR ($TOTAL)"
