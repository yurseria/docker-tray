#!/usr/bin/env bash
# Docker Tray - Quick installer
# Usage: curl -fsSL https://raw.githubusercontent.com/yurseria/docker-tray/main/scripts/install.sh | bash
set -euo pipefail

REPO="yurseria/docker-tray"
APP_NAME="Docker Tray"

# ── Helpers ──

info()  { printf '\033[1;34m%s\033[0m\n' "$*"; }
error() { printf '\033[1;31mError: %s\033[0m\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || error "'$1' is required but not installed."
}

# ── Detect platform ──

detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) OS="macos" ;;
    *)      error "Unsupported OS: $os. $APP_NAME currently supports macOS only." ;;
  esac

  case "$arch" in
    x86_64|amd64)  ARCH="x64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *)             error "Unsupported architecture: $arch" ;;
  esac
}

# ── Fetch latest release info ──

fetch_latest_release() {
  need curl
  need jq

  info "Fetching latest release..."
  RELEASE_JSON="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest")"
  VERSION="$(echo "$RELEASE_JSON" | jq -r '.tag_name')"
  info "Latest version: $VERSION"
}

# ── Find matching asset ──

find_asset() {
  local pattern="\.dmg$"

  # Filter assets by platform pattern and architecture
  ASSET_URL="$(echo "$RELEASE_JSON" | jq -r \
    --arg pat "$pattern" --arg arch "$ARCH" \
    '[.assets[] | select(.name | test($pat)) | select(.name | test($arch; "i"))] | first | .browser_download_url // empty'
  )"

  # Fallback: try without arch filter (single-arch release)
  if [ -z "$ASSET_URL" ]; then
    ASSET_URL="$(echo "$RELEASE_JSON" | jq -r \
      --arg pat "$pattern" \
      '[.assets[] | select(.name | test($pat))] | first | .browser_download_url // empty'
    )"
  fi

  if [ -z "$ASSET_URL" ]; then
    error "No matching asset found for $OS/$ARCH. Please download manually:\n  https://github.com/${REPO}/releases/latest"
  fi

  ASSET_NAME="$(basename "$ASSET_URL")"
  info "Found: $ASSET_NAME"
}

# ── Download & install ──

install_macos() {
  local tmpdir
  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT

  info "Downloading $ASSET_NAME..."
  curl -fSL --progress-bar -o "$tmpdir/$ASSET_NAME" "$ASSET_URL"

  info "Mounting disk image..."
  local mount_point
  mount_point="$(hdiutil attach "$tmpdir/$ASSET_NAME" -nobrowse -noautoopen | tail -1 | awk '{print $NF}')"

  local app
  app="$(find "$mount_point" -maxdepth 1 -name '*.app' | head -1)"
  if [ -z "$app" ]; then
    hdiutil detach "$mount_point" -quiet 2>/dev/null || true
    error "Could not find .app in disk image."
  fi

  info "Installing to /Applications..."
  rm -rf "/Applications/$(basename "$app")"
  cp -R "$app" /Applications/

  hdiutil detach "$mount_point" -quiet 2>/dev/null || true

  # Gatekeeper quarantine 속성 제거
  xattr -rd com.apple.quarantine "/Applications/$(basename "$app")" 2>/dev/null || true

  info "$APP_NAME has been installed to /Applications."
}

# ── Main ──

main() {
  info "$APP_NAME Installer"
  echo

  detect_platform
  fetch_latest_release
  find_asset
  install_macos

  echo
  info "Done! Enjoy $APP_NAME."
}

main
