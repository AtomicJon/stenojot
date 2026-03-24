import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LevelMeter } from './LevelMeter';

describe('LevelMeter', () => {
  it('renders the label', () => {
    // Arrange
    const label = 'Mic';

    // Act
    render(<LevelMeter label={label} rms={0} />);

    // Assert
    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it('displays 0% for zero rms', () => {
    // Arrange & Act
    render(<LevelMeter label="Mic" rms={0} />);

    // Assert
    expect(screen.getByText('0%')).toBeInTheDocument();
  });

  it('displays 100% for full-scale rms', () => {
    // Arrange & Act
    render(<LevelMeter label="Mic" rms={1.0} />);

    // Assert
    expect(screen.getByText('100%')).toBeInTheDocument();
  });

  it('sets the fill bar width based on dB-scale percentage', () => {
    // Arrange — rms at -30 dB ≈ 50%
    const rms = Math.pow(10, -30 / 20);

    // Act
    const { container } = render(<LevelMeter label="Mic" rms={rms} />);

    // Assert
    const fill = container.querySelector('.fill');
    expect(fill).not.toBeNull();
    const width = parseFloat(
      fill!.getAttribute('style')?.match(/width:\s*([\d.]+)%/)?.[1] ?? '0',
    );
    expect(width).toBeCloseTo(50, 0);
  });

  it('renders a threshold marker when thresholdRms is provided', () => {
    // Arrange
    const thresholdRms = 0.01;

    // Act
    const { container } = render(
      <LevelMeter label="Mic" rms={0.5} thresholdRms={thresholdRms} />,
    );

    // Assert
    const threshold = container.querySelector('.threshold');
    expect(threshold).not.toBeNull();
  });

  it('does not render a threshold marker when thresholdRms is omitted', () => {
    // Arrange & Act
    const { container } = render(<LevelMeter label="Mic" rms={0.5} />);

    // Assert
    const threshold = container.querySelector('.threshold');
    expect(threshold).toBeNull();
  });
});
