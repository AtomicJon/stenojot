import { useState, useEffect, useRef, useCallback } from "react";
import {
  getAudioDevices,
  startRecording,
  stopRecording,
  getAudioLevels,
} from "../../lib/commands";
import type { AudioDevice, AudioLevels } from "../../types";
import s from "./RecordingPage.module.scss";

/** Main recording page with device selection, controls, and audio meters. */
export function RecordingPage() {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [micDeviceId, setMicDeviceId] = useState("");
  const [systemDeviceId, setSystemDeviceId] = useState("");
  const [isRecording, setIsRecording] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [audioLevels, setAudioLevels] = useState<AudioLevels>({
    mic_rms: 0,
    system_rms: 0,
  });

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const levelPollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    async function fetchDevices() {
      try {
        const devs = await getAudioDevices();
        setDevices(devs);
        const defaultDev = devs.find((d) => d.is_default);
        if (defaultDev) {
          setMicDeviceId(defaultDev.id);
          setSystemDeviceId(defaultDev.id);
        } else if (devs.length > 0) {
          setMicDeviceId(devs[0].id);
          setSystemDeviceId(devs[0].id);
        }
      } catch (err) {
        console.error("Failed to get audio devices:", err);
      }
    }
    fetchDevices();
  }, []);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
      if (levelPollRef.current) clearInterval(levelPollRef.current);
    };
  }, []);

  const handleStart = useCallback(async () => {
    try {
      await startRecording(micDeviceId, systemDeviceId);
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

  const formatTime = (totalSeconds: number): string => {
    const mins = Math.floor(totalSeconds / 60);
    const secs = totalSeconds % 60;
    return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  };

  const getLevelPercent = (value: number): number => {
    return Math.min(Math.max(value * 100, 0), 100);
  };

  const getLevelVariant = (value: number): string => {
    if (value < 0.33) return s.levelLow;
    if (value < 0.66) return s.levelMid;
    return s.levelHigh;
  };

  return (
    <>
      {/* Header */}
      <header className={s.header}>
        <div className={s.headerTitle}>
          {isRecording && <span className={s.recordingDot} />}
          <h1>EchoNotes</h1>
        </div>
        {isRecording && (
          <span className={s.recordingBadge}>Recording</span>
        )}
      </header>

      {/* Device Selection */}
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
                {devices.map((d) => (
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
                {devices.map((d) => (
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

      {/* Controls */}
      <section className={`${s.panel} ${s.controlsPanel}`}>
        <button
          className={`${s.recordBtn} ${isRecording ? s.recording : ""}`}
          onClick={isRecording ? handleStop : handleStart}
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
        <div className={s.transcriptPlaceholder}>
          Transcript will appear here during recording...
        </div>
      </section>
    </>
  );
}
