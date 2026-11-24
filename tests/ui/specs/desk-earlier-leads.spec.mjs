// Earlier leads in the Desk's lead chat (#792, option B of #788): after a hand-off the chat starts at a moss band
// ("desk-lead-13 → web-lead"), "Show earlier" over it naming the lead before, how many messages and from when to when.
// A click puts that lead's messages back above the band, and the same marker for the lead before it at their top.
import { test, expect, openDesk } from "./cue.mjs";


test("the lead chat starts at the hand-off band; Show earlier brings back each earlier lead in turn", async ({ page, cue }) => {
  await cue.open("desk");
  // The lead's own messages are all in its thread (none earlier in Cue's log), so its chat starts where it took over.
  await cue.override("session_log_page", ({ sessionId }) => []);
  await cue.override("transcript_page", () => []);
  await cue.override("desk_lead_before", ({ sid }) => {
    const t = Date.now();
    if (sid === "lead") return { sid: "l13", name: "desk-lead-13", at_ms: t - 1 * 3600e3, handoff: true, count: 2, first_ms: t - 5 * 3600e3, last_ms: t - 1 * 3600e3, note: "HANDOFF.md", cwd: "/tmp" };
    if (sid === "l13") return { sid: "l12", name: "desk-lead-12", at_ms: t - 5 * 3600e3, handoff: true, count: 1, first_ms: t - 9 * 3600e3, last_ms: t - 5 * 3600e3, note: "", cwd: "" };
    return null;
  });
  await cue.override("session_log", ({ sessionId }) => {
    const t = Date.now();
    if (sessionId === "l13") return [{ at_ms: t - 4 * 3600e3, role: "you", text: "Why does the rail dot one wait?" }, { at_ms: t - 4 * 3600e3 + 60e3, role: "agent", text: "It waits after Search stays centred: both change header.js." }];
    if (sessionId === "l12") return [{ at_ms: t - 8 * 3600e3, role: "agent", text: "Took the Desk on this morning." }];
    return [];
  });
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  const band = chat.locator(".dk-hoff");
  await expect(band).toHaveCount(1);
  await expect(band.locator(".dk-hoff-a")).toHaveText("desk-lead-13→web-lead");
  await expect(band.locator(".dk-hoff-b")).toHaveText(/^Hand-off · \d\d:\d\d · the Desk passed to a fresh lead$/);
  await expect(band.locator(".dk-hoff-note")).toHaveText("Hand-off note");
  // Show earlier sits over the band; nothing of the earlier lead's yet, and no Earlier messages row.
  const earl = chat.locator(".dk-earl");
  await expect(earl).toHaveText(/^Show earlier ·desk-lead-13 · 2 messages · \d\d:\d\d to \d\d:\d\d$/);
  await expect(chat.locator(".cv-older")).toHaveCount(0);
  await expect(chat).not.toContainText("Why does the rail dot one wait?");
  const order = () => chat.evaluate((c) => [...c.children].map((el) => el.classList.contains("dk-earl") ? "earl" : el.classList.contains("dk-hoff") ? "band" : el.classList.contains("dk-earlier") ? "msgs" : "row").filter((x, n, a) => x !== "row" || a[n - 1] !== "row"));
  expect(await order()).toEqual(["earl", "band", "row"]);
  await expect(chat.locator("[title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) await page.locator(".dk2b-chat").screenshot({ path: `${process.env.SHOT_DIR}/earlier-band.png` });
  // Show earlier: desk-lead-13's messages above the band, and the marker now for desk-lead-12 over them.
  await earl.click();
  await expect(chat.locator(".dk-earlier")).toContainText("Why does the rail dot one wait?");
  await expect(chat.locator(".dk-earlier")).toContainText("It waits after Search stays centred");
  await expect(chat.locator(".dk-earl")).toHaveText(/^Show earlier ·desk-lead-12 · 1 message · /);
  await expect(chat.locator(".dk-hoff .dk-hoff-a")).toHaveText(["desk-lead-12→desk-lead-13", "desk-lead-13→web-lead"]);
  expect(await order()).toEqual(["earl", "band", "msgs", "band", "row"]);
  if (process.env.SHOT_DIR) await page.locator(".dk2b-chat").screenshot({ path: `${process.env.SHOT_DIR}/earlier-shown.png` });
  // The first lead: its messages, and nothing to show before it.
  await chat.locator(".dk-earl").click();
  await expect(chat.locator(".dk-earlier").first()).toContainText("Took the Desk on this morning.");
  await expect(chat.locator(".dk-earl")).toHaveCount(0);
  expect(await order()).toEqual(["msgs", "band", "msgs", "band", "row"]);
});

test("a lead that was the Desk's first has no band and keeps its Earlier messages", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const chat = page.locator(".dk-chat");
  await expect(chat.locator(".cv-older")).toHaveCount(1);
  await expect(chat.locator(".dk-hoff, .dk-earl")).toHaveCount(0);
});
