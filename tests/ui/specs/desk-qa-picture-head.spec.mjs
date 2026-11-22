// A picture pasted into the Desk's "Add an item…" line (#728): it shows as a small preview under the line, and nothing
// else in the Items head moves: "Items", the List / Cards switch, Away and Pause stay where they were, in both views,
// and the switch still flips the view with the picture there.
import { test, expect, openDesk } from "./cue.mjs";

const head = (page) => page.locator(".dk-card.dk-items .dk2b-ih");
/** Where the head's parts are: the label, each switch button, the add line, Away and Pause. */
const spots = (page) => head(page).evaluate((h) => {
  const at = (sel) => { const el = h.querySelector(sel); if (!el) return null; const r = el.getBoundingClientRect(); return [Math.round(r.x), Math.round(r.y), Math.round(r.height)]; };
  return { items: at(":scope > b"), list: at('[data-desk-cards-view="list"]'), cards: at('[data-desk-cards-view="cards"]'), line: at('[data-qa="line"]'), away: at(".dk-aw-sw"), pause: at(".dk-pz") };
});
/** A picture pasted into the add line, as ⌘V with a screenshot does. */
const pastePicture = (page) => page.evaluate(() => {
  const dt = new DataTransfer();
  dt.items.add(new File([new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10])], "shot.png", { type: "image/png" }));
  document.querySelector('.dk-qa [data-qa="line"]').dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
});

for (const view of ["list", "cards"]) {
  test(`a pasted picture shows under the add line and moves nothing in the head (${view})`, async ({ page, cue }) => {
    await cue.open("desk");
    await openDesk(page);
    await head(page).locator(`[data-desk-cards-view="${view}"]`).click();
    await expect(head(page).locator(`[data-desk-cards-view="${view}"]`)).toHaveAttribute("aria-pressed", "true");
    const before = await spots(page);
    for (const k of ["items", "list", "cards", "line", "away", "pause"]) expect(before[k], `${k} is in the head`).not.toBeNull();
    await pastePicture(page);
    const chip = head(page).locator(".dk-qa .chip");
    await expect(chip).toHaveCount(1);
    expect(await spots(page)).toEqual(before);
    // The preview sits under the line, starting where the line starts.
    const c = await chip.boundingBox(), l = before.line;
    expect(c.y).toBeGreaterThanOrEqual(l[1] + l[2]);
    expect(Math.abs(c.x - l[0])).toBeLessThan(2);
    // The switch still flips the view, the picture kept.
    const other = view === "list" ? "cards" : "list";
    await head(page).locator(`[data-desk-cards-view="${other}"]`).click();
    await expect(head(page).locator(`[data-desk-cards-view="${other}"]`)).toHaveAttribute("aria-pressed", "true");
    await expect(head(page).locator(".dk-qa .chip")).toHaveCount(1);
  });
}
