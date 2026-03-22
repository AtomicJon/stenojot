import { useState, useEffect, useCallback } from "react";
import {
  getModelInfo,
  downloadModel,
  deleteModel,
  setModelsDir,
} from "../../lib/commands";
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

/** Settings page for managing the Whisper transcription model and storage location. */
export function SettingsPage() {
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
    } catch (err) {
      console.error("Failed to download model:", err);
    } finally {
      setIsDownloading(false);
    }
  }, [loadModelInfo]);

  /** Delete the Whisper transcription model file. */
  const handleDelete = useCallback(async () => {
    try {
      await deleteModel();
      await loadModelInfo();
    } catch (err) {
      console.error("Failed to delete model:", err);
    }
  }, [loadModelInfo]);

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
        <div className={s.loading}>Loading model information...</div>
      </>
    );
  }

  return (
    <>
      {/* Header */}
      <header className={s.header}>
        <h1>Settings</h1>
      </header>

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
