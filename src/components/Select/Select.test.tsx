import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Select } from "./Select";

const options = [
  { value: "mic-1", label: "Built-in Microphone" },
  { value: "mic-2", label: "USB Microphone" },
];

describe("Select", () => {
  it("renders the label text", () => {
    // Arrange
    const label = "Microphone";

    // Act
    render(
      <Select
        label={label}
        value="mic-1"
        options={options}
        onChange={() => {}}
      />,
    );

    // Assert
    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it("renders all options", () => {
    // Arrange & Act
    render(
      <Select
        label="Source"
        value="mic-1"
        options={options}
        onChange={() => {}}
      />,
    );

    // Assert
    const opts = screen.getAllByRole("option");
    expect(opts).toHaveLength(2);
    expect(opts[0]).toHaveTextContent("Built-in Microphone");
    expect(opts[1]).toHaveTextContent("USB Microphone");
  });

  it("selects the provided value", () => {
    // Arrange & Act
    render(
      <Select
        label="Source"
        value="mic-2"
        options={options}
        onChange={() => {}}
      />,
    );

    // Assert
    const select = screen.getByRole("combobox");
    expect(select).toHaveValue("mic-2");
  });

  it("calls onChange with the new value on selection", () => {
    // Arrange
    const handleChange = vi.fn();
    render(
      <Select
        label="Source"
        value="mic-1"
        options={options}
        onChange={handleChange}
      />,
    );

    // Act
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "mic-2" },
    });

    // Assert
    expect(handleChange).toHaveBeenCalledWith("mic-2");
  });

  it("disables the select when disabled prop is true", () => {
    // Arrange & Act
    render(
      <Select
        label="Source"
        value="mic-1"
        options={options}
        onChange={() => {}}
        disabled
      />,
    );

    // Assert
    expect(screen.getByRole("combobox")).toBeDisabled();
  });
});
