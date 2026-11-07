// The main screens, each a screenshot compared with its baseline (shots.mjs). The clock is frozen at the
// scenarios' NOW and nothing animates (?still=1), so a screen draws the same pixels every run.
import { test, expect } from "./cue.mjs";
import { compareShot } from "./shots.mjs";

// The smallest the window goes (tauri.conf.json minWidth; Cue has no phone layout of its own).
const NARROWEST = { width: 900, height: 600 };
const SETTINGS_TABS = ["general", "agents", "chat", "summaries", "voice", "insights", "actions", "usage"];

/** Opens a session's chat from its card in Waiting. */
const openCard = (page, id) => page.locator(`.nrow[data-big="${id}"]`).click();
const openSettings = async (page, tab) => {
  await page.keyboard.press(",");
  await page.locator(`[data-act="set-tab"][data-tab="${tab}"]`).click();
  await expect(page.locator(`[data-act="set-tab"][data-tab="${tab}"]`)).toHaveAttribute("aria-selected", "true");
};
/** agent-relay's terminal open, its screen written, and nothing focused (a focused terminal's cursor blinks). */
const openTerminal = async (page) => {
  await openCard(page, "n1");
  await page.locator('.tstrip[data-sid="relay"]').click();
  // Its screen is in the terminal's buffer (the fake's permission menu); it's drawn by the time the shot is taken.
  await page.waitForFunction(() => {
    const b = typeof termObj !== "undefined" && termObj?.buffer.active;
    return b && Array.from({ length: b.length }, (_, i) => b.getLine(i)?.translateToString()).some((l) => l?.includes("Do you want to proceed?"));
  });
};

/** name → { scenario, flags, viewport, setup(page): what to open, done(page): what's on screen once it's drawn } */
const SCREENS = {
  "board-empty": { scenario: "board-empty", done: (p) => p.locator(".switch-view").waitFor() },
  "board-waiting": { scenario: "board-waiting", done: (p) => p.locator('.nrow[data-big="n6"]').waitFor() },
  "many-sessions": { scenario: "many-sessions", done: (p) => p.locator('.nrow[data-big="mi0"]').waitFor() },
  "relay-review": { scenario: "relay-review", setup: (p) => openCard(p, "n1"), done: (p) => p.locator(".active-pane .rv-find").waitFor() },
  "status-list": { scenario: "status-list", setup: async (p) => { await openCard(p, "n1"); await p.locator('.active-pane .st-bar[data-sid="relay"]').click(); }, done: (p) => p.locator(".active-pane .st-drawer").waitFor() },
  "chat-long-thread": { scenario: "board-waiting", setup: (p) => openCard(p, "n1"), done: (p) => p.locator(".active-pane pre").first().waitFor() },
  "terminal-open": { scenario: "terminal", setup: openTerminal, done: (p) => p.evaluate(() => document.activeElement?.blur()) },
  "terminal-stalled": { scenario: "terminal", flags: "stalled=1", setup: async (p) => { await openTerminal(p); await p.keyboard.type("ls"); }, done: async (p) => { await p.locator(".term-stalled").waitFor(); await p.evaluate(() => document.activeElement?.blur()); } },
  ...Object.fromEntries(SETTINGS_TABS.map((tab) => [`settings-${tab}`, { scenario: "board-waiting", setup: (p) => openSettings(p, tab), done: (p) => p.locator(".sheet .set-row, .sheet .set-group").first().waitFor() }])),
  "insights": { scenario: "board-waiting", setup: (p) => p.locator('[data-view="insights"]').click(), done: (p) => p.waitForFunction(() => CueBridge.called("insights_summary").some((c) => "result" in c)) },
  "shelf-parked": { scenario: "parked-ended", setup: (p) => p.locator('[data-shelf="parked"]').click(), done: (p) => p.locator('.prow.parked[data-session="guide"]').waitFor() },
  "shelf-closed": { scenario: "parked-ended", setup: (p) => p.locator('[data-shelf="closed"]').click(), done: (p) => p.locator('.prow.closed[data-session="lost2"]').waitFor() },
  "sessions-view": { scenario: "parked-ended", setup: (p) => p.locator('[data-view="sessions"]').click(), done: (p) => p.locator('[data-sid="q2"]').first().waitFor() },
  "narrowest-window": { scenario: "board-waiting", viewport: NARROWEST, done: (p) => p.locator('[data-big="n6"]').first().waitFor() },
};

for (const [name, s] of Object.entries(SCREENS)) {
  test(`screen: ${name}`, async ({ cue, page }) => {
    if (s.viewport) await page.setViewportSize(s.viewport);
    await cue.open(s.scenario, { flags: s.flags, frozen: true });
    await s.setup?.(page);
    await s.done(page);
    // Fonts in, and two frames drawn since the last change (xterm and the status drawer draw on a frame).
    await page.evaluate(() => document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))));
    const png = await page.screenshot({ animations: "disabled", caret: "hide" });
    const r = compareShot(name, png);
    expect(r.baseline, `baseline for ${name}`).not.toBe("missing");
    expect(r.differ, `${name}: pixels that differ from tests/ui/baseline/${name}.png (see tests/ui/shots/${name}.diff.png)`).toBe(0);
  });
}
