// The Desk specs' shared helpers (desk-*.spec.mjs that were one desk.spec.mjs).
import { deskSessionsSettled, expect } from "./cue.mjs";

/** Items as they read, top to bottom: the card's group heads as "# …" (WAITING ON YOU, WORKING, KEPT COPIES…), then
 *  UP NEXT and DONE TODAY (open or folded) on the green; holder heads by name, items by id (desk/c.js deskCList). */
export const listOrder = (page) => page.locator(".dkn-items .dkn-body > :not(.dkn-band), .dkn-items .dkn-body > .dkn-band > .dkn-gh, .dkn-items .dkn-body > .dkn-band > .dkn-bb > :not(.dkn-wl, .dkn-wmore), .dkn-items .dkn-body > .dkn-band > .dkn-bb > .dkn-wl > *, .dkn-items .dkn-dr > .dkn-dh, .dkn-items .dkn-dr > [data-dkn-list] > *").evaluateAll((els) => els.map((el) =>
  el.dataset.deskItem || (el.matches(".dk-holder") ? `# ${el.querySelector(".dk-nm").textContent}` : el.matches(".dkn-gh, .dkn-dh, .dk2b-fold") ? `# ${[...el.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim()).textContent.trim()}` : null)).filter(Boolean));

/** (Items' groups are bands since desk-live: a band's heading, then its rows, Working's capped list inside it.) */
/** Item `id`'s group in Items: the head ("WAITING ON YOU", "WORKING", "UP NEXT", "DONE TODAY", …) it sits under, or null. */
export const groupOf = async (page, id) => { const o = await listOrder(page); let g = null; for (const x of o) { if (x.startsWith("# ")) g = x.slice(2); else if (x === String(id)) return g; } return null; };

/** A row's words of where it stands: Now on the card's rows, the line under the title in Up next and Done today. */
export const statusOf = (page, id) => page.locator(`.dkn-items [data-desk-item="${id}"] :is(.dkn-now .dkn-nw, .dkn-now .dkn-nask, .dkn-a2 > span:last-child)`);

/** The Desk run by host instead, so "lead" (with its crew) is a session in the list like any other. */
export const crewNotLead = (cue) => cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead = { sid: "host", name: "host-lead", state: "live" }; });

/** The animation the app's own styles give `el` (the test page turns every animation off, so not what it computes). */
export const blinkOf = (loc) => loc.evaluate((el) => [...document.styleSheets].flatMap((sh) => { try { return [...sh.cssRules]; } catch { return []; } })
  .filter((r) => r.selectorText && !r.selectorText.startsWith("*") && el.matches(r.selectorText) && r.style?.animationName).map((r) => r.style.animationName).pop() || "none");

export const boxOf = (loc) => loc.evaluate((el) => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, right: r.right }; });

/** Opens DONE TODAY under Items, should it be folded to its heading. */
export async function openDone(page) {
  const head = page.locator('.dkn-items [data-desk-c-fold="done"]');
  if ((await head.getAttribute("aria-expanded")) === "false") await head.click();
  await page.locator('.dkn-items [data-desk-c-fold="done"][aria-expanded="true"]').waitFor();
}

/** The Desk's sessions list: All N sessions at the foot of the sessions column opens it in the chat's place. */
export async function openSessions(page) {
  if (await page.locator(".dk-sessions").count()) return;
  await page.locator(".dkn-side [data-desk2b-sessions]").click();
  await page.locator(".dk-sessions").waitFor();
  await deskSessionsSettled(page);
}
/** Opens the ⋯ menu inside `scope` (an item's page) unless it's open already (#669): Start, Park and the rest are in it. */
export async function openMore(scope) {
  const b = scope.locator(".dk-more").first();
  if ((await b.getAttribute("aria-expanded")) !== "true") await b.click();
  await expect(b).toHaveAttribute("aria-expanded", "true");
}
