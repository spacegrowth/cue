// Cue UI core — the window's own state: held redraws while you select, toasts, zoom, icons, what stays in memory,
// the open session, and drafts.
let toastTimer;
// ---------- selecting text: redraws wait while you drag, and letting go copies it ----------
let mouseDown = false, heldDraw = false, selBefore = "", copied = null;   // copied: { at, words } for the pill   // selBefore: what was selected when you pressed
const inField = (n) => !!(n?.nodeType === 1 ? n : n?.parentElement)?.closest?.("input, textarea");
let composing = false;   // an IME composition is in progress in a box: redraws wait, Enter is the IME's
addEventListener("compositionstart", (e) => { if (inField(e.target)) composing = true; });
addEventListener("compositionend", () => { composing = false; if (heldDraw) { heldDraw = false; renderMain(); } });
// The box lost focus mid-composition (a click elsewhere, ⌘Tab): the composition is over for Cue's
// purposes, whether or not its end event came, so redraws never stay held.
addEventListener("focusout", () => { if (composing) { composing = false; if (heldDraw) { heldDraw = false; renderMain(); } } }, true);
// A selection made by this press (a click on a button leaves an older one in place: not that).
const dragSelecting = () => mouseDown && !getSelection().isCollapsed && getSelection().toString() !== selBefore;
// Pressing a link or a button: a redraw before you let go would swap it for a copy, and the browser drops a
// click whose pressed element left the page (Cue redraws on every change, many times a second with agents at
// work). It waits until the click has gone through.
let onPress = false;
addEventListener("mousedown", (e) => { mouseDown = e.button === 0 && !inField(e.target); onPress = mouseDown && !!e.target.closest?.("a[data-href], a[data-path], button, [role=button]"); selBefore = getSelection().toString(); }, true);
addEventListener("mouseup", () => {
  if (!mouseDown) return;
  const sel = getSelection(), text = sel.toString(), fresh = dragSelecting();
  mouseDown = false;
  const wasPress = onPress;
  onPress = false;
  // Selected text in the window (not in a box you type in, where ⌘C works as ever): on your clipboard now.
  if (fresh && text.trim() && !inField(sel.anchorNode)) {
    if (!document.execCommand("copy")) navigator.clipboard?.writeText(text);
    copied = { at: Date.now(), words: text.length > 40 ? text.trim().split(/\s+/).length : 0 };
    if (!heldDraw && !showCopied()) toast("Copied");
  }
  // Pressed on a link or a button: after the click (it fires after this), so it's still there for it.
  if (heldDraw) { heldDraw = false; wasPress ? setTimeout(renderMain) : renderMain(); }
}, true);
// Let go outside the window (no mouseup here): the held redraw happens anyway.
addEventListener("blur", () => { mouseDown = onPress = false; if (heldDraw) { heldDraw = false; renderMain(); } });

/** "Copied", in Moss, at the right end just above the chat's text box (or its bottom, with no box). Put back
 *  after each redraw, its fade carrying on where it was. False when there's no chat to show it in. */
const COPIED_MS = 1600;
function showCopied() {
  const left = copied ? COPIED_MS - (Date.now() - copied.at) : 0;
  if (left <= 0) return false;
  const host = document.querySelector(".active-pane .foot") || document.querySelector(".active-pane");
  if (!host) return false;
  host.querySelector(":scope > .copied")?.remove();
  host.insertAdjacentHTML("beforeend", `<span class="copied" style="animation-delay:-${COPIED_MS - left}ms">Copied${copied.words ? ` · ${copied.words} words` : ""}</span>`);
  return true;
}

function toast(msg, ms = 2600) {
  let el = document.querySelector(".toast");
  if (!el) { el = document.createElement("div"); el.className = "toast"; document.body.append(el); }
  el.textContent = msg;
  el.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove("show"), ms);
}
async function setSetting(key, value) {
  try { await invoke("set_setting", { key, value }); } catch (e) { toast(`Couldn't save: ${e}`); }
}
/** Text size: the window zooms in steps (config.rs ZOOM_STEPS); Cue keeps the one you pick. */
const ZOOMS = [[0.9, "Compact"], [1, "Default"], [1.15, "Large"], [1.3, "Larger"]];
/** One step bigger (1) or smaller (-1), or back to Default (0). */
function stepZoom(dir) {
  const cur = ZOOMS.findIndex(([z]) => z === (state.settings?.appearance?.zoom ?? 1));
  const i = dir === 0 ? 1 : Math.max(0, Math.min(ZOOMS.length - 1, (cur < 0 ? 1 : cur) + dir));
  const [z, name] = ZOOMS[i];
  toast(`Text size: ${name}${i === cur && dir ? (dir > 0 ? " (the largest)" : " (the smallest)") : ""}`);
  if (i !== cur) setSetting("appearance.zoom", z);
}
const SEARCH_ICON = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg>`;
const GEAR_ICON = `<svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>`;
const MIC_ICON = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="3" width="6" height="12" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/></svg>`;
/** The tada (party popper): a turn you asked to be told about. `filled` fills its cone. */
const tadaIcon = (size = 14, filled = false) => `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5.8 11.3 2 22l10.7-3.79"/><path d="M4 3h.01"/><path d="M22 8h.01"/><path d="M15 2h.01"/><path d="M22 20h.01"/><path d="m22 2-2.24.75a2.9 2.9 0 0 0-1.96 3.12c.1.86-.57 1.63-1.45 1.63h-.38c-.86 0-1.6.6-1.76 1.44L14 10"/><path d="m22 13-.82-.33c-.86-.34-1.82.2-1.98 1.11-.11.7-.72 1.22-1.43 1.22H17"/><path d="m11 2 .33.82c.34.86-.2 1.82-1.11 1.98C9.52 4.9 9 5.52 9 6.23V7"/><path d="M11 13c1.93 1.93 2.83 4.17 2 5-.83.83-3.07-.07-5-2-1.93-1.93-2.83-4.17-2-5 .83-.83 3.07.07 5 2Z"${filled ? ` fill="currentColor"` : ""}/></svg>`;
const IMAGE_ICON = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="1.7"/><path d="M21 16l-5-5-8 8"/></svg>`;

/** A saved upload as something an <img> can show (Tauri's asset protocol). */
const fileSrc = (path) => (T.core.convertFileSrc ? T.core.convertFileSrc(path) : path);
// ---------- memory: fast first, with a ceiling ----------
/** Switching sessions should be instant, so what a session loads stays: its steps for as long as it's
 *  live (small), and its heavy parts (full tool outputs and diffs, older messages you scrolled back to)
 *  while they fit a generous budget, the least recently opened going first. The budget is a ceiling
 *  for a long day, not something normal use reaches. Images, the newest that fit theirs. Anything
 *  dropped loads again when it's shown. */
const HEAVY_BYTES = 256 * 2 ** 20;  // heavy parts of the sessions besides the open one, roughly
const IMAGE_BYTES = 48 * 2 ** 20;   // images (a screenshot is a few MB as a data: URL)
const recentSids = [];   // most recently opened first
/** Roughly what a cached value holds: its text (two bytes a character), plus a little per value. */
const sizeOf = (v) => (typeof v === "string" ? v.length * 2 : v && typeof v === "object" ? Object.values(v).reduce((n, x) => n + sizeOf(x), 16) : 8);
function seenSession(sid) {
  if (!sid) return;
  const at = recentSids.indexOf(sid);
  if (at >= 0) recentSids.splice(at, 1);
  recentSids.unshift(sid);
  recentSids.length = Math.min(recentSids.length, 50);
  trimCaches();
}
function trimCaches() {
  const sidOf = (k) => String(k).split("|")[0];
  const open = new Set([active?.sid, quietOpen].filter(Boolean));
  const live = new Set([...state.sessions.map((s) => s.session_id), ...(state.live || []).map((q) => q.session_id)]);
  // Steps and commands: kept for every live session (and the ones you opened lately).
  const near = new Set([...open, ...live, ...recentSids]);
  for (const m of [stepFeeds, cmdLists, cmdMissed]) for (const k of [...m.keys()]) if (!near.has(sidOf(k))) m.delete(k);
  for (const k of [...btwLog.keys()]) if (!near.has(k)) btwLog.delete(k);
  // Per-session bits of this window's: a terminal's last line, "!" lines (the + New box's stay), forwardable messages
  // (keyed by session or card id, then ":" and the time).
  for (const k of [...termLast.keys()]) if (!near.has(k)) termLast.delete(k);
  for (const k of [...ranLog.keys()]) if (k !== "new" && !near.has(k)) ranLog.delete(k);
  const cards = new Set(state.items.map((i) => i.id));
  const rows = new Set(state.history.map((i) => i.id));
  for (const id of [...fullItems.keys()]) if (!rows.has(id)) fullItems.delete(id);   // History's cards asked for in full: while their rows last
  for (const k of [...fwdMsgs.keys()]) { const who = k.slice(0, k.lastIndexOf(":")); if (!near.has(who) && !cards.has(who)) fwdMsgs.delete(k); }
  // The heavy parts: the open session's always; the rest newest first, while they fit.
  const heavy = [older, stepDetail];
  const bytes = (sid) => heavy.reduce((n, m) => n + [...m].reduce((b, [k, v]) => b + (sidOf(k) === sid ? sizeOf(v) : 0), 0), 0);
  const keep = new Set(open);
  let total = 0;
  for (const sid of recentSids) if (!open.has(sid) && (total += bytes(sid)) <= HEAVY_BYTES) keep.add(sid);
  for (const m of heavy) for (const k of [...m.keys()]) if (!keep.has(sidOf(k))) m.delete(k);
  // Images: the newest that fit (always the newest few, however big).
  let img = 0, kept = 0;
  for (const [k, v] of [...localImgs].reverse()) { img += sizeOf(v); if (++kept > 4 && img > IMAGE_BYTES) localImgs.delete(k); }
}

/** What the Active pane shows: your pick (kept after you act), else the oldest thing waiting on you. */
function current() {
  // A parked session is open: nothing live to show, and it stays the one open.
  if (parkedActive()) return null;
  if (active) {
    let it = active.id ? findItem(active.id) : null;
    const s = sessionOf(active.sid);
    // Same session asking again (it finished the turn you replied to): show the new ask in place.
    // Not when you opened an old answer yourself (from Recently answered): then show exactly that.
    if (!active.exact && !isPending(it)) { const next = state.items.find((i) => i.session_id === active.sid); if (next) { it = next; active.id = next.id; } }
    if (it || s) return { it, s };
    // It just ended (Closed was last asked for before it did): ask again and keep it open, so it shows
    // as closed with Resume instead of Active moving on. Once per session, so one that isn't there still moves on.
    if (active.sid && closedAskedFor !== active.sid) { closedAskedFor = active.sid; queueMicrotask(loadClosed); return null; }
    if (active.sid && closedLoading) return null;
  }
  const first = needsYou()[0];
  active = first ? { id: first.id, sid: first.session_id } : null;
  return first ? { it: first, s: sessionOf(first.session_id) } : null;
}
/** Where you were: the session open in Active, and which view. Kept across restarts, so reopening Cue
 *  lands you back on it instead of on whatever's oldest. Main window only. */
const SPOT = "cue.spot";
let spotRestored = false;   // the first draws (before restoreSpot) mustn't overwrite where you were
function saveSpot() {
  if (!spotRestored) return;
  try { localStorage.setItem(SPOT, JSON.stringify({ sid: active?.sid || null, view })); } catch {}
}
function restoreSpot() {
  spotRestored = true;
  let spot = null;
  try { spot = JSON.parse(localStorage.getItem(SPOT) || "null"); } catch {}
  if (!spot) return;
  if (["board", "sessions", "history", "insights"].includes(spot.view)) view = spot.view;
  // Only if that session is still around; otherwise the usual oldest-first pick stands.
  const card = state.items.find((i) => i.session_id === spot.sid);
  if (spot.sid && (card || sessionOf(spot.sid) || parkedOf(spot.sid))) active = { id: card?.id || null, sid: spot.sid };
  renderMain();
}
/** A session opened from another project (⌘K, a chip, a notice): the window goes to its project, so it's shown
 *  with its own sessions, not in the one you were on. All projects stays All projects. `id`: its card, when
 *  that's all there is to go by. */
function followProject(sid, id = null) {
  const proj = sid
    ? sessionOf(sid)?.project ?? state.items.find((i) => i.session_id === sid)?.project ?? parkedOf(sid)?.project ?? (state.live || []).find((q) => q.session_id === sid)?.project
    : findItem(id)?.project;
  if (projFilter && proj && proj !== projFilter) setProjFilter(proj);
}
function setActive(id, sid, exact = false) {
  view = "board";
  quietOpen = null;
  active = { id: id || null, sid: sid || findItem(id)?.session_id || null, exact };
  followProject(active.sid, id);
  seenSession(active.sid);
  if (termSid && termSid !== active.sid) closeTerm(true);
  loadSteps(active.sid);   // now, not on the next tick
  sheet = menuFor = redirectFor = btwFor = null;
  renderMain();
  // Picked a session (any list, a chip, the live list's Enter): ready to type to it.
  focusComposer();
}
/** Another project picked (or All projects): the chat opens on the top of its Waiting (or none), and the
 *  terminal open closes with what was there. */
function openTopWaiting() {
  active = quietOpen = null;
  if (termSid) closeTerm(true);
  current();
}
/** The next thing waiting on you that isn't on screen. */
const nextUp = () => needsYou().find((i) => i.id !== active?.id && i.session_id !== active?.sid);
function goNext() { const n = nextUp(); if (n) setActive(n.id, n.session_id); }
/** History: everything you answered, newest first, by day. Click a row to read it in place. */
// ---------- "N live" in the header: every live session, one click from any screen, and + New ----------
let liveOpen = false;              // the drop-down is open
let liveSpot = false;              // …opened with ⌘K: in the middle of the window, like Spotlight
let liveSel = 0;                   // the highlighted row (↑ ↓ move it, Enter opens it)
let liveShown = [];                // the rows on screen, in order: [{ sid, act }]
let newOpen = false;               // its New session form is showing
/** + New starts as you last left it: agent, permissions (per agent), where it runs, flags, folder
 *  (per machine) and machine. Only the name and the message start empty. */
const remembered = (k) => { try { return localStorage.getItem(`cue.${k}`); } catch { return null; } };
const remember = (k, v) => { try { localStorage.setItem(`cue.${k}`, v); } catch {} };
let newAgent = remembered("newAgent") || "claude";
let agentsAvail = null;            // the agents installed on this Mac (asked once)

// ---------- Sessions view: every live session as a map of project tiles ----------
const homeless = (p) => String(p || "").replace(/^\/Users\/[^/]+(?=\/|$)/, "~");
const baseName = (p) => String(p || "").split("/").filter(Boolean).pop() || "";
const lastSaid = (s) => [...(s.thread || [])].reverse().find((e) => e.role === "agent")?.text || "";
/** relay names tabs "[Exec] web-lines-cleanup", "[Lead] relay-tmux-backend": the LEAD / EXEC tag says that already. */
const bareName = (n) => String(n || "").replace(/^\[(?:ex-)?(?:Exec|Lead)\]\s*/i, "");

/** Cue's mark (the app icon without its tile): an open C around a dot. */
/** The mark at 14px, and a terminal (a prompt in a window), for the "Runs in" choice. */
const CUE_MARK_SM = `<svg width="14" height="14" viewBox="200 200 624 624" aria-hidden="true"><circle cx="512" cy="512" r="250" fill="none" stroke="currentColor" stroke-width="92" stroke-dasharray="1180 400" stroke-linecap="round" transform="rotate(40 512 512)"/><circle cx="512" cy="512" r="84" fill="currentColor"/></svg>`;
/** A bare prompt: what runs an action (a command), told apart from Listen's speaker. */
const RUN_ICON = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 17l6-5-6-5M12 19h8"/></svg>`;
const TERM_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2.5" y="4" width="19" height="16" rx="3"/><path d="M7 9.5l3 2.5-3 2.5M12.5 15H17"/></svg>`;
const CUE_MARK = `<svg width="28" height="28" viewBox="200 200 624 624" aria-hidden="true"><circle cx="512" cy="512" r="250" fill="none" stroke="currentColor" stroke-width="92" stroke-dasharray="1180 400" stroke-linecap="round" transform="rotate(40 512 512)"/><circle cx="512" cy="512" r="84" fill="#B5C27A"/></svg>`;
// Drafts survive a restart: each session's unsent text and images are saved in ~/.cue/cue.db.
const savedDrafts = {};     // key -> what the database last got, so only changes are written
let draftTimer = null;
const draftJson = (d) => JSON.stringify({ text: d?.text || "", images: (d?.images || []).map(({ name, mime, data }) => ({ name, mime, data })) });
function saveDrafts() {
  clearTimeout(draftTimer);
  draftTimer = setTimeout(() => {
    const keys = new Set([...Object.keys(drafts), ...Object.keys(savedDrafts)].filter((k) => k.startsWith("s:")));
    for (const key of keys) {
      const now = draftJson(drafts[key]);
      if (now === (savedDrafts[key] ?? draftJson(null))) continue;
      savedDrafts[key] = now;
      const { text, images } = JSON.parse(now);
      invoke("set_draft", { key, text, images }).catch(() => {});
    }
  }, 300);
}
async function loadDrafts() {
  const saved = await invoke("get_drafts").catch(() => ({}));
  for (const [k, d] of Object.entries(saved || {})) {
    drafts[k] = { text: d.text || "", choices: {}, images: d.images || [] };
    savedDrafts[k] = draftJson(drafts[k]);
  }
  // Drafts from before the database lived in this window's storage: move them over once.
  try {
    const old = JSON.parse(localStorage.getItem("cue.drafts") || "{}");
    for (const [k, d] of Object.entries(old)) if (!drafts[k]) drafts[k] = { text: d.text || "", choices: {}, images: d.images || [] };
    localStorage.removeItem("cue.drafts");
    if (Object.keys(old).length) saveDrafts();
  } catch { /* nothing to move */ }
}
