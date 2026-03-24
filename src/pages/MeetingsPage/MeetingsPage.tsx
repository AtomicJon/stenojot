import { useState, useEffect, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  listMeetings,
  readMeetingTranscript,
  readMeetingSummary,
  generateSummary,
} from '../../lib/commands';
import { formatFileSize } from '../../lib/format';
import { Button } from '../../components/Button';
import { Panel } from '../../components/Panel';
import type { MeetingEntry } from '../../types';
import s from './MeetingsPage.module.scss';

/** Which view tab is active in the detail viewer. */
type ViewTab = 'summary' | 'transcript';

/** Page for browsing past meeting transcripts and summaries. */
export function MeetingsPage() {
  const [meetings, setMeetings] = useState<MeetingEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedMeeting, setSelectedMeeting] = useState<MeetingEntry | null>(
    null,
  );
  const [viewTab, setViewTab] = useState<ViewTab>('summary');
  const [content, setContent] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);

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
      setGenerating(false);
    });
    const unlistenError = listen('summary-error', (event) => {
      console.error('Summary generation failed:', event.payload);
      setGenerating(false);
    });
    const unlistenGenerating = listen('summary-generating', () => {
      setGenerating(true);
    });
    return () => {
      unlistenMeetings.then((u) => u());
      unlistenSummary.then((u) => u());
      unlistenError.then((u) => u());
      unlistenGenerating.then((u) => u());
    };
  }, [loadMeetings]);

  /** Open a meeting's content for reading. */
  const handleOpen = useCallback(async (meeting: MeetingEntry) => {
    setSelectedMeeting(meeting);
    const tab = meeting.has_summary ? 'summary' : 'transcript';
    setViewTab(tab);
    try {
      const text =
        tab === 'summary' && meeting.has_summary
          ? await readMeetingSummary(meeting.summary_path)
          : await readMeetingTranscript(meeting.transcript_path);
      setContent(text);
    } catch (err) {
      console.error('Failed to read meeting file:', err);
      setContent('Failed to load file.');
    }
  }, []);

  /** Switch between summary and transcript tabs. */
  const handleTabChange = useCallback(
    async (tab: ViewTab) => {
      if (!selectedMeeting) return;
      setViewTab(tab);
      setContent(null);
      try {
        const text =
          tab === 'summary' && selectedMeeting.has_summary
            ? await readMeetingSummary(selectedMeeting.summary_path)
            : await readMeetingTranscript(selectedMeeting.transcript_path);
        setContent(text);
      } catch (err) {
        console.error('Failed to read meeting file:', err);
        setContent('Failed to load file.');
      }
    },
    [selectedMeeting],
  );

  /** Go back to the meeting list. */
  const handleBack = useCallback(() => {
    setSelectedMeeting(null);
    setContent(null);
  }, []);

  /** Trigger manual summary generation for the selected meeting. */
  const handleGenerateSummary = useCallback(async () => {
    if (!selectedMeeting) return;
    setGenerating(true);
    try {
      await generateSummary(selectedMeeting.transcript_path);
    } catch (err) {
      console.error('Failed to trigger summary generation:', err);
      setGenerating(false);
    }
  }, [selectedMeeting]);

  // Detail view for a selected meeting
  if (selectedMeeting && content !== null) {
    return (
      <div className={s.fixedPage}>
        <header className={s.header}>
          <Button variant="link" onClick={handleBack}>
            Back to Meetings
          </Button>
        </header>

        {/* Tab toggle when summary exists */}
        {selectedMeeting.has_summary && (
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
        {!selectedMeeting.has_summary && (
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
              <button
                key={meeting.transcript_path}
                className={s.meetingCard}
                onClick={() => handleOpen(meeting)}
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
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
