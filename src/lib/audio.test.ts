import { describe, it, expect } from 'vitest';
import { getLevelPercent, rmsFromPercent } from './audio';

describe('getLevelPercent', () => {
  it('returns 0 for zero rms', () => {
    // Arrange
    const rms = 0;

    // Act
    const result = getLevelPercent(rms);

    // Assert
    expect(result).toBe(0);
  });

  it('returns 0 for negative rms', () => {
    // Arrange
    const rms = -0.5;

    // Act
    const result = getLevelPercent(rms);

    // Assert
    expect(result).toBe(0);
  });

  it('returns 100 for full-scale rms of 1.0 (0 dB)', () => {
    // Arrange
    const rms = 1.0;

    // Act
    const result = getLevelPercent(rms);

    // Assert
    expect(result).toBe(100);
  });

  it('returns 50 for rms at -30 dB (midpoint of -60..0 range)', () => {
    // Arrange — -30 dB = 10^(-30/20) ≈ 0.0316
    const rms = Math.pow(10, -30 / 20);

    // Act
    const result = getLevelPercent(rms);

    // Assert
    expect(result).toBeCloseTo(50, 1);
  });

  it('clamps to 0 for extremely quiet signals below -60 dB', () => {
    // Arrange — -80 dB = 10^(-80/20) = 0.0001
    const rms = Math.pow(10, -80 / 20);

    // Act
    const result = getLevelPercent(rms);

    // Assert
    expect(result).toBe(0);
  });

  it('clamps to 100 for rms above 1.0', () => {
    // Arrange
    const rms = 2.0;

    // Act
    const result = getLevelPercent(rms);

    // Assert
    expect(result).toBe(100);
  });
});

describe('rmsFromPercent', () => {
  it('returns full scale for 100%', () => {
    // Arrange
    const pct = 100;

    // Act
    const result = rmsFromPercent(pct);

    // Assert
    expect(result).toBeCloseTo(1.0, 5);
  });

  it('returns -30 dB rms for 50%', () => {
    // Arrange
    const pct = 50;

    // Act
    const result = rmsFromPercent(pct);

    // Assert
    const expected = Math.pow(10, -30 / 20);
    expect(result).toBeCloseTo(expected, 5);
  });

  it('returns near-silence for 0%', () => {
    // Arrange
    const pct = 0;

    // Act
    const result = rmsFromPercent(pct);

    // Assert — -60 dB = 10^(-60/20) = 0.001
    expect(result).toBeCloseTo(0.001, 5);
  });
});

describe('getLevelPercent / rmsFromPercent roundtrip', () => {
  it('converting percent to rms and back yields the original percent', () => {
    // Arrange
    const percentages = [10, 25, 50, 75, 90];

    for (const pct of percentages) {
      // Act
      const rms = rmsFromPercent(pct);
      const result = getLevelPercent(rms);

      // Assert
      expect(result).toBeCloseTo(pct, 1);
    }
  });
});
