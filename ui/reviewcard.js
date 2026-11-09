// The Review rows at the top of a lead's chat: one line per packet of its team that waits on you, in one group that
// scrolls past a few rows. Where each stands comes from Cue (state.reviews, one entry per packet); this only draws it.

/** Each state's buttons on its row, in order; ⋯ holds the rest. */
const REVIEW_ACTS = { waiting: ["review", "diff"], auto: ["diff"], queued: ["diff"], reviewing: ["diff"], reviewed: ["commit", "fixes"] };
const reviewOpen = new Map();   // executor → what's open under its row: "findings" or "asked"
let reviewMenu = null;          // the executor whose ⋯ menu is open

/** What a row says after the name. */
function reviewState(r) {
  const packet = r.packet ? ` packet ${r.packet}` : "";
  const n = r.findings.length;
  switch (r.state) {
    case "reviewing": return "reviewing…";
    case "queued": return "review queued";
    case "reviewed": return `reviewed${r.recommendation ? `: ${r.recommendation}` : ""}${n ? ` · ${n} note${n === 1 ? "" : "s"}` : ""}`;
    case "auto": return `finished${packet} · its lead is reviewing it`;
    default: return [`finished${packet}`, r.stat, r.missed ? "its lead hasn't picked this up" : ""].filter(Boolean).join(" · ");
  }
}

/** One button: Review and Diff are Cue's (data-crew); Commit and Send fixes type their words into the lead
 *  (data-rv-text: not data-text, which marks a text box the window gives the cursor back to after a redraw). */
function reviewButton(act, exec, lead) {
  const say = (text, label, primary) => `<button class="btn ${primary ? "primary" : ""}" data-rv-say="${esc(lead)}" data-rv-text="${esc(text)}">${label}</button>`;
  switch (act) {
    case "review": return crewBtn(exec, "review", "Review", true);
    case "diff": return crewBtn(exec, "diff", "Diff");
    case "commit": return say("commit it", "Commit", true);
    default: return say("send the fixes", "Send fixes");
  }
}

/** A finding as a plain line: its number and text, the kind it is (note, should-fix…) muted after; the file:line it
 *  starts with left out. */
function findingLine(line) {
  const m = line.match(/^(\d+\.)\s+(?:\S+:\d+\S*\s+—\s+)?(?:(blocker|must-fix|should-fix|nit|note|question)\s+—\s+)?(.+)$/i);
  return m ? `<div>${m[1]} ${esc(m[3])}${m[2] ? ` <span class="rv-k">${esc(m[2].toLowerCase())}</span>` : ""}</div>` : `<div>${esc(line)}</div>`;
}

/** One packet's row, and what's open under it. */
function reviewRow(exec, r, lead) {
  const open = reviewOpen.get(exec);
  const st = esc(reviewState(r));
  const fold = r.state === "reviewed" && r.findings.length;
  const state = fold ? `<button class="rv-st" data-rv-open="${esc(exec)}">${st}<span class="rv-chev">${open === "findings" ? "⌄" : "›"}</span></button>` : `<span class="rv-st">${st}</span>`;
  const under = open === "findings" && fold ? r.findings.map(findingLine).join("") : open === "asked" && r.goal ? `<div>${esc(r.goal)}</div>` : "";
  return `<div class="rv-row"><b class="rv-name">${esc(r.name || "its executor")}</b>${state}<span class="grow"></span>${REVIEW_ACTS[r.state].map((a) => reviewButton(a, exec, lead)).join("")}<button class="rv-more ${reviewMenu === exec ? "on" : ""}" data-rv-more="${esc(exec)}" aria-label="More">⋯</button></div>${under ? `<div class="rv-under">${under}</div>` : ""}`;
}

/** The open ⋯ menu: Diff when the row doesn't show it, Set aside, and what the packet asked. */
function reviewMenuHtml(exec, r) {
  const diff = REVIEW_ACTS[r.state].includes("diff") ? "" : `<button data-crew="diff" data-crew-sid="${esc(exec)}">Diff</button>`;
  const asked = r.goal ? `<button data-rv-asked="${esc(exec)}">What it was asked</button>` : "";
  return `<div class="more-menu rv-menu">${diff}<button data-crew="aside" data-crew-sid="${esc(exec)}">Set aside</button>${asked}</div>`;
}

/** The lead's rows, "" when nothing waits on you. */
function reviewRows(lead) {
  const rows = Object.entries(state.reviews || {}).filter(([, r]) => r.lead === lead);
  if (!rows.length) return "";
  const menu = rows.find(([exec]) => exec === reviewMenu);
  return `<div class="rv-wrap"><div class="rv-rows" data-scroll-key="${esc(lead)}">${rows.map(([exec, r]) => reviewRow(exec, r, lead)).join("")}</div>${menu ? reviewMenuHtml(...menu) : ""}</div>`;
}

/** The ⋯ menu under its button (over it when there isn't room below the rows): placed after each draw and as the
 *  rows scroll. Gone with its row. */
function placeReviewMenu() {
  const menu = document.querySelector(".rv-menu"), at = menu && document.querySelector(".rv-more.on");
  if (!menu) return;
  if (!at) { reviewMenu = null; return menu.remove(); }
  const wrap = menu.parentElement.getBoundingClientRect(), r = at.getBoundingClientRect();
  const below = r.bottom + 4 + menu.offsetHeight <= innerHeight - 8;
  menu.style.top = `${(below ? r.bottom + 4 : r.top - 4 - menu.offsetHeight) - wrap.top}px`;
  menu.style.right = `${wrap.right - r.right}px`;
}
if (typeof document !== "undefined") document.addEventListener("scroll", (e) => { if (e.target.classList?.contains("rv-rows")) placeReviewMenu(); }, true);
