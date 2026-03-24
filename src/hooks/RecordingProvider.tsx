import { useCallback, useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import {
  getAudioDevices,
  getSystemAudioDevices,
  getSettings,
  setPreferredMic,
  setPreferredSystemDevice,
  startRecording,
  stopRecording,
  pauseRecording,
  resumeRecording,
  saveCurrentTranscript,
  getAudioLevels,
  getModelInfo,
  getMicGain,
  setMicGain as setMicGainCmd,
  getVadThreshold,
  setVadThreshold as setVadThresholdCmd,
} from '../lib/commands';
import { listen } from '@tauri-apps/api/event';
import type { AudioDevice, AudioLevels, TranscriptSegment } from '../types';
import { RecordingContext } from './useRecording';
import type { RecordingState, SummaryStatus } from './useRecording';

/** How often to auto-save the transcript during recording (ms). */
const AUTO_SAVE_INTERVAL_MS = 30_000;

/** Props for the RecordingProvider component. */
interface RecordingProviderProps {
  children: ReactNode;
}

/** Provides global recording state to all descendant components. */
export function RecordingProvider({ children }: RecordingProviderProps) {
  const [micDevices, setMicDevices] = useState<AudioDevice[]>([]);
  const [systemDevices, setSystemDevices] = useState<AudioDevice[]>([]);
  const [micDeviceId, setMicDeviceId] = useState('');
  const [systemDeviceId, setSystemDeviceId] = useState('');
  const [isRecording, setIsRecording] = useState(false);
  const [isPaused, setIsPaused] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [audioLevels, setAudioLevels] = useState<AudioLevels>({
    mic_rms: 0,
    system_rms: 0,
    is_paused: false,
    auto_stopped: false,
  });
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [modelReady, setModelReady] = useState(false);
  const [micGainValue, setMicGainValue] = useState(1.0);
  const [vadThresholdValue, setVadThresholdValue] = useState(0.005);
  const [lastTranscriptPath, setLastTranscriptPath] = useState<string | null>(
    null,
  );
  const [currentTranscriptPath, setCurrentTranscriptPath] = useState<
    string | null
  >(null);
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>('idle');
  const [summaryError, setSummaryError] = useState<string | null>(null);

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const levelPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const autoSaveRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const handleStopRef = useRef<(() => Promise<string | null>) | null>(null);
  const isPausedRef = useRef(false);

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
        console.error('Failed to load persisted settings:', err);
      }

      try {
        const micDevs = await getAudioDevices();
        setMicDevices(micDevs);
        const preferred = preferredMicId
          ? micDevs.find((d) => d.id === preferredMicId)
          : null;
        const defaultMic = micDevs.find((d) => d.is_default);
        setMicDeviceId(preferred?.id ?? defaultMic?.id ?? micDevs[0]?.id ?? '');
      } catch (err) {
        console.error('Failed to get mic devices:', err);
      }

      try {
        const sysDevs = await getSystemAudioDevices();
        setSystemDevices(sysDevs);
        const preferred = preferredSystemId
          ? sysDevs.find((d) => d.id === preferredSystemId)
          : null;
        const defaultSys = sysDevs.find((d) => d.is_default);
        setSystemDeviceId(
          preferred?.id ?? defaultSys?.id ?? sysDevs[0]?.id ?? '',
        );
      } catch (err) {
        console.error('Failed to get system audio devices:', err);
      }

      try {
        const [gain, vad] = await Promise.all([
          getMicGain(),
          getVadThreshold(),
        ]);
        setMicGainValue(gain);
        setVadThresholdValue(vad);
      } catch (err) {
        console.error('Failed to get audio settings:', err);
      }

      try {
        const info = await getModelInfo();
        setModelReady(info.downloaded);
      } catch (err) {
        console.error('Failed to check model status:', err);
      }
    }
    init();
  }, []);

  // Cleanup intervals on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
      if (levelPollRef.current) clearInterval(levelPollRef.current);
      if (autoSaveRef.current) clearInterval(autoSaveRef.current);
    };
  }, []);

  // Listen for summary generation events
  useEffect(() => {
    const unlistenGenerating = listen('summary-generating', () => {
      setSummaryStatus('generating');
    });
    const unlistenGenerated = listen('summary-generated', () => {
      setSummaryStatus('complete');
    });
    const unlistenError = listen<string>('summary-error', (event) => {
      setSummaryStatus('error');
      setSummaryError(event.payload);
    });
    return () => {
      unlistenGenerating.then((u) => u());
      unlistenGenerated.then((u) => u());
      unlistenError.then((u) => u());
    };
  }, []);

  const refreshModelStatus = useCallback(async () => {
    try {
      const info = await getModelInfo();
      setModelReady(info.downloaded);
    } catch (err) {
      console.error('Failed to check model status:', err);
    }
  }, []);

  const handleStart = useCallback(async () => {
    try {
      setSegments([]);
      setSummaryStatus('idle');
      setSummaryError(null);
      const result = await startRecording(
        micDeviceId,
        systemDeviceId,
        (segment) => {
          setSegments((prev) => [...prev, segment]);
        },
      );
      setIsRecording(true);
      setIsPaused(false);
      isPausedRef.current = false;
      setElapsedSeconds(0);
      setCurrentTranscriptPath(result.transcript_path);

      // Elapsed timer — skips ticks while paused
      timerRef.current = setInterval(() => {
        if (!isPausedRef.current) {
          setElapsedSeconds((prev) => prev + 1);
        }
      }, 1000);

      // Audio level polling
      levelPollRef.current = setInterval(async () => {
        try {
          const levels = await getAudioLevels();
          setAudioLevels(levels);
          if (levels.auto_stopped) {
            handleStopRef.current?.();
          }
        } catch {
          // Silently ignore polling errors
        }
      }, 100);

      // Periodic transcript save
      autoSaveRef.current = setInterval(async () => {
        try {
          await saveCurrentTranscript();
        } catch {
          // Silently ignore save errors
        }
      }, AUTO_SAVE_INTERVAL_MS);
    } catch (err) {
      console.error('Failed to start recording:', err);
    }
  }, [micDeviceId, systemDeviceId]);

  const handleStop = useCallback(async (): Promise<string | null> => {
    let transcriptPath: string | null = null;
    try {
      const result = await stopRecording();
      transcriptPath = result.transcript_path;
      setLastTranscriptPath(transcriptPath);
    } catch (err) {
      console.error('Failed to stop recording:', err);
    } finally {
      setIsRecording(false);
      setIsPaused(false);
      isPausedRef.current = false;
      setCurrentTranscriptPath(null);
      setAudioLevels({
        mic_rms: 0,
        system_rms: 0,
        is_paused: false,
        auto_stopped: false,
      });
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
      if (levelPollRef.current) {
        clearInterval(levelPollRef.current);
        levelPollRef.current = null;
      }
      if (autoSaveRef.current) {
        clearInterval(autoSaveRef.current);
        autoSaveRef.current = null;
      }
    }
    return transcriptPath;
  }, []);

  // Keep ref in sync so the level poll can trigger auto-stop
  handleStopRef.current = handleStop;

  const handlePause = useCallback(async () => {
    try {
      await pauseRecording();
      setIsPaused(true);
      isPausedRef.current = true;
    } catch (err) {
      console.error('Failed to pause recording:', err);
    }
  }, []);

  const handleResume = useCallback(async () => {
    try {
      await resumeRecording();
      setIsPaused(false);
      isPausedRef.current = false;
    } catch (err) {
      console.error('Failed to resume recording:', err);
    }
  }, []);

  const handleGainChange = useCallback(async (value: number) => {
    setMicGainValue(value);
    try {
      await setMicGainCmd(value);
    } catch (err) {
      console.error('Failed to set mic gain:', err);
    }
  }, []);

  const handleVadChange = useCallback(async (value: number) => {
    setVadThresholdValue(value);
    try {
      await setVadThresholdCmd(value);
    } catch (err) {
      console.error('Failed to set VAD threshold:', err);
    }
  }, []);

  const handleSetMicDeviceId = useCallback((id: string) => {
    setMicDeviceId(id);
    setPreferredMic(id).catch((err) =>
      console.error('Failed to persist mic preference:', err),
    );
  }, []);

  const handleSetSystemDeviceId = useCallback((id: string) => {
    setSystemDeviceId(id);
    setPreferredSystemDevice(id).catch((err) =>
      console.error('Failed to persist system device preference:', err),
    );
  }, []);

  const value: RecordingState = {
    isRecording,
    isPaused,
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
    handlePause,
    handleResume,
    currentTranscriptPath,
    lastTranscriptPath,
    refreshModelStatus,
    summaryStatus,
    summaryError,
  };

  return (
    <RecordingContext.Provider value={value}>
      {children}
    </RecordingContext.Provider>
  );
}
