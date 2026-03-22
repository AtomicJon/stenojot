import { useState, useEffect, useCallback } from "react";
import {
  getModelInfo,
  downloadModel,
  deleteModel,
  setModelsDir,
} from "../../lib/commands";
import { useRecording } from "../../hooks/useRecording";
import type { ModelInfo } from "../../types";
import s from "./SettingsPage.module.scss";

/**
 * Format bytes to a human-readable file size string.
 * @param bytes - Size in bytes
 */
function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(1)} ${units[i]}`;
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
  const [customPath, setCustomPath] = useState("");
  const [pathSaved, setPathSaved] = useState(false);

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
      <section className={s.panel}>
        <h2>Audio Sources</h2>
        {isRecording && (
          <p className={s.disabledNote}>
            Audio sources cannot be changed while recording.
          </p>
        )}
        <div className={s.deviceSelectors}>
          <label className={s.deviceLabel}>
            <span>Microphone</span>
            <select
              value={micDeviceId}
              onChange={(e) => setMicDeviceId(e.target.value)}
              disabled={isRecording}
              className={isRecording ? s.selectDisabled : undefined}
            >
              {micDevices.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                  {d.is_default ? " (Default)" : ""}
                </option>
              ))}
            </select>
          </label>
          <label className={s.deviceLabel}>
            <span>System Audio</span>
            <select
              value={systemDeviceId}
              onChange={(e) => setSystemDeviceId(e.target.value)}
              disabled={isRecording}
              className={isRecording ? s.selectDisabled : undefined}
            >
              {systemDevices.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                  {d.is_default ? " (Default)" : ""}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>

      {/* Model Management */}
      <section className={s.panel}>
        <h2>Transcription Model</h2>

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
            <button
              className={s.downloadBtn}
              onClick={handleDownload}
              disabled={isDownloading}
            >
              {isDownloading ? "Downloading..." : "Download Model"}
            </button>
          )}
          {modelInfo.downloaded && (
            <button className={s.deleteBtn} onClick={handleDelete}>
              Delete Model
            </button>
          )}
        </div>
      </section>

      {/* Storage Location */}
      <section className={s.panel}>
        <h2>Storage Location</h2>
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
          <button className={s.saveBtn} onClick={handleSavePath}>
            {pathSaved ? "Saved" : "Save"}
          </button>
        </div>

        <button className={s.resetLink} onClick={handleResetPath}>
          Reset to Default
        </button>
      </section>
    </>
  );
}
