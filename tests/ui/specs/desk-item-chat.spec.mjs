// An item's page shows its holder's own chat (#418, option B of the item chat mocks): the chat the holder's session
// shows, its turns about other items folded to a line each that opens in place; your send-backs of this item and its
// Working line in it; the item itself (state, You'll see, What to try, its record, Open and Drop) in its panel beside
// the chat; the Items list stays at the right.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`[data-desk-item="${id}"]`).first().click();
};
// host-lead's chat: a turn about #d4 (two replies), then the hand-off of #d5 and its reply.
const chat = (cue) => cue.change((st) => {
  const now = Date.now(), s = st.sessions.find((x) => x.session_id === "host");
  s.thread = [
    { role: "you", text: "About #d4 (Rail dot blinks): the dot blinks for a reply I didn't send", at_ms: now - 900000 },
    { role: "agent", text: "Looking at the rail dot.", at_ms: now - 880000 },
    { role: "agent", text: "Fixed the rail dot: it blinks only for a session you wrote to.", at_ms: now - 860000 },
    { role: "you", text: "#d5 Hosted sessions can't open the canvas.", at_ms: now - 600000 },
    { role: "agent", text: "Chose an environment setting over a settings file. Testing the canvas now.", at_ms: now - 300000 },
  ];
});

test("an item's page is its holder's chat, other items' turns folded, the item beside it, the Items list at the right", async ({ page, cue }) => {
  await cue.open("desk");
  await chat(cue);
  await openItem(page, "d5");
  const right = page.locator(".dk-right"), ichat = right.locator(".dk-ichat");
  await expect(right.locator(".dk-ph3")).toContainText("host-lead's chat · other items' turns folded");
  await expect(right.locator(".dk-itab")).toHaveText(["Chat", "Record0", "Decisions0", "Files0"].map((t) => new RegExp(t.replace(/\d/, ""))));
  // This item's turn as the chat shows it; #d4's folded to one line.
  await expect(ichat).toContainText("Chose an environment setting over a settings file.");
  await expect(ichat).not.toContainText("Looking at the rail dot.");
  const fold = ichat.locator(".dk-fold");
  await expect(fold).toHaveCount(1);
  await expect(fold).toContainText("Worked on #d4");
  await expect(fold).toContainText("2 replies");
  // Opened in place, then folded again.
  await fold.click();
  await expect(ichat).toContainText("Looking at the rail dot.");
  await expect(ichat.locator(".dk-fold")).toHaveAttribute("aria-expanded", "true");
  await ichat.locator(".dk-fold").click();
  await expect(ichat).not.toContainText("Looking at the rail dot.");
  // The item in its panel beside the chat; the Items list behind ‹ Items N, the item in its place (#666).
  const side = right.locator('.dk-pg[data-desk-page="item"] > .dk-panel[data-side-for="d5"]');
  await expect(side).toBeVisible();
  await expect(page.locator(".dk-board > .dk-items")).toHaveCount(0);
  await expect(right.locator(".dk-ph .dk-back")).toHaveText(/^‹ Items \d+$/);
  // Narrow, the whole panel over the chat; wide enough for both, beside it.
  // (Polled: a redraw can catch either one between boxes.)
  await expect(async () => { const [c, p] = [await ichat.boundingBox(), await side.boundingBox()]; expect(p.y + p.height).toBeLessThanOrEqual(c.y + 1); }).toPass();
  await page.setViewportSize({ width: 1728, height: 1000 });
  await expect(async () => { const [c, p] = [await ichat.boundingBox(), await side.boundingBox()]; expect(p.x).toBeGreaterThan(c.x + 200); }).toPass();
  await page.screenshot({ path: test.info().outputPath("item-chat-wide.png") });
  await page.setViewportSize({ width: 1280, height: 800 });
  await expect(side.locator(".dk-pnow .dk-psh")).toHaveText("This item");
  await expect(side).toContainText("What to try");
  await expect(side.locator(".dk-sfoot")).toContainText("Open host-lead");
  await expect(side.locator(".dk-sfoot")).toContainText("Drop this item");
  // No hover text on any of it.
  expect(await page.locator(".dk-right .dk-pg [title], .dk-panel[data-side-for] [title]").count()).toBe(0);
  await page.screenshot({ path: test.info().outputPath("item-chat.png") });
  // Back to the lead's chat; the Items list still there.
  await right.locator(".dk-back").click();
  await expect(page.locator(".dk-panel[data-side-for]")).toHaveCount(0);
  await expect(page.locator(".dk-board > .dk-items")).toBeVisible();
});

test("its working line and your send-back of it show in the chat", async ({ page, cue }) => {
  await cue.open("desk");
  await chat(cue);
  await cue.change((st) => { st.desk.projects.find((x) => x.project === "cue").checks = [{ id: "d5", kind: "bug", title: "Hosted sessions can't open the canvas", state: "working", at: Date.now() - 120000, back: "The canvas still opens blank", last: false }]; });
  await openItem(page, "d5");
  const ichat = page.locator(".dk-right .dk-ichat");
  await expect(ichat.locator(".dk-chk-done")).toContainText("sent");
  await expect(ichat.locator('[data-key="where"]')).toHaveCount(1);
});

test("what you send on its page shows in the chat at once", async ({ page, cue }) => {
  await cue.open("desk");
  await chat(cue);
  await cue.override("send_to_session", () => new Promise(() => {}));
  await openItem(page, "d5");
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(page.locator(".dk-right .dk-ichat").getByText("Check the narrow window too")).toHaveCount(1);
});

test("the pictures an item was added with show in its column, its chat being its holder's", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5").images = ["data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=", "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="]; });
  await openItem(page, "d5");
  const side = page.locator('.dk-panel[data-side-for="d5"]');
  await expect(side.locator(".dk-psh", { hasText: "Pictures" })).toBeVisible();
  await expect(side.locator(".dk-tpics .thumb")).toHaveCount(2);
});

test("a reply that asks you shows once, in the chat: no second box under it repeating it", async ({ page, cue }) => {
  await cue.open("desk");
  await chat(cue);
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "host");
    s.thread.push({ role: "agent", text: "Should the canvas open in a new window or in place?", at_ms: Date.now() - 1000 });
    Object.assign(st.desk.projects.find((d) => d.project === "cue").items.find((i) => i.id === "d5"), { state: "needs_you" });
  });
  await openItem(page, "d5");
  const right = page.locator(".dk-right");
  await expect(right.locator(".dk-ichat .cv-agent", { hasText: "Should the canvas open" })).toHaveCount(1);
  await expect(right.getByText("Should the canvas open in a new window or in place?")).toHaveCount(1);
  await expect(right.locator(".dk-chain")).toHaveCount(0);
});

// One chat per item (#575, step 1 of option A in docs/item-chat-plan.md): the item's page draws its session's chat only
// for the times the session held it (its stints), so what you type there or in the session on the left shows in both.
const held = (cue, stints) => cue.change((st, stints) => {
  const i = st.desk.projects.find((x) => x.project === "cue").items.find((x) => x.id === "d5");
  const now = Date.now();
  i.stints = stints.map((t) => ({ sid: "host", from: now - t.from, until: t.until == null ? null : now - t.until, next: t.next || "" }));
}, stints);

test("its page is its session's chat from when it took the item: what came before isn't drawn, not even folded", async ({ page, cue }) => {
  await cue.open("desk");
  await chat(cue);
  await held(cue, [{ from: 610000 }]);
  await openItem(page, "d5");
  const right = page.locator(".dk-right"), ichat = right.locator(".dk-ichat");
  await expect(right.locator(".dk-ph3")).toContainText("host-lead's chat · from when it took this item");
  await expect(ichat).toContainText("Chose an environment setting over a settings file.");
  await expect(ichat).not.toContainText("Looking at the rail dot.");
  await expect(ichat.locator(".dk-fold")).toHaveCount(0);
  // Something you type in the session on the left, without "#d5", is the item's too: it's in its stint.
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").thread.push({ role: "you", text: "Also check the narrow window", at_ms: Date.now() - 1000 }); });
  await expect(ichat.getByText("Also check the narrow window")).toHaveCount(1);
  expect(await page.locator(".dk-right .dk-pg [title]").count()).toBe(0);
});

test("once its session moved on, its page keeps that piece of the chat and says where it went", async ({ page, cue }) => {
  await cue.open("desk");
  await chat(cue);
  await cue.change((st) => {
    const s = st.sessions.find((x) => x.session_id === "host"), now = Date.now();
    s.thread.push({ role: "you", text: "#d4 Rail dot blinks for a reply you didn't send.", at_ms: now - 200000 }, { role: "agent", text: "On the rail dot now.", at_ms: now - 100000 });
  });
  await held(cue, [{ from: 610000, until: 199000, next: "d4" }]);
  await openItem(page, "d5");
  const right = page.locator(".dk-right"), ichat = right.locator(".dk-ichat");
  await expect(right.locator(".dk-ph3")).toContainText("host-lead's chat · while it had this item");
  await expect(ichat).toContainText("Chose an environment setting over a settings file.");
  await expect(ichat).not.toContainText("On the rail dot now.");
  const moved = ichat.locator(".dk-moved");
  await expect(moved).toHaveCount(1);
  await expect(moved).toContainText("host-lead moved on to #d4 Rail dot blinks");
  // Its session's own line now is its next item's: not here.
  await expect(ichat.locator('[data-key="where"]')).toHaveCount(0);
  await page.screenshot({ path: test.info().outputPath("item-chat-moved-on.png") });
  await moved.locator("[data-desk-item]").click();
  await expect(page.locator('.dk-panel[data-side-for="d4"]')).toBeVisible();
});

test("a stint that began before the loaded messages loads earlier ones until it's in", async ({ page, cue }) => {
  await cue.open("desk");
  await chat(cue);
  // The fake log's earlier messages start three hours back, a minute apart: the stint starts in them.
  await held(cue, [{ from: 3 * 3600000 - 30 * 60000 }]);
  await openItem(page, "d5");
  const ichat = page.locator(".dk-right .dk-ichat");
  await expect(ichat.getByText("Earlier message 31", { exact: true })).toHaveCount(1);
  await expect(ichat.getByText("Earlier message 29", { exact: true })).toHaveCount(0);
});
