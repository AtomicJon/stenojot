import { useState, useEffect, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import { listMeetings } from '../../lib/commands';
import { formatFileSize, meetingSlug } from '../../lib/format';
import type { MeetingEntry } from '../../types';
import s from './MeetingsPage.module.scss';

/** Page for browsing past meeting transcripts and summaries. */
export function MeetingsPage() {
  const [meetings, setMeetings] = useState<MeetingEntry[]>([]);
  const [loading, setLoading] = useState(true);

  const loadMeetings = useCallback(async () => {
    try {
      const list = await listMeetings();
      setMeetings(list);
    } catch (err) {
      console.error('Failed to list meetings:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadMeetings();
  }, [loadMeetings]);

  // Refresh when backend emits meetings-changed event
  useEffect(() => {
    const unlistenMeetings = listen('meetings-changed', () => {
      loadMeetings();
    });
    const unlistenSummary = listen('summary-generated', () => {
      loadMeetings();
    });
    return () => {
      unlistenMeetings.then((u) => u());
      unlistenSummary.then((u) => u());
    };
  }, [loadMeetings]);

  return (
    <div className={s.scrollPage}>
      <div className={s.scrollPageInner}>
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
              <Link
                key={meeting.transcript_path}
                className={s.meetingCard}
                to={`/meetings/${encodeURIComponent(meetingSlug(meeting.date, meeting.time, meeting.name))}`}
                state={{ meeting }}
              >
                <div className={s.cardHeader}>
                  <span className={s.cardName}>{meeting.name}</span>
                  <div className={s.cardBadges}>
                    {meeting.has_summary && (
                      <span className={s.summaryBadge}>Summary</span>
                    )}
                    <span className={s.cardSize}>
                      {formatFileSize(meeting.size_bytes)}
                    </span>
                  </div>
                </div>
                <div className={s.cardMeta}>
                  <span className={s.cardDate}>{meeting.date}</span>
                  <span className={s.cardTime}>{meeting.time}</span>
                </div>
              </Link>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
