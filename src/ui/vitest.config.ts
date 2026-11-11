import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// The new UI has its own test config so the existing one (frontend/**) is untouched.
export default defineConfig({
  plugins: [react()],
  test: { environment: "jsdom", include: ["ui/**/*.test.{ts,tsx}"], maxWorkers: 2 },
});
