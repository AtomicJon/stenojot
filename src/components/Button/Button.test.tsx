import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Button } from './Button';
import { ButtonVariant } from './Button.const';

describe('Button', () => {
  it('renders children text', () => {
    // Arrange
    const label = 'Click me';

    // Act
    render(<Button onClick={() => {}}>{label}</Button>);

    // Assert
    expect(screen.getByRole('button')).toHaveTextContent(label);
  });

  it('calls onClick when clicked', () => {
    // Arrange
    const handleClick = vi.fn();
    render(<Button onClick={handleClick}>Go</Button>);

    // Act
    fireEvent.click(screen.getByRole('button'));

    // Assert
    expect(handleClick).toHaveBeenCalledOnce();
  });

  it('does not fire onClick when disabled', () => {
    // Arrange
    const handleClick = vi.fn();
    render(
      <Button onClick={handleClick} disabled>
        Go
      </Button>,
    );

    // Act
    fireEvent.click(screen.getByRole('button'));

    // Assert
    expect(handleClick).not.toHaveBeenCalled();
  });

  it('applies the default variant class by default', () => {
    // Arrange & Act
    render(<Button onClick={() => {}}>OK</Button>);

    // Assert
    expect(screen.getByRole('button')).toHaveClass('default');
  });

  it('applies the danger variant class', () => {
    // Arrange & Act
    render(
      <Button onClick={() => {}} variant={ButtonVariant.danger}>
        Delete
      </Button>,
    );

    // Assert
    expect(screen.getByRole('button')).toHaveClass('danger');
  });

  it('applies the link variant class', () => {
    // Arrange & Act
    render(
      <Button onClick={() => {}} variant={ButtonVariant.link}>
        Reset
      </Button>,
    );

    // Assert
    expect(screen.getByRole('button')).toHaveClass('link');
  });
});
