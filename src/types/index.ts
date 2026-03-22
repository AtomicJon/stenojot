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

/** A single segment of transcribed speech. */
export interface TranscriptSegment {
  text: string;
  speaker: "Me" | "Others";
  start_ms: number;
  end_ms: number;
  is_final: boolean;
}
