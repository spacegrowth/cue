// A session's terminal in Cue: what's typed reaches it in order (ui/invoke.js's lanes), a terminal that stops
// reading says so once and Resume lets typing through again, and closing and resizing it.
import { test, expect } from "./cue.mjs";
import { openTerminal } from "./term.mjs";

/** What reached the terminal, joined, in the order term_write calls were made. */
const written = async (cue) => (await cue.calls("term_write")).filter((c) => !("error" in c)).map((c) => c.args.data).join("");

test.describe("its terminal", () => {
  test.beforeEach(async ({ cue }) => { await cue.open("terminal"); });

  test("opens attached, once", async ({ cue, page }) => {
    await openTerminal(page);
    const opens = await cue.calls("term_open");
    expect(opens).toHaveLength(1);
    expect(opens[0].args).toMatchObject({ sessionId: "relay" });
    expect(opens[0].args.cols).toBeGreaterThan(20);
  });

  test("keystrokes land in the order typed, one at a time, even when the first is slow", async ({ cue, page }) => {
    await openTerminal(page);
    // The backend takes the first keystroke slowly and the rest at once: without the lane they'd land out of order.
    await cue.override("term_write", async (args) => {
      const log = (window.__landed ||= []);
      const start = performance.now();
      await new Promise((r) => setTimeout(r, log.length === 0 ? 150 : 0));
      log.push({ data: args.data, start, end: performance.now() });
      return null;
    });
    await page.keyboard.type("git log");
    await page.waitForFunction(() => (window.__landed || []).length === 7);
    const landed = await page.evaluate(() => window.__landed);
    expect(landed.map((l) => l.data).join("")).toBe("git log");
    // Each one only once the one before it was back.
    for (let i = 1; i < landed.length; i++) expect(landed[i].start).toBeGreaterThanOrEqual(landed[i - 1].end);
  });

  test("Enter and Backspace reach it as the bytes a terminal sends", async ({ cue, page }) => {
    await openTerminal(page);
    await page.keyboard.type("lsx");
    await page.keyboard.press("Backspace");
    await page.keyboard.press("Enter");
    await expect.poll(() => written(cue)).toBe("lsx\x7f\r");
  });

  test("closing it detaches the client it opened", async ({ cue, page }) => {
    await openTerminal(page);
    await page.locator('.tstrip[data-sid="relay"]').click();
    await cue.called("term_close", { sessionId: "relay", gen: 1 });
    await expect(page.locator(".term-host")).toHaveCount(0);
  });

  test("a smaller window resizes it", async ({ cue, page }) => {
    await openTerminal(page);
    const cols = (await cue.calls("term_open"))[0].args.cols;
    await page.setViewportSize({ width: 1000, height: 700 });
    await page.waitForFunction((c) => CueBridge.called("term_resize").some((x) => x.args.cols < c), cols);
  });
});

test.describe("a terminal that stopped reading", () => {
  test.beforeEach(async ({ cue }) => { await cue.open("terminal", { flags: "stalled=1" }); });

  test("says so once, and what's typed after isn't sent", async ({ cue, page }) => {
    await openTerminal(page);
    await page.keyboard.type("abc");
    await expect(page.locator(".term-stalled")).toHaveCount(1);
    await page.keyboard.type("def");
    await expect(page.locator(".term-stalled")).toHaveCount(1);
    expect((await cue.calls("term_write")).map((c) => c.args.data)).toEqual(["a"]);
  });

  test("Resume lets typing through again", async ({ cue, page }) => {
    await openTerminal(page);
    await page.keyboard.type("a");
    await page.locator(".term-stalled button").click();
    await cue.called("term_resume", { sessionId: "relay" });
    await expect(page.locator(".term-stalled")).toHaveCount(0);
    await page.locator(".term-host").click();
    await page.keyboard.type("q");
    await cue.called("term_write", { sessionId: "relay", data: "q" });
    expect(await written(cue)).toBe("q");
  });
});

test.describe("its text size", () => {
  test.beforeEach(async ({ cue }) => { await cue.open("terminal"); });

  test("is the chat's", async ({ page }) => {
    await openTerminal(page);
    const { term, chat } = await page.evaluate(() => ({
      term: termObj.options.fontSize,
      // A chat message's code block: the chat's text size (--type-body), in a monospace font like the terminal's.
      chat: (() => { const m = document.createElement("div"); m.className = "msg"; m.innerHTML = "<pre>x</pre>"; document.body.append(m); const px = parseFloat(getComputedStyle(m.firstChild).fontSize); m.remove(); return px; })(),
    }));
    expect(chat).toBe(14);
    expect(term).toBe(chat);
  });

  test("follows Text size at once, drawn at the new zoom's pixels, refitted, still at the bottom", async ({ page }) => {
    await openTerminal(page);
    const cell = () => page.evaluate(() => ({ cols: termObj.cols, device: termObj._core._renderService.dimensions.device.cell.width, css: termObj._core._renderService.dimensions.css.cell.width }));
    const before = await cell();
    // The app's zoom as the Mac window does it: the page laid out that much smaller and drawn at that many more pixels
    // per point, with no event a terminal hears (it learns of it only when its font is set).
    await page.evaluate(() => {
      const dpr = window.devicePixelRatio * 1.3;
      Object.defineProperty(window, "devicePixelRatio", { configurable: true, get: () => dpr });
      document.documentElement.style.zoom = "1.3";
    });
    await page.evaluate(() => setSetting("appearance.zoom", 1.3));
    await page.waitForFunction((d) => termObj._core._renderService.dimensions.device.cell.width > d * 1.2, before.device);
    const after = await cell();
    expect(after.cols).toBeLessThan(before.cols);
    expect(after.css).toBeCloseTo(before.css, 0);   // the same size in points (to xterm's whole device pixels)
    expect(await page.evaluate(() => termObj.buffer.active.viewportY >= termObj.buffer.active.baseY)).toBe(true);
  });
});
