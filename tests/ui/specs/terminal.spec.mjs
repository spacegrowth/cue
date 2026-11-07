// A session's terminal in Cue: what's typed reaches it in order (ui/invoke.js's lanes), a terminal that stops
// reading says so once and Resume lets typing through again, closing and resizing it, and a remote executor's
// own terminal opening once from each way in.
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

// dns-cutover: a remote executor Cue holds as a session, in a tmux session of its own on build-box. From each way
// in, its chat opens and its own terminal attaches, once; its lead's row opens only the lead's chat.
test.describe("a remote executor's terminal", () => {
  test.beforeEach(async ({ cue }) => { await cue.open("crew-remote"); });
  const open = (page) => page.evaluate(() => active?.sid);
  const termOpens = async (cue, sid) => (await cue.calls("term_open")).filter((c) => c.args.sessionId === sid).length;

  test("from its row in its lead's tree", async ({ cue, page }) => {
    await page.locator('.team-tree [data-exec-term="rx-dns"]').first().click();
    await cue.called("term_open", { sessionId: "rx-dns" });
    expect(await open(page)).toBe("rx-dns");
    await page.locator('.team-tree [data-exec-term="rx-dns"]').first().click();
    expect(await termOpens(cue, "rx-dns")).toBe(1);
  });

  test("from its lead's Agents tab", async ({ cue, page }) => {
    await page.locator('.working[data-session="rlead"], .team-top[data-session="rlead"]').first().click();
    await page.locator('.active-pane [data-bg-tab="agents"][data-sid="rlead"]').first().click();
    await page.locator('.bg-page [data-exec-term="rx-dns"]').first().click();
    await cue.called("term_open", { sessionId: "rx-dns" });
    expect(await open(page)).toBe("rx-dns");
    expect(await termOpens(cue, "rx-dns")).toBe(1);
  });

  test("from the live list", async ({ cue, page }) => {
    await page.keyboard.press("l");
    await page.locator('.lv-row[data-sid="rx-dns"]').click();
    await cue.called("term_open", { sessionId: "rx-dns" });
    expect(await open(page)).toBe("rx-dns");
    expect(await termOpens(cue, "rx-dns")).toBe(1);
  });

  test("its lead's own row opens the lead's chat, no terminal", async ({ cue, page }) => {
    await page.locator('.working[data-session="rlead"], .team-top[data-session="rlead"]').first().click();
    await expect.poll(() => open(page)).toBe("rlead");
    await expect(page.locator(".active-pane")).toContainText("infra-ops");
    expect(await cue.calls("term_open")).toEqual([]);
  });
});
