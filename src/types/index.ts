/** A single audio input/output device. */
export interface AudioDevice {
  name: string;
  id: string;
  is_default: boolean;
}

/** Real-time RMS levels for both audio streams. */
export interface AudioLevels {
  mic_rms: number;
  system_rms: number;
}

/** Detailed information about the Whisper transcription model. */
export interface ModelInfo {
  name: string;
  path: string;
  downloaded: boolean;
  size_bytes: number;
  models_dir: string;
}

/** Persisted application settings from the backend. */
export interface PersistedSettings {
  mic_device_id: string | null;
  system_device_id: string | null;
  mic_gain: number;
  vad_threshold: number;
  models_dir: string | null;
}

/** A single segment of transcribed speech. */
export interface TranscriptSegment {
  text: string;
  speaker: "Me" | "Others";
  start_ms: number;
  end_ms: number;
  is_final: boolean;
}
