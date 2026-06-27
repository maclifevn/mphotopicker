import { defineConfig } from "vite";

// Tauri expects a fixed port and ignores the src-tauri dir when watching.
export default defineConfig({
  root: "src",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "esnext",
  },
});
