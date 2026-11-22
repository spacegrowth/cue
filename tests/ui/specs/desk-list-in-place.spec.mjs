// The Desk's Items list and Progress kept across a draw (#480): as items are added, dropped or change group, the list
// updates in place. The rows that didn't change are the same elements, untouched inside; a row that changed group is
// the same element in its new group; the list keeps its scroll and an open feature row stays open.
import { test, expect, openDesk } from "./cue.mjs";
import { groupOf } from "./desk.mjs";

// The list is drawn again as the Desk's finished items come in: wait till it has been still for a moment.
const settle = (page, sel = ".dk-items") => page.evaluate((sel) => new Promise((res) => {
  let t;
  const done = () => { mo.disconnect(); res(); };
  const mo = new MutationObserver(() => { clearTimeout(t); t = setTimeout(done, 300); });
  mo.observe(document.querySelector(sel), { childList: true, subtree: true, attributes: true, characterData: true });
  t = setTimeout(done, 300);
}), sel);
/** Each row (`rowSel`) marked with its key, and a watch on what changes inside each from now on. */
const mark = (page, rowSel, key) => page.evaluate(([rowSel, key]) => {
  window.__rows = new Map();
  window.__touched = new Set();
  for (const r of document.querySelectorAll(rowSel)) { r.__mark = r.dataset[key]; window.__rows.set(r.dataset[key], r); }
  window.__mo?.disconnect();
  window.__mo = new MutationObserver((ms) => { for (const m of ms) { const r = (m.target.nodeType === 1 ? m.target : m.target.parentElement)?.closest(rowSel); if (r?.__mark) window.__touched.add(r.__mark); } });
  window.__mo.observe(document.body, { childList: true, subtree: true, attributes: true, characterData: true });
}, [rowSel, key]);
/** For each id: whether its row now is the very element marked before, and whether anything inside it changed. */
const kept = (page, rowSel, key, ids) => page.evaluate(([rowSel, key, ids]) => Object.fromEntries(ids.map((id) => {
  const now = [...document.querySelectorAll(rowSel)].find((r) => r.dataset[key] === id);
  return [id, { same: !!now && now === window.__rows.get(id), touched: window.__touched.has(id) }];
})), [rowSel, key, ids]);

const changeItems = (cue) => cue.change((st) => {
  const d = st.desk.projects.find((p) => p.project === "cue");
  const q = d.items.find((i) => i.id === "d4");
  // Added: a new one queued. Dropped: d6. Moved: d4 from Queued to In progress.
  d.items.push({ ...q, id: "d20", title: "A new queued item", state: "queued", created: q.created + 60, updated: q.updated + 60 });
  d.items = d.items.filter((i) => i.id !== "d6");
  q.state = "working";
});

for (const engine of ["with moveBefore", "without moveBefore"]) {
  test(`the Items list updates in place as items are added, dropped and move group (#480, ${engine})`, async ({ page, cue }) => {
    if (engine === "without moveBefore") await page.addInitScript(() => { delete Element.prototype.moveBefore; });
    await cue.open("desk");
    await openDesk(page);
    await settle(page);
    expect(await groupOf(page, "d4")).toBe("UP NEXT");
    const box = page.locator(".dk-items > .dk-scroll");
    const boxEl = await box.elementHandle();
    // A short window, so the list scrolls (B2's list shows 3 rows a group: it's short). Not by a style set on the list
    // here: its attributes are brought up to the fresh draw's, so that would go.
    await page.setViewportSize({ width: 1280, height: 400 });
    await expect.poll(() => box.evaluate((el) => el.scrollHeight - el.clientHeight)).toBeGreaterThan(40);
    await box.evaluate((el) => { el.scrollTop = 40; });
    const top = await box.evaluate((el) => el.scrollTop);
    expect(top).toBeGreaterThan(0);
    await mark(page, ".dk-items .dk-it[data-desk-item]", "deskItem");
    const heads = await page.evaluate(() => { window.__heads = [...document.querySelectorAll(".dk-items [data-desk2b-group]")]; return window.__heads.length; });
    expect(heads).toBeGreaterThan(1);

    await changeItems(cue);
    await expect(page.locator('.dk-items .dk-it[data-desk-item="d20"]')).toHaveCount(1);
    await settle(page);

    // Dropped is gone; the new one is Up next; d4 moved to Working as the same element.
    await expect(page.locator('.dk-items .dk-it[data-desk-item="d6"]')).toHaveCount(0);
    expect(await groupOf(page, "d20")).toBe("UP NEXT");
    expect(await groupOf(page, "d4")).toBe("WORKING");
    const k = await kept(page, ".dk-items .dk-it[data-desk-item]", "deskItem", ["d1", "d2", "d3", "d5", "d4"]);
    // The rows that didn't change: the same elements, nothing inside them touched.
    for (const id of ["d1", "d2", "d3", "d5"]) expect(k[id], `row ${id}`).toEqual({ same: true, touched: false });
    expect(k.d4.same, "the moved row is the same element").toBe(true);
    // The list itself and its group heads are the same elements; its scroll stayed.
    expect(await box.evaluate((el, was) => el === was, boxEl)).toBe(true);
    expect(await page.evaluate(() => window.__heads.filter((h) => h.isConnected).length)).toBeGreaterThan(1);
    expect(await box.evaluate((el) => el.scrollTop)).toBe(top);
  });
}

test("Progress updates in place as items change (#480)", async ({ page, cue }) => {
  await cue.open("desk");
  await openDesk(page);
  // B2 has no Progress button in the list's head (#666); it still opens by its toggle.
  await page.evaluate(() => deskProgToggle(true));
  await page.locator(".dk-prog .dk-gr[data-desk-prog-row]").first().waitFor();
  await settle(page, ".dk-prog");
  await mark(page, ".dk-prog .dk-gr[data-desk-prog-row]", "deskProgRow");
  const before = await page.evaluate(() => [...window.__rows.keys()]);
  expect(before).toEqual(expect.arrayContaining(["d1", "d3", "d5"]));
  // d4 starts work: it joins Progress; the others are left as they were.
  await changeItems(cue);
  await expect(page.locator('.dk-prog [data-desk-prog-row="d4"]')).toHaveCount(1);
  await settle(page, ".dk-prog");
  const k = await kept(page, ".dk-prog .dk-gr[data-desk-prog-row]", "deskProgRow", ["d1", "d3", "d5"]);
  for (const id of ["d1", "d3", "d5"]) expect(k[id].same, `row ${id}`).toBe(true);
});
