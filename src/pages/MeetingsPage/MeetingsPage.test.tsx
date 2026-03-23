import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MeetingsPage } from "./MeetingsPage";

vi.mock("../../lib/commands", () => ({
  listMeetings: vi.fn(),
  readMeetingTranscript: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { listMeetings, readMeetingTranscript } from "../../lib/commands";

const mockListMeetings = vi.mocked(listMeetings);
const mockReadTranscript = vi.mocked(readMeetingTranscript);

describe("MeetingsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows empty state when no meetings exist", async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([]);

    // Act
    render(<MeetingsPage />);

    // Assert
    await waitFor(() => {
      expect(screen.getByText(/no meetings yet/i)).toBeInTheDocument();
    });
  });

  it("renders meeting cards from backend data", async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: "Standup",
        date: "2026-03-22",
        time: "14:00",
        has_transcript: true,
        has_summary: false,
        transcript_path: "/output/standup.md",
        summary_path: "",
        size_bytes: 2048,
      },
      {
        name: "Planning",
        date: "2026-03-22",
        time: "15:30",
        has_transcript: true,
        has_summary: false,
        transcript_path: "/output/planning.md",
        summary_path: "",
        size_bytes: 4096,
      },
    ]);

    // Act
    render(<MeetingsPage />);

    // Assert
    await waitFor(() => {
      expect(screen.getByText("Standup")).toBeInTheDocument();
      expect(screen.getByText("Planning")).toBeInTheDocument();
    });
    expect(screen.getAllByText("2026-03-22")).toHaveLength(2);
  });

  it("opens transcript when a meeting card is clicked", async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: "Standup",
        date: "2026-03-22",
        time: "14:00",
        has_transcript: true,
        has_summary: false,
        transcript_path: "/output/standup.md",
        summary_path: "",
        size_bytes: 1024,
      },
    ]);
    mockReadTranscript.mockResolvedValue("# Standup Transcript\n\nHello.");

    // Act
    render(<MeetingsPage />);
    await waitFor(() => {
      expect(screen.getByText("Standup")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Standup"));

    // Assert
    await waitFor(() => {
      expect(screen.getByText(/Standup Transcript/)).toBeInTheDocument();
    });
    expect(mockReadTranscript).toHaveBeenCalledWith("/output/standup.md");
  });

  it("shows back button when viewing a transcript", async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: "Meeting",
        date: "2026-03-22",
        time: "10:00",
        has_transcript: true,
        has_summary: false,
        transcript_path: "/output/meeting.md",
        summary_path: "",
        size_bytes: 512,
      },
    ]);
    mockReadTranscript.mockResolvedValue("Content here");

    // Act
    render(<MeetingsPage />);
    await waitFor(() => {
      expect(screen.getByText("Meeting")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Meeting"));

    // Assert
    await waitFor(() => {
      expect(screen.getByText("Back to Meetings")).toBeInTheDocument();
    });
  });

  it("returns to meeting list when back button is clicked", async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: "Retro",
        date: "2026-03-22",
        time: "16:00",
        has_transcript: true,
        has_summary: false,
        transcript_path: "/output/retro.md",
        summary_path: "",
        size_bytes: 256,
      },
    ]);
    mockReadTranscript.mockResolvedValue("Retro content");

    // Act
    render(<MeetingsPage />);
    await waitFor(() => {
      expect(screen.getByText("Retro")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Retro"));
    await waitFor(() => {
      expect(screen.getByText("Back to Meetings")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText("Back to Meetings"));

    // Assert
    await waitFor(() => {
      expect(screen.getByText("Meetings")).toBeInTheDocument();
      expect(screen.getByText("Retro")).toBeInTheDocument();
    });
  });

  it("displays file sizes on meeting cards", async () => {
    // Arrange
    mockListMeetings.mockResolvedValue([
      {
        name: "Demo",
        date: "2026-03-22",
        time: "11:00",
        has_transcript: true,
        has_summary: false,
        transcript_path: "/output/demo.md",
        summary_path: "",
        size_bytes: 1048576, // 1 MB
      },
    ]);

    // Act
    render(<MeetingsPage />);

    // Assert
    await waitFor(() => {
      expect(screen.getByText("1.0 MB")).toBeInTheDocument();
    });
  });
});
