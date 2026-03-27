# StenoJot

A desktop meeting transcription app built with Tauri v2, React, and TypeScript. Captures microphone and system audio, runs local Whisper transcription, and displays a live speaker-attributed transcript.

## Prerequisites

- [Node.js](https://nodejs.org/) (see `.nvmrc` for version) — run `nvm use` before any yarn/node commands
- [Rust](https://www.rust-lang.org/tools/install) toolchain
- System dependencies: `clang`, PulseAudio/PipeWire development libraries

## Getting Started

```bash
nvm use
yarn install
yarn tauri dev       # Start full app (Vite on :1420 + Tauri window)
```

## Commands

```bash
yarn tauri dev           # Start full app (Vite on :1420 + Tauri window)
yarn tauri build         # Production build with platform installers
yarn build               # Frontend only (TypeScript + Vite)
```

## Testing

```bash
yarn test                # Run all tests (UI + Tauri backend)
yarn test:ui             # Frontend tests only (vitest)
yarn test:ui:watch       # Frontend tests in watch mode
yarn test:tauri          # Rust backend tests only (cargo test)
```

## Linting & Formatting

```bash
yarn lint                # ESLint + TypeScript type-check (frontend)
yarn lint:tauri          # Clippy with -D warnings (Rust)
yarn lint:all            # Both of the above

yarn format              # Prettier auto-fix (frontend)
yarn format:tauri        # cargo fmt (Rust)
yarn format:all          # Auto-fix both frontend + Rust

yarn format:check        # Prettier check only (frontend)
yarn format:tauri:check  # cargo fmt --check (Rust)
yarn format:all:check    # Check both frontend + Rust
```

## Full CI Check

Run everything CI runs, locally:

```bash
yarn ci                  # format:all:check + lint:all + test
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
