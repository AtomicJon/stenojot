import { describe, it, expect } from 'vitest';
import { formatTime, formatTimestamp, formatFileSize } from './format';

describe('formatTime', () => {
  it('formats zero seconds as 00:00', () => {
    // Arrange
    const seconds = 0;

    // Act
    const result = formatTime(seconds);

    // Assert
    expect(result).toBe('00:00');
  });

  it('pads single-digit minutes and seconds', () => {
    // Arrange
    const seconds = 65;

    // Act
    const result = formatTime(seconds);

    // Assert
    expect(result).toBe('01:05');
  });

  it('handles large values', () => {
    // Arrange
    const seconds = 3661;

    // Act
    const result = formatTime(seconds);

    // Assert
    expect(result).toBe('61:01');
  });

  it('formats exact minutes with zero seconds', () => {
    // Arrange
    const seconds = 120;

    // Act
    const result = formatTime(seconds);

    // Assert
    expect(result).toBe('02:00');
  });
});

describe('formatTimestamp', () => {
  it('converts milliseconds to MM:SS format', () => {
    // Arrange
    const ms = 65000;

    // Act
    const result = formatTimestamp(ms);

    // Assert
    expect(result).toBe('01:05');
  });

  it('floors fractional seconds', () => {
    // Arrange
    const ms = 1999;

    // Act
    const result = formatTimestamp(ms);

    // Assert
    expect(result).toBe('00:01');
  });

  it('handles zero milliseconds', () => {
    // Arrange
    const ms = 0;

    // Act
    const result = formatTimestamp(ms);

    // Assert
    expect(result).toBe('00:00');
  });
});

describe('formatFileSize', () => {
  it("returns '0 B' for zero bytes", () => {
    // Arrange
    const bytes = 0;

    // Act
    const result = formatFileSize(bytes);

    // Assert
    expect(result).toBe('0 B');
  });

  it('formats bytes', () => {
    // Arrange
    const bytes = 500;

    // Act
    const result = formatFileSize(bytes);

    // Assert
    expect(result).toBe('500.0 B');
  });

  it('formats kilobytes', () => {
    // Arrange
    const bytes = 1024;

    // Act
    const result = formatFileSize(bytes);

    // Assert
    expect(result).toBe('1.0 KB');
  });

  it('formats megabytes', () => {
    // Arrange
    const bytes = 1536 * 1024;

    // Act
    const result = formatFileSize(bytes);

    // Assert
    expect(result).toBe('1.5 MB');
  });

  it('formats gigabytes', () => {
    // Arrange
    const bytes = 2.5 * 1024 * 1024 * 1024;

    // Act
    const result = formatFileSize(bytes);

    // Assert
    expect(result).toBe('2.5 GB');
  });
});
