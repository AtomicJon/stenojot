import { useState, useEffect, useCallback } from "react";
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
} from "../../lib/commands";
import { formatFileSize } from "../../lib/format";
import { useRecording } from "../../hooks/useRecording";
import { Button } from "../../components/Button";
import { Panel } from "../../components/Panel";
import { Select } from "../../components/Select";
import type { ModelInfo } from "../../types";
import s from "./SettingsPage.module.scss";

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
  const [customPath, setCustomPath] = useState("");
  const [pathSaved, setPathSaved] = useState(false);
  const [outputDir, setOutputDirState] = useState("");
  const [outputDirSaved, setOutputDirSaved] = useState(false);
  const [silenceTimeout, setSilenceTimeoutState] = useState<number>(300);
  const [timeoutSaved, setTimeoutSaved] = useState(false);
  const [whisperModel, setWhisperModelState] = useState("base");
  const [initialPrompt, setInitialPromptState] = useState("");
  const [promptSaved, setPromptSaved] = useState(false);
  const [maxSegmentSeconds, setMaxSegmentSecondsState] = useState(15);
  const [segmentSaved, setSegmentSaved] = useState(false);

  /** Load model info from the backend. */
  const loadModelInfo = useCallback(async () => {
    try {
      const info = await getModelInfo();
      setModelInfo(info);
      setCustomPath(info.models_dir);
    } catch (err) {
      console.error("Failed to get model info:", err);
    }
  }, []);

  useEffect(() => {
    loadModelInfo();
    async function loadOutputSettings() {
      try {
        const [dir, settings] = await Promise.all([getOutputDir(), getSettings()]);
        setOutputDirState(dir);
        setSilenceTimeoutState(settings.silence_timeout_seconds ?? 0);
        setWhisperModelState(settings.whisper_model);
        setInitialPromptState(settings.initial_prompt ?? "");
        setMaxSegmentSecondsState(settings.max_segment_seconds);
      } catch (err) {
        console.error("Failed to load output settings:", err);
      }
    }
    loadOutputSettings();
  }, [loadModelInfo]);

  /** Download the Whisper transcription model. */
  const handleDownload = useCallback(async () => {
    setIsDownloading(true);
    try {
      await downloadModel();
      await loadModelInfo();
      await refreshModelStatus();
    } catch (err) {
      console.error("Failed to download model:", err);
    } finally {
      setIsDownloading(false);
    }
  }, [loadModelInfo, refreshModelStatus]);

  /** Delete the Whisper transcription model file. */
  const handleDelete = useCallback(async () => {
    try {
      await deleteModel();
      await loadModelInfo();
      await refreshModelStatus();
    } catch (err) {
      console.error("Failed to delete model:", err);
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
      console.error("Failed to set models dir:", err);
    }
  }, [customPath, loadModelInfo]);

  /** Reset the models directory to the default location. */
  const handleResetPath = useCallback(async () => {
    try {
      await setModelsDir("");
      await loadModelInfo();
    } catch (err) {
      console.error("Failed to reset models dir:", err);
    }
  }, [loadModelInfo]);

  /** Save a custom output directory path. */
  const handleSaveOutputDir = useCallback(async () => {
    try {
      await setOutputDir(outputDir);
      setOutputDirSaved(true);
      setTimeout(() => setOutputDirSaved(false), 2000);
    } catch (err) {
      console.error("Failed to set output dir:", err);
    }
  }, [outputDir]);

  /** Reset the output directory to the default location. */
  const handleResetOutputDir = useCallback(async () => {
    try {
      await setOutputDir("");
      const dir = await getOutputDir();
      setOutputDirState(dir);
    } catch (err) {
      console.error("Failed to reset output dir:", err);
    }
  }, []);

  /** Save the silence timeout setting. */
  const handleSaveSilenceTimeout = useCallback(async () => {
    try {
      await setSilenceTimeout(silenceTimeout);
      setTimeoutSaved(true);
      setTimeout(() => setTimeoutSaved(false), 2000);
    } catch (err) {
      console.error("Failed to set silence timeout:", err);
    }
  }, [silenceTimeout]);

  /** Change the Whisper model and reload model info. */
  const handleModelChange = useCallback(async (model: string) => {
    setWhisperModelState(model);
    try {
      await setWhisperModel(model);
      await loadModelInfo();
      await refreshModelStatus();
    } catch (err) {
      console.error("Failed to set whisper model:", err);
    }
  }, [loadModelInfo, refreshModelStatus]);

  /** Save the initial prompt setting. */
  const handleSavePrompt = useCallback(async () => {
    try {
      await setInitialPrompt(initialPrompt);
      setPromptSaved(true);
      setTimeout(() => setPromptSaved(false), 2000);
    } catch (err) {
      console.error("Failed to set initial prompt:", err);
    }
  }, [initialPrompt]);

  /** Save the max segment seconds setting. */
  const handleSaveMaxSegment = useCallback(async () => {
    try {
      await setMaxSegmentSeconds(maxSegmentSeconds);
      setSegmentSaved(true);
      setTimeout(() => setSegmentSaved(false), 2000);
    } catch (err) {
      console.error("Failed to set max segment seconds:", err);
    }
  }, [maxSegmentSeconds]);

  const micOptions = micDevices.map((d) => ({
    value: d.id,
    label: d.name + (d.is_default ? " (Default)" : ""),
  }));

  const systemOptions = systemDevices.map((d) => ({
    value: d.id,
    label: d.name + (d.is_default ? " (Default)" : ""),
  }));

  if (!modelInfo) {
    return (
      <>
        <header className={s.header}>
          <h1>Settings</h1>
        </header>
        <div className={s.loading}>Loading settings...</div>
      </>
    );
  }

  return (
    <>
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
              { value: "tiny", label: "Tiny (~75 MB — fastest, least accurate)" },
              { value: "base", label: "Base (~142 MB — fast, good accuracy)" },
              { value: "small", label: "Small (~466 MB — balanced)" },
              { value: "medium", label: "Medium (~1.5 GB — slower, high accuracy)" },
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
            {modelInfo.downloaded ? "Downloaded" : "Not Downloaded"}
          </span>

          <span className={s.infoLabel}>Path</span>
          <span className={s.infoValueMono} title={modelInfo.path}>
            {modelInfo.path}
          </span>

          <span className={s.infoLabel}>Size</span>
          <span className={s.infoValue}>
            {modelInfo.downloaded
              ? formatFileSize(modelInfo.size_bytes)
              : "\u2014"}
          </span>

          <span className={s.infoLabel}>Directory</span>
          <span className={s.infoValueMono} title={modelInfo.models_dir}>
            {modelInfo.models_dir}
          </span>
        </div>

        <div className={s.actions}>
          {!modelInfo.downloaded && (
            <Button onClick={handleDownload} disabled={isDownloading}>
              {isDownloading ? "Downloading..." : "Download Model"}
            </Button>
          )}
          {modelInfo.downloaded && (
            <Button variant="danger" onClick={handleDelete}>
              Delete Model
            </Button>
          )}
        </div>
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
            {pathSaved ? "Saved" : "Save"}
          </Button>
        </div>

        <Button variant="link" onClick={handleResetPath}>
          Reset to Default
        </Button>
      </Panel>

      {/* Output Directory */}
      <Panel title="Output Directory">
        <p className={s.sectionDesc}>
          Where transcript files are saved. Defaults to ~/EchoNotes/.
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
            {outputDirSaved ? "Saved" : "Save"}
          </Button>
        </div>

        <Button variant="link" onClick={handleResetOutputDir}>
          Reset to Default
        </Button>
      </Panel>

      {/* Auto-Stop */}
      <Panel title="Auto-Stop">
        <p className={s.sectionDesc}>
          Automatically stop recording after a period of silence. Set to 0 to disable.
        </p>

        <div className={s.pathInputRow}>
          <input
            type="number"
            className={s.numberInput}
            value={silenceTimeout}
            onChange={(e) => {
              setSilenceTimeoutState(Math.max(0, parseInt(e.target.value) || 0));
              setTimeoutSaved(false);
            }}
            min={0}
            step={30}
          />
          <span className={s.inputSuffix}>seconds</span>
          <Button onClick={handleSaveSilenceTimeout}>
            {timeoutSaved ? "Saved" : "Save"}
          </Button>
        </div>
      </Panel>

      {/* Initial Prompt */}
      <Panel title="Initial Prompt">
        <p className={s.sectionDesc}>
          Provide domain-specific terms, names, or jargon to improve transcription
          accuracy. Leave empty for default behavior.
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
            placeholder="e.g. Kubernetes, PostgreSQL, EchoNotes, Jon"
          />
          <Button onClick={handleSavePrompt}>
            {promptSaved ? "Saved" : "Save"}
          </Button>
        </div>
      </Panel>

      {/* Max Segment Duration */}
      <Panel title="Max Segment Duration">
        <p className={s.sectionDesc}>
          Maximum audio duration before forcing transcription. Larger values reduce
          overhead but increase latency (1–30 seconds).
        </p>

        <div className={s.pathInputRow}>
          <input
            type="number"
            className={s.numberInput}
            value={maxSegmentSeconds}
            onChange={(e) => {
              setMaxSegmentSecondsState(
                Math.min(30, Math.max(1, parseInt(e.target.value) || 1))
              );
              setSegmentSaved(false);
            }}
            min={1}
            max={30}
            step={1}
          />
          <span className={s.inputSuffix}>seconds</span>
          <Button onClick={handleSaveMaxSegment}>
            {segmentSaved ? "Saved" : "Save"}
          </Button>
        </div>
      </Panel>
    </>
  );
}
