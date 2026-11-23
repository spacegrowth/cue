// Pictures you attach to a message on an item's page (#541): they show as pictures in its chat, as a session's own
// chat shows them, never as their file paths in your words. While the message is on its way, the pictures show too.
import { test, expect, openDesk } from "./cue.mjs";

const PNG = [137, 80, 78, 71, 13, 10, 26, 10];
const openItem = async (page, id) => {
  await openDesk(page);
  await page.locator(`.dkn-items [data-desk-item="${id}"]`).first().click();
};
/** The item's chat: its holder's, in the session page that opened in the middle. */
const chatOf = (page, sid = "host") => page.locator(`.dk-right .dkn-sp[data-desk-page="session"] [data-desk-chat="${sid}"]`);
/** Two pictures dropped on the item page's box. */
const dropTwo = (page) => page.evaluate((png) => {
  const dt = new DataTransfer();
  for (const name of ["one.png", "two.png"]) dt.items.add(new File([new Uint8Array(png)], name, { type: "image/png" }));
  const el = document.querySelector(".dk-right .dk-box textarea");
  for (const type of ["dragover", "drop"]) el.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: dt }));
}, PNG);

test("two pictures sent from an item's page show as two pictures in its chat, with no file paths", async ({ page, cue }) => {
  await cue.open("desk");
  await openItem(page, "d5");
  const box = page.locator(".dk-right .dk-box");
  await dropTwo(page);
  await expect(box.locator(".chip")).toHaveCount(2);
  await box.locator("textarea").fill("is A this or more?");
  await page.locator(".dk-right [data-desk-send]").click();
  const call = await cue.called("desk_item_act", { id: "d5" });
  expect(call.args.images.map((i) => [i.name, i.mime])).toEqual([["one.png", "image/png"], ["two.png", "image/png"]]);
  // In its chat: your words, the two pictures under them, openable; no paths.
  const ichat = chatOf(page);
  const mine = ichat.locator(".cv-you", { hasText: "is A this or more?" });
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".thumb img")).toHaveCount(2);
  await expect(mine.locator("[data-lightbox]")).toHaveCount(2);
  await expect(ichat).not.toContainText("Pictures with it");
  await expect(ichat).not.toContainText(".png");
  await mine.locator("[data-lightbox]").nth(1).click();
  await expect(page.locator(".lightbox .lb-count")).toHaveText("2 / 2 · ← →");
});

// What you send from an opened item's box shows in its session's chat at once (desk/c.js deskCItemPending, #748).
test("on their way, the two pictures show with your words, not a count", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.override("desk_item_act", () => new Promise(() => {}));
  await openItem(page, "d5");
  await dropTwo(page);
  await page.locator(".dk-right .dk-box textarea").fill("and this one");
  await page.locator(".dk-right [data-desk-send]").click();
  const mine = chatOf(page).locator(".cv-you", { hasText: "and this one" });
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".thumb img")).toHaveCount(2);
  await expect(mine).not.toContainText("2 pictures");
});

test("to a session at its prompt, the two pictures are in its chat as it took them", async ({ page, cue }) => {
  await cue.open("desk");
  await cue.change((st) => { st.sessions.find((x) => x.session_id === "host").state = "idle"; });
  await openItem(page, "d5");
  await dropTwo(page);
  await page.locator(".dk-right .dk-box textarea").fill("this is what I see");
  await page.locator(".dk-right [data-desk-send]").click();
  const ichat = chatOf(page), mine = ichat.locator(".cv-you", { hasText: "this is what I see" });
  await expect(mine).toHaveCount(1);
  await expect(mine.locator(".thumb img")).toHaveCount(2);
  await expect(ichat).not.toContainText("Pictures with it");
});
