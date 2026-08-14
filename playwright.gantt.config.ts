import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/gantt",
  testMatch: "**/*.pw.ts",
  timeout: 90_000,
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:4174",
    browserName: "chromium",
    colorScheme: "light",
    deviceScaleFactor: 2,
    viewport: { width: 1440, height: 900 },
  },
  webServer: {
    command: "npm run prototype:gantt",
    port: 4174,
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
