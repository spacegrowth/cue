// What the window looks like to the pointer and the keyboard, asked by Cue (`page-look`, hub/page_look.rs) for when
// the cursor acts up while it runs (#456): the box with the keyboard, what's under the pointer, anything covering
// the window. Only read: asking changes nothing on the page.
import { test, expect, openDesk } from "./cue.mjs";

test("asked how the page looks, the window says which box has the keyboard, what's under the pointer and what covers it", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  const box = page.locator(".dk-right .dk-box textarea[data-text]").first();
  await box.click();
  const b = await box.boundingBox();
  await page.mouse.move(b.x + 10, b.y + 10);
  // Animation delays (live dot, lead bar) follow the clock on their own; everything else must stay as it was.
  const app = () => page.locator("#app").evaluate((e) => e.innerHTML.replace(/animation-delay:\s*-?\d+ms;?/g, ""));
  const before = await app();
  await cue.emit("page-look", { id: 7 });
  const said = await cue.called("page_look_said", { id: 7 });
  const look = said.args.look;
  expect(look.active.el).toMatch(/^textarea/);
  expect(look.active.editable).toBe(true);
  expect(look.under.el).toMatch(/^textarea/);
  expect(look.under.cursor).toBe("text");
  expect(look.pointer.x).toBe(Math.round(b.x + 10));
  expect(look.covers).toEqual([]);
  // A layer over the whole window that takes the pointer is named.
  await page.evaluate(() => document.body.append(Object.assign(document.createElement("div"), { className: "stuck-layer", style: "position:fixed;inset:0;z-index:99" })));
  await cue.emit("page-look", { id: 8 });
  const over = (await cue.called("page_look_said", { id: 8 })).args.look;
  expect(over.under.el).toBe("div.stuck-layer");
  expect(over.covers.map((c) => c.el)).toContain("div.stuck-layer");
  await page.evaluate(() => document.querySelector(".stuck-layer").remove());
  // Nothing changed by asking.
  await expect(box).toBeFocused();
  expect(await app()).toBe(before);
});
