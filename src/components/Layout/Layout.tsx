import { Link, Outlet } from "react-router-dom";
import { useRecording } from "../../hooks/useRecording";
import { formatTime } from "../../lib/format";
import s from "./Layout.module.scss";

/** App shell with top navigation, global recording controls, and consistent layout wrapping all routes. */
export function Layout() {
  const {
    isRecording,
    elapsedSeconds,
    modelReady,
    handleStart,
    handleStop,
  } = useRecording();

  return (
    <div className={s.shell}>
      <nav className={s.nav}>
        <Link to="/" className={s.navBrand}>
          EchoNotes
        </Link>
        <div className={s.navRight}>
          {isRecording && (
            <div className={s.recordingStatus}>
              <span className={s.recordingDot} />
              <span className={s.timer}>{formatTime(elapsedSeconds)}</span>
            </div>
          )}
          <button
            className={`${s.navRecordBtn} ${isRecording ? s.navRecordBtnActive : ""}`}
            onClick={isRecording ? handleStop : handleStart}
            disabled={!modelReady && !isRecording}
          >
            {isRecording ? "Stop" : "Record"}
          </button>
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
