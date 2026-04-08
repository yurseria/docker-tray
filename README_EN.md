# Docker Tray

<p align="center">
  <img src="src-tauri/icons/icon.png" width="128" />
</p>

<p align="center">
  A lightweight macOS menubar app for managing Docker containers, images, volumes, and networks.
  <br/>
  Built with Tauri + React + TypeScript.
  <br/>
  <br/>
  <a href="https://www.apple.com/macos/"><img src="https://img.shields.io/badge/macOS-13+-000000?style=for-the-badge&logo=apple&logoColor=white" alt="macOS"></a>
  <a href="https://www.docker.com/"><img src="https://img.shields.io/badge/Docker-Required-2496ED?style=for-the-badge&logo=docker&logoColor=white" alt="Docker"></a>
  <a href="https://nodejs.org/"><img src="https://img.shields.io/badge/Node.js-v22.x-339933?style=for-the-badge&logo=node.js&logoColor=white" alt="Node.js"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.77+-DEA584?style=for-the-badge&logo=rust&logoColor=white" alt="Rust"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="License"></a>
  <br/>
  <br/>
  <a href="./README.md">한국어</a> / English
</p>

<p align="center">
  <img src="assets/screenshot.png" width="420" />
</p>

## Features

- **System Tray**: Lives in your menubar, click to toggle
- **Container Management**: Start, stop, restart, remove containers
- **Compose Support**: Import and run `docker-compose.yaml` files
- **Image Management**: Pull, create containers from, and remove images
- **Volume & Network**: Browse and remove volumes and networks
- **Logs Viewer**: Open container logs in a separate window
- **File Explorer**: Browse and transfer files inside containers
- **Terminal Access**: Open a shell into running containers (supports Ghostty, iTerm, Terminal.app)
- **Detail View**: Click any item to see basic info, right-click to delete
- **Resizable Window**: Drag the bottom edge to adjust height

## Install

### Script install (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/yurseria/docker-tray/main/scripts/install.sh | bash
```

Automatically removes Gatekeeper quarantine attributes.

### Manual install

If you download the `.dmg` from [Releases](https://github.com/yurseria/docker-tray/releases/latest), the app is unsigned. After installation, run:

```bash
xattr -rd com.apple.quarantine /Applications/Docker\ Tray.app
```

## Tech Stack

- **Frontend**: React 19, TypeScript, Vite
- **Backend**: Rust, Tauri 2, Bollard (Docker API)
- **Node**: 22 (see `.nvmrc`)

## Prerequisites

- [Rust](https://rustup.rs/)
- [Node.js 22+](https://nodejs.org/)
- [Docker Desktop](https://www.docker.com/products/docker-desktop/)

## Development

```bash
npm install
npm run dev:tauri
```

## Build

```bash
npm run tauri build
```

The built app will be in `src-tauri/target/release/bundle/`.

## Project Structure

```
├── src/                    # React frontend
│   ├── components/         # UI components
│   ├── hooks/              # useDocker hook
│   └── types.ts            # TypeScript types
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── docker.rs       # Docker API commands
│   │   └── lib.rs          # Tauri app setup, tray, windows
│   └── tauri.conf.json     # Tauri config
└── vite.config.ts
```

## License

MIT
