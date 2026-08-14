import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: ".scratch/gantt-spike/dist",
    emptyOutDir: true,
    rollupOptions: {
      input: "gantt-prototype.html",
    },
  },
  preview: {
    host: "127.0.0.1",
    port: 4174,
    strictPort: true,
  },
});
