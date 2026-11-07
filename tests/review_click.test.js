// Run: node tests/review_click.test.js  (needs Google Chrome; drives tests/review_click.html headless)
// The Review rows' buttons and the lead's team card take a real click even when Cue's state changes while the
// button is held down: Cue redraws the window on every change, many times a second with agents at work. The
// browser's own mouse presses each button (Chrome DevTools Protocol), and a state change lands between the press
// and the release, the way it does in Cue's window.
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");

const CHROME = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PAGE = `file://${path.join(__dirname, "review_click.html")}`;
const wait = (ms) => new Promise((f) => setTimeout(f, ms));

/** Chrome headless on a private profile, and a DevTools session on its page. */
async function openPage() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "cue-click-"));
  const chrome = spawn(CHROME, ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${dir}`, "--window-size=1400,900", "--allow-file-access-from-files", PAGE], { stdio: "ignore" });
  const portFile = path.join(dir, "DevToolsActivePort");
  for (let n = 0; n < 100 && !fs.existsSync(portFile); n++) await wait(100);
  const port = fs.readFileSync(portFile, "utf8").split("\n")[0];
  let target;
  for (let n = 0; n < 50 && !target; n++) { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page"); if (!target) await wait(100); }
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((f) => ws.addEventListener("open", f));
  let id = 0;
  const waiting = new Map();
  ws.addEventListener("message", (m) => { const r = JSON.parse(m.data); waiting.get(r.id)?.(r); waiting.delete(r.id); });
  const send = (method, params = {}) => new Promise((f) => { waiting.set(++id, f); ws.send(JSON.stringify({ id, method, params })); });
  const run = async (expr) => {
    const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(`${expr}: ${r.result.exceptionDetails.exception?.description}`);
    return r.result?.result?.value;
  };
  // Chrome closed through DevTools (all its processes, not just the first), then its profile removed.
  const close = async () => {
    ws.close();
    const gone = new Promise((f) => chrome.once("exit", f));
    const browser = new WebSocket((await (await fetch(`http://127.0.0.1:${port}/json/version`)).json()).webSocketDebuggerUrl);
    browser.addEventListener("open", () => browser.send(JSON.stringify({ id: 1, method: "Browser.close" })));
    await gone;
    fs.rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  };
  return { send, run, close };
}

/** Press and release the button `sel` with the mouse; `between` runs while it's held down. What Cue was asked. */
async function click(page, sel, between = "") {
  await page.run("CALLS.length = 0");
  const at = await page.run(`(() => { const r = document.querySelector(${JSON.stringify(sel)})?.getBoundingClientRect(); return r && { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
  assert.ok(at, `${sel} is on screen`);
  const mouse = (type) => page.send("Input.dispatchMouseEvent", { type, x: at.x, y: at.y, button: "left", buttons: type === "mousePressed" ? 1 : 0, clickCount: 1 });
  await mouse("mousePressed");
  if (between) await page.run(between);
  await mouse("mouseReleased");
  await wait(50);
  return page.run("CALLS.map((c) => c.action || c.text)");
}

(async () => {
  const page = await openPage();
  try {
    for (let n = 0; n < 50 && !(await page.run("typeof setActive === 'function' && !!state.sessions?.length")); n++) await wait(100);
    await page.run(`setActive(null, "L")`);
    assert.ok(await page.run(`!!document.querySelector(".rv-rows [data-crew=review]")`), "the lead's Review row is drawn");
    let tick = 0;
    const change = (rv) => `EMIT.state({ payload: STATE(${JSON.stringify(rv)}, ${++tick}) })`;

    // Waiting: Review, Diff and Set aside, each with Cue's state changing while it's held down.
    assert.deepEqual(await click(page, ".rv-rows [data-crew=review]"), ["review"], "Review: a plain click");
    for (const act of ["review", "diff"]) assert.deepEqual(await click(page, `.rv-rows [data-crew=${act}]`, change("waiting")), [act], `${act}: a redraw while it's held down`);
    // ⋯ opens its menu, and Set aside in it runs, each with a redraw while it's held down.
    await click(page, "[data-rv-more]", change("waiting"));
    assert.ok(await page.run(`!!document.querySelector(".rv-menu [data-crew=aside]")`), "⋯ opens its menu");
    assert.deepEqual(await click(page, ".rv-menu [data-crew=aside]", change("waiting")), ["aside"], "Set aside: a redraw while it's held down");
    assert.ok(await page.run(`!document.querySelector(".rv-menu")`), "the menu closes as its item runs");
    // Reviewed: Commit and Send fixes type their words into the lead.
    await page.run(change("reviewed"));
    for (const [sel, text] of [["[data-rv-text='commit it']", "commit it"], ["[data-rv-text='send the fixes']", "send the fixes"]]) {
      assert.deepEqual(await click(page, `.rv-rows ${sel}`, change("reviewed")), [text], `${text}: a redraw while it's held down`);
    }
    // › opens the findings under the row.
    await click(page, "[data-rv-open]", change("reviewed"));
    assert.ok(await page.run(`!!document.querySelector(".rv-under")`), "› opens the findings");
    // The lead's own team card and title chips: every Cue button on it (data-crew) behaves the same.
    const crew = await page.run(`[...document.querySelectorAll(".active-pane [data-crew]:not(.rv-wrap *)")].map((b) => b.dataset.crew)`);
    for (const act of crew) assert.deepEqual(await click(page, `.active-pane [data-crew="${act}"]:not(.rv-wrap *)`, change("reviewed")), [act], `team ${act}: a redraw while it's held down`);
    // Let go somewhere else: no click, and the change that came meanwhile is drawn.
    await page.run("CALLS.length = 0");
    const at = await page.run(`(() => { const r = document.querySelector(".rv-rows [data-rv-say]").getBoundingClientRect(); return { x: r.left + 5, y: r.top + 5 }; })()`);
    await page.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", buttons: 1, clickCount: 1 });
    await page.run(change("waiting"));
    await page.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: 5, y: 5, button: "left", buttons: 0, clickCount: 1 });
    await wait(50);
    assert.deepEqual(await page.run("CALLS.length"), 0, "let go elsewhere: nothing runs");
    assert.ok(await page.run(`!!document.querySelector(".rv-rows [data-crew=review]")`), "the change held during the press is drawn after it");
    console.log(`review_click.test.js ok (${crew.length} team buttons)`);
  } finally { await page.close(); }
})().catch((e) => { console.error(e.message || e); process.exit(1); });
