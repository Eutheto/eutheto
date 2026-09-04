import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  plugins: [vue(), tailwindcss()],
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
});
