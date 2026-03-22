import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Slider } from "./Slider";

describe("Slider", () => {
  it("renders the label text", () => {
    // Arrange
    const label = "Mic Gain (1.0x)";

    // Act
    render(
      <Slider
        label={label}
        value={1}
        min={0}
        max={10}
        step={0.1}
        onChange={() => {}}
      />,
    );

    // Assert
    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it("sets the input value to the provided value", () => {
    // Arrange
    const value = 5;

    // Act
    render(
      <Slider
        label="Volume"
        value={value}
        min={0}
        max={10}
        step={1}
        onChange={() => {}}
      />,
    );

    // Assert
    const input = screen.getByRole("slider");
    expect(input).toHaveValue(String(value));
  });

  it("calls onChange with the parsed float value on input change", () => {
    // Arrange
    const handleChange = vi.fn();
    render(
      <Slider
        label="Volume"
        value={1}
        min={0}
        max={10}
        step={0.1}
        onChange={handleChange}
      />,
    );

    // Act
    fireEvent.change(screen.getByRole("slider"), { target: { value: "3.5" } });

    // Assert
    expect(handleChange).toHaveBeenCalledWith(3.5);
  });

  it("respects min and max attributes", () => {
    // Arrange & Act
    render(
      <Slider
        label="Gain"
        value={5}
        min={1}
        max={20}
        step={1}
        onChange={() => {}}
      />,
    );

    // Assert
    const input = screen.getByRole("slider");
    expect(input).toHaveAttribute("min", "1");
    expect(input).toHaveAttribute("max", "20");
  });
});
