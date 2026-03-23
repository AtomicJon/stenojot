import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { listMeetings, readMeetingTranscript } from "../../lib/commands";
import { formatFileSize } from "../../lib/format";
import { Button } from "../../components/Button";
import { Panel } from "../../components/Panel";
import type { MeetingEntry } from "../../types";
import s from "./MeetingsPage.module.scss";

/** Page for browsing past meeting transcripts. */
export function MeetingsPage() {
  const [meetings, setMeetings] = useState<MeetingEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [transcriptContent, setTranscriptContent] = useState<string | null>(null);

  const loadMeetings = useCallback(async () => {
    try {
      const list = await listMeetings();
      setMeetings(list);
    } catch (err) {
      console.error("Failed to list meetings:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadMeetings();
  }, [loadMeetings]);

  // Refresh when backend emits meetings-changed event
  useEffect(() => {
    const unlistenPromise = listen("meetings-changed", () => {
      loadMeetings();
    });
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [loadMeetings]);

  /** Open a transcript for reading. */
  const handleOpen = useCallback(async (path: string) => {
    setSelectedPath(path);
    try {
      const content = await readMeetingTranscript(path);
      setTranscriptContent(content);
    } catch (err) {
      console.error("Failed to read transcript:", err);
      setTranscriptContent("Failed to load transcript.");
    }
  }, []);

  /** Go back to the meeting list. */
  const handleBack = useCallback(() => {
    setSelectedPath(null);
    setTranscriptContent(null);
  }, []);

  if (selectedPath && transcriptContent !== null) {
    return (
      <>
        <header className={s.header}>
          <Button variant="link" onClick={handleBack}>
            Back to Meetings
          </Button>
        </header>
        <Panel>
          <pre className={s.transcriptContent}>{transcriptContent}</pre>
        </Panel>
      </>
    );
  }

  return (
    <>
      <header className={s.header}>
        <h1>Meetings</h1>
      </header>

      {loading && <p className={s.emptyState}>Loading meetings...</p>}

      {!loading && meetings.length === 0 && (
        <p className={s.emptyState}>
          No meetings yet. Start a recording to create your first transcript.
        </p>
      )}

      {!loading && meetings.length > 0 && (
        <div className={s.meetingList}>
          {meetings.map((meeting) => (
            <button
              key={meeting.transcript_path}
              className={s.meetingCard}
              onClick={() => handleOpen(meeting.transcript_path)}
            >
              <div className={s.cardHeader}>
                <span className={s.cardName}>{meeting.name}</span>
                <span className={s.cardSize}>
                  {formatFileSize(meeting.size_bytes)}
                </span>
              </div>
              <div className={s.cardMeta}>
                <span className={s.cardDate}>{meeting.date}</span>
                <span className={s.cardTime}>{meeting.time}</span>
              </div>
            </button>
          ))}
        </div>
      )}
    </>
  );
}
