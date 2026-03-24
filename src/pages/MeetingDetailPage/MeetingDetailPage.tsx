import { useState, useEffect, useCallback } from 'react';
import { useParams, useLocation, Link } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import {
  listMeetings,
  readMeetingTranscript,
  readMeetingSummary,
  generateSummary,
} from '../../lib/commands';
import { meetingSlug } from '../../lib/format';
import { Button } from '../../components/Button';
import { Panel } from '../../components/Panel';
import type { MeetingEntry } from '../../types';
import s from './MeetingDetailPage.module.scss';

/** Which view tab is active in the detail viewer. */
type ViewTab = 'summary' | 'transcript';

/** Page for viewing a single meeting's transcript and summary. */
export function MeetingDetailPage() {
  const { meetingId } = useParams<{ meetingId: string }>();
  const location = useLocation();

  const [meeting, setMeeting] = useState<MeetingEntry | null>(
    (location.state as { meeting?: MeetingEntry } | null)?.meeting ?? null,
  );
  const [viewTab, setViewTab] = useState<ViewTab>('summary');
  const [content, setContent] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // If we don't have meeting data from Link state, look it up from the list
  useEffect(() => {
    if (meeting || !meetingId) return;

    let cancelled = false;
    (async () => {
      try {
        const meetings = await listMeetings();
        const found = meetings.find(
          (m) => meetingSlug(m.date, m.time, m.name) === meetingId,
        );
        if (cancelled) return;
        if (found) {
          setMeeting(found);
        } else {
          setError('Meeting not found.');
        }
      } catch {
        if (!cancelled) setError('Failed to load meeting.');
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [meeting, meetingId]);

  // Load content once we have the meeting
  useEffect(() => {
    if (!meeting) return;

    let cancelled = false;
    const tab = meeting.has_summary ? 'summary' : 'transcript';
    setViewTab(tab);

    (async () => {
      try {
        const text =
          tab === 'summary' && meeting.has_summary
            ? await readMeetingSummary(meeting.summary_path)
            : await readMeetingTranscript(meeting.transcript_path);
        if (!cancelled) setContent(text);
      } catch {
        if (!cancelled) setContent('Failed to load file.');
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [meeting]);

  // Listen for summary generation events
  useEffect(() => {
    const unlistenSummary = listen('summary-generated', () => {
      setGenerating(false);
      // Reload the meeting to pick up the new summary
      if (!meetingId) return;
      (async () => {
        try {
          const meetings = await listMeetings();
          const found = meetings.find(
            (m) => meetingSlug(m.date, m.time, m.name) === meetingId,
          );
          if (found) {
            setMeeting(found);
            setContent(null);
          }
        } catch {
          // ignore
        }
      })();
    });
    const unlistenError = listen('summary-error', (event) => {
      console.error('Summary generation failed:', event.payload);
      setGenerating(false);
    });
    const unlistenGenerating = listen('summary-generating', () => {
      setGenerating(true);
    });
    return () => {
      unlistenSummary.then((u) => u());
      unlistenError.then((u) => u());
      unlistenGenerating.then((u) => u());
    };
  }, [meetingId]);

  /** Switch between summary and transcript tabs. */
  const handleTabChange = useCallback(
    async (tab: ViewTab) => {
      if (!meeting) return;
      setViewTab(tab);
      setContent(null);
      try {
        const text =
          tab === 'summary' && meeting.has_summary
            ? await readMeetingSummary(meeting.summary_path)
            : await readMeetingTranscript(meeting.transcript_path);
        setContent(text);
      } catch {
        setContent('Failed to load file.');
      }
    },
    [meeting],
  );

  /** Trigger manual summary generation for this meeting. */
  const handleGenerateSummary = useCallback(async () => {
    if (!meeting) return;
    setGenerating(true);
    try {
      await generateSummary(meeting.transcript_path);
    } catch (err) {
      console.error('Failed to trigger summary generation:', err);
      setGenerating(false);
    }
  }, [meeting]);

  if (error) {
    return (
      <div className={s.fixedPage}>
        <header className={s.header}>
          <Link to="/meetings" className={s.backLink}>
            Back to Meetings
          </Link>
        </header>
        <p className={s.errorState}>{error}</p>
      </div>
    );
  }

  if (!meeting || content === null) {
    return (
      <div className={s.fixedPage}>
        <header className={s.header}>
          <Link to="/meetings" className={s.backLink}>
            Back to Meetings
          </Link>
        </header>
        <p className={s.loadingState}>Loading...</p>
      </div>
    );
  }

  return (
    <div className={s.fixedPage}>
      <header className={s.header}>
        <Link to="/meetings" className={s.backLink}>
          Back to Meetings
        </Link>
      </header>

      {/* Tab toggle when summary exists */}
      {meeting.has_summary && (
        <div className={s.tabBar}>
          <button
            className={`${s.tab} ${viewTab === 'summary' ? s.tabActive : ''}`}
            onClick={() => handleTabChange('summary')}
          >
            Summary
          </button>
          <button
            className={`${s.tab} ${viewTab === 'transcript' ? s.tabActive : ''}`}
            onClick={() => handleTabChange('transcript')}
          >
            Transcript
          </button>
        </div>
      )}

      {/* Generate summary button when no summary exists */}
      {!meeting.has_summary && (
        <div className={s.generateRow}>
          <Button onClick={handleGenerateSummary} disabled={generating}>
            {generating ? 'Generating Summary...' : 'Generate Summary'}
          </Button>
        </div>
      )}

      <Panel className={s.detailPanel}>
        <pre className={s.transcriptContent}>{content}</pre>
      </Panel>
    </div>
  );
}
