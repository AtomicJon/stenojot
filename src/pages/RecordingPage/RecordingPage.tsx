import { useState, useEffect, useRef, useCallback } from "react";
import { Link } from "react-router-dom";
import {
  getAudioDevices,
  getSystemAudioDevices,
  startRecording,
  stopRecording,
  getAudioLevels,
  getModelInfo,
  getMicGain,
  setMicGain,
  getVadThreshold,
  setVadThreshold,
} from "../../lib/commands";
import type { AudioDevice, AudioLevels, TranscriptSegment } from "../../types";
import s from "./RecordingPage.module.scss";

/**
 * Format milliseconds to MM:SS display string.
 * @param ms - Time in milliseconds
 */
function formatTimestamp(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const mins = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;
  return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

/** Main recording page with device selection, controls, audio meters, and live transcript. */
export function RecordingPage() {
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
  const transcriptEndRef = useRef<HTMLDivElement | null>(null);

  // Auto-scroll transcript to bottom when new segments arrive
  useEffect(() => {
    transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [segments]);

  // Fetch audio devices and check model status on mount
  useEffect(() => {
    async function init() {
      try {
        const micDevs = await getAudioDevices();
        setMicDevices(micDevs);
        const defaultMic = micDevs.find((d) => d.is_default);
        setMicDeviceId(defaultMic?.id ?? micDevs[0]?.id ?? "");
      } catch (err) {
        console.error("Failed to get mic devices:", err);
      }

      try {
        const sysDevs = await getSystemAudioDevices();
        setSystemDevices(sysDevs);
        const defaultSys = sysDevs.find((d) => d.is_default);
        setSystemDeviceId(defaultSys?.id ?? sysDevs[0]?.id ?? "");
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

  /** Start recording and streaming transcript segments. */
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

  /** Stop the current recording session. */
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

  /** Update mic gain on the backend and local state. */
  const handleGainChange = useCallback(async (value: number) => {
    setMicGainValue(value);
    try {
      await setMicGain(value);
    } catch (err) {
      console.error("Failed to set mic gain:", err);
    }
  }, []);

  /** Update VAD sensitivity threshold on the backend and local state. */
  const handleVadChange = useCallback(async (value: number) => {
    setVadThresholdValue(value);
    try {
      await setVadThreshold(value);
    } catch (err) {
      console.error("Failed to set VAD threshold:", err);
    }
  }, []);

  const formatTime = (totalSeconds: number): string => {
    const mins = Math.floor(totalSeconds / 60);
    const secs = totalSeconds % 60;
    return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  };

  const minDb = -60;

  /**
   * Convert linear RMS (0.0–1.0) to a perceptual dB-based percentage (0–100).
   * Maps -60 dB → 0% and 0 dB → 100%.
   */
  const getLevelPercent = (rms: number): number => {
    if (rms <= 0) return 0;
    const db = 20 * Math.log10(rms);
    const percent = ((db - minDb) / (0 - minDb)) * 100;
    return Math.min(Math.max(percent, 0), 100);
  };

  /** Convert a dB-scale percentage (0–100) back to linear RMS. */
  const rmsFromPercent = (pct: number): number => {
    const db = minDb + (pct / 100) * (0 - minDb);
    return Math.pow(10, db / 20);
  };

  const getLevelVariant = (rms: number): string => {
    const pct = getLevelPercent(rms);
    if (pct < 50) return s.levelLow;
    if (pct < 80) return s.levelMid;
    return s.levelHigh;
  };

  return (
    <>
      {/* Header */}
      <header className={s.header}>
        <div className={s.headerTitle}>
          {isRecording && <span className={s.recordingDot} />}
          <h1>Recording</h1>
        </div>
        {isRecording && (
          <span className={s.recordingBadge}>Recording</span>
        )}
      </header>

      {/* Model not downloaded notice */}
      {!modelReady && (
        <div className={s.modelNotice}>
          <span className={s.modelNoticeText}>
            Transcription model not downloaded.
          </span>
          <Link to="/settings" className={s.modelNoticeLink}>
            Go to Settings to download
          </Link>
        </div>
      )}

      {/* Device Selection — hidden during recording */}
      {!isRecording && (
        <section className={s.panel}>
          <h2>Audio Sources</h2>
          <div className={s.deviceSelectors}>
            <label className={s.deviceLabel}>
              <span>Microphone</span>
              <select
                value={micDeviceId}
                onChange={(e) => setMicDeviceId(e.target.value)}
              >
                {micDevices.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                    {d.is_default ? " (Default)" : ""}
                  </option>
                ))}
              </select>
            </label>
            <label className={s.deviceLabel}>
              <span>System Audio</span>
              <select
                value={systemDeviceId}
                onChange={(e) => setSystemDeviceId(e.target.value)}
              >
                {systemDevices.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                    {d.is_default ? " (Default)" : ""}
                  </option>
                ))}
              </select>
            </label>
          </div>
        </section>
      )}

      {/* Audio Tuning — always visible so you can adjust during recording */}
      <section className={s.panel}>
        <h2>Audio Tuning</h2>
        <div className={s.deviceSelectors}>
          <label className={s.deviceLabel}>
            <span>Mic Gain ({micGainValue.toFixed(1)}x)</span>
            <input
              type="range"
              min="0.1"
              max="10"
              step="0.1"
              value={micGainValue}
              onChange={(e) => handleGainChange(parseFloat(e.target.value))}
              className={s.gainSlider}
            />
          </label>
          <label className={s.deviceLabel}>
            <span>
              Detection Threshold ({Math.round(getLevelPercent(vadThresholdValue))}%)
            </span>
            <input
              type="range"
              min="1"
              max="60"
              step="1"
              value={Math.round(getLevelPercent(vadThresholdValue))}
              onChange={(e) =>
                handleVadChange(rmsFromPercent(parseFloat(e.target.value)))
              }
              className={s.gainSlider}
            />
          </label>
        </div>
      </section>

      {/* Controls */}
      <section className={`${s.panel} ${s.controlsPanel}`}>
        <button
          className={`${s.recordBtn} ${isRecording ? s.recording : ""}`}
          onClick={isRecording ? handleStop : handleStart}
          disabled={!modelReady && !isRecording}
        >
          {isRecording ? "Stop Recording" : "Start Recording"}
        </button>
        {isRecording && (
          <div className={s.timer}>{formatTime(elapsedSeconds)}</div>
        )}
      </section>

      {/* Audio Level Meters */}
      {isRecording && (
        <section className={`${s.panel} ${s.levelsPanel}`}>
          <h2>Audio Levels</h2>
          <div className={s.levelRow}>
            <span className={s.levelLabel}>Mic</span>
            <div className={s.levelBarTrack}>
              <div
                className={`${s.levelBarFill} ${getLevelVariant(audioLevels.mic_rms)}`}
                style={{ width: `${getLevelPercent(audioLevels.mic_rms)}%` }}
              />
              <div
                className={s.thresholdMarker}
                style={{ left: `${getLevelPercent(vadThresholdValue)}%` }}
              />
            </div>
            <span className={s.levelValue}>
              {Math.round(getLevelPercent(audioLevels.mic_rms))}%
            </span>
          </div>
          <div className={s.levelRow}>
            <span className={s.levelLabel}>System</span>
            <div className={s.levelBarTrack}>
              <div
                className={`${s.levelBarFill} ${getLevelVariant(audioLevels.system_rms)}`}
                style={{ width: `${getLevelPercent(audioLevels.system_rms)}%` }}
              />
            </div>
            <span className={s.levelValue}>
              {Math.round(getLevelPercent(audioLevels.system_rms))}%
            </span>
          </div>
        </section>
      )}

      {/* Transcript Panel */}
      <section className={`${s.panel} ${s.transcriptPanel}`}>
        <h2>Transcript</h2>
        {segments.length === 0 ? (
          <div className={s.transcriptPlaceholder}>
            {isRecording
              ? "Listening for speech..."
              : "Transcript will appear here during recording..."}
          </div>
        ) : (
          <div className={s.transcriptList}>
            {segments.map((seg, i) => (
              <div
                key={`${seg.start_ms}-${i}`}
                className={`${s.segment} ${seg.speaker === "Me" ? s.segmentMe : s.segmentOthers}`}
              >
                <div className={s.segmentHeader}>
                  <span className={s.segmentTimestamp}>
                    {formatTimestamp(seg.start_ms)}
                  </span>
                  <span className={s.segmentSpeaker}>{seg.speaker}</span>
                </div>
                <div className={s.segmentText}>{seg.text}</div>
              </div>
            ))}
            <div ref={transcriptEndRef} />
          </div>
        )}
      </section>
    </>
  );
}
