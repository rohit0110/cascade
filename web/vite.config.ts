import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// In dev, proxy the API to a locally running cascade-server.
export default defineConfig({
  plugins: [react()],
  server: { proxy: { "/api": "http://localhost:7070", "/metrics": "http://localhost:7070" } },
  build: { outDir: "dist", emptyOutDir: true },
});
