// The Desk's clicks, by attribute (deskItem → data-desk-item): each handler takes its element.
// Each Desk file with clicks of its own adds them with deskClicks({ ... }) as it loads; they're read when a click
// comes (desk/act.js deskTake, deskSel), so the Desk's files can load in any order after this one.

/** Every Desk click rule, from all the files that add one. */
const DESK_CLICKS = {};

/** Add `map`'s click rules to the Desk's. A name already taken is a mistake: it's said, and the first one stays. */
function deskClicks(map) {
  for (const [k, run] of Object.entries(map)) {
    if (k in DESK_CLICKS) console.error(`deskClicks: ${k} is added twice`);
    else DESK_CLICKS[k] = run;
  }
}
