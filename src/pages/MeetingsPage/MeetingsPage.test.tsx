import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { MeetingsPage } from './MeetingsPage';

vi.mock('../../lib/commands', () => ({
  listMeetings: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { listMeetings } from '../../lib/commands';

const mockListMeetings = vi.mocked(listMeetings);

describe('MeetingsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows empty state when no meetings exist', async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([]);

    // Act
    render(
      <MemoryRouter>
        <MeetingsPage />
      </MemoryRouter>,
    );

    // Assert
    await waitFor(() => {
      expect(screen.getByText(/no meetings yet/i)).toBeInTheDocument();
    });
  });

  it('renders meeting cards from backend data', async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: 'Standup',
        date: '2026-03-22',
        time: '14:00',
        has_transcript: true,
        has_summary: false,
        transcript_path: '/output/standup.md',
        summary_path: '',
        size_bytes: 2048,
      },
      {
        name: 'Planning',
        date: '2026-03-22',
        time: '15:30',
        has_transcript: true,
        has_summary: false,
        transcript_path: '/output/planning.md',
        summary_path: '',
        size_bytes: 4096,
      },
    ]);

    // Act
    render(
      <MemoryRouter>
        <MeetingsPage />
      </MemoryRouter>,
    );

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Standup')).toBeInTheDocument();
      expect(screen.getByText('Planning')).toBeInTheDocument();
    });
    expect(screen.getAllByText('2026-03-22')).toHaveLength(2);
  });

  it('meeting cards link to detail pages', async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: 'Standup',
        date: '2026-03-22',
        time: '14:00',
        has_transcript: true,
        has_summary: false,
        transcript_path: '/output/standup.md',
        summary_path: '',
        size_bytes: 1024,
      },
    ]);

    // Act
    render(
      <MemoryRouter>
        <MeetingsPage />
      </MemoryRouter>,
    );

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Standup')).toBeInTheDocument();
    });
    const link = screen.getByText('Standup').closest('a');
    expect(link).toHaveAttribute(
      'href',
      '/meetings/2026-03-22_14.00_Standup',
    );
  });

  it('displays file sizes on meeting cards', async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: 'Demo',
        date: '2026-03-22',
        time: '11:00',
        has_transcript: true,
        has_summary: false,
        transcript_path: '/output/demo.md',
        summary_path: '',
        size_bytes: 1048576, // 1 MB
      },
    ]);

    // Act
    render(
      <MemoryRouter>
        <MeetingsPage />
      </MemoryRouter>,
    );

    // Assert
    await waitFor(() => {
      expect(screen.getByText('1.0 MB')).toBeInTheDocument();
    });
  });

  it('shows summary badge when meeting has a summary', async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: 'Review',
        date: '2026-03-22',
        time: '09:00',
        has_transcript: true,
        has_summary: true,
        transcript_path: '/output/review.md',
        summary_path: '/output/review-summary.md',
        size_bytes: 512,
      },
    ]);

    // Act
    render(
      <MemoryRouter>
        <MeetingsPage />
      </MemoryRouter>,
    );

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Summary')).toBeInTheDocument();
    });
  });
});
