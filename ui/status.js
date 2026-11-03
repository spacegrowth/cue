// The status list an agent ends its replies with, and how the chat shows it. The session writes:
//   **Where we are**
//   ✓ done   → now   ○ left
//   Waiting on you: <a decision, or "nothing">      (older replies: "⏸ Waiting on you: …")
// No reply shows the block: the latest list is a ring and count in the box, ahead of the mic (statusChipHtml), which
// opens it. The rules are kept in this one place.

const STATUS_HEADING = /^[ \t]*\*\*Where we are\*\*:?[ \t]*$/gm;
const STATUS_MARKS = { "✓": "done", "→": "now", "○": "left" };
const STATUS_WAITING = /^(?:⏸\s*)?Waiting on you:\s*(.*)$/i;
const STATUS_NOTHING = /^(?:nothing|none)\.?$/i;

/** A reply split into its words and its status list: { body, items: [{ state, text }], waiting, now } or, when it
 *  doesn't end with a list (none, or more writing after it), { body: text, items: [] }. `waiting`: what it waits on,
 *  "" for nothing. `now`: the item it's on (the first →, else the first ○). */
function parseStatus(text) {
  const reply = String(text ?? "");
  const none = { body: reply, items: [], waiting: "", now: null };
  const at = [...reply.matchAll(STATUS_HEADING)].pop()?.index;
  if (at == null) return none;
  const items = [];
  let waiting = "";
  for (const raw of reply.slice(at).split("\n").slice(1)) {
    const line = raw.trim();
    if (!line) continue;
    const state = STATUS_MARKS[line[0]];
    const wait = STATUS_WAITING.exec(line);
    if (state) items.push({ state, text: line.slice(1).trim() });
    else if (wait) waiting = STATUS_NOTHING.test(wait[1].trim()) ? "" : wait[1].trim();
    else return none;   // writing after the list: not its status list
  }
  if (!items.length) return none;
  const body = reply.slice(0, at).trimEnd().replace(/(?:^|\n)[ \t]*-{3,}$/, "").trimEnd();
  return { body, items, waiting, now: items.find((i) => i.state === "now") || items.find((i) => i.state === "left") || null };
}
/** The reply without its status list: what every reply shows, and what is read aloud. */
const stripStatus = (text) => parseStatus(text).body;

/** A small progress ring: the done part solid, the item it's on paler after it, the rest a faint track. One size at
 *  any list length. */
function statusRing(done, now, total) {
  const size = 14, w = 2.4, r = (size - w) / 2, c = 2 * Math.PI * r, arc = (n) => `${(c * n) / total} ${c}`;
  const circle = (cls, dash) => `<circle class="${cls}" cx="${size / 2}" cy="${size / 2}" r="${r}"${dash ? ` stroke-dasharray="${dash}"` : ""}/>`;
  return `<svg class="st-ring" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" fill="none" stroke-width="${w}" aria-hidden="true">${circle("trk")}${circle("now", arc(done + now))}${circle("done", arc(done))}</svg>`;
}

/** The chip for the latest reply's list: a small ring and "done/total" (a click or hover opens the card above
 *  it: the whole list, then what it waits on, as plain lines). `key` says which list; the card shows while it
 *  is the open one (statusPop). Nothing for a reply without a list. */
function statusChipHtml(status, key, open = statusPop?.key === key) {
  const { items, waiting } = status;
  if (!items.length) return "";
  const done = items.filter((i) => i.state === "done").length;
  const now = items.filter((i) => i.state === "now").length;
  const mark = (state) => Object.keys(STATUS_MARKS).find((m) => STATUS_MARKS[m] === state);
  // Numbered, so an item can be named ("do 3"), its mark at the end; a long one wraps under its own text.
  const list = items.map((i, n) => `<div class="st-item ${i.state}"><span class="st-n">${n + 1}</span><span>${esc(i.text)}</span><span class="st-mark">${mark(i.state)}</span></div>`).join("");
  return `<div class="cv-schip ${open ? "open" : ""}" data-status-chip="${esc(key)}"><button class="cv-schip-btn" aria-expanded="${open}" aria-label="Status list, ${done} of ${items.length} done">${statusRing(done, now, items.length)}${done}/${items.length}</button><div class="cv-spop"><div class="st-list">${list}<div class="st-wait">Waiting on you: ${esc(waiting || "nothing")}</div></div></div></div>`;
}

/** The open card, if any: { key, pinned }. Hovering the chip opens it until the pointer leaves; a click keeps it open
 *  (and a second click, Esc or a click elsewhere closes it). Painted straight onto the chip, not by redrawing. */
let statusPop = null;
function statusPopPaint() {
  document.querySelectorAll("[data-status-chip]").forEach((el) => {
    const open = statusPop?.key === el.dataset.statusChip;
    el.classList.toggle("open", open);
    el.classList.toggle("pinned", open && statusPop.pinned);
    el.firstElementChild?.setAttribute("aria-expanded", open);
    // The card is fixed (the chat clips what sticks out of it), right-aligned to the chip and ending right on top
    // of it (no gap, so the pointer can move from the chip into the card): it never reaches the box below. Fixed
    // means fixed to the pane, not the window (the pane is a size container), so place it from where 0,0 lands.
    if (open) {
      const r = el.getBoundingClientRect(), pop = el.lastElementChild;
      pop.style.left = pop.style.top = "0px";
      const o = pop.getBoundingClientRect();
      pop.style.maxHeight = `${Math.max(80, r.top - Math.max(o.top, 0) - 8)}px`;
      pop.style.left = `${Math.max(o.left + 8, r.right - pop.offsetWidth) - o.left}px`;
      pop.style.top = `${r.top - pop.offsetHeight - o.top}px`;
    }
  });
}
function bindStatusChip() {
  const chip = (e) => e.target.closest?.("[data-status-chip]");
  document.addEventListener("mouseover", (e) => {
    const c = chip(e);
    if (c && !statusPop) { statusPop = { key: c.dataset.statusChip, pinned: false }; statusPopPaint(); }
  });
  document.addEventListener("mouseout", (e) => {
    const c = chip(e);
    if (c && statusPop && !statusPop.pinned && !c.contains(e.relatedTarget)) { statusPop = null; statusPopPaint(); }
  });
  // Pressing inside the card (to select its text) keeps it open, and the click that ends a drag out of it doesn't close it.
  let selecting = false;
  document.addEventListener("mousedown", (e) => {
    selecting = !!(statusPop && e.target.closest?.(".cv-spop"));
    if (selecting) statusPop.pinned = true;
  }, true);
  document.addEventListener("click", (e) => {
    if (selecting) { selecting = false; return; }
    const c = chip(e);
    if (c && e.target.closest(".cv-schip-btn")) statusPop = statusPop?.pinned ? null : { key: c.dataset.statusChip, pinned: true };
    else if (statusPop && !c) statusPop = null;
    else return;
    statusPopPaint();
  }, true);
  document.addEventListener("scroll", () => { if (statusPop) statusPopPaint(); }, true);
  // Esc closes the card and does nothing else (Esc twice stops a working session).
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !statusPop) return;
    e.stopImmediatePropagation();
    statusPop = null;
    statusPopPaint();
  }, true);
}
if (typeof document !== "undefined") bindStatusChip();

if (typeof module !== "undefined") module.exports = { parseStatus, stripStatus, statusChipHtml };
