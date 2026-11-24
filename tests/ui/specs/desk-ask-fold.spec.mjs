// A permission card you answer on the Desk folds shut as its rows do (#793): Allow or Deny goes to Cue at once, the
// card stays where it was while it folds (its height easing to nothing, what's under it sliding up), then it's gone.
// With reduced motion it goes at once.
import { test, expect, openDesk } from "./cue.mjs";

const T = 1_800_000_000_000;

/** Item 227's session asks to run a command, on the item; a step held for your OK sits over the Items list. */
async function setup(cue, page) {
  await cue.open("desk");
  await cue.change((st, T) => {
    const worker = st.sessions.find((s) => s.session_id === "host");
    worker.state = "deciding";
    const d = st.desk.projects.find((x) => x.project === "cue");
    for (const item of d.items.filter((i) => i.holder?.sid === "host")) { item.state = "working"; item.now_on = false; item.ask = null; }
    const it = d.items.find((item) => item.holder?.sid === "host");
    Object.assign(it, { id: "227", kind: "feature", title: "Give the Desk its own icon", state: "needs_you", ask: { item_id: "perm", sid: "host", kind: "permission", text: "printf fold-check" } });
    st.items.push({ id: "perm", session_id: "host", project: "cue", kind: "permission", status: "pending", tool_name: "Bash", tool_input: { command: "printf fold-check" }, suggestions: [], created_ms: T });
    d.held = [{ id: "held-one", sid: "site", who: "site", what: "delete files", command: "rm -rf build", at: T }];
  }, T);
  await openDesk(page);
}

/** The test page turns animations off; the fold needs them back (as desk-check-still's motionOn). */
const motionOn = (page) => page.evaluate(() => { for (const st of document.querySelectorAll("style")) if (st.textContent.includes("animation:none!important")) st.remove(); });

/** Whether `sel` is folding now: the rows' own fold animation, not clickable, and, stepped through it (paused, so a busy
 *  machine can't skip it), its height easing from where it began to nothing, never in one jump: "folding", else
 *  what's wrong. */
const folding = (page, sel) => page.evaluate((sel) => {
  const el = document.querySelector(sel);
  if (!el) return "gone";
  const [a, ...more] = el.getAnimations();
  if (!el.classList.contains("dk-leave") || a?.animationName !== "dkn-leave" || more.length || getComputedStyle(el).pointerEvents !== "none") return `still:${a?.animationName}`;
  const full = parseFloat(el.style.getPropertyValue("--h")), end = a.effect.getComputedTiming().endTime;
  a.pause();
  const hs = [0, 0.1, 0.25, 0.5, 0.75, 1].map((t) => { a.currentTime = t * end; return el.getBoundingClientRect().height; });
  const down = hs.every((h, k) => k === 0 || h < hs[k - 1]), jump = Math.max(...hs.slice(1).map((h, k) => hs[k] - h));
  return Math.abs(hs[0] - full) < 1 && hs[5] < 1 && down && jump < full * 0.6 ? "folding" : `jumpy:${full}:${hs.map(Math.round)}`;
}, sel);

/** Stops the page's clock where it is, so the card stays while it's looked at; runFor moves it on. */
const pauseClock = async (page) => page.clock.pauseAt(await page.evaluate(() => Date.now() + 1000));

for (const scheme of ["light", "dark"]) {
  test(`Allow on an item's permission sends at once and its card folds shut, then goes (${scheme}) (#793)`, async ({ cue, page }) => {
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: "no-preference" });
    await setup(cue, page);
    await page.locator('.dk-items [data-desk-item="227"]').click();
    const card = page.locator(".dk-right .dk-ask");
    await expect(card).toContainText("printf fold-check");
    await motionOn(page);
    await pauseClock(page);
    await page.locator('.dk-right .dk-ask [data-act="allow"][data-id="perm"]').click();
    // Sent at once, not after the fold.
    await cue.called("respond", { id: "perm", decision: { behavior: "allow" } });
    expect(await folding(page, ".dk-right .dk-ask.dk-leave")).toBe("folding");
    // Gone once shut.
    await page.clock.runFor(400);
    await expect(page.locator(".dk-right .dk-ask")).toHaveCount(0);
  });
}

test("No on a held step sends at once and its row folds shut over the Items list (#793)", async ({ cue, page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await setup(cue, page);
  const held = page.locator('.dk-items [data-held-row="held-one"]');
  await expect(held).toBeVisible();
  await motionOn(page);
  await pauseClock(page);
  await held.locator('[data-allow="0"]').click();
  await cue.called("desk_held_answer", { id: "held-one", allow: false });
  expect(await folding(page, '.dk-items .dk-prows.dk-leave:has([data-held-row="held-one"])')).toBe("folding");
  await page.clock.runFor(400);
  await expect(held).toHaveCount(0);
});

test("with reduced motion an answered permission goes at once (#793)", async ({ cue, page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await setup(cue, page);
  await page.locator('.dk-items [data-desk-item="227"]').click();
  await page.locator('.dk-right .dk-ask [data-act="deny"][data-id="perm"]').click();
  await cue.called("respond", { id: "perm", decision: { behavior: "deny" } });
  expect(await page.locator(".dk-right .dk-ask.dk-leave").count()).toBe(0);
});
