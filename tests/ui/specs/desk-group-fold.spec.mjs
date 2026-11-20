// The Desk's item list (#430): a click on a group's header (In progress, Waiting on you, Queued, …) folds or opens it
// with a short slide; its chevron turns, its count stays, and it stays as you left it, across restarts.
import { test, expect, openDesk } from "./cue.mjs";

const head = (page, key) => page.locator(`.dk-items [data-desk-group="${key}"]`);
const body = (page, key) => page.locator(`.dk-items [data-desk-gbody="${key}"]`);
// The tests' window turns transitions off (load.js, still=1); this one watches the slide, so it turns them back on.
// The list is drawn again as the Desk's finished items come in: wait till it has been still for a moment.
const settle = (page) => page.evaluate(() => new Promise((res) => {
  let t;
  const done = () => { mo.disconnect(); res(); };
  const mo = new MutationObserver(() => { clearTimeout(t); t = setTimeout(done, 300); });
  mo.observe(document.querySelector(".dk-items"), { childList: true, subtree: true });
  t = setTimeout(done, 300);
}));
const moving = (page) => page.evaluate(() => [...document.head.querySelectorAll("style")].find((el) => el.textContent.includes("transition:none!important"))?.remove());

test("a group's header folds and opens it with a short slide; its count stays; it stays folded after a restart (#430)", async ({ page, cue }) => {
  await cue.open("desk");
  await moving(page);
  await openDesk(page);
  await settle(page);
  for (const key of ["you", "working", "queued"]) await expect(head(page, key)).toHaveAttribute("aria-expanded", "true");
  const tall = (await body(page, "queued").boundingBox()).height;
  expect(tall).toBeGreaterThan(40);
  await expect(head(page, "queued").locator(".dk-gchev")).toBeVisible();
  // About 200ms, height and fade together; no hover text.
  expect(await body(page, "queued").evaluate((el) => getComputedStyle(el).transitionDuration)).toMatch(/^0\.2s/);
  await expect(page.locator(".dk-items [data-desk-group][title]")).toHaveCount(0);

  // Fold Queued: midway it's part of the way shut, then shut; the header and its count stay.
  const n = await head(page, "queued").locator(".n").textContent();
  // Its heights frame by frame as it folds: some part of the way, then none.
  const hs = await page.evaluate(() => new Promise((res) => {
    const b = document.querySelector('.dk-items [data-desk-gbody="queued"]'), hs = [], t0 = performance.now();
    document.querySelector('.dk-items [data-desk-group="queued"]').click();
    const tick = () => { hs.push(b.getBoundingClientRect().height); performance.now() - t0 < 400 ? requestAnimationFrame(tick) : res(hs); };
    requestAnimationFrame(tick);
  }));
  expect(hs.some((h) => h > 1 && h < tall - 1)).toBe(true);
  expect(hs.at(-1)).toBe(0);
  await expect.poll(async () => (await body(page, "queued").boundingBox())?.height ?? 0).toBe(0);
  await expect(head(page, "queued")).toHaveAttribute("aria-expanded", "false");
  await expect(head(page, "queued").locator(".n")).toHaveText(n);
  await expect(page.locator('.dk-items [data-desk-gbody="queued"] .dk-it').first()).toBeHidden();
  // The others stay open; nothing opened.
  await expect(body(page, "working").locator(".dk-it").first()).toBeVisible();
  await expect(page.locator(".dk-it.on")).toHaveCount(0);

  // A restart: still folded.
  await cue.open("desk");
  await openDesk(page);
  await expect(head(page, "queued")).toHaveAttribute("aria-expanded", "false");
  await expect(body(page, "queued")).toHaveClass(/shut/);

  // By the keyboard too: Enter opens it again, and dragging works as before.
  await head(page, "queued").focus();
  await page.keyboard.press("Enter");
  await expect(head(page, "queued")).toHaveAttribute("aria-expanded", "true");
  await expect.poll(async () => Math.round((await body(page, "queued").boundingBox()).height)).toBe(Math.round(tall));
  await expect(body(page, "queued").locator(".dk-it").first()).toBeVisible();
});

test("with reduced motion a group folds at once, no slide (#430)", async ({ page, cue }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await cue.open("desk");
  await moving(page);
  await openDesk(page);
  await settle(page);
  expect(await body(page, "working").evaluate((el) => getComputedStyle(el).transitionDuration)).toBe("0s");
  await head(page, "working").click();
  expect((await body(page, "working").boundingBox())?.height ?? 0).toBe(0);
  await head(page, "working").click();
  await expect(body(page, "working").locator(".dk-it").first()).toBeVisible();
});
