# Bundled VAD model licenses

These ONNX models are embedded in the binary via `include_bytes!` (see `src/audio/vad/`).

- **silero_vad.onnx** — Silero VAD v5, from the pinned **v5.1.2** tag of
  [snakers4/silero-vad](https://github.com/snakers4/silero-vad/blob/v5.1.2/src/silero_vad/data/silero_vad.onnx).
  License: **MIT**. (Note: the `master` branch export is a `spox`-built model with
  different runtime behavior — use the pinned versioned export.)
- **ten-vad.onnx** — TEN VAD, from [TEN-framework/ten-vad](https://github.com/TEN-framework/ten-vad). License: **Apache-2.0**.
