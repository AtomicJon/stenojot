import { createContext, useContext } from 'react';
import type { AudioDevice, AudioLevels, TranscriptSegment } from '../types';

/** Status of background summary generation after recording stops. */
export type SummaryStatus = 'idle' | 'generating' | 'complete' | 'error';

/** Shape of the recording context exposed to all pages. */
export interface RecordingState {
  /** Whether a recording session is active. */
  isRecording: boolean;
  /** Whether the recording is currently paused. */
  isPaused: boolean;
  /** Elapsed seconds since recording started (excludes paused time). */
  elapsedSeconds: number;
  /** Live RMS audio levels for mic and system streams. */
  audioLevels: AudioLevels;
  /** Accumulated transcript segments from the current session. */
  segments: TranscriptSegment[];
  /** Whether the Whisper model is downloaded and ready. */
  modelReady: boolean;

  /** Available microphone input devices. */
  micDevices: AudioDevice[];
  /** Available system audio monitor sources. */
  systemDevices: AudioDevice[];
  /** Currently selected mic device ID. */
  micDeviceId: string;
  /** Currently selected system audio device ID. */
  systemDeviceId: string;
  /** Set the selected mic device ID. */
  setMicDeviceId: (id: string) => void;
  /** Set the selected system audio device ID. */
  setSystemDeviceId: (id: string) => void;

  /** Current mic gain multiplier. */
  micGainValue: number;
  /** Update the mic gain multiplier (takes effect immediately). */
  handleGainChange: (value: number) => void;
  /** Current VAD threshold (linear RMS). */
  vadThresholdValue: number;
  /** Update the VAD threshold (takes effect immediately). */
  handleVadChange: (value: number) => void;

  /** Start a recording session. */
  handleStart: () => Promise<void>;
  /** Stop the current recording session. Returns the transcript path if saved. */
  handleStop: () => Promise<string | null>;
  /** Pause the current recording. */
  handlePause: () => Promise<void>;
  /** Resume a paused recording. */
  handleResume: () => Promise<void>;
  /** Path to the current session's transcript file. */
  currentTranscriptPath: string | null;
  /** Path to the last saved transcript file (after stop). */
  lastTranscriptPath: string | null;
  /** Re-check model download status. */
  refreshModelStatus: () => Promise<void>;
  /** Status of background summary generation. */
  summaryStatus: SummaryStatus;
  /** Error message from the last failed summary generation, if any. */
  summaryError: string | null;
}

export const RecordingContext = createContext<RecordingState | null>(null);

/** Access the global recording state. Must be used within a RecordingProvider. */
export function useRecording(): RecordingState {
  const ctx = useContext(RecordingContext);
  if (!ctx) {
    throw new Error('useRecording must be used within RecordingProvider');
  }
  return ctx;
}
