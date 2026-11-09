// The UI tests (tests/ui/specs): the real UI in headless Chromium on the fake bridge, served from the repo.
// Run with tests/run.sh ui. The browser is Playwright's own build, pinned by its package version, so a
// Chrome update can't move the screenshots; the page's clock, time zone, locale, colors and size are fixed too.
import { defineConfig } from "@playwright/test";
import { createHash } from "node:crypto";
import { existsSync, realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

// A port of this checkout's own, so two worktrees can run their UI tests at once.
const port = +process.env.CUE_UI_PORT || 20000 + (parseInt(createHash("sha1").update(import.meta.url).digest("hex").slice(0, 6), 16) % 20000);

// The optional add-on's UI tests (src-tauri/ext/ui/tests), when it's there: a project of their own, run on the window
// with the add-on's part in it (load.js ?addon=1). They reach this folder's helpers through CUE_UI_SPECS.
const addon = fileURLToPath(new URL("../../src-tauri/ext/ui/tests", import.meta.url));
process.env.CUE_UI_SPECS = fileURLToPath(new URL("specs/", import.meta.url));

export default defineConfig({
  testMatch: "*.spec.mjs",
  projects: [{ name: "cue", testDir: "specs" }, ...(existsSync(addon) ? [{ name: "addon", testDir: realpathSync(addon) }] : [])],
  fullyParallel: true,
  forbidOnly: true,
  retries: 0,
  reporter: [["list"]],
  outputDir: process.env.TMPDIR ? `${process.env.TMPDIR}/cue-ui-results` : "/tmp/cue-ui-results",
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
