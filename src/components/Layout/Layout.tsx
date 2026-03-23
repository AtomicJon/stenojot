import { Link, Outlet, useLocation } from "react-router-dom";
import { useRecording } from "../../hooks/useRecording";
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
  } = useRecording();

  const location = useLocation();
  const isOnRecordingPage = location.pathname === "/";

  return (
    <div className={s.shell}>
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
      <main className={s.layout}>
        <Outlet />
      </main>
    </div>
  );
}
