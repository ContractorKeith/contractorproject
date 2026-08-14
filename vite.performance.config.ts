import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "dist-performance",
    rollupOptions: {
      input: resolve(import.meta.dirname, "tests/browser/gantt.html"),
    },
  },
});
