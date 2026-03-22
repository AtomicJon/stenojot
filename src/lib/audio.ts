/** Minimum dB value representing silence on the level meter scale. */
const MIN_DB = -60;

/**
 * Convert linear RMS (0.0–1.0) to a perceptual dB-based percentage (0–100).
 * Maps -60 dB → 0% and 0 dB → 100%.
 * @param rms - Linear RMS amplitude
 */
export function getLevelPercent(rms: number): number {
  if (rms <= 0) return 0;
  const db = 20 * Math.log10(rms);
  const percent = ((db - MIN_DB) / (0 - MIN_DB)) * 100;
  return Math.min(Math.max(percent, 0), 100);
}

/**
 * Convert a dB-scale percentage (0–100) back to linear RMS.
 * Inverse of `getLevelPercent`.
 * @param pct - Percentage on the dB scale
 */
export function rmsFromPercent(pct: number): number {
  const db = MIN_DB + (pct / 100) * (0 - MIN_DB);
  return Math.pow(10, db / 20);
}
