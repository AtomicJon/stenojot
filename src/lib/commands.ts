import { invoke } from "@tauri-apps/api/core";
import type { AudioDevice, AudioLevels } from "../types";

/** List all available audio input devices. */
export function getAudioDevices(): Promise<AudioDevice[]> {
  return invoke<AudioDevice[]>("get_audio_devices");
}

/** Start recording from the given mic and system audio devices. */
export function startRecording(
  micDeviceId: string,
  systemDeviceId: string
): Promise<void> {
  return invoke("start_recording", { micDeviceId, systemDeviceId });
}

/** Stop the current recording. */
export function stopRecording(): Promise<void> {
  return invoke("stop_recording");
}

/** Get current audio RMS levels for both streams. */
export function getAudioLevels(): Promise<AudioLevels> {
  return invoke<AudioLevels>("get_audio_levels");
}
