import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  AudioDevice,
  AudioLevels,
  MeetingEntry,
  ModelInfo,
  PersistedSettings,
  StartRecordingResult,
  StopRecordingResult,
  TranscriptSegment,
} from "../types";

/** List available microphone input devices (via cpal/ALSA). */
export function getAudioDevices(): Promise<AudioDevice[]> {
  return invoke<AudioDevice[]>("get_audio_devices");
}

/** List available system audio monitor sources (via PulseAudio/PipeWire). */
export function getSystemAudioDevices(): Promise<AudioDevice[]> {
  return invoke<AudioDevice[]>("get_system_audio_devices");
}

/**
 * Start recording from the given mic and system audio devices.
 * Creates the transcript file immediately and streams segments via a Tauri Channel.
 * @param micDeviceId - ID of the microphone device
 * @param systemDeviceId - ID of the system audio device
 * @param onTranscript - Callback invoked for each incoming transcript segment
 */
export function startRecording(
  micDeviceId: string,
  systemDeviceId: string,
  onTranscript: (segment: TranscriptSegment) => void
): Promise<StartRecordingResult> {
  const channel = new Channel<TranscriptSegment>();
  channel.onmessage = onTranscript;
  return invoke<StartRecordingResult>("start_recording", {
    micDeviceId,
    systemDeviceId,
    onTranscript: channel,
  });
}

/** Pause the current recording session. Audio is discarded while paused. */
export function pauseRecording(): Promise<void> {
  return invoke("pause_recording");
}

/** Resume a paused recording session. */
export function resumeRecording(): Promise<void> {
  return invoke("resume_recording");
}

/** Save the current transcript to disk (periodic save during recording). */
export function saveCurrentTranscript(): Promise<number> {
  return invoke<number>("save_current_transcript");
}

/** Stop the current recording and return transcript info. */
export function stopRecording(): Promise<StopRecordingResult> {
  return invoke<StopRecordingResult>("stop_recording");
}

/** Get current audio RMS levels for both streams. */
export function getAudioLevels(): Promise<AudioLevels> {
  return invoke<AudioLevels>("get_audio_levels");
}

/**
 * Set the microphone gain multiplier (0.1–10.0). Takes effect immediately.
 * @param gain - Gain multiplier (1.0 = unity, 2.0 = double volume)
 */
export function setMicGain(gain: number): Promise<void> {
  return invoke("set_mic_gain", { gain });
}

/** Get the current microphone gain multiplier. */
export function getMicGain(): Promise<number> {
  return invoke<number>("get_mic_gain");
}

/**
 * Set the VAD (voice activity detection) sensitivity threshold.
 * Lower values detect quieter speech (0.0005–0.1).
 * @param threshold - RMS threshold value
 */
export function setVadThreshold(threshold: number): Promise<void> {
  return invoke("set_vad_threshold", { threshold });
}

/** Get the current VAD sensitivity threshold. */
export function getVadThreshold(): Promise<number> {
  return invoke<number>("get_vad_threshold");
}

/** Retrieve persisted application settings from disk. */
export function getSettings(): Promise<PersistedSettings> {
  return invoke<PersistedSettings>("get_settings");
}

/**
 * Set and persist the preferred microphone device ID.
 * @param deviceId - ID of the preferred microphone device
 */
export function setPreferredMic(deviceId: string): Promise<void> {
  return invoke("set_preferred_mic", { deviceId });
}

/**
 * Set and persist the preferred system audio device ID.
 * @param deviceId - ID of the preferred system audio monitor source
 */
export function setPreferredSystemDevice(deviceId: string): Promise<void> {
  return invoke("set_preferred_system_device", { deviceId });
}

/** Retrieve detailed information about the Whisper transcription model. */
export function getModelInfo(): Promise<ModelInfo> {
  return invoke<ModelInfo>("get_model_info");
}

/**
 * Check whether the Whisper transcription model is downloaded and ready.
 * Uses `get_model_info` internally and returns the `downloaded` flag.
 */
export async function checkModelStatus(): Promise<boolean> {
  const info = await getModelInfo();
  return info.downloaded;
}

/** Download the Whisper transcription model. */
export function downloadModel(): Promise<void> {
  return invoke("download_model");
}

/** Delete the Whisper transcription model file. */
export function deleteModel(): Promise<void> {
  return invoke("delete_model");
}

/**
 * Override the directory where models are stored.
 * Pass an empty string to reset to the default location.
 * @param path - Absolute path to the custom models directory
 */
export function setModelsDir(path: string): Promise<void> {
  return invoke("set_models_dir", { path });
}

/**
 * Set and persist the output directory for transcript files.
 * Pass an empty string to reset to the default (`~/EchoNotes/`).
 * @param path - Absolute path to the output directory
 */
export function setOutputDir(path: string): Promise<void> {
  return invoke("set_output_dir", { path });
}

/** Get the resolved output directory path. */
export function getOutputDir(): Promise<string> {
  return invoke<string>("get_output_dir");
}

/**
 * Set and persist the auto-stop silence timeout.
 * Pass 0 to disable auto-stop.
 * @param seconds - Timeout in seconds (0 = disabled)
 */
export function setSilenceTimeout(seconds: number): Promise<void> {
  return invoke("set_silence_timeout", { seconds });
}

/**
 * Set and persist the Whisper model name.
 * @param model - Model name (e.g. "tiny", "base", "small", "medium")
 */
export function setWhisperModel(model: string): Promise<void> {
  return invoke("set_whisper_model", { model });
}

/**
 * Set and persist the initial prompt for Whisper transcription.
 * Provides context (domain terms, names) to improve accuracy.
 * Pass an empty string to clear.
 * @param prompt - Initial prompt text
 */
export function setInitialPrompt(prompt: string): Promise<void> {
  return invoke("set_initial_prompt", { prompt });
}

/**
 * Set and persist the maximum segment duration in seconds (1–30).
 * Larger values reduce overhead but increase latency.
 * @param seconds - Maximum segment duration
 */
export function setMaxSegmentSeconds(seconds: number): Promise<void> {
  return invoke("set_max_segment_seconds", { seconds });
}

/** List meetings in the output directory. */
export function listMeetings(): Promise<MeetingEntry[]> {
  return invoke<MeetingEntry[]>("list_meetings");
}

/**
 * Read the contents of a transcript file.
 * @param path - Absolute path to the transcript file
 */
export function readMeetingTranscript(path: string): Promise<string> {
  return invoke<string>("read_meeting_transcript", { path });
}

/**
 * Set and persist the LLM provider for summary generation.
 * @param provider - Provider identifier ("ollama", "anthropic", "openai")
 */
export function setLlmProvider(provider: string): Promise<void> {
  return invoke("set_llm_provider", { provider });
}

/**
 * Set and persist the LLM model name override.
 * Pass an empty string to use the provider's default model.
 * @param model - Model name
 */
export function setLlmModel(model: string): Promise<void> {
  return invoke("set_llm_model", { model });
}

/**
 * Set and persist the API key for cloud LLM providers.
 * Pass an empty string to clear.
 * @param key - API key
 */
export function setLlmApiKey(key: string): Promise<void> {
  return invoke("set_llm_api_key", { key });
}

/**
 * Set and persist a custom base URL for the LLM provider.
 * Pass an empty string to use the provider's default URL.
 * @param url - Custom base URL
 */
export function setLlmBaseUrl(url: string): Promise<void> {
  return invoke("set_llm_base_url", { url });
}

/**
 * Enable or disable automatic summary generation after recording stops.
 * @param enabled - Whether auto-summary is enabled
 */
export function setAutoSummary(enabled: boolean): Promise<void> {
  return invoke("set_auto_summary", { enabled });
}

/**
 * Manually trigger summary generation for an existing transcript.
 * Runs in the background; listen for `summary-generated` or `summary-error` events.
 * @param transcriptPath - Absolute path to the transcript file
 */
export function generateSummary(transcriptPath: string): Promise<void> {
  return invoke("generate_summary", { transcriptPath });
}

/**
 * Read the contents of a summary file.
 * @param path - Absolute path to the summary file
 */
export function readMeetingSummary(path: string): Promise<string> {
  return invoke<string>("read_meeting_summary", { path });
}
