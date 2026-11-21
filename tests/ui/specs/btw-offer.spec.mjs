// A message kept for a session that has been at its turn a long time (#535): under it, why it may wait a while, Ask it
// now, by the way (the message out of Cue's keeping, asked on the side) and Keep it queued. Under the side answer, Queue
// it again puts it back. Only a Claude Code session's, and only once its turn has run 45 minutes.
import { test, expect } from "./cue.mjs";

/** Session `sid` working for `min` minutes, with `text` kept for it. */
const keptFor = (cue, sid, min, text, harness = "claude") => cue.change((st, [sid, min, text, harness]) => {
  const s = st.sessions.find((x) => x.session_id === sid), t = now() - min * 60_000;
  Object.assign(s, { state: "working", harness, since_ms: t, segments: [{ kind: "waiting", start_ms: t - 600_000, end_ms: t }, { kind: "working", start_ms: t, end_ms: null }] });
  s.held = { role: "you", text, at_ms: now() - 12 * 60_000, images: [] };
}, [sid, min, text, harness]);

const openEng = async (page) => page.locator('.working[data-session="eng"]').first().click();

test("busy 64 minutes: the offer under the kept message, asked by the way, then queued again", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await keptFor(cue, "eng", 64, "Why did it skip the CSV export?");
  await openEng(page);
  const pane = page.locator(".active-pane"), mine = pane.locator(".cv-you", { hasText: "Why did it skip the CSV export?" });
  await expect(mine.locator(".msg-line")).toContainText("Not sent yet");
  const offer = mine.locator(".btw-offer");
  await expect(offer.locator(".btw-offer-why")).toHaveText("It's been at this 1h 04m, so this may wait a while.");
  await expect(offer.locator("button")).toHaveText(["Ask it now, by the way", "Keep it queued"]);
  await expect(offer.locator(".btw-offer-note")).toHaveText("A side question gets an answer from what it already knows. It can't run anything or change what it's doing.");
  await expect(offer.locator("[title]")).toHaveCount(0);

  await offer.getByRole("button", { name: "Ask it now, by the way" }).click();
  // Out of Cue's keeping, asked on the side: the panel has the question and, once it answers, its answer.
  await expect(mine).toHaveCount(0);
  expect(await page.evaluate(() => sessionOf("eng").held ?? null)).toBe(null);
  const pop = pane.locator(".btw-pop");
  await expect(pop.locator(".btw-q")).toHaveText("Why did it skip the CSV export?");
  await expect(pop.locator(".btw-a.msg")).toContainText("finance script");
  // The answer didn't help: back to waiting for its turn's end, as before.
  await pop.getByRole("button", { name: "Queue it again" }).click();
  await expect(pop.locator(".btw-requeued")).toHaveText("Queued again");
  await expect(pane.locator(".cv-you", { hasText: "Why did it skip the CSV export?" }).locator(".msg-line")).toContainText("Not sent yet");
});

test("Keep it queued: the offer goes, the message stays kept", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await keptFor(cue, "eng", 50, "and the footer too");
  await openEng(page);
  const mine = page.locator(".active-pane .cv-you", { hasText: "and the footer too" });
  await expect(mine.locator(".btw-offer-why")).toHaveText("It's been at this 50m, so this may wait a while.");
  await mine.getByRole("button", { name: "Keep it queued" }).click();
  await expect(mine.locator(".btw-offer")).toHaveCount(0);
  await expect(mine.locator(".msg-line")).toContainText("Not sent yet");
});

test("no offer before 45 minutes, nor for a session by the way can't ask", async ({ cue, page }) => {
  await cue.open("board-waiting");
  await keptFor(cue, "eng", 30, "and the footer too");
  await openEng(page);
  const mine = page.locator(".active-pane .cv-you", { hasText: "and the footer too" });
  await expect(mine.locator(".msg-line")).toContainText("Not sent yet");
  await expect(mine.locator(".btw-offer")).toHaveCount(0);
  await keptFor(cue, "eng", 90, "and the footer too", "codex");
  await expect(mine.locator(".msg-line")).toContainText("Not sent yet");
  await expect(mine.locator(".btw-offer")).toHaveCount(0);
});
