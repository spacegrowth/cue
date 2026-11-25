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
  // Slim, its names at the chat's text size, edge to edge of the chat and no wider, so nothing scrolls sideways (#826).
  const fit = await band.evaluate((b) => { const c = b.closest(".dk-chat"), r = b.getBoundingClientRect(), q = c.getBoundingClientRect();
    return { l: r.left - q.left, r: q.right - r.right, h: r.height, wide: c.scrollWidth - c.clientWidth, font: getComputedStyle(b.querySelector(".dk-hoff-a")).fontSize, body: getComputedStyle(document.documentElement).getPropertyValue("--type-body").trim() }; });
  expect(fit).toEqual({ l: 0, r: 0, h: expect.any(Number), wide: 0, font: fit.body, body: expect.any(String) });
  expect(fit.h).toBeLessThan(56);
  // Show earlier sits over the band; the earlier lead's last exchanges muted between them (#841), and no Earlier
  // messages row.
  const earl = chat.locator(".dk-earl");
  await expect(earl).toHaveText(/^Show earlier ·desk-lead-13 · 2 messages · \d\d:\d\d to \d\d:\d\d$/);
  await expect(chat.locator(".cv-older")).toHaveCount(0);
  await expect(chat.locator(".dk-tail")).toContainText("Why does the rail dot one wait?");
  const order = () => chat.evaluate((c) => [...c.children].map((el) => el.classList.contains("dk-earl") ? "earl" : el.classList.contains("dk-hoff") ? "band" : el.classList.contains("dk-tail") ? "tail" : el.classList.contains("dk-earlier") ? "msgs" : "row").filter((x, n, a) => x !== "row" || a[n - 1] !== "row"));
  expect(await order()).toEqual(["earl", "tail", "band", "row"]);
  await expect(chat.locator("[title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) await page.locator(".dk2b-chat").screenshot({ path: `${process.env.SHOT_DIR}/earlier-band.png` });
  // Show earlier: desk-lead-13's messages above the band in full (its last exchanges no longer apart), and the marker
  // now for desk-lead-12 over them.
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

test("after a hand-off the earlier lead's last 5 exchanges sit muted over the band: yours and its replies only", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("session_log_page", () => []);
  await cue.override("transcript_page", () => []);
  await cue.override("desk_lead_before", ({ sid }) => (sid === "lead" ? { sid: "l13", name: "desk-lead-13", at_ms: Date.now() - 60e3, handoff: true, count: 20, first_ms: Date.now() - 3600e3, last_ms: Date.now() - 60e3, note: "", cwd: "" } : null));
  await cue.override("session_log", ({ sessionId }) => {
    if (sessionId !== "l13") return [];
    const t = Date.now() - 3600e3, out = [];
    for (let n = 1; n <= 7; n++) {
      out.push({ at_ms: t + n * 60e3, role: "you", text: `Question ${n} from you` });
      out.push({ at_ms: t + n * 60e3 + 10e3, role: "agent", text: `Answer ${n} from the lead` });
    }
    // Cue's own messages to the lead and a Desk block alone: never in the tail.
    out.push({ at_ms: t + 9 * 60e3, role: "you", text: "Hand off: write a short note so a fresh session can carry on from here." });
    out.push({ at_ms: t + 9 * 60e3 + 5e3, role: "you", text: '<desk project="cue" changes="1">\n#12 working\n</desk>' });
    return out;
  });
  await openDesk(page);
  const tail = page.locator(".dk-chat .dk-tail");
  await expect(tail).toContainText("Question 3 from you");
  await expect(tail).toContainText("Answer 7 from the lead");
  await expect(tail).not.toContainText("Question 2 from you");
  await expect(tail).not.toContainText("Hand off:");
  await expect(tail).not.toContainText("#12 working");
  // Read-only and muted, above the band.
  expect(await tail.evaluate((el) => ({ inert: el.inert, op: Number(getComputedStyle(el).opacity), next: el.nextElementSibling?.classList.contains("dk-hoff") }))).toEqual({ inert: true, op: expect.any(Number), next: true });
  expect(Number(await tail.evaluate((el) => getComputedStyle(el).opacity))).toBeLessThan(1);
  await expect(page.locator(".dk-chat [title]")).toHaveCount(0);
  if (process.env.SHOT_DIR) {
    for (const [scheme, w, h] of [["light", 1280, 800], ["dark", 1280, 800], ["light", 900, 600], ["dark", 900, 600]]) {
      await page.emulateMedia({ colorScheme: scheme });
      await page.setViewportSize({ width: w, height: h });
      await page.locator(".dk-chat .dk-hoff").scrollIntoViewIfNeeded();
      await page.screenshot({ path: `${process.env.SHOT_DIR}/tail-${scheme}-${w}.png` });
    }
  }
});

test("while the Desk hands off, the chat says so and the box is held; it opens in place once the new lead is ready", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk2b-chat .foot .composer");
  const before = await box.boundingBox();
  await cue.change((st) => { const d = st.desk.projects.find((p) => p.lead?.sid === "lead"); d.lead.handoff = { from: "lead", to: "", since_ms: Date.now() }; });
  await expect(page.locator(".dk-chat .dk-hwait")).toHaveText("Handing off to a new lead…");
  await expect(box).toHaveClass(/held/);
  expect(await box.evaluate((el) => el.inert)).toBe(true);
  await expect(box.locator("textarea")).toHaveAttribute("placeholder", "Handing off to a new lead…");
  expect(await box.boundingBox()).toEqual(before);
  if (process.env.SHOT_DIR) {
    for (const [scheme, w, h] of [["light", 1280, 800], ["dark", 1280, 800], ["dark", 900, 600]]) {
      await page.emulateMedia({ colorScheme: scheme });
      await page.setViewportSize({ width: w, height: h });
      await page.screenshot({ path: `${process.env.SHOT_DIR}/handing-${scheme}-${w}.png` });
    }
    await page.emulateMedia({ colorScheme: "light" });
    await page.setViewportSize({ width: 1280, height: 800 });
  }
  // The new lead is ready: the line goes, the box opens where it was.
  await cue.change((st) => { const d = st.desk.projects.find((p) => p.lead?.sid === "lead"); d.lead.handoff = null; });
  await expect(page.locator(".dk-chat .dk-hwait")).toHaveCount(0);
  await expect(box).not.toHaveClass(/held/);
  expect(await box.evaluate((el) => el.inert)).toBe(false);
  expect(await box.boundingBox()).toEqual(before);
});

test("a hand-off that takes too long opens the box again and says so", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await cue.change((st) => { const d = st.desk.projects.find((p) => p.lead?.sid === "lead"); d.lead.handoff = { from: "lead", to: "l2", since_ms: Date.now() - 4 * 60e3 }; });
  await expect(page.locator(".dk-chat .dk-hwait")).toContainText("The hand-off is taking longer than 3 minutes.");
  await expect(page.locator(".dk2b-chat .foot .composer")).not.toHaveClass(/held/);
});

test("a lead's chat never offers Move to Cue or its terminal: before, during and after a hand-off", async ({ page, cue }) => {
  // #842: the lead, and the new lead taking over, look like a terminal app's Claude session to the chat (no hosting, no
  // tmux): still nothing about terminals, because each is the Desk's lead.
  await cue.open("desk");
  await cue.change((st) => {
    const asTerminal = (s) => Object.assign(s, { harness: "claude", hosted: false, tmux_pane: null, term_program: "iTerm.app", state: "waiting" });
    asTerminal(st.sessions.find((s) => s.session_id === "lead"));
    st.sessions.push(asTerminal({ ...st.sessions.find((s) => s.session_id === "lead"), session_id: "l2", name: "desk-lead-2", thread: [] }));
  });
  await openDesk(page);
  const box = page.locator(".dk2b-chat .foot");
  await expect(box.locator(".composer")).toBeVisible();
  const offers = page.locator(".tplace, [data-act=move-cue], [data-act=move-term], [data-act=go-session]");
  await expect(offers).toHaveCount(0);
  // Handing off: the old lead's note done, the new lead started but not yet the Desk's lead.
  await cue.change((st) => { const d = st.desk.projects.find((p) => p.lead?.sid === "lead"); d.lead.handoff = { from: "lead", to: "l2", since_ms: Date.now() - 4 * 60e3 }; });
  await expect(page.locator(".dk-chat .dk-hwait")).toBeVisible();
  await expect(offers).toHaveCount(0);
  expect(await page.evaluate(() => [placeTab("lead"), placeTab("l2")])).toEqual(["", ""]);
  // After: the new lead is the Desk's lead.
  await cue.change((st) => { const d = st.desk.projects.find((p) => p.lead?.sid === "lead"); d.lead.sid = "l2"; d.lead.handoff = null; });
  await expect(page.locator(".dk-chat .dk-hwait")).toHaveCount(0);
  await expect(offers).toHaveCount(0);
  expect(await page.evaluate(() => placeTab("l2"))).toBe("");
});
