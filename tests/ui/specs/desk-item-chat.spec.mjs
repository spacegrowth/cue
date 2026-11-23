// An item opens its holder's own chat (#418, #744): the chat the holder's session shows, the item's bar over it (desk/c.js
// deskCItemChat). What you send from its box shows in that chat; a reply that asks you shows once, in it.
import { test, expect, openDesk } from "./cue.mjs";

const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`.dkn-items [data-desk-item="${id}"]`).first().click();
};
/** The item's chat: its holder's, in the session page that opened in the middle. */
const chatOf = (page, sid = "host") => page.locator(`.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="${sid}"]`);
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

test("what you send on its page shows in the chat at once", async ({ page, cue }) => {
  await cue.open("desk");
  await chat(cue);
  await cue.override("send_to_session", () => new Promise(() => {}));
  await openItem(page, "d5");
  await page.locator(".dk-right .dk-box textarea").fill("Check the narrow window too");
  await page.locator(".dk-right [data-desk-send]").click();
  await expect(chatOf(page).getByText("Check the narrow window too")).toHaveCount(1);
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
  await expect(chatOf(page).locator(".cv-agent", { hasText: "Should the canvas open" })).toHaveCount(1);
  // Not again anywhere else on the page: not in its bar, its ask or its box.
  await expect(right.getByText("Should the canvas open in a new window or in place?")).toHaveCount(1);
  await expect(right.locator(".dk-chain")).toHaveCount(0);
});
