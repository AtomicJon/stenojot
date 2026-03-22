import { invoke, Channel } from "@tauri-apps/api/core";
import type { AudioDevice, AudioLevels, ModelInfo, TranscriptSegment } from "../types";

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
 * Streams transcript segments back via a Tauri Channel.
 * @param micDeviceId - ID of the microphone device
 * @param systemDeviceId - ID of the system audio device
 * @param onTranscript - Callback invoked for each incoming transcript segment
 */
export function startRecording(
  micDeviceId: string,
  systemDeviceId: string,
  onTranscript: (segment: TranscriptSegment) => void
): Promise<void> {
  const channel = new Channel<TranscriptSegment>();
  channel.onmessage = onTranscript;
  return invoke("start_recording", {
    micDeviceId,
    systemDeviceId,
    onTranscript: channel,
  });
}

/** Stop the current recording. */
export function stopRecording(): Promise<void> {
  return invoke("stop_recording");
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
