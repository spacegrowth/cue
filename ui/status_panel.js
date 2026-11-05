// The status list above the chat: one line under a session's tabs (so it's in view on every tab: Chat, Actions,
// Agents, About), in the theme's own color. A click on it slides the whole list down over the chat, hanging from the
// line: the list, Parallelize when a group is ready, the session's switch and what the list costs. Relay's buttons
// (Enable relay; a lead's Run on relay and Stop relay) sit on the line itself, so they're always in view. A click on the line again, anywhere else, or Esc slides it back up. The
// ring by the box (statusChipHtml) puts the line away or brings it back; that choice is kept per session.

/** How many sessions' put-away choices are kept (the oldest go first). */
const STATUS_PANEL_KEEP = 200;
const STATUS_PANEL_STORE = "cue.statusPanel";
/** How long the list takes to unroll (the CSS animation's length). */
const STATUS_SLIDE_MS = 220;
/** The open list is at most this share of the chat's height (it scrolls past that), so the chat keeps the rest. */
const STATUS_DRAWER_SHARE = 0.5;

/** Per session: { away: put away with the ring } (kept). Open and when it opened: this run only. */
let statusPanelSaved = {};
try { statusPanelSaved = JSON.parse(localStorage.getItem(STATUS_PANEL_STORE) || "{}") || {}; } catch {}
const statusOpen = new Map();   // sid → when it opened (ms)
const statusDrawerScroll = new Map();   // sid → { key, top }: which list its open list shows and where it's scrolled

/** How the panel is set for a session: { away, open, slide } (`slide`: it just opened, so it slides down). */
const statusPanelView = (sid) => ({
  away: !!statusPanelSaved[sid]?.away, open: statusOpen.has(sid),
  slide: statusOpen.has(sid) && Date.now() - statusOpen.get(sid) < STATUS_SLIDE_MS,
});

/** Flip one of a session's choices: "away" (kept) or "open" (this run only). */
function statusPanelFlip(sid, what) {
  if (what === "open") { statusDrawerScroll.delete(sid); return statusOpen.has(sid) ? statusOpen.delete(sid) : statusOpen.set(sid, Date.now()); }
  const { [sid]: was = {}, ...rest } = statusPanelSaved;
  // The session moves to the end (most recent), the oldest drop off past the limit; back to the default isn't kept.
  const kept = Object.entries(rest).slice(-(STATUS_PANEL_KEEP - 1));
  statusPanelSaved = Object.fromEntries(was.away ? kept : [...kept, [sid, { away: true }]]);
  try { localStorage.setItem(STATUS_PANEL_STORE, JSON.stringify(statusPanelSaved)); } catch {}
}

/** The line: how far along, and its first step not done (and how many run beside it): "now:" while it's marked →,
 *  "next:" while none of it is started. What it waits on is in the open list. */
function statusOneLine(status, done) {
  const steps = statusSteps(status.items);
  const step = steps.find((st) => (Array.isArray(st) ? st : [st]).some((i) => i.state !== "done"));
  const group = Array.isArray(step) ? step.filter((i) => i.state !== "done") : step ? [step] : [];
  const label = group.some((i) => i.state === "now") ? "now:" : "next:";
  const at = group[0] ? `<span class="st-dim">${label}</span> ${esc(group[0].text)}${group.length > 1 ? ` <span class="st-dim">+ ${group.length - 1} side by side</span>` : ""}` : `<span class="st-dim">all done</span>`;
  return `<b>${done} of ${status.items.length}</b><span class="st-line">${at}</span>`;
}

/** The panel for a session's latest list (`sw`: { sid, on, working }, as for its ring, `key`: which list it is, and
 *  `spent`: what the list has cost this session, in words, or "", said beside its switch; `view`: statusPanelView).
 *  Nothing while it has no list, the list is off for it, or it's put away. */
function statusPanelHtml(status, sw, view) {
  const p = statusParts(status, sw);
  if (!p.listed || view.away) return "";
  const sid = esc(sw.sid), ring = statusRing(p.done, p.now, status.items.length), open = !!view.open;
  const bar = `<button class="st-bar st-inv" data-act="status-open" data-sid="${sid}" aria-expanded="${open}" aria-label="${open ? "Close the status list" : "Open the status list"}">${ring}${statusOneLine(status, p.done)}<span class="st-chev">${open ? "▴" : "▾"}</span></button>`;
  const drawer = open ? `<div class="st-drawer st-inv${view.slide ? " slide" : ""}" data-sid="${sid}" data-key="${esc(sw.key || "")}">${p.list}${p.par}${p.toggle}${p.cost}</div>` : "";
  return `<div class="st-panel"><div class="st-head st-inv">${bar}${p.relay}</div>${drawer}</div>`;
}

/** An open list is at most half the chat's height and ends above its bottom (the box stays free; on a tab with the
 *  chat hidden, its pane stands in), less a margin; it scrolls past that. Opened, or a new list in it (a reply came),
 *  it's scrolled to the step it's on (the done ones above it); otherwise it stays where you scroll it, across redraws. Measured after each redraw and on
 *  resize: the pane is a width container only, so CSS can't size it from the pane's height. */
const STATUS_DRAWER_MARGIN = 12;
function statusPanelFit() {
  document.querySelectorAll(".st-panel .st-drawer").forEach((d) => {
    const pane = d.closest(".active-pane"), chat = pane?.querySelector("[data-chat]")?.getBoundingClientRect();
    if (!pane) return;
    const area = chat?.height ? chat : pane.getBoundingClientRect();
    const room = Math.min(area.bottom - d.getBoundingClientRect().top - STATUS_DRAWER_MARGIN, area.height * STATUS_DRAWER_SHARE);
    d.style.maxHeight = `${Math.max(120, room)}px`;
    const { sid, key } = d.dataset, at = statusDrawerScroll.get(sid);
    if (at?.key === key) d.scrollTop = at.top;
    else {
      // The step it's on a quarter of the way down, measured on screen (its offsets are against the list's own boxes).
      const now = d.querySelector(".st-item.now, .st-item.left");
      d.scrollTop = now ? Math.max(0, d.scrollTop + now.getBoundingClientRect().top - d.getBoundingClientRect().top - d.clientHeight / 4) : 0;
      statusDrawerScroll.set(sid, { key, top: d.scrollTop });
    }
  });
}
/** A drag that selects the list's words (to copy them) and ends outside it isn't a click away: it stays open. */
let statusSelecting = false;
/** Close an open list (a click on its line again, anywhere but the panel, or Esc): it rolls back up, straight on the
 *  page (no redraw, so a click elsewhere still reaches what it was on). */
function statusPanelCloseAll() {
  if (!statusOpen.size) return false;
  statusOpen.clear();
  statusDrawerScroll.clear();
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  document.querySelectorAll(".st-panel .st-drawer").forEach((d) => {
    const gone = () => d.remove();
    if (still) return gone();
    // Rolled up by script: its promise settles whether it finishes or is cut short (a CSS animation's end event can
    // fail to come in the app's web view, which left the list in place, invisible).
    d.classList.add("closing");
    d.animate([{ clipPath: "inset(0 0 -40px 0)" }, { clipPath: "inset(0 0 100% 0)" }], { duration: 160, easing: "ease-in", fill: "forwards" }).finished.then(gone, gone);
  });
  document.querySelectorAll(".st-panel .st-bar").forEach((b) => {
    b.setAttribute("aria-expanded", "false");
    b.setAttribute("aria-label", "Open the status list");
    b.querySelector(".st-chev").textContent = "▾";
  });
  return true;
}
if (typeof document !== "undefined") {
  window.addEventListener("resize", statusPanelFit);
  document.addEventListener("scroll", (e) => {
    const d = e.target;
    if (d.classList?.contains("st-drawer")) statusDrawerScroll.set(d.dataset.sid, { key: d.dataset.key, top: d.scrollTop });
  }, true);
  document.addEventListener("mousedown", (e) => { statusSelecting = !!e.target.closest?.(".st-drawer"); }, true);
  document.addEventListener("click", (e) => {
    if (statusSelecting) { statusSelecting = false; return; }
    if (!e.target.closest?.(".st-panel")) statusPanelCloseAll();
  }, true);
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && statusPanelCloseAll()) e.stopImmediatePropagation(); }, true);
}

if (typeof module !== "undefined") module.exports = { statusPanelHtml, statusPanelView, statusPanelFlip };
