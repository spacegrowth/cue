// The status list an agent ends its replies with, and how the chat shows it. The session writes:
//   **Where we are**
//   ✓ done   → now   ○ left       (∥ after the mark: runs side by side with the ∥ items next to it)
//   Waiting on you: <a decision, or "nothing">      (older replies: "⏸ Waiting on you: …")
// Shown as a track: one line down through the steps in order, a dot each; each run of ∥ items is one step, its items
// together in one box beside a single dot.
// No reply shows the block: the latest list is a ring and count in the box, ahead of the mic (statusChipHtml), which
// opens it, and where it's turned on or off for the session. The rules are kept in this one place.

const STATUS_HEADING = /^[ \t]*\*\*Where we are\*\*:?[ \t]*$/gm;
const STATUS_MARKS = { "✓": "done", "→": "now", "○": "left" };
const STATUS_WAITING = /^(?:⏸\s*)?Waiting on you:\s*(.*)$/i;
const STATUS_NOTHING = /^(?:nothing|none)\.?$/i;
const STATUS_PAR = /^(?:∥|\|\|)\s*/;
/** What the Parallelize button sends the session: it decides, Cue only asks. */
const PARALLELIZE = "Parallelize the current step: run the ∥ items that are ready now side by side as background agents, if they touch different files and it's worth the cost; otherwise keep them in order and say why. Mark each one running with →.";

/** A reply split into its words and its status list: { body, items: [{ state, text, par }], waiting, now } or, when it
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
    if (state) {
      const text = line.slice(1).trim(), par = STATUS_PAR.test(text);
      items.push({ state, text: text.replace(STATUS_PAR, ""), par });
    }
    else if (wait) waiting = STATUS_NOTHING.test(wait[1].trim()) ? "" : wait[1].trim();
    else return none;   // writing after the list: not its status list
  }
  if (!items.length) return none;
  const body = reply.slice(0, at).trimEnd().replace(/(?:^|\n)[ \t]*-{3,}$/, "").trimEnd();
  return { body, items, waiting, now: items.find((i) => i.state === "now") || items.find((i) => i.state === "left") || null };
}
/** The reply without its status list: what every reply shows, and what is read aloud. */
const stripStatus = (text) => parseStatus(text).body;

/** The items, numbered, as steps in order: an item alone, or (an array) a group of two or more ∥ items next to
 *  each other, which run side by side. A lone ∥ item is a step like any other. */
function statusSteps(items) {
  const steps = [];
  items.forEach((i, k) => {
    const it = { ...i, n: k + 1 }, last = steps[steps.length - 1];
    if (it.par && Array.isArray(last)) last.push(it);
    else steps.push(it.par ? [it] : it);
  });
  return steps.map((st) => (Array.isArray(st) && st.length === 1 ? st[0] : st));
}
/** The side-by-side group the session could start now: the first step not done, when it's a group with two or more
 *  items left and not already running side by side (two or more →). Else null. */
function parallelReady(steps) {
  const open = (i) => i.state !== "done";
  const next = steps.find((st) => (Array.isArray(st) ? st.some(open) : open(st)));
  if (!Array.isArray(next)) return null;
  return next.filter(open).length >= 2 && next.filter((i) => i.state === "now").length < 2 ? next : null;
}

/** The list as a track (see the top): one line down through the steps in order, a dot each; a side-by-side group
 *  is one step, its items together in one box beside a single dot. Each item: its number, its text, its mark at the
 *  end (a spinner for a group item that's running). */
function statusTree(items) {
  const mark = (i) => (i.par && i.state === "now" ? `<span class="st-spin"></span>` : Object.keys(STATUS_MARKS).find((m) => STATUS_MARKS[m] === i.state));
  const cells = (i, dot = "") => `<span class="st-n">${i.n}</span>${dot}<span class="st-text">${esc(i.text)}</span><span class="st-mark">${mark(i)}</span>`;
  const steps = statusSteps(items), end = steps.length - 1;
  return steps.map((st, k) => {
    const at = [k === 0 && "first", k === end && "last"].filter(Boolean).join(" ");
    const cls = (...names) => [...names, at].filter(Boolean).join(" ");
    if (!Array.isArray(st)) return `<div class="${cls("st-item st-step", st.state)}">${cells(st, '<span class="st-dot"></span>')}</div>`;
    const state = st.every((i) => i.state === "done") ? "done" : st.some((i) => i.state === "now") ? "now" : "left";
    return `<div class="${cls("st-group st-step", state)}"><span></span><span class="st-dot"></span><div class="st-box">${st.map((i) => `<div class="st-item ${i.state}">${cells(i)}</div>`).join("")}</div></div>`;
  }).join("");
}

/** A small progress ring: the done part solid, the item it's on paler after it, the rest a faint track. One size at
 *  any list length. */
function statusRing(done, now, total) {
  const size = 14, w = 2.4, r = (size - w) / 2, c = 2 * Math.PI * r, arc = (n) => `${(c * n) / total} ${c}`;
  const circle = (cls, dash) => `<circle class="${cls}" cx="${size / 2}" cy="${size / 2}" r="${r}"${dash ? ` stroke-dasharray="${dash}"` : ""}/>`;
  return `<svg class="st-ring" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" fill="none" stroke-width="${w}" aria-hidden="true">${circle("trk")}${circle("now", arc(done + now))}${circle("done", arc(done))}</svg>`;
}

/** The ring beside the mic, for the latest reply's list: the ring and "done/total" (a click or hover opens the card
 *  above it: the whole list, then what it waits on, as plain lines). `key` says which list; the card shows while it
 *  is the open one (statusPop). Nothing for a reply without a list.
 *  With `sw` ({ sid, on, working }: a session's own ring, while the status list is on in Settings) it's also where the
 *  list is turned on or off for that session: its card ends with the switch. Off: a faint empty ring, its card the
 *  switch alone; on with no list yet: the empty ring at full strength, its card the switch and when its first list
 *  comes (when this turn ends, while it's working; else with your next message). */
function statusChipHtml(status, key, open = statusPop?.key === key, sw = null) {
  const { items, waiting } = status;
  const listed = items.length && (!sw || sw.on);
  if (!listed && !sw) return "";
  const done = items.filter((i) => i.state === "done").length;
  const now = items.filter((i) => i.state === "now").length;
  // Numbered, so an item can be named ("do 3"), its mark at the end; a long one wraps under its own text.
  const list = listed ? `<div class="st-list">${statusTree(items)}<div class="st-wait">Waiting on you: ${esc(waiting || "nothing")}</div></div>` : "";
  // A group ready to start side by side: the button asks the session to (a session's own ring only).
  const ready = listed && sw ? parallelReady(statusSteps(items)) : null;
  const par = ready ? `<div class="st-par"><span>${ready.filter((i) => i.state !== "done").length} can run side by side<small>Asks the session; it decides</small></span><button class="btn" data-act="status-parallel" data-sid="${esc(sw.sid)}">Parallelize</button></div>` : "";
  const ring = listed ? `${statusRing(done, now, items.length)}${done}/${items.length}` : statusRing(0, 0, 1);
  const label = listed ? `Status list, ${done} of ${items.length} done` : `Status list ${sw.on ? "on" : "off"} for this session`;
  const toggle = sw ? `<div class="st-toggle ${list ? "" : "solo"}"><span>Status list for this session</span><button class="toggle ${sw.on ? "on" : ""}" data-act="status-session" data-sid="${esc(sw.sid)}" data-on="${sw.on ? 0 : 1}" role="switch" aria-checked="${sw.on}" aria-label="Status list for this session"><span></span></button></div>` : "";
  const starts = sw?.on && !listed ? `<div class="st-note">Status tracking starts ${sw.working ? "when this turn ends" : "with your next message"}</div>` : "";
  return `<div class="cv-schip ${open ? "open" : ""} ${listed ? "" : sw.on ? "st-empty" : "st-empty st-off"}" data-status-chip="${esc(key)}"><button class="cv-schip-btn" aria-expanded="${open}" aria-label="${label}">${ring}</button><div class="cv-spop">${list}${par}${toggle}${starts}</div></div>`;
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

/** The status list is on in Settings → Progress (it is unless turned off there): sessions' rings show. */
const statusShown = (st = typeof state !== "undefined" ? state.settings?.status : null) => st?.on !== false;
/** On for this session: shown, and turned on from its ring (each session starts off). */
const statusOn = (sid, st = typeof state !== "undefined" ? state.settings?.status : null) => statusShown(st) && (st?.sessions || []).includes(sid);

if (typeof module !== "undefined") module.exports = { parseStatus, stripStatus, statusChipHtml, statusOn, statusShown, statusSteps, parallelReady, PARALLELIZE };
