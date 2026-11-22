// The UI tests (tests/ui/specs): the real UI in headless Chromium on the fake bridge, served from the repo.
// Run with tests/run.sh ui. The browser is Playwright's own build, pinned by its package version, so a
// Chrome update can't move the screenshots; the page's clock, time zone, locale, colors and size are fixed too.
import { defineConfig } from "@playwright/test";
import { createHash } from "node:crypto";
import { existsSync, realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

// A port of this checkout's own, so two worktrees can run their UI tests at once.
const mine = createHash("sha1").update(import.meta.url).digest("hex");
const port = +process.env.CUE_UI_PORT || 20000 + (parseInt(mine.slice(0, 6), 16) % 20000);

// The optional add-on's UI tests (src-tauri/ext/ui/tests), when it's there: a project of their own, run on the window
// with the add-on's part in it (load.js ?addon=1). They reach this folder's helpers through CUE_UI_SPECS.
const addon = fileURLToPath(new URL("../../src-tauri/ext/ui/tests", import.meta.url));
process.env.CUE_UI_SPECS = fileURLToPath(new URL("specs/", import.meta.url));

export default defineConfig({
  testMatch: "*.spec.mjs",
  projects: [{ name: "cue", testDir: "specs" }, ...(existsSync(addon) ? [{ name: "addon", testDir: realpathSync(addon) }] : [])],
  fullyParallel: true,
  // Two browsers at a time, not one per core: each browser that opens makes macOS check the permissions of the app
  // that started the run, and many at once (above all next to a build) can stall the Mac's window server.
  workers: +process.env.CUE_UI_WORKERS || 2,
  forbidOnly: true,
  retries: 0,
  reporter: [["list"]],
  // This checkout's own too: a run empties its folder as it starts, so a shared one loses another run's traces.
  outputDir: `${process.env.TMPDIR || "/tmp"}/cue-ui-results-${mine.slice(0, 8)}`,
  use: {
    baseURL: `http://127.0.0.1:${port}/tests/ui/`,
    browserName: "chromium",
    headless: true,
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
    timezoneId: "UTC",
    locale: "en-US",
    colorScheme: "light",
  },
  webServer: {
    command: `node serve.mjs ${port}`,
    url: `http://127.0.0.1:${port}/tests/ui/app.html`,
    reuseExistingServer: false,
    stdout: "ignore",
  },
});
