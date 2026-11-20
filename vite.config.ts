import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Ne pas regarder le build Rust : CMake/Cargo réécrivent `target/`
    // (un `scandir` en cours de build faisait planter le watcher Vite,
    // et avec lui le serveur de dev).
    watcher: {
      ignored: ["**/target/**"],
    },
  },
});
