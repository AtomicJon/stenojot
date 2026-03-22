import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import {
  getAudioDevices,
  getSystemAudioDevices,
  getSettings,
  setPreferredMic,
  setPreferredSystemDevice,
  startRecording,
  stopRecording,
  getAudioLevels,
  getModelInfo,
  getMicGain,
  setMicGain as setMicGainCmd,
  getVadThreshold,
  setVadThreshold as setVadThresholdCmd,
} from "../lib/commands";
import type { AudioDevice, AudioLevels, TranscriptSegment } from "../types";

/** Shape of the recording context exposed to all pages. */
export interface RecordingState {
  /** Whether a recording session is active. */
  isRecording: boolean;
  /** Elapsed seconds since recording started. */
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
  /** Stop the current recording session. */
  handleStop: () => Promise<void>;
  /** Re-check model download status. */
  refreshModelStatus: () => Promise<void>;
}

const RecordingContext = createContext<RecordingState | null>(null);

/** Access the global recording state. Must be used within a RecordingProvider. */
export function useRecording(): RecordingState {
  const ctx = useContext(RecordingContext);
  if (!ctx) throw new Error("useRecording must be used within RecordingProvider");
  return ctx;
}

/** Props for the RecordingProvider component. */
interface RecordingProviderProps {
  children: ReactNode;
}

/** Provides global recording state to all descendant components. */
export function RecordingProvider({ children }: RecordingProviderProps) {
  const [micDevices, setMicDevices] = useState<AudioDevice[]>([]);
  const [systemDevices, setSystemDevices] = useState<AudioDevice[]>([]);
  const [micDeviceId, setMicDeviceId] = useState("");
  const [systemDeviceId, setSystemDeviceId] = useState("");
  const [isRecording, setIsRecording] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [audioLevels, setAudioLevels] = useState<AudioLevels>({
    mic_rms: 0,
    system_rms: 0,
  });
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [modelReady, setModelReady] = useState(false);
  const [micGainValue, setMicGainValue] = useState(1.0);
  const [vadThresholdValue, setVadThresholdValue] = useState(0.005);

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const levelPollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Fetch devices, audio settings, and model status on mount.
  // Persisted settings are used to restore preferred devices if still available.
  useEffect(() => {
    async function init() {
      // Load persisted settings first so we can restore preferred devices
      let preferredMicId: string | null = null;
      let preferredSystemId: string | null = null;
      try {
        const saved = await getSettings();
        preferredMicId = saved.mic_device_id;
        preferredSystemId = saved.system_device_id;
      } catch (err) {
        console.error("Failed to load persisted settings:", err);
      }

      try {
        const micDevs = await getAudioDevices();
        setMicDevices(micDevs);
        // Restore preferred device if it's still connected, else fall back to default
        const preferred = preferredMicId
          ? micDevs.find((d) => d.id === preferredMicId)
          : null;
        const defaultMic = micDevs.find((d) => d.is_default);
        setMicDeviceId(preferred?.id ?? defaultMic?.id ?? micDevs[0]?.id ?? "");
      } catch (err) {
        console.error("Failed to get mic devices:", err);
      }

      try {
        const sysDevs = await getSystemAudioDevices();
        setSystemDevices(sysDevs);
        const preferred = preferredSystemId
          ? sysDevs.find((d) => d.id === preferredSystemId)
          : null;
        const defaultSys = sysDevs.find((d) => d.is_default);
        setSystemDeviceId(preferred?.id ?? defaultSys?.id ?? sysDevs[0]?.id ?? "");
      } catch (err) {
        console.error("Failed to get system audio devices:", err);
      }

      try {
        const [gain, vad] = await Promise.all([getMicGain(), getVadThreshold()]);
        setMicGainValue(gain);
        setVadThresholdValue(vad);
      } catch (err) {
        console.error("Failed to get audio settings:", err);
      }

      try {
        const info = await getModelInfo();
        setModelReady(info.downloaded);
      } catch (err) {
        console.error("Failed to check model status:", err);
      }
    }
    init();
  }, []);

  // Cleanup intervals on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
      if (levelPollRef.current) clearInterval(levelPollRef.current);
    };
  }, []);

  const refreshModelStatus = useCallback(async () => {
    try {
      const info = await getModelInfo();
      setModelReady(info.downloaded);
    } catch (err) {
      console.error("Failed to check model status:", err);
    }
  }, []);

  const handleStart = useCallback(async () => {
    try {
      setSegments([]);
      await startRecording(micDeviceId, systemDeviceId, (segment) => {
        setSegments((prev) => [...prev, segment]);
      });
      setIsRecording(true);
      setElapsedSeconds(0);

      timerRef.current = setInterval(() => {
        setElapsedSeconds((prev) => prev + 1);
      }, 1000);

      levelPollRef.current = setInterval(async () => {
        try {
          const levels = await getAudioLevels();
          setAudioLevels(levels);
        } catch {
          // Silently ignore polling errors
        }
      }, 100);
    } catch (err) {
      console.error("Failed to start recording:", err);
    }
  }, [micDeviceId, systemDeviceId]);

  const handleStop = useCallback(async () => {
    try {
      await stopRecording();
    } catch (err) {
      console.error("Failed to stop recording:", err);
    } finally {
      setIsRecording(false);
      setAudioLevels({ mic_rms: 0, system_rms: 0 });
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
      if (levelPollRef.current) {
        clearInterval(levelPollRef.current);
        levelPollRef.current = null;
      }
    }
  }, []);

  const handleGainChange = useCallback(async (value: number) => {
    setMicGainValue(value);
    try {
      await setMicGainCmd(value);
    } catch (err) {
      console.error("Failed to set mic gain:", err);
    }
  }, []);

  const handleVadChange = useCallback(async (value: number) => {
    setVadThresholdValue(value);
    try {
      await setVadThresholdCmd(value);
    } catch (err) {
      console.error("Failed to set VAD threshold:", err);
    }
  }, []);

  const handleSetMicDeviceId = useCallback((id: string) => {
    setMicDeviceId(id);
    setPreferredMic(id).catch((err) =>
      console.error("Failed to persist mic preference:", err),
    );
  }, []);

  const handleSetSystemDeviceId = useCallback((id: string) => {
    setSystemDeviceId(id);
    setPreferredSystemDevice(id).catch((err) =>
      console.error("Failed to persist system device preference:", err),
    );
  }, []);

  const value: RecordingState = {
    isRecording,
    elapsedSeconds,
    audioLevels,
    segments,
    modelReady,
    micDevices,
    systemDevices,
    micDeviceId,
    systemDeviceId,
    setMicDeviceId: handleSetMicDeviceId,
    setSystemDeviceId: handleSetSystemDeviceId,
    micGainValue,
    handleGainChange,
    vadThresholdValue,
    handleVadChange,
    handleStart,
    handleStop,
    refreshModelStatus,
  };

  return (
    <RecordingContext.Provider value={value}>
      {children}
    </RecordingContext.Provider>
  );
}
