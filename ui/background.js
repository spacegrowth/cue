// ---------- The Agents and Scripts tabs: a session's background work, all of it ----------
// Beside Actions, each shown once the session has that kind of work: Agents (its background agents and
// teammates) and Scripts (the shell commands it ran in the background). Each lists one row per piece of work
// (chat.js bgRow), what's running first, then a teammate waiting, then what's over, newest first; the tab
// shows a live dot and how many run while any do. A click opens it in full (chat.js helperPane); Back comes
// back here. The live feed keeps a session's newest few; "Show earlier" reads its whole transcript for the
// rest, only once asked, and an earlier one is read in full only when it's opened.
const BG_TABS = [["agents", "Agents", (h) => !h.script], ["scripts", "Scripts", (h) => !!h.script]];
const bgTabOn = new Map();     // sid → "agents" | "scripts": the tab it shows
const bgEarlier = new Map();   // sid → { loading, list, err }: the session's whole history, once asked for
const bgOpened = new Map();    // "sid|id" → an earlier one in full (null while it's read)

/** The open tab's name ("Agents"), for its full view's Back; "" when neither is open. */
const bgTabName = (sid) => BG_TABS.find(([k]) => k === bgTabOn.get(sid))?.[1] || "";

/** The tabs' buttons: each only for a session with that kind of work (or the tab open), with a live dot and
 *  how many run while any do. */
function bgTabBtn(sid) {
  const all = bgAll(sid);
  return BG_TABS.map(([k, label, is]) => {
    const hs = all.filter(is), on = bgTabOn.get(sid) === k;
    if (!hs.length && !on) return "";
    const n = hs.filter(helperRunning).length;
    return `<button class="stab ${on ? "on" : ""}" data-bg-tab="${k}" data-sid="${esc(sid)}">${label}${n ? `<span class="stab-n"><span class="dot-live"></span>${n} running</span>` : ""}</button>`;
  }).join("");
}
/** Where the open tab's page goes, in place of the chat (extensions.js placeExt hides the chat under it);
 *  not while one is open in full. */
const bgSlot = (sid) => (sid && bgTabOn.has(sid) && !helperViewed(sid) ? `<div class="bg-slot" data-bg-slot>${bgPage(sid, bgTabOn.get(sid))}</div>` : "");

/** The session's background work: the live feed's, with the earlier ones its history adds (live wins). */
function bgAll(sid) {
  const live = helpersOf(sid), ids = new Set(live.map((h) => h.id));
  return [...live, ...(bgEarlier.get(sid)?.list || []).filter((h) => !ids.has(h.id))];
}
/** Running first (oldest first), then a teammate waiting, then what's over (newest first). */
const bgOrder = (a, b) => (helperRunning(b) - helperRunning(a)) || (!!b.teammate - !!a.teammate) || (helperRunning(a) ? a.started_ms - b.started_ms : (b.finished_ms || b.last_ms || b.started_ms) - (a.finished_ms || a.last_ms || a.started_ms));

function bgPage(sid, tab) {
  const [, label, is] = BG_TABS.find(([k]) => k === tab) || BG_TABS[0];
  const rows = bgAll(sid).filter(is).sort(bgOrder).map((h) => bgRow(sid, h, true)).join("");
  const e = bgEarlier.get(sid);
  const more = e?.list ? "" : e?.loading ? `<div class="bg-more dim">Reading the session's transcript…</div>` : `<button class="bg-more" data-bg-earlier="${esc(sid)}">Show earlier</button>${e?.err ? `<span class="dim"> · couldn't read it: ${esc(e.err)}</span>` : ""}`;
  return `<div class="bg-page">${rows || `<div class="ac-empty">No ${label.toLowerCase()} in this session yet.</div>`}${more}</div>`;
}

/** An earlier one (not in the live feed) for its full view: read once, when it's opened. */
function bgFull(sid, id) {
  if (!(bgEarlier.get(sid)?.list || []).some((h) => h.id === id)) return null;
  const k = `${sid}|${id}`;
  if (bgOpened.has(k)) return bgOpened.get(k);
  bgOpened.set(k, null);
  invoke("session_bg_helper", { sessionId: sid, id })
    .then((h) => { if (h) { bgOpened.set(k, h); renderMain(); } else bgOpened.delete(k); })
    .catch(() => bgOpened.delete(k));
  return null;
}

function bgShowEarlier(sid) {
  bgEarlier.set(sid, { loading: true });
  renderMain();
  invoke("session_bg_history", { sessionId: sid })
    .then((list) => bgEarlier.set(sid, { list: list || [] }))
    .catch((x) => bgEarlier.set(sid, { err: String(x || "unknown") }))
    .finally(renderMain);
}

document.addEventListener("click", (e) => {
  const tab = e.target.closest?.("[data-bg-tab]");
  if (tab) {
    e.stopPropagation();
    const sid = tab.dataset.sid;
    actTabOn.delete(sid);
    extTabOf.delete(sid);
    aboutTabOn.delete(sid);
    bgTabOn.set(sid, tab.dataset.bgTab);
    helperView = null;
    return renderMain();
  }
  const more = e.target.closest?.("[data-bg-earlier]");
  if (more) { e.stopPropagation(); bgShowEarlier(more.dataset.bgEarlier); }
}, true);
