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
  /** True if the recording is currently paused. */
  is_paused: boolean;
  /** True if the worker auto-stopped due to silence timeout. */
  auto_stopped: boolean;
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
  output_dir: string | null;
  silence_timeout_seconds: number | null;
  whisper_model: string;
  initial_prompt: string | null;
  max_segment_seconds: number;
}

/** Result returned by `start_recording`. */
export interface StartRecordingResult {
  transcript_path: string;
  meeting_name: string;
}

/** Result returned by `stop_recording`. */
export interface StopRecordingResult {
  transcript_path: string | null;
  segment_count: number;
}

/** A meeting entry parsed from the transcript output directory. */
export interface MeetingEntry {
  name: string;
  date: string;
  time: string;
  has_transcript: boolean;
  has_summary: boolean;
  transcript_path: string;
  summary_path: string;
  size_bytes: number;
}

/** A single segment of transcribed speech. */
export interface TranscriptSegment {
  text: string;
  speaker: "Me" | "Others";
  start_ms: number;
  end_ms: number;
  is_final: boolean;
}
