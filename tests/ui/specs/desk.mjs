// The Desk specs' shared helpers (desk-*.spec.mjs that were one desk.spec.mjs).

/** The list as it reads, top to bottom: group labels and holder heads as "# …", items by id. */
/** A group's rows are in its fold (#430), after its label. */
export const listOrder = (page) => page.locator(".dk-items .dk-scroll > *, .dk-items .dk-scroll > .dk-gbody > .dk-gin > *").evaluateAll((els) => els.map((el) =>
  el.dataset.deskItem || (el.matches(".dk-holder") ? `# ${el.querySelector(".dk-nm").textContent}` : el.matches(".dk-gh") ? `# ${[...el.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim()).textContent.trim()}` : null)).filter(Boolean));

/** The Desk run by host instead, so "lead" (with its crew) is a session in the list like any other. */
export const crewNotLead = (cue) => cue.change((st) => { st.desk.projects.find((d) => d.project === "cue").lead = { sid: "host", name: "host-lead", state: "live" }; });

/** The animation the app's own styles give `el` (the test page turns every animation off, so not what it computes). */
export const blinkOf = (loc) => loc.evaluate((el) => [...document.styleSheets].flatMap((sh) => { try { return [...sh.cssRules]; } catch { return []; } })
  .filter((r) => r.selectorText && !r.selectorText.startsWith("*") && el.matches(r.selectorText) && r.style?.animationName).map((r) => r.style.animationName).pop() || "none");

export const boxOf = (loc) => loc.evaluate((el) => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, right: r.right }; });
