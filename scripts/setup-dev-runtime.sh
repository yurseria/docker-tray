#!/usr/bin/env bash
# Local test build: install colima → bundle runtime → tauri build → cleanup
# Mirrors the CI release workflow for local testing.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUNTIME_DIR="$SCRIPT_DIR/../src-tauri/runtime"

info()    { printf '\033[1;34m%s\033[0m\n' "$*"; }
success() { printf '\033[1;32m%s\033[0m\n' "$*"; }

had_colima=false
had_docker=false

cleanup() {
    info "Cleaning up runtime binaries..."
    rm -rf "$RUNTIME_DIR/colima" "$RUNTIME_DIR/lima" "$RUNTIME_DIR/docker"

    if ! $had_colima; then
        info "Removing colima..."
        brew uninstall colima
    fi
    if ! $had_docker; then
        info "Removing docker CLI..."
        brew uninstall docker
    fi
}
trap cleanup EXIT

if brew list colima &>/dev/null; then had_colima=true; else
    info "Installing colima..."
    brew install colima
fi

if brew list docker &>/dev/null; then had_docker=true; else
    info "Installing docker CLI..."
    brew install docker
fi

info "Bundling runtime..."
bash "$SCRIPT_DIR/bundle-runtime.sh"

info "Building app..."
npm run build:tauri

success "Build complete: src-tauri/target/release/bundle/"
