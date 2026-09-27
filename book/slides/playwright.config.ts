import { defineConfig } from "@playwright/test"

export default defineConfig({
  testDir: "./scripts",
  testMatch: "0_shots.ts",
  fullyParallel: true,
  workers: 3,
  outputDir: ".playwright-results",
  reporter: "line",
  use: {
    baseURL: "http://127.0.0.1:5198",
    viewport: { width: 1440, height: 900 },
  },
  webServer: {
    command: "pnpm build && pnpm preview --host 127.0.0.1 --port 5198 --strictPort",
    url: "http://127.0.0.1:5198",
    reuseExistingServer: false,
  },
})
