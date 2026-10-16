import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    strictPort: true,
    watch: {
      ignored: [
        "**/target/**",
        "**/reports/**",
        "**/src-tauri/**",
        "**/crates/**",
        "**/scripts/**",
        "**/dist/**",
        "**/*.tsbuildinfo",
      ],
    },
  },
  test: { environment: "jsdom", include: ["frontend/**/*.test.{ts,tsx}"] },
});
