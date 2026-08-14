export type ThemePreference = "system" | "light" | "dark";

const THEME_STORAGE_KEY = "contractorproject.theme";

export function loadThemePreference(): ThemePreference {
  const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
  return stored === "light" || stored === "dark" ? stored : "system";
}

export function watchTheme(
  preference: ThemePreference,
  onResolved: (theme: "light" | "dark") => void,
): () => void {
  window.localStorage.setItem(THEME_STORAGE_KEY, preference);
  const media = window.matchMedia?.("(prefers-color-scheme: dark)");
  const resolve = () => {
    onResolved(preference === "system" ? (media?.matches ? "dark" : "light") : preference);
  };

  resolve();
  if (preference !== "system" || !media) return () => undefined;

  media.addEventListener("change", resolve);
  return () => media.removeEventListener("change", resolve);
}
