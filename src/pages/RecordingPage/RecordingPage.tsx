import { useEffect, useRef } from "react";
import { Link } from "react-router-dom";
import { useRecording } from "../../hooks/useRecording";
import { getLevelPercent, rmsFromPercent } from "../../lib/audio";
import { formatTimestamp } from "../../lib/format";
import { LevelMeter } from "../../components/LevelMeter";
import { Panel } from "../../components/Panel";
import { Slider } from "../../components/Slider";
import s from "./RecordingPage.module.scss";

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

      {/* Audio Tuning */}
      <Panel title="Audio Tuning">
        <div className={s.sliderGroup}>
          <Slider
            label={`Mic Gain (${micGainValue.toFixed(1)}x)`}
            value={micGainValue}
            min={0.1}
            max={10}
            step={0.1}
            onChange={handleGainChange}
          />
          <Slider
            label={`Detection Threshold (${Math.round(getLevelPercent(vadThresholdValue))}%)`}
            value={Math.round(getLevelPercent(vadThresholdValue))}
            min={1}
            max={60}
            step={1}
            onChange={(pct) => handleVadChange(rmsFromPercent(pct))}
          />
        </div>
      </Panel>

      {/* Audio Level Meters */}
      {isRecording && (
        <Panel title="Audio Levels" className={s.levelsPanel}>
          <LevelMeter
            label="Mic"
            rms={audioLevels.mic_rms}
            thresholdRms={vadThresholdValue}
          />
          <LevelMeter label="System" rms={audioLevels.system_rms} />
        </Panel>
      )}

      {/* Transcript */}
      <Panel title="Transcript" className={s.transcriptPanel}>
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
      </Panel>
    </>
  );
}
