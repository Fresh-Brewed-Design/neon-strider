import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Set by `tauri dev` when the app runs on another device (e.g. a phone) and needs the dev server on the network.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],

  // Keep Rust's compiler errors on screen during `tauri dev`.
  clearScreen: false,
  // The HUD is its own page so the always-on overlay loads only its own small
  // bundle, not the dashboard's pages and animation library.
  build: {
    rollupOptions: {
      input: { main: "index.html", hud: "hud.html" },
    },
  },
  // Tauri expects the dev server on this exact port (tauri.conf.json `devUrl`).
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    include: ["src/**/*.{test,spec}.{ts,tsx}", "scripts/**/*.test.ts"],
  },
});
