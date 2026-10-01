import { useEffect, useState } from "react";
import { watchTheme, type ThemePreference } from "../theme";
import "./ThemeToggle.css";

type ResolvedTheme = "light" | "dark";

interface ThemeToggleProps {
  theme: ThemePreference;
  onChange: (theme: ThemePreference) => void;
}

export function ThemeToggle({ theme, onChange }: ThemeToggleProps) {
  const [resolvedTheme, setResolvedTheme] = useState<ResolvedTheme>("light");

  useEffect(() => watchTheme(theme, setResolvedTheme), [theme]);

  const nextTheme: ResolvedTheme = resolvedTheme === "light" ? "dark" : "light";

  return (
    <button
      className="theme-toggle"
      type="button"
      aria-label={`Switch to ${nextTheme} theme`}
      title={`Switch to ${nextTheme} theme`}
      onClick={() => onChange(nextTheme)}
    >
      <svg
        aria-hidden="true"
        viewBox="0 0 24 24"
        focusable="false"
        className="theme-toggle__icon"
      >
        <path d="M20.2 15.3A8.5 8.5 0 0 1 8.7 3.8 8.5 8.5 0 1 0 20.2 15.3Z" />
      </svg>
    </button>
  );
}
