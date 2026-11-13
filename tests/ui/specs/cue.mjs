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
