// Cue's real window, over WebDriver: clicks and keys go through the embedded driver; `run` evaluates in
// the page (the window's own globals: state, invoke, …), which also measures how fast the main thread answers.
import assert from "node:assert/strict";
import { waitFor } from "./poll.js";

/** The window must answer within this (ms) the whole time, or it counts as frozen. */
export const RESPONSIVE_MS = 500;

export class Window {
  constructor(browser) {
    this.b = browser;
  }

  /** Evaluate `fn(...args)` in the window. */
  run(fn, ...args) {
    return this.b.execute(fn, ...args);
  }

  /** Evaluate `fn(...args)` in the window over the driver's direct channel (`/wdio/eval`): one dispatch, its
   *  result posted back as plain JSON. The W3C `execute` path polls for its result in further evaluations, and
   *  under a long run's load WebKit intermittently fails one of those ("a result of an unsupported type",
   *  code 5) and the call with it; this path is the driver's own for WebKit and recovers a lost completion.
   *  `args` and the result must be plain JSON. */
  async runDirect(fn, ...args) {
    const { hostname, port } = this.b.options;
    const script = `const done = arguments[0]; Promise.resolve().then(() => (${fn}).apply(null, ${JSON.stringify(args)}))
      .then((value) => done({ ok: true, value: value === undefined ? null : value }), (e) => done({ ok: false, error: String(e && e.stack || e) }));`;
    const res = await fetch(`http://${hostname}:${port}/wdio/eval`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ script }) });
    const said = await res.json();
    if (!res.ok) throw new Error(`the window's script failed: ${said.error}`);
    return said.value;
  }

  /** How long one evaluation takes to come back (ms): the window's main thread has to answer it. */
  async latency() {
    const t = performance.now();
    const said = await this.runDirect(() => "ok");
    if (said !== "ok") throw new Error(`the window answered ${JSON.stringify(said)} to a timing probe`);
    return performance.now() - t;
  }

  /** Wait until `fn(...args)`, evaluated in the window, is truthy; returns its value. */
  waitUntil(fn, what, timeout = 10_000, ...args) {
    return waitFor(() => this.b.execute(fn, ...args), what, { timeout });
  }

  /** Wait until the window's text contains `text`. */
  waitText(text, timeout = 10_000) {
    return this.waitUntil((t) => document.body.innerText.includes(t), `"${text}" in the window`, timeout, text);
  }

  /** The element for `selector`, once it exists. */
  async find(selector, timeout = 10_000) {
    const el = await this.b.$(selector);
    await el.waitForExist({ timeout, timeoutMsg: `${selector} never appeared` });
    return el;
  }

  async click(selector, timeout) {
    await (await this.find(selector, timeout)).click();
  }

  /** A PNG of the window, as a Buffer. */
  async screenshot() {
    return Buffer.from(await this.b.takeScreenshot(), "base64");
  }

  async close() {
    await this.b.deleteSession().catch(() => {});
  }
}

/** Measures the window's answer time, over and over, while a journey does something heavy; `stop()`
 *  returns every sample (ms). A window whose main thread is stuck makes a sample as long as the stall. */
export function watchLatency(win) {
  const samples = [];
  let on = true;
  const done = (async () => {
    while (on) samples.push(await win.latency());
  })();
  return {
    samples,
    async stop() {
      on = false;
      await done;
      return samples;
    },
  };
}

/** Type `n` keys, 20 at a time, then wait for `after` if given, measuring the window's answer time the
 *  whole while; the samples (ms). */
export async function typeWatched(world, keys, n, after) {
  const probe = watchLatency(world.ui);
  try {
    for (let i = 0; i < n / 20; i++) await keys.addValue("y".repeat(20));
    if (after) await after();
  } finally {
    await probe.stop();
  }
  return probe.samples;
}

export function assertResponsive(samples) {
  const worst = Math.max(...samples);
  assert.ok(samples.length > 20, `the window was asked often enough (${samples.length})`);
  assert.ok(worst < RESPONSIVE_MS, `the window answered within ${RESPONSIVE_MS} ms throughout (worst ${worst.toFixed(0)} ms)`);
}
