import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri verwacht een vaste poort en geen scherm-wissen.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_"],
  build: { target: "es2022", sourcemap: true },
});
