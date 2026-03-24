/**
 * Format seconds to MM:SS display string.
 * @param totalSeconds - Elapsed time in seconds
 */
export function formatTime(totalSeconds: number): string {
  const mins = Math.floor(totalSeconds / 60);
  const secs = totalSeconds % 60;
  return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
}

/**
 * Format milliseconds to MM:SS display string.
 * @param ms - Time in milliseconds
 */
export function formatTimestamp(ms: number): string {
  return formatTime(Math.floor(ms / 1000));
}

/**
 * Format bytes to a human-readable file size string.
 * @param bytes - Size in bytes
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(1)} ${units[i]}`;
}
