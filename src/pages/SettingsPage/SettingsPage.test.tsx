import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
} from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { SettingsPage } from './SettingsPage';
import type { ModelInfo, PersistedSettings } from '../../types';

/* ── Mocks ───────────────────────────────────────────────────────── */

vi.mock('../../hooks/useRecording', () => ({
  useRecording: () => ({
    isRecording: false,
    micDevices: [],
    systemDevices: [],
    micDeviceId: '',
    systemDeviceId: '',
    setMicDeviceId: vi.fn(),
    setSystemDeviceId: vi.fn(),
    refreshModelStatus: vi.fn(),
  }),
}));

vi.mock('../../lib/commands', () => ({
  getModelInfo: vi.fn(),
  downloadModel: vi.fn(),
  deleteModel: vi.fn(),
  setModelsDir: vi.fn(),
  getOutputDir: vi.fn(),
  setOutputDir: vi.fn(),
  setSilenceTimeout: vi.fn(),
  getSettings: vi.fn(),
  setWhisperModel: vi.fn(),
  setSttEngine: vi.fn(),
  setSttModel: vi.fn(),
  getEngineModels: vi.fn(),
  setInitialPrompt: vi.fn(),
  setMaxSegmentSeconds: vi.fn(),
  setLlmProvider: vi.fn(),
  setLlmModel: vi.fn(),
  setLlmApiKey: vi.fn(),
  setLlmBaseUrl: vi.fn(),
  setAutoSummary: vi.fn(),
}));

type ListenCallback = (event: { payload: unknown }) => void;
let capturedListeners: Map<string, ListenCallback>;

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, cb: ListenCallback) => {
    capturedListeners.set(event, cb);
    return Promise.resolve(() => {
      capturedListeners.delete(event);
    });
  }),
}));

import {
  getModelInfo,
  downloadModel,
  getSettings,
  getOutputDir,
  getEngineModels,
} from '../../lib/commands';

const mockGetModelInfo = vi.mocked(getModelInfo);
const mockDownloadModel = vi.mocked(downloadModel);
const mockGetSettings = vi.mocked(getSettings);
const mockGetOutputDir = vi.mocked(getOutputDir);
const mockGetEngineModels = vi.mocked(getEngineModels);

/* ── Fixtures ────────────────────────────────────────────────────── */

const notDownloadedModel: ModelInfo = {
  name: 'base',
  path: '/models/ggml-base.bin',
  downloaded: false,
  size_bytes: 0,
  models_dir: '/models',
  engine: 'whisper',
};

const downloadedModel: ModelInfo = {
  name: 'base',
  path: '/models/ggml-base.bin',
  downloaded: true,
  size_bytes: 147_951_465,
  models_dir: '/models',
  engine: 'whisper',
};

const defaultSettings: PersistedSettings = {
  mic_device_id: null,
  system_device_id: null,
  mic_gain: 1.0,
  vad_threshold: 0.01,
  models_dir: null,
  output_dir: null,
  stt_engine: 'whisper',
  whisper_model: 'base',
  stt_model: null,
  silence_timeout_seconds: 300,
  initial_prompt: '',
  max_segment_seconds: 15,
  llm_provider: 'ollama',
  llm_model: '',
  llm_api_key: '',
  llm_base_url: '',
  auto_summary: true,
};

/* ── Helpers ─────────────────────────────────────────────────────── */

function renderPage() {
  return render(
    <MemoryRouter>
      <SettingsPage />
    </MemoryRouter>,
  );
}

/* ── Tests ───────────────────────────────────────────────────────── */

describe('SettingsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    capturedListeners = new Map();
    mockGetSettings.mockResolvedValue(defaultSettings);
    mockGetOutputDir.mockResolvedValue('/output');
    mockGetEngineModels.mockResolvedValue([
      { id: 'base', label: 'Base (~142 MB)', engine: 'whisper', hf_repo: null },
    ]);
  });

  it('shows Download Model button when model is not downloaded', async () => {
    // Arrange
    mockGetModelInfo.mockResolvedValue(notDownloadedModel);

    // Act
    renderPage();

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Download Model')).toBeInTheDocument();
    });
  });

  it('shows Delete Model button when model is downloaded', async () => {
    // Arrange
    mockGetModelInfo.mockResolvedValue(downloadedModel);

    // Act
    renderPage();

    // Assert
    await waitFor(() => {
      expect(screen.getByText('Delete Model')).toBeInTheDocument();
    });
  });

  it('shows progress bar with percentage during download', async () => {
    // Arrange
    let resolveDownload: () => void;
    mockGetModelInfo.mockResolvedValue(notDownloadedModel);
    mockDownloadModel.mockReturnValue(
      new Promise<void>((resolve) => {
        resolveDownload = resolve;
      }),
    );

    renderPage();
    await waitFor(() => {
      expect(screen.getByText('Download Model')).toBeInTheDocument();
    });

    // Act — start download
    fireEvent.click(screen.getByText('Download Model'));
    await waitFor(() => {
      expect(screen.getByText('Downloading\u2026')).toBeInTheDocument();
    });

    // Act — simulate progress event at 50%
    const listener = capturedListeners.get('download-progress');
    expect(listener).toBeDefined();
    act(() => {
      listener!({
        payload: { bytes_downloaded: 50_000_000, total_bytes: 100_000_000 },
      });
    });

    // Assert — progress label visible with percentage
    expect(screen.getByText(/50%/)).toBeInTheDocument();

    // Cleanup — complete the download
    mockGetModelInfo.mockResolvedValue(downloadedModel);
    await act(async () => {
      resolveDownload!();
    });
  });

  it('shows bytes-only label when total_bytes is 0', async () => {
    // Arrange
    let resolveDownload: () => void;
    mockGetModelInfo.mockResolvedValue(notDownloadedModel);
    mockDownloadModel.mockReturnValue(
      new Promise<void>((resolve) => {
        resolveDownload = resolve;
      }),
    );

    renderPage();
    await waitFor(() => {
      expect(screen.getByText('Download Model')).toBeInTheDocument();
    });

    // Act — start download
    fireEvent.click(screen.getByText('Download Model'));
    await waitFor(() => {
      expect(screen.getByText('Downloading\u2026')).toBeInTheDocument();
    });

    // Act — simulate progress event with unknown total
    const listener = capturedListeners.get('download-progress');
    act(() => {
      listener!({
        payload: { bytes_downloaded: 10_000_000, total_bytes: 0 },
      });
    });

    // Assert — shows bytes-only fallback
    expect(screen.getByText(/downloaded/)).toBeInTheDocument();
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();

    // Cleanup
    mockGetModelInfo.mockResolvedValue(downloadedModel);
    await act(async () => {
      resolveDownload!();
    });
  });

  it('clears progress bar after download completes', async () => {
    // Arrange
    mockGetModelInfo.mockResolvedValue(notDownloadedModel);
    mockDownloadModel.mockResolvedValue(undefined);

    renderPage();
    await waitFor(() => {
      expect(screen.getByText('Download Model')).toBeInTheDocument();
    });

    // Act — start and complete download
    mockGetModelInfo.mockResolvedValue(downloadedModel);
    fireEvent.click(screen.getByText('Download Model'));

    // Assert — after completion, progress bar should be gone
    await waitFor(() => {
      expect(screen.queryByText(/%/)).not.toBeInTheDocument();
    });
  });
});
