// Fewer sessions on show (#363, #485): the Desk's session list folds cold idle sessions holding no item under Older,
// shut by default, that opens in place; Give to… lists the sessions with work (the hub's `with_work`), the rest on one
// "N older" line.
import { test, expect, openDesk } from "./cue.mjs";
import { openSessions } from "./desk.mjs";


/** Two idle sessions on the Desk, "busy" with work (an item) and "spare" with none. */
const twoIdle = async (cue) => {
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site");
    for (const sid of ["busy", "spare"]) st.sessions.push({ ...site, session_id: sid, project: "cue", name: sid, state: "idle", thread: [] });
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.members.push("busy", "spare");
    d.with_work = [d.lead.sid, "host", "busy"];
  });
};

/** Idle sessions on the Desk (#485): "warm" (its cache alive), "cold" (cache gone, no item), "held" (cache gone, but
 *  it holds an open item), "stale" (cold and idle for over a day). */
const fourIdle = async (page, cue) => {
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site"), t = Date.now();
    for (const [sid, h] of [["warm", 1], ["cold", 3], ["held", 4], ["stale", 30]]) st.sessions.push({ ...site, session_id: sid, project: "cue", name: sid, state: "idle", since_ms: t - h * 3600000, thread: [] });
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.members.push("warm", "cold", "held", "stale");
    d.items.push({ id: "d485", project: "cue", kind: "bug", title: "Held by held", state: "queued", holder: { kind: "session", sid: "held", name: "held" }, updated: Math.floor(t / 1000) });
  });
  await page.evaluate(() => {
    const t = now();
    insCache.set("warm", { tokens: 90000, until_ms: t + 20 * 60000, last_ms: t - 3600000, said: "Warm reply." });
    for (const sid of ["cold", "held", "stale"]) insCache.set(sid, { tokens: 50000, until_ms: t - 60000, last_ms: t - 3 * 3600000, said: `${sid} reply.` });
    renderMain();
  });
};
/** The session list top to bottom: "# <header>" (its button in [ ]) or a row's session id. */
const order = (side) => side.locator(".dk-scroll > *").evaluateAll((els) => els.map((el) => {
  if (!el.matches(".dk-gh")) return el.dataset.deskChatOf || el.dataset.deskCrew;
  const btn = el.querySelector("button"), own = [...el.childNodes].filter((n) => n !== btn).map((n) => n.textContent).join("");
  return `# ${own.trim().replace(/\s+/g, " ")}${btn ? ` [${btn.textContent}]` : ""}`;
}));

test("cold idle sessions holding no item fold under Older, shut till you open it; warm ones and ones holding an item stay in Idle (#485)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await fourIdle(page, cue);
  const side = page.locator(".dk-sessions"), olderH = side.locator('[data-desk-older="side:cue"]');
  await expect(olderH).toHaveAttribute("aria-expanded", "false");
  await expect.poll(() => order(side)).toEqual(["# Working 1", "host", "# Idle 2", "warm", "held", "# Older 2 [Close 1 idle over a day]"]);
  // The strip still counts every session on the Desk.
  await expect(side.locator(".dk-strip .dk-sub")).toHaveText(/^5 sessions/);
  await expect(side.locator(".dk-scroll [title]")).toHaveCount(0);
  // A click on its name opens it in place (its Close button is beside it); the keyboard does too.
  await olderH.click({ position: { x: 24, y: 6 } });
  await expect(olderH).toHaveAttribute("aria-expanded", "true");
  await expect.poll(() => order(side)).toEqual(["# Working 1", "host", "# Idle 2", "warm", "held", "# Older 2 [Close 1 idle over a day]", "cold", "stale"]);
  await olderH.focus();
  await page.keyboard.press("Enter");
  await expect(olderH).toHaveAttribute("aria-expanded", "false");
  await expect(side.locator('[data-desk-chat-of="cold"]')).toHaveCount(0);
  if (process.env.SHOT_DIR) {
    await olderH.click({ position: { x: 24, y: 6 } });
    for (const theme of ["light", "dark"]) {
      await page.evaluate((t) => { document.documentElement.dataset.theme = t; }, theme);
      await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-older-${theme}.png`, clip: await side.boundingBox() });
    }
  }
});

test("an item given to a session in Older brings it back to Idle; with the item gone and its cache cold it folds again (#485)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await fourIdle(page, cue);
  const side = page.locator(".dk-sessions");
  await expect(side.locator('[data-desk-chat-of="cold"]')).toHaveCount(0);
  await cue.change((st) => {
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.items.push({ id: "d486", project: "cue", kind: "task", title: "For cold", state: "queued", holder: { kind: "session", sid: "cold", name: "cold" }, updated: Math.floor(Date.now() / 1000) });
  });
  await expect.poll(() => order(side)).toEqual(["# Working 1", "host", "# Idle 3", "warm", "cold", "held", "# Older 1 [Close 1 idle over a day]"]);
  await expect(side.locator('[data-desk-chat-of="cold"] .dk-s2')).toContainText("For cold");
  await cue.change((st) => { const d = st.desk.projects.find((x) => x.project === "cue"); d.items = d.items.filter((i) => i.id !== "d486"); });
  await expect.poll(() => order(side)).toEqual(["# Working 1", "host", "# Idle 2", "warm", "held", "# Older 2 [Close 1 idle over a day]"]);
});

test("a session in Older for over a day: Close closes it, its chat kept; none offered before a day (#485)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await fourIdle(page, cue);
  const close = page.locator(".dk-sessions [data-desk-close-idle]");
  await expect(close).toHaveText("Close 1 idle over a day");
  await expect(close).toHaveAttribute("data-desk-close-idle", "stale");
  await close.click();
  expect(await cue.calls("close_session")).toMatchObject([{ args: { sessionId: "stale" } }]);
  expect(await cue.calls("delete_session")).toEqual([]);
  await expect(page.locator(".toast").last()).toContainText("Closed 1 session · their chats are kept");
  await expect(page.locator('.dk-sessions [data-desk-chat-of="stale"]')).toHaveCount(0);
  await expect(close).toHaveCount(0);
  await expect(page.locator('.dk-sessions [data-desk-older="side:cue"] .n')).toHaveText("1");
});

test("a session row keeps its shape and place whether or not its size and cache are known (#485)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);  // the sessions open on the right, from All N sessions (#666)
  await cue.change((st) => {
    st.sessions.push({ ...st.sessions.find((x) => x.session_id === "site"), session_id: "spare", project: "cue", name: "spare", state: "idle", since_ms: Date.now() - 600000, thread: [] });
    st.desk.projects.find((x) => x.project === "cue").members.push("spare");
  });
  const side = page.locator(".dk-sessions"), row = side.locator('[data-desk-chat-of="spare"]');
  const shape = () => row.evaluate((el) => ({ h: el.getBoundingClientRect().height, lines: [...el.children].map((c) => c.className) }));
  // Its log read and nothing in it: said in words, never a blank line (#592).
  await expect(row.locator(".dk-size")).toHaveText("no reply yet · no cache");
  const before = await shape(), at = await order(side);
  expect(before.lines).toEqual(["dk-s1", "dk-s2", "dk-s3 dk-size"]);
  // The numbers come in: the same lines, the same height, the same place.
  await page.evaluate(() => { insCache.set("spare", { tokens: 51000, until_ms: now() + 30 * 60000, last_ms: now() - 600000, said: "Spare's reply." }); renderMain(); });
  await expect(row.locator(".dk-size")).toContainText("51k");
  expect(await shape()).toEqual(before);
  expect(await order(side)).toEqual(at);
  // And gone again (a card that brought them closed): still the same.
  await page.evaluate(() => { insCache.delete("spare"); renderMain(); });
  await expect(row.locator(".dk-size")).toHaveText("no reply yet · no cache");
  expect(await shape()).toEqual(before);
});

test("Give to… lists the sessions with work, the rest under one older line", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await twoIdle(cue);
  // B2 (#666): no Give to… on the row; it's on the item's page.
  await expect(page.locator('.dk-items [data-desk-item="d6"] [data-desk-give]')).toHaveCount(0);
  await page.locator('.dk-items [data-desk-item="d6"]').click();
  await page.locator('[data-desk-give="d6"]').click();
  const menu = page.locator(".dk-givemenu");
  await expect(menu.locator('[data-desk-give-to="busy"]')).toHaveCount(1);
  await expect(menu.locator('[data-desk-give-to="spare"]')).toHaveCount(0);
  const older = menu.locator('[data-desk-older="give:d6"]');
  await expect(older).toHaveText(/older · show/);
  await older.click();
  await expect(menu.locator('[data-desk-give-to="spare"]')).toHaveCount(1);
});

test("sessions no process runs any more fold under Closed at the bottom, muted and saying closed; one that wakes moves back out (#834)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await openSessions(page);
  await cue.change((st) => {
    const site = st.sessions.find((x) => x.session_id === "site"), t = Date.now();
    // "gone" ended (Cue's record has no process for it); "kept" ended too but holds an open item; "awake" still runs.
    for (const [sid, pid, m] of [["gone", null, 5], ["kept", null, 6], ["awake", 9000003, 7]]) st.sessions.push({ ...site, session_id: sid, project: "cue", name: sid, state: "idle", agent_pid: pid, since_ms: t - m * 60000, thread: [] });
    const d = st.desk.projects.find((x) => x.project === "cue");
    d.members.push("gone", "kept", "awake");
    d.items.push({ id: "d834", project: "cue", kind: "bug", title: "Held by kept", state: "queued", holder: { kind: "session", sid: "kept", name: "kept" }, updated: Math.floor(t / 1000) });
  });
  const side = page.locator(".dk-sessions"), closedH = side.locator('[data-desk-older="closed:cue"]');
  await expect(closedH).toHaveAttribute("aria-expanded", "false");
  await expect.poll(async () => (await order(side)).slice(-1)).toEqual(["# Closed 1"]);
  const rows = await order(side);
  expect(rows.slice(-1)).toEqual(["# Closed 1"]);
  expect(rows).toContain("kept");
  expect(rows).toContain("awake");
  expect(rows).not.toContain("gone");
  await closedH.click();
  await expect(closedH).toHaveAttribute("aria-expanded", "true");
  const gone = side.locator('[data-desk-chat-of="gone"]');
  await expect(gone).toHaveClass(/past/);
  await expect(gone.locator(".dk-st")).toHaveText("closed");
  expect((await order(side)).slice(-2)).toEqual(["# Closed 1", "gone"]);
  await expect(side.locator(".dk-scroll [title]")).toHaveCount(0);
  // Anywhere on the heading folds it shut again, and opens it once more.
  await closedH.click({ position: { x: 200, y: 6 } });
  await expect(closedH).toHaveAttribute("aria-expanded", "false");
  await expect(gone).toHaveCount(0);
  await closedH.click({ position: { x: 200, y: 6 } });
  await expect(closedH).toHaveAttribute("aria-expanded", "true");
  if (process.env.SHOT_DIR) {
    for (const theme of ["light", "dark"]) {
      await page.evaluate((th) => { document.documentElement.dataset.theme = th; }, theme);
      await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-closed-${theme}.png`, clip: await side.boundingBox() });
    }
    await page.setViewportSize({ width: 900, height: 600 });
    await page.screenshot({ path: `${process.env.SHOT_DIR}/desk-closed-small.png` });
  }
  // It wakes (a message started it again): out of Closed on the next update, no longer muted.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "gone").agent_pid = 9000004; });
  await expect(closedH).toHaveCount(0);
  await expect(side.locator('[data-desk-chat-of="gone"]')).not.toHaveClass(/past/);
  await expect(side.locator('[data-desk-chat-of="gone"] .dk-st')).toHaveText("idle");
});
