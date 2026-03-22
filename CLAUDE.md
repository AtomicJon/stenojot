# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Self-maintenance:** When introducing new patterns, processes, conventions, or style guidelines to this project, update this file to reflect them. CLAUDE.md should always be the authoritative source of how this project works.

## Commands

```bash
yarn tauri dev         # Start full app (Vite on :1420 + Tauri window)
yarn tauri build       # Production build with platform installers
yarn build             # Frontend only (TypeScript + Vite)
npx tsc --noEmit       # Type-check without emitting
npx vite build         # Frontend bundle only
cargo check            # Rust type-check (run from src-tauri/)
cargo test             # Rust tests (run from src-tauri/)
```

**Important:** Always run `nvm use` before any yarn/node commands.

## Architecture

**EchoNotes** — a Tauri v2 desktop meeting transcription app. React/TypeScript frontend communicates with a Rust backend via Tauri's IPC bridge. See `PLAN.md` for the full product vision and implementation phases.

### Frontend → Backend flow

React hook → `src/lib/commands.ts` wrapper → `invoke()` → Rust `#[tauri::command]` handler in `src-tauri/src/commands.rs` → returns typed result.

### Routing

React Router v7 with routes wrapped in a `Layout` shell. Currently: `/` (RecordingPage).

## Coding Conventions

### Style System (CRITICAL)

**Goal:** Build a reusable style system that can evolve into a full component library. Every visual property must be driven by shared tokens — no one-off values anywhere. If a new value is needed, add it to the token files first, then reference it.

**Token naming rule:** Numeric token suffixes must match the actual value they represent. `$space-8: 8px`, `$radius-6: 6px`, `$border-1: 1px`. Never use arbitrary sequence numbers like `$space-3: 8px`. This makes tokens self-documenting and instantly legible without looking up definitions. This is a core principle of this project.

**What the tokens cover:**

- `_colors.scss` — Status colors (success/error/warning/neutral), surface/bg/hover shades, border colors, text hierarchy (primary/secondary/muted/inverse/heading), accent, recording theme
- `_sizing.scss` — Spacing scale (`$space-{4..48}`), border widths (`$border-1`), border radii (`$radius-{5,8,10,12,round}`), layout constraints (max-widths, dot size, level bar height)
- `_typography.scss` — Font stacks (`$font-sans`, `$font-mono`), size scale (`$font-{xs..3xl}`), weight scale (`$weight-{normal,semibold,bold}`)

**Usage:** Token files are imported in every SCSS module via `@use "../../styles" as *`. Global base styles (reset only) live in `src/global.scss`.

**Rules:**

- No hardcoded colors, pixel values, font sizes, or border widths in component SCSS or inline styles
- Status colors (recording/success/warning) must be expressed as SCSS classes, not inline `style={{ }}` in TSX
- When adding a new component, all its visual values must come from existing tokens or new tokens added to `src/styles/`
- Plain CSS is not used — all styling is SCSS Modules (`.module.scss`)

### Component Structure

Each component/page lives in its own directory:

```
ComponentName/
├── index.ts                    # Named re-export only
├── ComponentName.tsx           # Component implementation
└── ComponentName.module.scss   # Scoped styles
```

SCSS modules imported as `s` (`import s from "./ComponentName.module.scss"`).

### TypeScript

- No `any` types in command wrappers or component props
- All Tauri `invoke()` calls go through typed wrappers in `src/lib/commands.ts`
- Hooks return typed state; components use named exports
- All exported functions, hooks, components, types, and interfaces must have JSDoc comments (`/** ... */`). Describe what it does, not how. Document parameters and return values for non-obvious signatures. Internal/private helpers don't require JSDoc unless the logic is non-trivial.

### Rust

- All `pub` items (functions, structs, enums, modules) must have `///` doc comments. Use `//!` at the top of each module file to describe the module's purpose and role in the architecture.
- Inline comments (`//`) for non-obvious logic — explain _why_, not _what_. Don't comment self-explanatory code.
