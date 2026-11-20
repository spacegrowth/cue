// An item's page shows its holder's own chat (#418, option B of the item chat mocks): the chat the holder's session
// shows, its turns about other items folded to a line each that opens in place; your send-backs of this item and its
// Working line in it; the item itself (state, You'll see, What to try, its record, Open and Drop) in its panel beside
// the chat; the Items list stays at the right.
import { test, expect } from "./cue.mjs";

const openItem = async (page, id) => {
  await page.locator('[data-pj="toggle"]').click();
  await page.locator('.pj-pop [data-pj="pick"][data-project="cue"]').click();
  await page.locator('.switch-view [data-view="desk"]').click();
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
  // The item in its panel beside the chat, in the middle; the Items list stays at the right, the item's row lit.
  const side = right.locator('.dk-pg[data-desk-page="item"] > .dk-panel[data-side-for="d5"]');
  await expect(side).toBeVisible();
  await expect(page.locator(".dk-board > .dk-items")).toBeVisible();
  // Narrow, the whole panel over the chat; wide enough for both, beside it.
  let [chatBox, sideBox] = [await ichat.boundingBox(), await side.boundingBox()];
  expect(sideBox.y + sideBox.height).toBeLessThanOrEqual(chatBox.y + 1);
  await page.setViewportSize({ width: 1728, height: 1000 });
  await expect.poll(async () => (await side.boundingBox()).x).toBeGreaterThan((await ichat.boundingBox()).x + 200);
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
