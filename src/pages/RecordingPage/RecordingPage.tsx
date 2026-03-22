import { useEffect, useRef } from "react";
import { Link } from "react-router-dom";
import { useRecording } from "../../hooks/useRecording";
import s from "./RecordingPage.module.scss";

const MIN_DB = -60;

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

/**
 * Convert linear RMS (0.0–1.0) to a perceptual dB-based percentage (0–100).
 * Maps -60 dB → 0% and 0 dB → 100%.
 */
function getLevelPercent(rms: number): number {
  if (rms <= 0) return 0;
  const db = 20 * Math.log10(rms);
  const percent = ((db - MIN_DB) / (0 - MIN_DB)) * 100;
  return Math.min(Math.max(percent, 0), 100);
}

/** Recording page with audio level meters, tuning controls, and live transcript. */
export function RecordingPage() {
  const {
    isRecording,
    audioLevels,
    segments,
    modelReady,
    micGainValue,
    handleGainChange,
    vadThresholdValue,
    handleVadChange,
  } = useRecording();

  const transcriptEndRef = useRef<HTMLDivElement | null>(null);

  // Auto-scroll transcript to bottom when new segments arrive
  useEffect(() => {
    transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [segments]);

  /** Convert a dB-scale percentage (0–100) back to linear RMS. */
  const rmsFromPercent = (pct: number): number => {
    const db = MIN_DB + (pct / 100) * (0 - MIN_DB);
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
