// Desk home's table (#440): at a narrow and a wide window, each column's text and pills stay inside its own column
// (wrapping or shortening, never running into the next), and the Desk's name, its project and its people line show
// in full on the left.
import { test, expect } from "./cue.mjs";

const openHome = async (page, cue) => {
  await cue.open("desk");
  await page.locator('.switch-view [data-view="desk"]').click();
  // The Desk switcher moved to the header's Desks menu (#666).
  await page.locator('.top [data-hv-menu="desks"]').click(); await page.locator(".top [data-hv-desk-home]").click();
  await page.locator(".dk-desks .dk-desk").first().waitFor();
};
/** A full Desk: a long name and project, and every kind with working, paused, finishing, need-you and to-review. */
const fill = (cue) => cue.change((st) => {
  const d = st.desk.projects.find((p) => p.project === "cue");
  d.name = "Customer onboarding and billing dashboard";
  st.settings.project ||= {}; st.settings.project.names = { ...(st.settings.project.names || {}), cue: "customer-onboarding-site" };
  const base = d.items[0], states = ["working", "working", "paused", "needs_resume", "held", "ready_to_check"];
  let n = 0;
  for (const kind of ["task", "bug", "feature"]) for (const s of states) d.items.push({ ...base, id: `L${n++}`, kind, state: s, stopped: false, title: `Long item ${n}` });
});
/** Where something in a cell is drawn past the cell's own edges: its text and its pills, as clipped by what hides overflow. */
const spills = (page) => page.locator(".dk-desks .dk-desk").first().evaluate((row) => {
  const out = [];
  for (const cell of row.children) {
    const c = cell.getBoundingClientRect();
    const clip = (node, r) => {
      for (let e = node.nodeType === 1 ? node : node.parentElement; e; e = e === cell ? null : e.parentElement) {
        if (getComputedStyle(e).overflowX === "visible") continue;
        const b = e.getBoundingClientRect();
        r = { left: Math.max(r.left, b.left), right: Math.min(r.right, b.right) };
      }
      return r;
    };
    const check = (node, r) => {
      if (r.right - r.left < 1) return;
      const v = clip(node, r);
      if (v.right - v.left >= 1 && (v.left < c.left - 1 || v.right > c.right + 1)) out.push(`${cell.className}: "${(node.textContent || "").trim().slice(0, 40)}" ${Math.round(v.left)}–${Math.round(v.right)} outside ${Math.round(c.left)}–${Math.round(c.right)}`);
    };
    const w = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
    for (let n = w.nextNode(); n; n = w.nextNode()) {
      if (n.nodeType === 3) { const rg = document.createRange(); rg.selectNodeContents(n); for (const r of rg.getClientRects()) check(n, r); }
      else if (getComputedStyle(n).backgroundColor !== "rgba(0, 0, 0, 0)") check(n, n.getBoundingClientRect());
    }
  }
  return out;
});
const full = (l) => l.evaluate((e) => e.scrollWidth <= e.clientWidth + 1);

for (const width of [1000, 1728]) {
  test(`at ${width}px wide, every column keeps to itself and the name, project and people show in full (#440)`, async ({ page, cue }) => {
    await page.setViewportSize({ width, height: 900 });
    await openHome(page, cue);
    await fill(cue);
    const row = page.locator(".dk-desks .dk-desk").first();
    await expect(row.locator(".dh-nm")).toHaveText("Customer onboarding and billing dashboard");
    await expect(row.locator(".dh-ck").first()).toBeVisible();
    expect(await spills(page)).toEqual([]);
    // Each head sits over its own column.
    const lefts = (l) => l.evaluate((e) => [...e.children].map((c) => Math.round(c.getBoundingClientRect().left)));
    expect(await lefts(page.locator(".dh-hd"))).toEqual(await lefts(row));
    for (const l of [row.locator(".dh-nm"), row.locator(".dh-pj"), row.locator(".dh-cell").nth(1).locator(".dh-sm")]) expect(await full(l), await l.textContent()).toBe(true);
    if (process.env.SHOT_DIR) await page.locator(".dh-tbl").screenshot({ path: `${process.env.SHOT_DIR}/desk-home-${width}.png` });
  });
}
