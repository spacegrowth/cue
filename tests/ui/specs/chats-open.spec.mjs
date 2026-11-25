// What you're reading goes to Cue (box/end.js chatsOpenTell, #799): the chats on screen in its window, each with whether
// it's scrolled to its newest, so a reply you've seen isn't notified and opening a chat takes back what was.
import { test, expect, openDesk } from "./cue.mjs";

const lastChats = (page) => page.evaluate(() => CueBridge.called("chats_open").at(-1)?.args.chats || []);

test("an item's chat open on the Desk is told as its holder's, at its newest", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  await page.locator('.dkn-items [data-desk-item="d5"]').first().click();
  await page.locator('.dk-right [data-desk-chat="host"]').waitFor();
  await expect.poll(() => lastChats(page)).toContainEqual(["host", true]);
});
