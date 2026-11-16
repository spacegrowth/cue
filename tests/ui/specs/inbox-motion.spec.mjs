// The Inbox's motion through redraws (ui/inbox.js, app/render.js replayMotion): a redraw rebuilds every row, and
// must not start an animation over. Just after a Send, redraws come several a second.
import { test, expect } from "./cue.mjs";

test("after Send, the row sinks into Running once: redraws don't replay its fade, and the Thinking dots keep counting", async ({ cue, page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await cue.open("board-waiting");
  // The tests' page stills every animation; this one is about them.
  await page.evaluate(() => { for (const st of document.querySelectorAll("style")) if (st.textContent.includes("animation:none!important")) st.remove(); });
  await page.evaluate(() => localStorage.setItem("cue.inboxFold.run", "1"));
  await page.locator('.switch-view [data-view="inbox"]').click();
  await page.locator('.ib-row[data-inbox-open="sec"]').click();
  // Every frame: the sent row's opacity, and the second Thinking dot's.
  await page.evaluate(() => {
    window.__frames = [];
    const t0 = performance.now();
    const f = () => {
      const r = document.querySelector('.ib-row[data-inbox-open="sec"]'), d = document.querySelector(".ib-dots i:nth-child(2)");
      window.__frames.push({ t: performance.now() - t0, row: r ? +getComputedStyle(r).opacity : null, dot: d ? +getComputedStyle(d).opacity : null });
      requestAnimationFrame(f);
    };
    requestAnimationFrame(f);
  });
  await page.locator('.inbox-page textarea[data-text="s:sec"]').fill("yes, run it");
  await page.locator('.inbox-page [data-act="send"]').click();
  await page.waitForTimeout(300);
  await cue.change((st) => {
    st.items = st.items.filter((i) => i.session_id !== "sec");
    const s = st.sessions.find((x) => x.session_id === "sec");
    Object.assign(s, { state: "working", since_ms: Date.now() });
    s.thread.push({ role: "you", text: "yes, run it", at_ms: Date.now() });
  });
  // Steps landing: a redraw each.
  for (let i = 0; i < 6; i++) { await page.waitForTimeout(150); await cue.change((st, i) => { st.sessions.find((x) => x.session_id === "sec").doing = `step ${i}`; }, i); }
  await page.waitForTimeout(300);
  const frames = await page.evaluate(() => window.__frames);
  const rows = frames.filter((f) => f.row !== null && f.row < 0.99);
  expect(rows.length).toBeGreaterThan(20);
  // The fade only rises (a replay would drop it back to its start).
  for (let n = 1; n < rows.length; n++) expect(rows[n].row).toBeGreaterThanOrEqual(rows[n - 1].row - 0.005);
  // Done fading within its 0.6 s.
  expect(rows.at(-1).row).toBeCloseTo(0.6, 2);
  // The dots count: the second one shows at some point while it's Thinking.
  expect(frames.some((f) => f.dot === 1)).toBe(true);
});
