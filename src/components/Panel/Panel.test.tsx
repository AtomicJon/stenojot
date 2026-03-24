import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Panel } from './Panel';

describe('Panel', () => {
  it('renders children content', () => {
    // Arrange
    const content = 'Panel body';

    // Act
    render(<Panel>{content}</Panel>);

    // Assert
    expect(screen.getByText(content)).toBeInTheDocument();
  });

  it('renders a title when provided', () => {
    // Arrange
    const title = 'Audio Settings';

    // Act
    render(<Panel title={title}>Content</Panel>);

    // Assert
    expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent(title);
  });

  it('does not render a heading when no title is given', () => {
    // Arrange & Act
    render(<Panel>Content</Panel>);

    // Assert
    expect(screen.queryByRole('heading')).not.toBeInTheDocument();
  });

  it('applies additional className when provided', () => {
    // Arrange
    const extra = 'custom-class';

    // Act
    const { container } = render(<Panel className={extra}>Content</Panel>);

    // Assert
    expect(container.firstChild).toHaveClass(extra);
  });
});
