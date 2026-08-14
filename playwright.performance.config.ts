import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/browser",
  testMatch: "**/*.perf.ts",
  fullyParallel: false,
  workers: 1,
  timeout: 45_000,
  use: {
    baseURL: "http://127.0.0.1:4176",
    trace: "retain-on-failure",
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1,
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        launchOptions: {
          args: ["--disable-frame-rate-limit", "--disable-gpu-vsync"],
        },
      },
    },
  ],
  webServer: {
    command: "vite preview --config vite.performance.config.ts --host 127.0.0.1 --port 4176",
    url: "http://127.0.0.1:4176/tests/browser/gantt.html",
    reuseExistingServer: false,
  },
});
