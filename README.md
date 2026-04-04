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
yarn lint                # Both of the above
yarn lint:tauri          # Clippy with -D warnings (Rust)
yarn lint:ui             # ESLint + TypeScript type-check (frontend)

yarn format              # Auto-fix both frontend + Rust
yarn format:ui           # Prettier auto-fix (frontend)
yarn format:tauri        # cargo fmt (Rust)

yarn format:check        # Check both frontend + Rust
yarn format:ui:check     # Prettier check only (frontend)
yarn format:tauri:check  # cargo fmt --check (Rust)
```

## Full CI Check

Run everything CI runs, locally:

```bash
yarn ci                  # format:all:check + lint:all + test
```
