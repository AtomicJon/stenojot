import { useEffect, useRef } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import { useRecording } from "../../hooks/useRecording";
import type { SummaryStatus } from "../../hooks/useRecording";
import { useToast } from "../Toast";
import { formatTime } from "../../lib/format";
import s from "./Layout.module.scss";

/** App shell with top navigation, global recording controls, and consistent layout wrapping all routes. */
export function Layout() {
  const {
    isRecording,
    isPaused,
    elapsedSeconds,
    modelReady,
    handleStart,
    handleStop,
    handlePause,
    handleResume,
    summaryStatus,
    summaryError,
  } = useRecording();
  const { showToast } = useToast();
  const prevStatus = useRef<SummaryStatus>("idle");

  useEffect(() => {
    if (prevStatus.current === summaryStatus) return;
    prevStatus.current = summaryStatus;

    if (summaryStatus === "complete") {
      showToast("Meeting summary generated successfully.", "success");
    } else if (summaryStatus === "error") {
      showToast(
        `Summary generation failed: ${summaryError ?? "Unknown error"}`,
        "error",
        10000,
      );
    }
  }, [summaryStatus, summaryError, showToast]);

  const location = useLocation();
  const isOnRecordingPage = location.pathname === "/";

  return (
    <div className={s.shell}>
      <div className={s.navOuter}>
      <nav className={s.nav}>
        <Link to="/" className={s.navBrand}>
          EchoNotes
        </Link>
        <div className={s.navRight}>
          {isRecording && !isOnRecordingPage && (
            <Link to="/" className={s.navSessionLink}>
              Current Session
            </Link>
          )}
          {isRecording && (
            <div className={s.recordingStatus}>
              <span className={isPaused ? s.recordingDotPaused : s.recordingDot} />
              <span className={s.timer}>
                {formatTime(elapsedSeconds)}
                {isPaused && <span className={s.pausedLabel}> Paused</span>}
              </span>
            </div>
          )}
          {isRecording && (
            <button
              className={s.navPauseBtn}
              onClick={isPaused ? handleResume : handlePause}
            >
              {isPaused ? "Resume" : "Pause"}
            </button>
          )}
          <button
            className={`${s.navRecordBtn} ${isRecording ? s.navRecordBtnActive : ""}`}
            onClick={isRecording ? handleStop : handleStart}
            disabled={!modelReady && !isRecording}
          >
            {isRecording ? "Stop" : "Record"}
          </button>
          <Link to="/meetings" className={s.navLink}>
            Meetings
          </Link>
          <Link to="/settings" className={s.navLink}>
            Settings
          </Link>
        </div>
      </nav>
      </div>
      <main className={s.content}>
        <Outlet />
      </main>
    </div>
  );
}
