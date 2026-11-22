// The Desk lead's lines that act on an item in its chat (#622): never raw "START: #612 | why: …". Each is a short plain
// line ("Started Bug #612"), its why under it once opened; the whole Desk's PAUSE/RESUME too.
import { test, expect, openDesk } from "./cue.mjs";

const push = (page, text) => page.evaluate((t) => { sessionOf("lead").thread.push({ at_ms: Date.now(), role: "agent", text: t }); renderMain(); }, text);

test("START, CLOSE, PARK, UNPARK and a whole-Desk PAUSE show as short lines, no raw protocol words", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await push(page, [
    "Moving things along.",
    "",
    "START: #d2 | why: the owner sent it back and is waiting; it's a Desk reliability bug",
    "- **CLOSE:** #d2 | commit: `abc1234def99` | why: already on main",
    "PARK: #d2 | why: the owner asked to wait",
    "UNPARK: #d2 — why: the owner asked for it again",
    "TAKE COPY: #d2 | its copy is stuck",
    "PAUSE: why: the build is broken",
  ].join("\n"));
  const chat = page.locator(".dk-chat");
  await expect(chat).toContainText("Moving things along.");
  for (const raw of ["START:", "CLOSE:", "PARK:", "UNPARK:", "TAKE COPY:", "PAUSE:", "**"]) await expect(chat).not.toContainText(raw);
  const lines = chat.locator(".dk-ln .mg-lh");
  await expect(lines).toHaveText([/^Started (Bug|Feature|Task) #d2$/, /^Closed (Bug|Feature|Task) #d2$/, /^Parked (Bug|Feature|Task) #d2$/,
    /^Unparked (Bug|Feature|Task) #d2$/, /^Took back the copy of (Bug|Feature|Task) #d2$/, "Paused this Desk"]);
  // A click on the line opens its why under it; its item's chip opens the item.
  await chat.locator(".dk-ln .mg-line").first().click({ position: { x: 4, y: 4 } });
  await expect(chat.locator(".dk-ln.open .mg-full")).toContainText("the owner sent it back and is waiting; it's a Desk reliability bug");
  await expect(chat.locator("[title]")).toHaveCount(0);
  await chat.locator('.dk-ln [data-desk-chk-item="d2"]').first().click();
  await expect(page.locator('.dk-right .dk-pg[data-desk-page="item"]')).toBeVisible();
});

test("a START Cue already acted on isn't drawn twice: its own line says it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const why = "the owner sent it back and is waiting";
  await cue.change((st, { why }) => {
    st.desk.projects.find((p) => p.project === "cue").decisions.push({ id: "s1", project: "cue", item_id: "d2", by: "web-lead", sid: "lead",
      text: "Started #d2 now", why, at: Math.floor(Date.now() / 1000), undo: "none", act: "start" });
  }, { why });
  await push(page, `Starting it.\n\nSTART: #d2 | why: ${why}`);
  const chat = page.locator(".dk-chat");
  await expect(chat).toContainText("Starting it.");
  await expect(chat).not.toContainText("START:");
  await expect(chat.locator(".dk-act .mg-lh", { hasText: "Started" })).toHaveText("Started #d2 now");
  await expect(chat.locator(".dk-ln:not(.dk-act) .mg-lh", { hasText: "Started" })).toHaveCount(0);
});

test("a START Cue wouldn't act on (no item number) stays as written", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await push(page, "START: the next one when ready");
  await expect(page.locator(".dk-chat .cv-agent").last()).toContainText("START: the next one when ready");
});
