import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ThemeToggle } from "./ThemeToggle";

const originalMatchMedia = Object.getOwnPropertyDescriptor(window, "matchMedia");

function setSystemTheme(dark: boolean) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockReturnValue({
      matches: dark,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }),
  });
}

describe("ThemeToggle", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    window.localStorage.clear();
    if (originalMatchMedia) {
      Object.defineProperty(window, "matchMedia", originalMatchMedia);
    } else {
      Reflect.deleteProperty(window, "matchMedia");
    }
  });

  it("switches from the resolved system theme to its opposite", async () => {
    setSystemTheme(true);
    const onChange = vi.fn();
    render(<ThemeToggle theme="system" onChange={onChange} />);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Switch to light theme" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Switch to light theme" }));
    expect(onChange).toHaveBeenCalledWith("light");
    expect(window.localStorage.getItem("contractorproject.theme")).toBe("system");
  });

  it("switches on every click after the preference changes", async () => {
    setSystemTheme(false);
    const onChange = vi.fn();
    const { rerender } = render(<ThemeToggle theme="system" onChange={onChange} />);

    const switchToDark = await screen.findByRole("button", { name: "Switch to dark theme" });
    fireEvent.click(switchToDark);
    expect(onChange).toHaveBeenLastCalledWith("dark");

    rerender(<ThemeToggle theme="dark" onChange={onChange} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Switch to light theme" })).toBeInTheDocument(),
    );
    expect(window.localStorage.getItem("contractorproject.theme")).toBe("dark");
    fireEvent.click(screen.getByRole("button", { name: "Switch to light theme" }));
    expect(onChange).toHaveBeenLastCalledWith("light");

    rerender(<ThemeToggle theme="light" onChange={onChange} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Switch to dark theme" })).toBeInTheDocument(),
    );
    expect(window.localStorage.getItem("contractorproject.theme")).toBe("light");
    fireEvent.click(screen.getByRole("button", { name: "Switch to dark theme" }));
    expect(onChange).toHaveBeenLastCalledWith("dark");
  });

  it("supports keyboard activation and has an action-oriented accessible name", async () => {
    setSystemTheme(false);
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<ThemeToggle theme="system" onChange={onChange} />);

    const button = await screen.findByRole("button", { name: "Switch to dark theme" });
    await user.tab();
    expect(button).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(onChange).toHaveBeenCalledWith("dark");
  });
});
