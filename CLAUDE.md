# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Self-maintenance:** When introducing new patterns, processes, conventions, or style guidelines to this project, update this file to reflect them. CLAUDE.md should always be the authoritative source of how this project works.

## Commands

```bash
yarn tauri dev           # Start full app (Vite on :1420 + Tauri window)
yarn tauri build         # Production build with platform installers
yarn build               # Frontend only (TypeScript + Vite)

# Testing
yarn test                # Run ALL tests (UI + Tauri backend)
yarn test:ui             # Frontend tests only (vitest)
yarn test:ui:watch       # Frontend tests in watch mode
yarn test:tauri          # Rust backend tests only (cargo test)

# Linting & type-checking
yarn lint                # Both of the above
yarn lint:ui             # ESLint + TypeScript type-check (frontend)
yarn lint:tauri          # Clippy with -D warnings (Rust)

# Formatting
yarn format              # Auto-fix both frontend + Rust
yarn format:check        # Check both frontend + Rust
yarn format:ui           # Prettier auto-fix (frontend)
yarn format:ui:check     # Prettier check only (frontend)
yarn format:tauri        # cargo fmt (Rust)
yarn format:tauri:check  # cargo fmt --check (Rust)

# Full CI check (runs locally what CI runs)
yarn ci                  # format:all:check + lint:all + test
```

**Important:** Always run `nvm use` before any yarn/node commands.

## Architecture

**StenoJot** — a Tauri v2 desktop meeting transcription app. React/TypeScript frontend communicates with a Rust backend via Tauri's IPC bridge. See `PLAN.md` for the full product vision and implementation phases.

### Frontend → Backend flow

React hook → `src/lib/commands.ts` wrapper → `invoke()` → Rust `#[tauri::command]` handler in `src-tauri/src/commands.rs` → returns typed result.

### Routing

React Router v7 with routes wrapped in a `Layout` shell. Currently: `/` (RecordingPage), `/meetings` (MeetingsPage), `/settings` (SettingsPage).

### LLM Integration

`src-tauri/src/llm/` module provides trait-based LLM provider abstraction. `LlmClient` trait with implementations for Ollama, Anthropic, and OpenAI. Summary generation runs on a background `std::thread` using `reqwest::blocking`. Chunked summarization for long transcripts (iterative refinement). Background tasks communicate results via Tauri events (`summary-generating`, `summary-generated`, `summary-error`).

## Coding Conventions

### Style System

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

### Testing

**Every new feature, function, or component must include tests.** Tests are not optional — they are part of the definition of done. Run `yarn test` before considering any task complete.

**General rules:**

- All tests follow the **AAA pattern** with explicit `// Arrange`, `// Act`, `// Assert` comments in every test
- **Minimize mocks.** Test real behavior wherever possible. Mocks are acceptable for: external services (network, PulseAudio), Tauri IPC, and cases where real setup complexity far outweighs the benefit
- When a function is untestable in isolation because it's tightly coupled to an external dependency (e.g. `pactl`), extract the pure logic into a separate testable function (see `parse_pactl_sources` pattern in `system_capture.rs`)

**Frontend tests (vitest + @testing-library/react):**

- Config: `vitest.config.ts`, setup: `src/test/setup.ts`
- Test files live next to the code they test: `ComponentName.test.tsx`, `module.test.ts`
- Utility/lib functions (`src/lib/`): test all exported functions with edge cases (zero, negative, boundary values, roundtrip conversions)
- Components (`src/components/`): test rendering, user interaction, prop variants, disabled states. Use `screen` queries and `fireEvent`, not implementation details
- Do not test Tauri `invoke()` wrappers (`src/lib/commands.ts`) — those are thin typed passthroughs

**Rust tests (cargo test, inline `#[cfg(test)]` modules):**

- Tests live in `#[cfg(test)] mod tests` at the bottom of each source file, not in separate test files
- Pure functions (audio pipeline, VAD, text filtering): test directly with constructed inputs
- Functions using shared global state (e.g. `CUSTOM_MODELS_DIR` mutex): use a test-level serialization mutex to prevent parallel test interference (see `manager.rs` pattern)
- File system tests: use `tempfile::tempdir()` for isolation — never touch real user directories
- Audio capture callbacks: test with real `ringbuf` producers/consumers and `Arc<AtomicU32>` — no mocking needed
