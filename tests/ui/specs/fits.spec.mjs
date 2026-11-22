// Every word fits (#610): each main screen — the session list, a chat, the Desk's list, an item's page, every Settings
// tab, and the mini window at the notch and on the rail — at each text size, in the smallest window and a normal one,
// in light and dark, with no line of text cut off, run out of its box, past the window, or over other words
// (fits.mjs says how each is found). Settings → Text size zooms the whole window (WebKit's page zoom), so a size here is
// the window laid out that much smaller (A− / A+ on a chat and ⌘+ / ⌘− step the same size). The smallest window grows with the zoom (lib.rs min_window), up to the
// screen's room: here a 13-inch screen's, the smallest Cue runs on. The mini window isn't zoomed: it keeps its own
// size.
import { test, expect, openDesk } from "./cue.mjs";
import { NARROWEST } from "./layout.mjs";
import { fitProblems, zoomed } from "./fits.mjs";

const NORMAL = { width: 1280, height: 800 };
/** The text sizes: Settings → Text size's steps. */
const SIZES = [
  { zoom: 0.9, name: "Compact" },
  { zoom: 1, name: "Default" },
  { zoom: 1.15, name: "Large" },
  { zoom: 1.3, name: "Larger" },
];
/** A 13-inch screen's room (1280×800, without the menu bar). */
const ROOM = { width: 1280, height: 775 };
/** The smallest window at zoom `z`, in points (lib.rs min_window). */
const smallest = (z) => ({ width: Math.min(NARROWEST.width * Math.max(z, 1), ROOM.width), height: Math.min(NARROWEST.height * Math.max(z, 1), ROOM.height) });
const WINDOWS = [["the smallest window", smallest], ["a normal window", () => NORMAL]];
const SETTINGS_TABS = ["general", "agents", "chat", "voice", "insights", "usage"];
const ITEMS = ["d1", "d2", "d5", "d6"];

/** Cue's state at text size `s` in `mode`, as the window has it after Settings saves them. */
const setLook = (cue, s, mode) => cue.change((st, [s, mode]) => { st.settings.appearance = { ...st.settings.appearance, zoom: s.zoom, mode }; }, [s, mode]);
/** Laid out and still: fonts in, two frames drawn. */
const settled = (page) => page.evaluate(() => document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))));
/** What doesn't fit on screen now, each said with where it is. */
const check = async (page, where, root = "#app") => { await settled(page); return (await fitProblems(page, root)).map((p) => `${where}: ${p}`); };

/** Each screen: opened from the scenario it's drawn on, then measured (once per view it has). */
const SCREENS = {
  "the session list": { scenario: "board-waiting", look: (page, where) => check(page, where) },
  "a chat": {
    scenario: "board-waiting",
    look: async (page, where) => {
      await page.locator('.nrow[data-big="n1"]').click({ position: { x: 12, y: 12 } });
      await page.locator(".active-pane .ap-chat").first().waitFor();
      return check(page, where);
    },
  },
  "the Desk's list": {
    scenario: "desk",
    look: async (page, where) => { await openDesk(page); return check(page, where); },
  },
  "an item's page": {
    scenario: "desk",
    look: async (page, where) => {
      await openDesk(page);
      const out = [];
      for (const id of ITEMS) {
        await page.locator(`.dk-items [data-desk-item="${id}"]`).first().click();
        await page.locator(`.dk-right`).first().waitFor();
        out.push(...(await check(page, `${where}, item ${id}`)));
      }
      return out;
    },
  },
  "Settings": {
    scenario: "board-waiting",
    look: async (page, where) => {
      await page.keyboard.press(",");
      const out = [];
      for (const tab of SETTINGS_TABS) {
        await page.locator(`[data-act="set-tab"][data-tab="${tab}"]`).click();
        await expect(page.locator(`[data-act="set-tab"][data-tab="${tab}"]`)).toHaveAttribute("aria-selected", "true");
        out.push(...(await check(page, `${where}, ${tab}`)));
      }
      return out;
    },
  },
};

for (const [screen, { scenario, look }] of Object.entries(SCREENS)) {
  for (const s of SIZES) {
    for (const [win, size] of WINDOWS) {
      test(`${screen}, text size ${s.name}, ${win}, light and dark: every word fits`, async ({ cue, page }) => {
        await page.setViewportSize(zoomed(size(s.zoom), s.zoom));
        const out = [];
        for (const mode of ["light", "dark"]) {
          await cue.open(scenario, { frozen: true });
          await setLook(cue, s, mode);
          out.push(...(await look(page, mode)));
        }
        expect(out).toEqual([]);
      });
    }
  }
}

// The mini window: a crew lead's chat (world-team) at the notch and on the rail, on a 13-inch screen and a 16-inch one.
// Its size is a share of the screen's (mini.js CAP), its width the chat's (WIDTH, and the notch's shoulders).
const SCREENS_MINI = [["a 13-inch screen", { width: 1280, height: 800 }], ["a 16-inch screen", { width: 1728, height: 1117 }]];
const MINI = (shape, screen) => (shape === "notch" ? { width: 560 + 2 * 14, height: Math.round(screen.height * 0.5) } : { width: 420, height: Math.round(screen.height * 0.45) });
for (const shape of ["notch", "rail"]) {
  {
    for (const [scr, screen] of SCREENS_MINI) {
      test(`the mini window at the ${shape}, ${scr}, light and dark: every word fits`, async ({ cue, page }) => {
        await page.setViewportSize(MINI(shape, screen));
        await page.addInitScript((h) => Object.defineProperty(Screen.prototype, "height", { get: () => h }), screen.height);
        const out = [];
        for (const mode of ["light", "dark"]) {
          await cue.openMini("world-team", shape, { frozen: true });
          await cue.change((st, mode) => { st.settings.appearance = { ...st.settings.appearance, mode }; }, mode);
          if (shape === "notch") await page.evaluate(() => document.body.dispatchEvent(new MouseEvent("mouseenter")));
          // The rail keeps itself out once brought out (the second load finds it so).
          else if (await page.locator("[data-out]").count()) await page.locator("[data-out]").click();
          await page.locator('.m-tab.on[data-mopen="lead"]').waitFor();
          out.push(...(await check(page, mode, "body")));
        }
        expect(out).toEqual([]);
      });
    }
  }
}
