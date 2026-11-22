// The Desk specs' shared helpers (desk-*.spec.mjs that were one desk.spec.mjs).
import { deskSessionsSettled } from "./cue.mjs";

/** The list as it reads, top to bottom: group heads as "# …" (B2's WAITS ON YOU, WORKING, UP NEXT, and the folded
 *  DONE TODAY and CALLS WHILE AWAY lines), holder heads by name, items by id. */
export const listOrder = (page) => page.locator(".dk-items .dk-scroll > *, .dk-items .dk-scroll > .dk-gbody > .dk-gin > *").evaluateAll((els) => els.map((el) =>
  el.dataset.deskItem || (el.matches(".dk-holder") ? `# ${el.querySelector(".dk-nm").textContent}` : el.matches(".dk-gh, .dk2b-gh, .dk2b-fold") ? `# ${[...el.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim()).textContent.trim()}` : null)).filter(Boolean));

/** Item `id`'s group in the B2 list: the head ("WAITS ON YOU", "WORKING", "UP NEXT", …) it sits under, or null. */
export const groupOf = async (page, id) => { const o = await listOrder(page); let g = null; for (const x of o) { if (x.startsWith("# ")) g = x.slice(2); else if (x === String(id)) return g; } return null; };

/** A row's status line, under its title. */
export const statusOf = (page, id) => page.locator(`.dk-items [data-desk-item="${id}"] .dk2b-ts`);

/** The Desk run by host instead, so "lead" (with its crew) is a session in the list like any other. */
export const crewNotLead = (cue) => cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead = { sid: "host", name: "host-lead", state: "live" }; });

/** The animation the app's own styles give `el` (the test page turns every animation off, so not what it computes). */
export const blinkOf = (loc) => loc.evaluate((el) => [...document.styleSheets].flatMap((sh) => { try { return [...sh.cssRules]; } catch { return []; } })
  .filter((r) => r.selectorText && !r.selectorText.startsWith("*") && el.matches(r.selectorText) && r.style?.animationName).map((r) => r.style.animationName).pop() || "none");

export const boxOf = (loc) => loc.evaluate((el) => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, right: r.right }; });

/** Opens the item list's DONE TODAY, folded to a line at first (B2, #666). */
export async function openDone(page) {
  const head = page.locator('.dk-items [data-desk2b-fold="done"]');
  if ((await head.getAttribute("aria-expanded")) === "false") await head.click();
  await page.locator('.dk-items [data-desk2b-fold="done"][aria-expanded="true"]').waitFor();
}

/** The Desk's sessions list (B2, #666): it's no column now but opens on the right in the list's place, from All N
 *  sessions under the Working pill in the strip over the lead's chat. */
export async function openSessions(page) {
  if (await page.locator(".dk-sessions").count()) return;
  const pill = page.locator(".dk2b-strip [data-desk2b-work]");
  if (await pill.count()) {
    if ((await pill.getAttribute("aria-expanded")) !== "true") await pill.click();
    await page.locator(".dk2b-strip [data-desk2b-sessions]").click();
  } else {
    // With Show panel on, the panel stands in for the strip: All sessions is in the header's Desks menu.
    await page.locator('.top [data-hv-menu="desks"]').click();
    await page.locator(".top [data-hv-desk-sessions]").click();
  }
  await page.locator(".dk-sessions").waitFor();
  await deskSessionsSettled(page);
}
