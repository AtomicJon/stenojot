import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { MeetingDetailPage } from './MeetingDetailPage';
import type { MeetingEntry } from '../../types';

vi.mock('../../lib/commands', () => ({
  listMeetings: vi.fn(),
  readMeetingTranscript: vi.fn(),
  readMeetingSummary: vi.fn(),
  generateSummary: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import {
  listMeetings,
  readMeetingTranscript,
  readMeetingSummary,
} from '../../lib/commands';

const mockListMeetings = vi.mocked(listMeetings);
const mockReadTranscript = vi.mocked(readMeetingTranscript);
const mockReadSummary = vi.mocked(readMeetingSummary);

const testMeeting: MeetingEntry = {
  name: 'Standup',
  date: '2026-03-22',
  time: '14:00',
  has_transcript: true,
  has_summary: false,
  transcript_path: '/output/standup.md',
  summary_path: '',
  size_bytes: 1024,
};

/** Render the detail page at a given route, optionally passing Link state. */
function renderDetailPage(
  meetingId: string,
  state?: { meeting: MeetingEntry },
) {
  return render(
    <MemoryRouter
      initialEntries={[{ pathname: `/meetings/${meetingId}`, state }]}
    >
      <Routes>
        <Route path="/meetings/:meetingId" element={<MeetingDetailPage />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe('MeetingDetailPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows transcript content when meeting is passed via state', async () => {
    // Arrange
    mockReadTranscript.mockResolvedValue('# Standup Notes\n\nHello world.');

    // Act
    renderDetailPage('2026-03-22_14.00_Standup', { meeting: testMeeting });

    // Assert
    await waitFor(() => {
      expect(screen.getByText(/Standup Notes/)).toBeInTheDocument();
    });
    expect(mockReadTranscript).toHaveBeenCalledWith('/output/standup.md');
  });

  it('fetches meeting from list when no state is provided', async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([testMeeting]);
    mockReadTranscript.mockResolvedValue('Transcript content');

    // Act
    renderDetailPage('2026-03-22_14.00_Standup');

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Transcript content')).toBeInTheDocument();
    });
    expect(mockListMeetings).toHaveBeenCalled();
  });

  it('shows error when meeting is not found', async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([]);

    // Act
    renderDetailPage('nonexistent-meeting');

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Meeting not found.')).toBeInTheDocument();
    });
  });

  it('renders back link to meetings list', async () => {
    // Arrange
    mockReadTranscript.mockResolvedValue('Content');

    // Act
    renderDetailPage('2026-03-22_14.00_Standup', { meeting: testMeeting });

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Back to Meetings')).toBeInTheDocument();
    });
    const link = screen.getByText('Back to Meetings');
    expect(link.closest('a')).toHaveAttribute('href', '/meetings');
  });

  it('shows summary tab when meeting has a summary', async () => {
    // Arrange
    const meetingWithSummary: MeetingEntry = {
      ...testMeeting,
      has_summary: true,
      summary_path: '/output/standup-summary.md',
    };
    mockReadSummary.mockResolvedValue('Meeting summary here.');

    // Act
    renderDetailPage('2026-03-22_14.00_Standup', {
      meeting: meetingWithSummary,
    });

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Meeting summary here.')).toBeInTheDocument();
    });
    expect(screen.getByText('Summary')).toBeInTheDocument();
    expect(screen.getByText('Transcript')).toBeInTheDocument();
  });

  it('switches between summary and transcript tabs', async () => {
    // Arrange
    const meetingWithSummary: MeetingEntry = {
      ...testMeeting,
      has_summary: true,
      summary_path: '/output/standup-summary.md',
    };
    mockReadSummary.mockResolvedValue('Summary text');
    mockReadTranscript.mockResolvedValue('Transcript text');

    // Act
    renderDetailPage('2026-03-22_14.00_Standup', {
      meeting: meetingWithSummary,
    });
    await waitFor(() => {
      expect(screen.getByText('Summary text')).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText('Transcript'));

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Transcript text')).toBeInTheDocument();
    });
  });
});
