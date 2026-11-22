// The `cue` fixture every UI test uses: the real UI opened on a scenario (tests/ui/app.html), the fake bridge's
// record of calls, and checks that run after every test: no page errors, no console errors, no command the
// bridge doesn't know, and no "Cue can't draw its window".
import { test as base, expect } from "@playwright/test";
import { NOW } from "./clock.mjs";

class Cue {
  constructor(page) { this.page = page; }

  /** Opens `scenario` with the clock at NOW: frozen there (`frozen`, for screenshots) or running on from it.
   *  `flags`: more of the page's query, for the fakes (e.g. "stalled=1"). Resolves once the board is drawn. */
  async open(scenario, { flags = "", frozen = false } = {}) {
    if (frozen) await this.page.clock.setFixedTime(NOW);
    else await this.page.clock.install({ time: NOW });
    await this.page.goto(`app.html?scenario=${scenario}&latency=0&still=1${flags ? `&${flags}` : ""}`);
    await this.page.locator('#app [data-view="board"]').waitFor();
  }

  /** Opens the mini window (tests/ui/mini.html) on `scenario` as `shape` ("notch" or "rail"), the clock as open()
   *  sets it. Resolves once it has drawn its first shape. */
  async openMini(scenario, shape, { flags = "", frozen = false } = {}) {
    if (frozen) await this.page.clock.setFixedTime(NOW);
    else await this.page.clock.install({ time: NOW });
    await this.page.goto(`mini.html?scenario=${scenario}&latency=0&still=1${flags ? `&${flags}` : ""}#${shape}`);
    await this.page.locator("#mini .m-panel").waitFor();
  }

  /** The calls of `cmd` so far, each { cmd, args, result | error }. */
  calls(cmd) { return this.page.evaluate((c) => CueBridge.called(c), cmd); }

  /** Waits for a call of `cmd` whose args match `args` (each field equal), and returns it. */
  async called(cmd, args = {}) {
    const h = await this.page.waitForFunction(([c, want]) => CueBridge.called(c).find((x) => Object.entries(want).every(([k, v]) => JSON.stringify(x.args[k]) === JSON.stringify(v))), [cmd, args]);
    return h.jsonValue();
  }

  /** `cmd` answered by `fn` (a function of its args, run in the page) instead of its fake; `arg` is passed after
   *  the command's args. */
  override(cmd, fn, arg) { return this.page.evaluate(([c, src, a]) => CueBridge.override(c, (args) => (0, eval)(`(${src})`)(args, a)), [cmd, fn.toString(), arg]); }

  /** An event from the app (state, term, …). */
  emit(name, payload) { return this.page.evaluate(([n, p]) => CueBridge.emit(n, p), [name, payload]); }

  /** Changes the world's state in the page (`fn(state, arg)`) and sends it, as the hub does after a change. */
  change(fn, arg) { return this.page.evaluate(([src, a]) => { (0, eval)(`(${src})`)(CueBridge.world.state, a); CueBridge.emit("state", CueBridge.world.snapshot()); }, [fn.toString(), arg]); }
}

export const test = base.extend({
  cue: async ({ page }, use) => {
    const errors = [];
    page.on("pageerror", (e) => errors.push(`page error: ${e.message}`));
    page.on("console", (m) => { if (m.type() === "error") errors.push(`console: ${m.text()}`); });
    await use(new Cue(page));
    const unknown = await page.evaluate(() => CueBridge.calls.filter((c) => /^unknown command/.test(c.error || "")).map((c) => c.cmd));
    expect(unknown, "commands the bridge doesn't know").toEqual([]);
    expect(await page.locator(".render-error").count(), "Cue can't draw its window").toBe(0);
    expect(errors).toEqual([]);
  },
});
export { expect };

/** Project `p` picked in the header's project menu (All projects with ""). The menu is in the header on the Board and
 *  Read only (#666): from another view it's picked on the Board, then that view is shown again. */
export const pick = async (page, p) => {
  const toggle = page.locator('[data-pj="toggle"]');
  // The window drawn first (just after a reload its scripts may not have run yet).
  await page.locator('.switch-view [data-view="board"]').waitFor();
  const was = (await toggle.count()) ? "" : await page.evaluate(() => view);
  if (was) await page.locator('.switch-view [data-view="board"]').click();
  await toggle.click();
  await page.locator(`.pj-pop [data-pj="pick"][data-project="${p}"]`).click();
  if (was) await page.locator(`.switch-view [data-view="${was}"]`).click();
};
/** The Desk opened from the header on project `p`; resolves once its first item row is drawn. */
/** The Desk's sessions list, B2's All N sessions (#666): the Working pill unfolded in the strip over the lead's chat,
 *  then All N sessions › opens the list on the right in the item list's place (its head: the Desk's name, its
 *  sessions count, How work lands, holds, the lead's card and ⋯). Resolves once it's drawn. */
export const deskAllSessions = async (page) => {
  if (await page.locator(".dk-card.dk-sessions").count()) return;
  // The Desks menu's All sessions: the strip's Working pill shows only while nothing waits on you (#666).
  await page.locator('.top [data-hv-menu="desks"]').click();
  await page.locator(".top .hv-menu [data-hv-desk-sessions]").click();
  await page.locator(".dk-card.dk-sessions").waitFor();
  await deskSessionsSettled(page);
};
/** All sessions settled: the chat's steps come in a moment after it opens and redraw the window once (chat/thread.js
 *  loadSteps); reading the list before then can read a row that's being swapped out. */
export const deskSessionsSettled = (page) => page.waitForFunction(() => new Promise((r) => {
  const card = document.querySelector(".dk-card.dk-sessions");
  setTimeout(() => requestAnimationFrame(() => r(!stepsBusy && !!card?.isConnected)), 400);
}), null, { polling: 100 });
/** The strip over the lead's chat (Working, what waits on you, Check) in the panel's place: B2 opens with the panel
 *  shown (#666), so Hide panel first when it is. */
export const deskShowStrip = async (page) => {
  if (await page.locator(".dk2b-chat .dk2b-strip").count()) return;
  await page.locator('.dk2b-chat [data-desk-panel="dispatcher-hidden"]').click();
  await page.locator(".dk2b-chat .dk2b-strip").waitFor();
};
/** The lead's full panel (Now, Waits on you, Working) shown: B2 opens with it shown (#666), and
 *  a chat card too narrow for it (the 1280 default) has no Show panel, so the window is made as wide as the Board's
 *  design size (1800) first when it's narrower. */
export const deskShowPanel = async (page) => {
  const tog = page.locator('.dk2b-chat [data-desk-panel="dispatcher-hidden"]');
  const vp = page.viewportSize();
  if (vp.width < 1800) await page.setViewportSize({ width: 1800, height: Math.max(vp.height, 900) });
  await tog.waitFor();
  // After a resize the card redraws once it has measured itself: read the toggle again until the panel is in.
  const now = page.locator(".dk2b-chat .dk-panel .dk-pnow");
  await expect.poll(async () => {
    if (await now.isVisible()) return true;
    if ((await tog.textContent().catch(() => "")).trim() === "Show panel") await tog.click().catch(() => {});
    return now.isVisible();
  }, { timeout: 10000 }).toBe(true);
};
export const openDesk = async (page, p = "cue") => {
  await pick(page, p);
  await page.locator('.switch-view [data-view="desk"]').click();
  // A row in view, in whichever group: the Queue may be folded (#576).
  await page.locator(".dk-items .dk-it:visible").first().waitFor();
};
