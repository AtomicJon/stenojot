import { useState, useEffect, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  getModelInfo,
  downloadModel,
  deleteModel,
  setModelsDir,
  getOutputDir,
  setOutputDir,
  setSilenceTimeout,
  getSettings,
  setWhisperModel,
  setInitialPrompt,
  setMaxSegmentSeconds,
  setLlmProvider,
  setLlmModel,
  setLlmApiKey,
  setLlmBaseUrl,
  setAutoSummary,
} from '../../lib/commands';
import { formatFileSize } from '../../lib/format';
import { useRecording } from '../../hooks/useRecording';
import { Button, ButtonVariant } from '../../components/Button';
import { Panel } from '../../components/Panel';
import { Select } from '../../components/Select';
import type { ModelInfo } from '../../types';
import s from './SettingsPage.module.scss';

/** Payload emitted by the Rust backend during model downloads. */
interface DownloadProgress {
  bytes_downloaded: number;
  total_bytes: number;
}

/** Settings page for managing audio sources, transcription model, and storage location. */
export function SettingsPage() {
  const {
    isRecording,
    micDevices,
    systemDevices,
    micDeviceId,
    systemDeviceId,
    setMicDeviceId,
    setSystemDeviceId,
    refreshModelStatus,
  } = useRecording();

  const [modelInfo, setModelInfo] = useState<ModelInfo | null>(null);
  const [isDownloading, setIsDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] =
    useState<DownloadProgress | null>(null);
  const [downloadGeneration, setDownloadGeneration] = useState(0);
  const [customPath, setCustomPath] = useState('');
  const [pathSaved, setPathSaved] = useState(false);
  const [outputDir, setOutputDirState] = useState('');
  const [outputDirSaved, setOutputDirSaved] = useState(false);
  const [silenceTimeout, setSilenceTimeoutState] = useState<number>(300);
  const [timeoutSaved, setTimeoutSaved] = useState(false);
  const [whisperModel, setWhisperModelState] = useState('base');
  const [initialPrompt, setInitialPromptState] = useState('');
  const [promptSaved, setPromptSaved] = useState(false);
  const [maxSegmentSeconds, setMaxSegmentSecondsState] = useState(15);
  const [segmentSaved, setSegmentSaved] = useState(false);
  const [llmProviderValue, setLlmProviderValue] = useState('ollama');
  const [llmModelValue, setLlmModelValue] = useState('');
  const [llmApiKeyValue, setLlmApiKeyValue] = useState('');
  const [llmBaseUrlValue, setLlmBaseUrlValue] = useState('');
  const [autoSummaryValue, setAutoSummaryValue] = useState(true);
  const [llmSaved, setLlmSaved] = useState(false);

  /** Load model info from the backend. */
  const loadModelInfo = useCallback(async () => {
    try {
      const info = await getModelInfo();
      setModelInfo(info);
      setCustomPath(info.models_dir);
    } catch (err) {
      console.error('Failed to get model info:', err);
    }
  }, []);

  useEffect(() => {
    async function loadInitialSettings() {
      try {
        const [info, dir, settings] = await Promise.all([
          getModelInfo(),
          getOutputDir(),
          getSettings(),
        ]);
        setModelInfo(info);
        setCustomPath(info.models_dir);
        setOutputDirState(dir);
        setSilenceTimeoutState(settings.silence_timeout_seconds ?? 0);
        setWhisperModelState(settings.whisper_model);
        setInitialPromptState(settings.initial_prompt ?? '');
        setMaxSegmentSecondsState(settings.max_segment_seconds);
        setLlmProviderValue(settings.llm_provider);
        setLlmModelValue(settings.llm_model ?? '');
        setLlmApiKeyValue(settings.llm_api_key ?? '');
        setLlmBaseUrlValue(settings.llm_base_url ?? '');
        setAutoSummaryValue(settings.auto_summary);
      } catch (err) {
        console.error('Failed to load settings:', err);
      }
    }
    loadInitialSettings();
  }, []);

  /** Subscribe to download events while this page is mounted. */
  useEffect(() => {
    const progressPromise = listen<DownloadProgress>(
      'download-progress',
      (event) => {
        setDownloadProgress(event.payload);
      },
    );
    const completePromise = listen('download-complete', () => {
      setIsDownloading(false);
      setDownloadProgress(null);
      setDownloadGeneration((g) => g + 1);
    });
    const errorPromise = listen<string>('download-error', (event) => {
      console.error('Model download failed:', event.payload);
      setIsDownloading(false);
      setDownloadProgress(null);
    });
    return () => {
      progressPromise.then((u) => u());
      completePromise.then((u) => u());
      errorPromise.then((u) => u());
    };
  }, []);

  /** Reload model info after a successful download. */
  useEffect(() => {
    if (downloadGeneration === 0) return;
    async function reload() {
      try {
        const info = await getModelInfo();
        setModelInfo(info);
        setCustomPath(info.models_dir);
      } catch (err) {
        console.error('Failed to reload model info:', err);
      }
      refreshModelStatus();
    }
    reload();
  }, [downloadGeneration, refreshModelStatus]);

  /** Kick off a background model download. */
  const handleDownload = useCallback(async () => {
    setIsDownloading(true);
    setDownloadProgress(null);
    try {
      await downloadModel();
    } catch (err) {
      console.error('Failed to start model download:', err);
      setIsDownloading(false);
    }
  }, []);

  /** Delete the Whisper transcription model file. */
  const handleDelete = useCallback(async () => {
    try {
      await deleteModel();
      await loadModelInfo();
      await refreshModelStatus();
    } catch (err) {
      console.error('Failed to delete model:', err);
    }
  }, [loadModelInfo, refreshModelStatus]);

  /** Save a custom models directory path. */
  const handleSavePath = useCallback(async () => {
    try {
      await setModelsDir(customPath);
      setPathSaved(true);
      await loadModelInfo();
      setTimeout(() => setPathSaved(false), 2000);
    } catch (err) {
      console.error('Failed to set models dir:', err);
    }
  }, [customPath, loadModelInfo]);

  /** Reset the models directory to the default location. */
  const handleResetPath = useCallback(async () => {
    try {
      await setModelsDir('');
      await loadModelInfo();
    } catch (err) {
      console.error('Failed to reset models dir:', err);
    }
  }, [loadModelInfo]);

  /** Save a custom output directory path. */
  const handleSaveOutputDir = useCallback(async () => {
    try {
      await setOutputDir(outputDir);
      setOutputDirSaved(true);
      setTimeout(() => setOutputDirSaved(false), 2000);
    } catch (err) {
      console.error('Failed to set output dir:', err);
    }
  }, [outputDir]);

  /** Reset the output directory to the default location. */
  const handleResetOutputDir = useCallback(async () => {
    try {
      await setOutputDir('');
      const dir = await getOutputDir();
      setOutputDirState(dir);
    } catch (err) {
      console.error('Failed to reset output dir:', err);
    }
  }, []);

  /** Save the silence timeout setting. */
  const handleSaveSilenceTimeout = useCallback(async () => {
    try {
      await setSilenceTimeout(silenceTimeout);
      setTimeoutSaved(true);
      setTimeout(() => setTimeoutSaved(false), 2000);
    } catch (err) {
      console.error('Failed to set silence timeout:', err);
    }
  }, [silenceTimeout]);

  /** Change the Whisper model and reload model info. */
  const handleModelChange = useCallback(
    async (model: string) => {
      setWhisperModelState(model);
      try {
        await setWhisperModel(model);
        await loadModelInfo();
        await refreshModelStatus();
      } catch (err) {
        console.error('Failed to set whisper model:', err);
      }
    },
    [loadModelInfo, refreshModelStatus],
  );

  /** Save the initial prompt setting. */
  const handleSavePrompt = useCallback(async () => {
    try {
      await setInitialPrompt(initialPrompt);
      setPromptSaved(true);
      setTimeout(() => setPromptSaved(false), 2000);
    } catch (err) {
      console.error('Failed to set initial prompt:', err);
    }
  }, [initialPrompt]);

  /** Handle LLM provider change. */
  const handleLlmProviderChange = useCallback(async (provider: string) => {
    setLlmProviderValue(provider);
    setLlmSaved(false);
    try {
      await setLlmProvider(provider);
    } catch (err) {
      console.error('Failed to set LLM provider:', err);
    }
  }, []);

  /** Save LLM settings (model, API key, base URL). */
  const handleSaveLlmSettings = useCallback(async () => {
    try {
      await Promise.all([
        setLlmModel(llmModelValue),
        setLlmApiKey(llmApiKeyValue),
        setLlmBaseUrl(llmBaseUrlValue),
      ]);
      setLlmSaved(true);
      setTimeout(() => setLlmSaved(false), 2000);
    } catch (err) {
      console.error('Failed to save LLM settings:', err);
    }
  }, [llmModelValue, llmApiKeyValue, llmBaseUrlValue]);

  /** Toggle auto-summary generation. */
  const handleAutoSummaryToggle = useCallback(async () => {
    const newValue = !autoSummaryValue;
    setAutoSummaryValue(newValue);
    try {
      await setAutoSummary(newValue);
    } catch (err) {
      console.error('Failed to set auto-summary:', err);
    }
  }, [autoSummaryValue]);

  /** Save the max segment seconds setting. */
  const handleSaveMaxSegment = useCallback(async () => {
    try {
      await setMaxSegmentSeconds(maxSegmentSeconds);
      setSegmentSaved(true);
      setTimeout(() => setSegmentSaved(false), 2000);
    } catch (err) {
      console.error('Failed to set max segment seconds:', err);
    }
  }, [maxSegmentSeconds]);

  const micOptions = micDevices.map((d) => ({
    value: d.id,
    label: d.name + (d.is_default ? ' (Default)' : ''),
  }));

  const systemOptions = systemDevices.map((d) => ({
    value: d.id,
    label: d.name + (d.is_default ? ' (Default)' : ''),
  }));

  if (!modelInfo) {
    return (
      <div className={s.scrollPage}>
        <div className={s.scrollPageInner}>
          <header className={s.header}>
            <h1>Settings</h1>
          </header>
          <div className={s.loading}>Loading settings...</div>
        </div>
      </div>
    );
  }

  return (
    <div className={s.scrollPage}>
      <div className={s.scrollPageInner}>
        {/* Header */}
        <header className={s.header}>
          <h1>Settings</h1>
        </header>

        {/* Audio Sources */}
        <Panel title="Audio Sources">
          {isRecording && (
            <p className={s.disabledNote}>
              Audio sources cannot be changed while recording.
            </p>
          )}
          <div className={s.fieldGroup}>
            <Select
              label="Microphone"
              value={micDeviceId}
              options={micOptions}
              onChange={setMicDeviceId}
              disabled={isRecording}
            />
            <Select
              label="System Audio"
              value={systemDeviceId}
              options={systemOptions}
              onChange={setSystemDeviceId}
              disabled={isRecording}
            />
          </div>
        </Panel>

        {/* Model Management */}
        <Panel title="Transcription Model">
          <div className={s.fieldGroup}>
            <Select
              label="Model"
              value={whisperModel}
              options={[
                {
                  value: 'tiny',
                  label: 'Tiny (~75 MB — fastest, least accurate)',
                },
                {
                  value: 'base',
                  label: 'Base (~142 MB — fast, good accuracy)',
                },
                { value: 'small', label: 'Small (~466 MB — balanced)' },
                {
                  value: 'medium',
                  label: 'Medium (~1.5 GB — slower, high accuracy)',
                },
                {
                  value: 'large-v3-turbo',
                  label: 'Large V3 Turbo (~1.6 GB — fast, very accurate)',
                },
              ]}
              onChange={handleModelChange}
              disabled={isRecording}
            />
          </div>

          <div className={s.infoGrid}>
            <span className={s.infoLabel}>Model</span>
            <span className={s.infoValue}>{modelInfo.name}</span>

            <span className={s.infoLabel}>Status</span>
            <span
              className={
                modelInfo.downloaded ? s.statusDownloaded : s.statusMissing
              }
            >
              {modelInfo.downloaded ? 'Downloaded' : 'Not Downloaded'}
            </span>

            <span className={s.infoLabel}>Path</span>
            <span className={s.infoValueMono} title={modelInfo.path}>
              {modelInfo.path}
            </span>

            <span className={s.infoLabel}>Size</span>
            <span className={s.infoValue}>
              {modelInfo.downloaded
                ? formatFileSize(modelInfo.size_bytes)
                : '\u2014'}
            </span>

            <span className={s.infoLabel}>Directory</span>
            <span className={s.infoValueMono} title={modelInfo.models_dir}>
              {modelInfo.models_dir}
            </span>
          </div>

          <div className={s.actions}>
            {!modelInfo.downloaded && (
              <Button onClick={handleDownload} disabled={isDownloading}>
                {isDownloading ? 'Downloading\u2026' : 'Download Model'}
              </Button>
            )}
            {modelInfo.downloaded && (
              <Button variant={ButtonVariant.danger} onClick={handleDelete}>
                Delete Model
              </Button>
            )}
          </div>

          {isDownloading && downloadProgress && (
            <div className={s.progressWrapper}>
              <div className={s.progressTrack}>
                <div
                  className={s.progressFill}
                  style={{
                    width:
                      downloadProgress.total_bytes > 0
                        ? `${(downloadProgress.bytes_downloaded / downloadProgress.total_bytes) * 100}%`
                        : '0%',
                  }}
                />
              </div>
              <span className={s.progressLabel}>
                {downloadProgress.total_bytes > 0
                  ? `${formatFileSize(downloadProgress.bytes_downloaded)} / ${formatFileSize(downloadProgress.total_bytes)} (${Math.round((downloadProgress.bytes_downloaded / downloadProgress.total_bytes) * 100)}%)`
                  : `${formatFileSize(downloadProgress.bytes_downloaded)} downloaded`}
              </span>
            </div>
          )}
        </Panel>

        {/* Storage Location */}
        <Panel title="Storage Location">
          <p className={s.sectionDesc}>
            Set a custom directory for storing transcription models.
          </p>

          <div className={s.pathInputRow}>
            <input
              type="text"
              className={s.pathInput}
              value={customPath}
              onChange={(e) => {
                setCustomPath(e.target.value);
                setPathSaved(false);
              }}
              placeholder="Enter custom models directory path"
            />
            <Button onClick={handleSavePath}>
              {pathSaved ? 'Saved' : 'Save'}
            </Button>
          </div>

          <Button variant={ButtonVariant.link} onClick={handleResetPath}>
            Reset to Default
          </Button>
        </Panel>

        {/* Output Directory */}
        <Panel title="Output Directory">
          <p className={s.sectionDesc}>
            Where transcript files are saved. Defaults to ~/StenoJot/.
          </p>

          <div className={s.pathInputRow}>
            <input
              type="text"
              className={s.pathInput}
              value={outputDir}
              onChange={(e) => {
                setOutputDirState(e.target.value);
                setOutputDirSaved(false);
              }}
              placeholder="Enter custom output directory path"
            />
            <Button onClick={handleSaveOutputDir}>
              {outputDirSaved ? 'Saved' : 'Save'}
            </Button>
          </div>

          <Button variant={ButtonVariant.link} onClick={handleResetOutputDir}>
            Reset to Default
          </Button>
        </Panel>

        {/* AI Summary */}
        <Panel title="AI Summary">
          <p className={s.sectionDesc}>
            Configure LLM provider for post-meeting summary generation.
          </p>

          <div className={s.fieldGroup}>
            <Select
              label="Provider"
              value={llmProviderValue}
              options={[
                { value: 'ollama', label: 'Ollama (Local)' },
                { value: 'anthropic', label: 'Anthropic (Claude)' },
                { value: 'openai', label: 'OpenAI (GPT)' },
              ]}
              onChange={handleLlmProviderChange}
            />
          </div>

          <div className={s.pathInputRow}>
            <input
              type="text"
              className={s.pathInput}
              value={llmModelValue}
              onChange={(e) => {
                setLlmModelValue(e.target.value);
                setLlmSaved(false);
              }}
              placeholder={
                llmProviderValue === 'anthropic'
                  ? 'claude-sonnet-4-20250514'
                  : llmProviderValue === 'openai'
                    ? 'gpt-4o'
                    : 'llama3.1'
              }
            />
          </div>

          {(llmProviderValue === 'anthropic' ||
            llmProviderValue === 'openai') && (
            <div className={s.pathInputRow}>
              <input
                type="password"
                className={s.pathInput}
                value={llmApiKeyValue}
                onChange={(e) => {
                  setLlmApiKeyValue(e.target.value);
                  setLlmSaved(false);
                }}
                placeholder="API Key"
              />
            </div>
          )}

          <div className={s.pathInputRow}>
            <input
              type="text"
              className={s.pathInput}
              value={llmBaseUrlValue}
              onChange={(e) => {
                setLlmBaseUrlValue(e.target.value);
                setLlmSaved(false);
              }}
              placeholder={
                llmProviderValue === 'ollama'
                  ? 'http://localhost:11434'
                  : 'Custom base URL (optional)'
              }
            />
            <Button onClick={handleSaveLlmSettings}>
              {llmSaved ? 'Saved' : 'Save'}
            </Button>
          </div>

          <label className={s.checkboxRow}>
            <input
              type="checkbox"
              checked={autoSummaryValue}
              onChange={handleAutoSummaryToggle}
            />
            <span>Auto-generate summary after recording stops</span>
          </label>
        </Panel>

        {/* Auto-Stop */}
        <Panel title="Auto-Stop">
          <p className={s.sectionDesc}>
            Automatically stop recording after a period of silence. Set to 0 to
            disable.
          </p>

          <div className={s.pathInputRow}>
            <input
              type="number"
              className={s.numberInput}
              value={silenceTimeout}
              onChange={(e) => {
                setSilenceTimeoutState(
                  Math.max(0, parseInt(e.target.value) || 0),
                );
                setTimeoutSaved(false);
              }}
              min={0}
              step={30}
            />
            <span className={s.inputSuffix}>seconds</span>
            <Button onClick={handleSaveSilenceTimeout}>
              {timeoutSaved ? 'Saved' : 'Save'}
            </Button>
          </div>
        </Panel>

        {/* Initial Prompt */}
        <Panel title="Initial Prompt">
          <p className={s.sectionDesc}>
            Provide domain-specific terms, names, or jargon to improve
            transcription accuracy. Leave empty for default behavior.
          </p>

          <div className={s.pathInputRow}>
            <input
              type="text"
              className={s.pathInput}
              value={initialPrompt}
              onChange={(e) => {
                setInitialPromptState(e.target.value);
                setPromptSaved(false);
              }}
              placeholder="e.g. Kubernetes, PostgreSQL, StenoJot, Jon"
            />
            <Button onClick={handleSavePrompt}>
              {promptSaved ? 'Saved' : 'Save'}
            </Button>
          </div>
        </Panel>

        {/* Max Segment Duration */}
        <Panel title="Max Segment Duration">
          <p className={s.sectionDesc}>
            Maximum audio duration before forcing transcription. Larger values
            reduce overhead but increase latency (1–30 seconds).
          </p>

          <div className={s.pathInputRow}>
            <input
              type="number"
              className={s.numberInput}
              value={maxSegmentSeconds}
              onChange={(e) => {
                setMaxSegmentSecondsState(
                  Math.min(30, Math.max(1, parseInt(e.target.value) || 1)),
                );
                setSegmentSaved(false);
              }}
              min={1}
              max={30}
              step={1}
            />
            <span className={s.inputSuffix}>seconds</span>
            <Button onClick={handleSaveMaxSegment}>
              {segmentSaved ? 'Saved' : 'Save'}
            </Button>
          </div>
        </Panel>
      </div>
    </div>
  );
}
