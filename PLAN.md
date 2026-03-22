# EchoNotes — Architecture & Implementation Plan

A desktop meeting transcription app inspired by Granola.ai, built with Tauri v2 (Rust backend + React/TypeScript frontend).

## Product Vision

EchoNotes captures system audio (remote participants) and microphone input (local user) during meetings, transcribes them in real-time, and provides an AI-enhanced notepad experience. Unlike bot-based tools, EchoNotes never joins the call — it captures audio directly from the device.

### Core Principles
- **No meeting bot** — captures audio natively, invisible to other participants
- **Privacy-first** — local transcription option, no mandatory cloud dependency
- **Notepad-first UX** — a familiar editor with AI enhancement, not a meeting management tool
- **Platform-agnostic** — works with any meeting platform (Zoom, Meet, Teams, etc.)
- **Markdown-native output** — all outputs are `.md` files, designed for Obsidian sync

### Meeting Output Requirements

Every meeting produces **two Markdown files**:

1. **Meeting Summary** (`summary.md`) — AI-generated, structured:
   - **Key Points Discussed** — substantive topics and decisions, filtering out small talk and filler
   - **Action Items** — extracted commitments with assignee where identifiable
   - Ignores irrelevant chit-chat, greetings, "can you hear me" moments, etc.

2. **Full Transcript** (`transcript.md`) — complete, timestamped, speaker-labeled:
   - Every utterance with `[HH:MM:SS]` timestamps and `Me` / `Others` labels
   - Unfiltered — the full record for reference when the summary isn't enough

**Output format: Markdown files** — not JSON, not SQLite. Both files are plain `.md` so they can be:
- Opened and read in any text editor
- Synced to an **Obsidian vault** (future feature: configurable output directory + vault sync)
- Version-controlled, grep-able, portable

Example output:
```
~/EchoNotes/
  2026-03-21 14.00 Sprint Planning.md
  2026-03-21 14.00 Sprint Planning - Transcript.md
  2026-03-21 15.30 1-on-1 with Alex.md
  2026-03-21 15.30 1-on-1 with Alex - Transcript.md
```

**Meeting name resolution** (priority order):
1. Calendar event title (when calendar integration is available)
2. Meeting platform window title (Zoom/Slack/Teams often include the meeting name)
3. LLM-generated title from transcript topics
4. Fallback: `Meeting at HH-MM`

---

## Feature Tiers

### Tier 1 — MVP (Walking Skeleton)
1. Microphone audio capture
2. System audio capture (loopback)
3. Real-time transcription (local via whisper-rs)
4. Live transcript display in the UI
5. Basic start/stop recording controls
6. Speaker labeling: "Me" (mic) vs "Others" (system audio)
7. Full transcript saved to Markdown file (timestamped, speaker-labeled)

### Tier 2 — Usable Product
8. Post-meeting AI summary generation via LLM (key points + action items, ignoring small talk)
9. Summary saved to Markdown file alongside transcript
10. Notepad editor for user notes during meetings
11. Meeting list/browser view (reads from output directory)
12. Configurable output directory (for Obsidian vault targeting)
13. Auto-stop after configurable silence duration

### Tier 3 — Polish & Power Features
14. Obsidian vault sync integration (configurable vault path, wikilinks, tags)
15. Calendar integration (Google Calendar) for auto-detection & meeting titles
16. Note templates (1:1, standup, retro, etc.) that shape the AI summary structure
17. "Ask EchoNotes" post-meeting chat over transcript
18. Cloud transcription option (Deepgram/AssemblyAI) for higher accuracy
19. Multi-speaker diarization within system audio
20. Custom vocabulary / jargon support

---

## Technical Architecture

### High-Level Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  Audio Layer (Rust, background threads)                         │
│                                                                 │
│  [Mic Input]              [System Audio Loopback]               │
│   (cpal)                   (platform-specific)                  │
│      │                            │                             │
│      ▼                            ▼                             │
│  [Ring Buffer]              [Ring Buffer]                       │
│      │                            │                             │
│      ▼                            ▼                             │
│  [Resample → 16kHz mono]   [Resample → 16kHz mono]             │
│      │                            │                             │
│      ▼                            ▼                             │
│  [VAD]                      [VAD]                               │
│      │                            │                             │
│      └──────────┬─────────────────┘                             │
│                 ▼                                                │
│         [Transcription Engine]                                  │
│          (whisper-rs / cloud)                                   │
│                 │                                                │
│                 ▼                                                │
│         [Tauri IPC Channel]                                     │
└─────────────────┬───────────────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────────────┐
│  Frontend (React/TypeScript)                                    │
│                                                                 │
│  [Transcript Panel]  [Notepad Editor]  [Controls]               │
└─────────────────────────────────────────────────────────────────┘
```

### System Audio Capture — Platform Strategy

There is no single cross-platform crate for system audio loopback. We need platform-specific implementations behind a unified Rust trait.

| Platform | Crate | Mechanism | Notes |
|----------|-------|-----------|-------|
| **Linux** | `pipewire` (0.9.x) | PipeWire monitor source | Default audio server on modern distros (Arch, Fedora, Ubuntu 22.04+). Fallback: `libpulse-binding` for PulseAudio |
| **Windows** | `wasapi` (0.19.x) | WASAPI loopback capture | Mature, well-documented. Uses `AUDCLNT_STREAMFLAGS_LOOPBACK` |
| **macOS** | `screencapturekit` (1.5.x) | ScreenCaptureKit framework | Requires macOS 12.3+. Needs Screen Recording permission |

**Decision: Start with Linux only** (developer's platform), add Windows and macOS behind the same trait interface. This lets us build the full pipeline on one platform first.

**Unified trait design:**
```rust
pub trait AudioCapture: Send + 'static {
    fn start(&mut self, sender: ringbuf::Producer<f32>) -> Result<(), CaptureError>;
    fn stop(&mut self) -> Result<(), CaptureError>;
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
}
```

~250-300 lines of platform-specific code per OS, unified behind this trait.

### Microphone Capture

**Chosen: `cpal` (0.15.x)**

The standard cross-platform audio input library for Rust. Supports WASAPI (Windows), CoreAudio (macOS), ALSA/PulseAudio/PipeWire (Linux). No platform-specific code needed for mic input.

### Audio Processing Pipeline

| Component | Crate | Purpose |
|-----------|-------|---------|
| Ring buffers | `ringbuf` (0.4.x) | Lock-free SPSC buffers between audio callbacks and processing threads |
| Resampling | `rubato` (0.14.x) | Convert from native device sample rate (44.1/48kHz) → 16kHz for Whisper |
| VAD | Energy-based (custom) | RMS threshold per chunk (~1024 samples). Simple, no dependencies. Upgrade to `silero-vad` later if needed |

**Audio format requirements for Whisper:** 16kHz, mono, f32 (or i16 PCM).

**Buffering strategy:**
- Accumulate 5-10 second segments with VAD-based boundaries
- Pre-speech buffer of ~250ms to avoid cutting off word beginnings
- Overlap segments slightly for continuity

### Transcription Engine

#### Option Analysis

| Option | Latency | Accuracy | Privacy | Cost | Offline | Build Complexity |
|--------|---------|----------|---------|------|---------|-----------------|
| **whisper-rs (local)** | ~1-3s/segment | Good (base/small) to Excellent (large) | Full privacy | Free | Yes | Medium (C++ build) |
| **Deepgram (cloud)** | ~200-500ms | Excellent | Audio sent to cloud | ~$0.25-0.65/hr | No | Low (WebSocket) |
| **AssemblyAI (cloud)** | ~200-500ms | Excellent | Audio sent to cloud | ~$0.37-0.65/hr | No | Low (WebSocket) |
| **Vosk (local)** | Low | Moderate | Full privacy | Free | Yes | Medium (dynamic lib) |

**Decision: whisper-rs for MVP.** Reasons:
1. **Privacy** — audio never leaves the device, critical for meeting recordings
2. **Offline capability** — works without internet
3. **No API costs** — important during development and for users
4. **Accuracy** — Whisper `base` model (140MB) is adequate for MVP; `small` (460MB) for better quality
5. **Ecosystem** — most popular Rust transcription crate, well-maintained

Cloud transcription (Deepgram) will be added in Tier 3 as an optional backend for users who want better real-time accuracy.

**Model selection:**
- Default: `whisper-base` (140MB download, ~1-3s processing per 10s segment on modern CPU)
- Optional: `whisper-small` (460MB, better accuracy, needs more CPU)
- Future: GPU acceleration via CUDA/Metal for larger models

### Speaker Identification

**MVP approach: Channel-based diarization (free)**

Since mic and system audio are captured as separate streams, we get two-party labeling automatically:
- Mic stream → "Me"
- System audio stream → "Others"

This is exactly how Granola handles desktop diarization. No ML models needed.

**Future: Multi-speaker diarization** within system audio via `native-pyannote-rs` (pure Rust, uses Burn framework).

### Tauri IPC Pattern

**Chosen: Tauri Channels (`tauri::ipc::Channel<T>`)** for streaming transcript data.

Channels provide ordered delivery, strong typing via Serde, and significantly better throughput than the event system. Events will be used only for low-frequency state changes (recording started/stopped).

```rust
#[derive(Clone, Serialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub speaker: Speaker, // Me | Others
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_final: bool,
}

#[tauri::command]
async fn start_recording(
    app: AppHandle,
    state: State<'_, Mutex<AudioState>>,
    on_transcript: Channel<TranscriptSegment>,
) -> Result<(), String> { ... }
```

### Data Persistence — Markdown-First

**All meeting output is plain Markdown files.** This is a core design decision — outputs must be human-readable, portable, and compatible with Obsidian.

Each meeting produces two files, named with an ISO 8601 date prefix and a meeting-specific name:
```
~/EchoNotes/                                          # Configurable output directory
  2026-03-21 14.00 Sprint Planning.md                       # AI-generated summary
  2026-03-21 14.00 Sprint Planning - Transcript.md          # Full transcript
  2026-03-21 15.30 1-on-1 with Alex.md
  2026-03-21 15.30 1-on-1 with Alex - Transcript.md
```

**File naming rules:**
- Format: `YYYY-MM-DD HH.MM <Meeting Name>.md` (summary) and `YYYY-MM-DD HH.MM <Meeting Name> - Transcript.md`
- Time uses `.` separator (not `:`) for filesystem compatibility, 24-hour format
- Meeting name sourced from (in priority order):
  1. Calendar event title (if calendar integration is available)
  2. Meeting platform window title (Zoom, Slack, Teams often include the meeting name)
  3. LLM-generated title based on transcript topics discussed
  4. Fallback: `Meeting`
- Sanitized for filesystem safety (no `/`, `\`, `:`, etc.)

**Summary file structure** (`2026-03-21 14.00 Sprint Planning.md`):
```markdown
# Sprint Planning

**Date:** 2026-03-21 14:00–14:45

## Key Points
- Decided to postpone the auth migration to next sprint
- Backend team will own the new caching layer
- QA flagged 3 regressions in the payment flow

## Action Items
- [ ] @Alex: File tickets for payment regressions by EOD Friday
- [ ] @Me: Draft RFC for caching layer by Monday
- [ ] @Jordan: Schedule follow-up with design for the onboarding flow
```

**Transcript file structure** (`2026-03-21 14.00 Sprint Planning - Transcript.md`):
```markdown
# Sprint Planning — Full Transcript

**Date:** 2026-03-21 14:00–14:45
**Participants:** Me, Others

---

[00:00:12] **Others:** Alright, let's get started. First item is the auth migration.
[00:00:18] **Me:** Yeah, I think we should push that to next sprint. We're still blocked on the dependency upgrade.
[00:00:25] **Others:** Makes sense. Let's move on to the caching discussion.
...
```

**Why not JSON/SQLite?** Markdown files are directly openable, grep-able, and sync naturally to Obsidian vaults. Internal metadata (for the meeting list UI, search, etc.) can use a lightweight `index.json` in the output root, regenerated from the `.md` files.

**Future Obsidian integration:** Configurable output path pointing at an Obsidian vault. Add YAML frontmatter, wikilinks, and tags to make notes first-class Obsidian citizens.

---

## Rust Crate Dependencies (Planned)

```toml
# Audio capture (Phase 1 — installed)
cpal = "0.15"              # Mic + system audio capture (cross-platform, user selects monitor source)
ringbuf = "0.4"            # Lock-free SPSC ring buffers
rubato = "0.15"            # Sample rate conversion (→ 16kHz for Whisper)
tokio = { version = "1", features = ["rt-multi-thread", "sync", "macros"] }

# Transcription (Phase 2 — not yet installed)
# whisper-rs = "0.16"      # Local Whisper transcription

# Platform-specific system audio (Phase 5 — not yet installed)
# pipewire = "0.9"         # Automatic monitor source detection (Linux)
# wasapi = "0.19"          # WASAPI loopback capture (Windows)
# screencapturekit = "1.5" # ScreenCaptureKit (macOS)

# Already present from boilerplate
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
```

---

## Implementation Phases

### Phase 1: Audio Capture Foundation ✅
**Complexity: Medium | Focus: Getting audio flowing**

- [x] **1.1** Audio capture via cpal with device enumeration (used cpal for both mic and system audio instead of PipeWire-specific code — on Linux with PipeWire, monitor sources appear as cpal input devices)
- [x] **1.2** Implement microphone capture via cpal
- [x] **1.3** Implement ring buffer pipeline with resampling (→ 16kHz mono via rubato)
- [x] **1.4** Add basic VAD (energy-based RMS threshold)
- [x] **1.5** Wire up Tauri commands: `start_recording`, `stop_recording`, `get_audio_devices`, `get_audio_levels`
- [x] **1.6** Basic frontend: device selector dropdowns, start/stop button, elapsed timer, animated audio level meters, transcript placeholder

**Implementation notes:**
- Used cpal for both mic and system audio (user selects monitor source from device list) rather than PipeWire-specific code. Simpler and still cross-platform. Platform-specific backends (PipeWire, WASAPI, ScreenCaptureKit) can be added later behind the `AudioCapture` trait if automatic monitor source detection is needed.
- RMS levels shared between audio callback threads and UI via `Arc<AtomicU32>` (storing f32 bits).
- Ring buffers drain on each `get_audio_levels` poll to prevent overflow until Phase 2 wires up the transcription consumer.
- `cpal::Stream` is `!Send`; `AppState` uses `unsafe impl Send + Sync` with all access gated behind a `Mutex`.

### Phase 2: Transcription Pipeline
**Complexity: Medium-High | Focus: Whisper integration**

- [ ] **2.1** Integrate whisper-rs, handle model download/management
- [ ] **2.2** Build segment accumulator (VAD-bounded chunks → Whisper)
- [ ] **2.3** Implement dual-stream transcription (mic and system audio independently)
- [ ] **2.4** Stream transcript segments to frontend via Tauri Channel
- [ ] **2.5** Frontend: live transcript display with speaker labels ("Me" / "Others")
- [ ] **2.6** Handle Whisper model selection (base/small) in settings

**Key risk:** Whisper processing speed on CPU. If too slow for real-time, options: (a) use `base` model, (b) increase segment length, (c) add GPU support.

### Phase 3: Markdown Output & Persistence
**Complexity: Medium | Focus: Producing useful output files**

- [ ] **3.1** Generate transcript Markdown file — full timestamped, speaker-labeled, on meeting end
- [ ] **3.2** Configurable output directory (default `~/EchoNotes/`)
- [ ] **3.3** File naming: `YYYY-MM-DD HH.MM <Meeting Name>.md` / `- Transcript.md` with name resolution (window title → LLM-generated → fallback)
- [ ] **3.4** Meeting list/browser view in the UI (reads from output directory)
- [ ] **3.5** Auto-stop after silence timeout

### Phase 4: AI Summary & Notepad
**Complexity: Medium | Focus: The "magic" feature**

- [ ] **4.1** Post-meeting LLM call: generate `summary.md` from transcript (key points + action items, ignore small talk)
- [ ] **4.2** LLM integration (local via Ollama, or cloud via OpenAI/Anthropic API)
- [ ] **4.3** Rich text notepad editor for user notes during meetings (TipTap)
- [ ] **4.4** Save user notes as `notes.md` alongside transcript and summary
- [ ] **4.5** Note template system (shapes the AI summary structure)
- [ ] **4.6** Configurable output path for Obsidian vault targeting

### Phase 5: Platform Expansion & Polish
**Complexity: High | Focus: Cross-platform + advanced features**

- [ ] **5.1** Windows system audio capture (WASAPI loopback)
- [ ] **5.2** macOS system audio capture (ScreenCaptureKit)
- [ ] **5.3** Cloud transcription backend (Deepgram) as alternative
- [ ] **5.4** Calendar integration
- [ ] **5.5** Multi-speaker diarization (native-pyannote-rs)

---

## Open Questions

1. **Model distribution** — Bundle Whisper model with the app (large binary) or download on first run?
   - *Recommendation:* Download on first run with a progress indicator. The `base` model is 140MB.

2. **Concurrent transcription** — Transcribe mic and system audio as separate Whisper instances, or mix into one stream?
   - *Recommendation:* Separate instances for speaker labeling. Two `base` model instances should fit in ~300MB RAM.

3. **LLM for note enhancement** — Local (Ollama) or cloud (OpenAI/Anthropic)?
   - *Recommendation:* Support both. Default to cloud for quality, offer local for privacy.

4. **Audio recording** — Should we save raw audio files, or transcript-only like Granola?
   - *Recommendation:* Transcript-only by default (privacy), with optional audio save for users who want it.

5. **Frontend editor** — Which rich text editor library?
   - *Candidates:* TipTap (ProseMirror-based, most flexible), Lexical (Meta, lightweight), Plate (based on Slate).
   - *Recommendation:* TipTap — best ecosystem, plugin support, and collaborative editing primitives for the hybrid human+AI text model.

6. **Obsidian integration depth** — Just file placement, or full Obsidian-native features?
   - *Recommendation:* Start with configurable output directory (user points it at their vault). Later add YAML frontmatter (`date`, `participants`, `tags`), wikilinks between related meetings, and Obsidian-compatible `- [ ]` action items (already planned in summary format).
