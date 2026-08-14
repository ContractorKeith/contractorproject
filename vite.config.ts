import { execFileSync } from "node:child_process";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(({ mode }) => {
  const verificationCommit =
    mode === "verification"
      ? execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim()
      : "";

  return {
    plugins: [react()],
    clearScreen: false,
    define: {
      "import.meta.env.VITE_APP_COMMIT": JSON.stringify(verificationCommit),
      "import.meta.env.VITE_GANTT_VERIFICATION": JSON.stringify(mode === "verification" ? "1" : "0"),
    },
    server: {
      port: 1420,
      strictPort: true,
      host: host || false,
      hmr: host
        ? {
            protocol: "ws",
            host,
            port: 1421,
          }
        : undefined,
      watch: {
        ignored: ["**/src-tauri/**"],
      },
    },
    test: {
      environment: "jsdom",
      environmentOptions: {
        jsdom: {
          url: "http://localhost/",
        },
      },
      setupFiles: ["./src/test/setup.ts"],
    },
  };
});
