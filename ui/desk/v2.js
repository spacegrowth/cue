// Cue UI desk — the new Desk page (#628, version 2 of the #607 mocks), tried with "Try the new Desk page" in Desk
// settings: off by default, remembered across launches. Same data and the same parts as today's page (desk/view.js
// deskView draws the one or the other): no sessions column; the Desks as pills in its head with Desk settings beside
// them, a drawer holding them (How work lands, Ask me before, this Desk's name and lead, its sessions, old working
// folders); Chats ▾ on the lead's chat; List | Progress in the Items head; an item's page led by its six steps and who
// is on it. Everything drawn here sits under `.dk-board.dk-v2` (styles/desk-v2.css).
// Loaded after desk/home.js (index.html, mini.html).

const DESK_V2_KEY = "cue.deskV2";
/** The new Desk page is on. Kept in this window's storage, like the Desk's other choices of look. */
let deskV2 = (() => { try { return localStorage.getItem(DESK_V2_KEY) === "1"; } catch { return false; } })();
/** Desk settings' drawer: { project, lands } while open (`lands`: How work lands opened with it, closed with it). */
let deskV2Set = null;
/** Chats ▾ on the lead's chat is open. */
let deskV2Chats = false;
/** The item whose ⋯ on its page is open. */
let deskV2More = null;
/** The Items card shows its list ("list") or each item's steps ("progress"). */
let deskV2Mode = "list";

/** Turn the new page on or off: at once, not with Save. Desk settings open stay open, as today's dialog. */
function deskV2Turn(on) {
  deskV2 = on;
  try { localStorage.setItem(DESK_V2_KEY, on ? "1" : "0"); } catch { /* not kept: on for this run only */ }
  if (typeof lands !== "undefined" && lands) lands.inDrawer = false;
  deskV2Set = null;
  deskV2Chats = false;
  deskV2More = null;
  deskProgShown = false;
  renderMain();
}
/** The switch: on today's Desk settings by its title (the screen stays as short as it was), in the new page's drawer
 *  as a section of its own. */
const deskV2Toggle = () => `<button class="dk2-sw${deskV2 ? " on" : ""}" role="switch" aria-checked="${deskV2}" data-desk2-try><span class="dk2-knob" aria-hidden="true"></span>Try the new Desk page</button>`;
const deskV2Try = () => `<div class="ld-sec dk2-try" data-ld-sec="try"><div class="set-group ld-h">Desk page</div>${deskV2Toggle()}
  <div class="ld-hint ld-sub">Changes at once. Turn it off to go back to today's page.</div></div>`;
if (typeof document !== "undefined") {
  document.addEventListener("click", (e) => {
    if (!e.target.closest?.("[data-desk2-try]")) return;
    e.stopPropagation();
    deskV2Turn(!deskV2);
  }, true);
}

const DESK_V2_GEAR = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/></svg>`;
const DESK_V2_CARET = `<svg class="dk2-caret" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>`;
const DESK_V2_DOTS = `<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/></svg>`;

/** Desk `d`'s sessions other than its lead's, in its folder. */
function deskV2Sessions(d) {
  const leadSid = deskLead(d)?.sid;
  return (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && s.session_id && s.session_id !== leadSid && deskHasSession(d, s.session_id));
}

// ---------- the head: the Desks, + New Desk, Desk settings ----------

/** The window head: a pill per Desk (what waits on you there, or what's open), + New Desk, Desk settings. */
function deskV2Head(project) {
  if (deskRecent[0] !== project && deskStepped !== project) deskRecentPut(project);
  const pill = (d) => {
    const on = d.project === project, need = deskWaitingItems(d).length, open = d.items.filter((i) => !["done", "dropped"].includes(i.state)).length;
    const sub = need ? `${need} waiting` : open ? `${open} open` : "nothing open";
    return `<button class="dk2-pill${on ? " on" : ""}${need ? " need" : ""}" data-desk-go="${esc(d.project)}" aria-current="${on}"><span class="dk2-pn">${esc(deskName(d))}</span><span class="dk2-ps">${sub}</span></button>`;
  };
  const set = deskV2Set?.project === project;
  return `<div class="dk2-head"><div class="dk2-pills">${deskSorted().map(pill).join("")}<button class="dk2-pill dk2-new" data-desk-new-desk><span class="dk2-pn">+ New Desk</span></button></div>
    <button class="btn small dk2-setb${set ? " on" : ""}" data-desk2-set="${esc(project)}" aria-expanded="${set}">${DESK_V2_GEAR}<span>Desk settings</span></button></div>`;
}

// ---------- Desk settings, a drawer ----------

/** Open Desk settings for `project`, with How work lands when the Desk has it. */
function deskV2Open(project) {
  const d = deskOf(project);
  deskV2Chats = false;
  deskV2Set = { project, lands: !!d?.lands, pick: deskPick };
  if (d?.lands) {
    landsChange(d.id || d.project);
    if (lands) lands.inDrawer = true;
  }
  renderMain();
}
/** Close it (How work lands with it). */
function deskV2Close() {
  deskV2Set = null;
  if (typeof lands !== "undefined" && lands?.inDrawer) lands = null;
}
/** The drawer, over the right of the Desk; a click outside it, its Esc or Cancel closes it (with unsaved changes it
 *  asks first, as today's dialog does). */
function deskV2Drawer(project) {
  if (!deskV2Set) return "";
  const d = deskOf(project);
  // Gone from under it: another Desk, How work lands saved or cancelled, handed to another session.
  if (deskV2Set.project !== project || !d || (deskV2Set.lands && !lands) || (deskPick === project && deskV2Set.pick !== project)) { deskV2Close(); return ""; }
  const l = lands?.inDrawer ? d.lands : null;
  const shut = l ? 'data-ld="close"' : "data-desk2-shut";
  const sec = (key, head, body, sub = "") => `<div class="ld-sec" data-ld-sec="${key}"><div class="set-group ld-h">${head}</div>${sub ? `<div class="ld-hint ld-sub">${sub}</div>` : ""}${body}</div>`;
  const parts = l ? l.parts.map((p) => (p.key === "how" ? landsAtOnce(l) + landsPart(p, l) : landsPart(p, l))).join("") : "";
  // This Desk: its name (✎ renames it in place) and the session that runs it.
  const renaming = pjRenaming === project;
  const name = renaming
    ? `<input class="pj-name-in" data-text="pjname:${esc(project)}" maxlength="40" placeholder="${esc(project)}" aria-label="Rename this Desk" spellcheck="false" value="${esc(draft(`pjname:${project}`).text)}"/>`
    : `<b>${esc(deskName(d))}</b>`;
  const lead = deskLead(d);
  const desk = `<div class="dk2-rows"><div class="dk2-row"><span class="dk2-k">Name</span><span class="dk2-v">${name}</span>${renaming ? "" : `<button class="btn small" data-pjren="${esc(project)}">Rename</button>`}</div>
    <div class="dk2-row"><span class="dk2-k">Run by</span><span class="dk2-v">${lead ? esc(deskLeadName(d)) : "Nobody yet"}</span><button class="btn small" data-desk-change>${lead ? "Hand to another session…" : "Choose a session…"}</button></div></div>`;
  // Its sessions, as the old left column listed them, with Close idle and + Add session.
  const o = deskOpen, sessions = deskV2Sessions(d).sort((a, b) => (b.since_ms || 0) - (a.since_ms || 0));
  const x = { project, d, o, leadSid: lead?.sid, lit: (sid) => (o?.kind === "chat" || o?.kind === "holder") && o.sid === sid };
  const working = sessions.filter((s) => deskSessWithHelpers(s)[0] === "").length;
  const stale = sessions.filter((s) => deskSessOlder(d, s) && deskSessStale(s)).map((s) => s.session_id);
  const picking = deskAddingSession === project;
  const tools = `<div class="dk2-stools"><button class="btn small" data-desk-close-idle="${esc(stale.join(" "))}"${stale.length ? "" : " disabled"}>Close idle${stale.length ? ` (${stale.length})` : ""}</button>
    <span class="dk-add-anchor"><button class="btn small" data-desk-add-session="${esc(project)}" aria-expanded="${picking}">+ Add session</button>${picking ? deskSessionsPicker(project, d, lead?.sid) : ""}</span></div>`;
  const rows = sessions.length ? sessions.map((s) => deskSessionsRow(x, s, false)).join("") : `<div class="dk-empty">No other sessions on this Desk.</div>`;
  const body = (l ? landsAskBefore() + sec("lands", "How work lands", `${l.why ? `<div class="ld-why">${esc(l.why)}</div>` : ""}<div class="ld-rows">${parts}${landsModel()}</div>`) + landsMach() : sec("ask", "Ask me before", deskSettingsLine(d) || `<div class="ld-hint">Set once a lead runs this Desk.</div>`))
    + sec("desk", "This Desk", desk)
    + sec("sessions", `Sessions on this Desk · ${sessions.length} · ${working} working`, `${tools}<div class="dk2-slist">${rows}</div>`)
    + (l ? sec("old", "Old working folders", `<div class="ld-rows">${landsOld()}</div>`) : "")
    + deskV2Try();
  const foot = l ? landsFoot().replace('<span class="ld-foot-t" aria-live="polite"></span>', '<span class="ld-foot-t" aria-live="polite">Changes wait until you press Save</span>')
    : `<div class="ld-foot"><span class="ld-foot-t">Changes wait until you press Save</span><span class="grow"></span><button class="btn" data-desk2-shut>Cancel</button><button class="btn primary" disabled>Save</button></div>`;
  return `<div class="dk2-scrim" ${l ? 'data-ld="scrim"' : "data-desk2-shut"}></div><div class="sheet ld-dialog dk2-drawer" role="dialog" aria-labelledby="dk2-set-title">
    <div class="sheet-head"><h3 class="move-title" id="dk2-set-title">Desk settings</h3><span class="grow"></span><kbd class="ld-esc" ${shut} role="button" aria-label="Close">Esc</kbd></div>
    <div class="sheet-body">${body}</div>${foot}</div>`;
}

// ---------- Chats ▾ on the lead's chat ----------

/** Chats ▾: the lead and each session of the Desk, a new session, a running one added to the Desk. */
function deskV2ChatsMenu(d) {
  const project = d.project;
  const item = (attrs, name, sub = "") => `<button class="dk2-mi" role="menuitem" ${attrs}><span class="dk2-mn">${esc(name)}</span>${sub ? `<span class="dk2-ms">${esc(sub)}</span>` : ""}</button>`;
  const lead = deskLead(d), picking = deskAddingSession === project && !deskV2Set;
  const cwd = (lead && sessionOf(lead.sid)?.cwd) || deskWorkspace(d) || "";
  const menu = deskV2Chats ? `<div class="dk2-menu" role="menu">${lead ? item("data-desk-lead-chat", deskLeadName(d), "The Desk lead") : ""}
    ${deskV2Sessions(d).map((s) => item(deskChatAttrs(s.session_id, d), deskHolderName(s.session_id), deskSessWithHelpers(s)[1])).join("")}
    <div class="dk2-msep"></div>${item(`data-lv="new" data-spot="1" data-cwd="${esc(cwd)}"`, "New session")}
    <span class="dk-add-anchor">${item(`data-desk-add-session="${esc(project)}"`, "Add a running session to this Desk…")}${picking ? deskSessionsPicker(project, d, lead?.sid) : ""}</span></div>` : "";
  return `<span class="dk2-chats"><button class="btn small dk2-chatsb" data-desk2-chats aria-haspopup="menu" aria-expanded="${deskV2Chats}">Chats${DESK_V2_CARET}</button>${menu}</span>`;
}

// ---------- the Items card: List | Progress, and the groups' order ----------

/** The groups top to bottom: Waiting on you, Ready to check, In progress, Landing, Waiting to install, Queue,
 *  Needs resume and Paused (still active, so above the often folded Done), Parked, Done; the rest after them. */
const DESK_V2_ORDER = ["you", "check", "working", "landing", "install", "queued", "resume", "paused", "held", "done", "kept", "extra"];
const deskV2Order = (k) => DESK_V2_ORDER.indexOf(k);
/** List | Progress in the Items head: one at a time. */
function deskV2ListSwitch() {
  const b = (k, label) => `<button class="dk2-seg-b${deskV2Mode === k ? " on" : ""}" data-desk2-mode="${k}" aria-pressed="${deskV2Mode === k}">${label}</button>`;
  return `<span class="dk2-seg" role="group" aria-label="Show">${b("list", "List")}${b("progress", "Progress")}</span>`;
}
/** Progress in the list's place: each item under way with its six steps, in the list's order (desk/progress.js). */
function deskV2Progress(g) {
  const rows = [...g.waiting, ...g.review, ...g.working, ...g.landing, ...g.install, ...g.resume, ...g.paused];
  const later = g.queued.length + g.held.length;
  const head = `<div class="dk2-pghead">${rows.length} under way${later ? ` · ${later} not started` : ""}</div>`;
  return `<div class="dk2-prog">${head}${rows.length ? rows.map(deskProgRow).join("") : `<div class="dk-empty">Nothing under way.</div>`}</div>`;
}

// ---------- an item's page: its steps, who is on it, its buttons ----------

/** What leads an item's page: its six steps with their times, who is on it, ‹ Lead chat, Open its chat, its one main
 *  button and ⋯ (No, its Chat, Record, Decisions and Files). */
function deskV2ItemTop(i, d, tab) {
  const look = deskProgLook(i), at = deskProgAt(i), st = deskProgStamps(i);
  const steps = deskProgSteps(i).map(({ state, took }, k) => `<div class="dk2-step ${state}${state === "cur" ? ` ${look}` : ""}">${deskProgMark(state, look)}<span class="dk2-sn">${DESK_PROG_STEPS[k]}</span><span class="dk2-stt">${took}</span></div>`).join("");
  // Who is on it: its session, the crew member when it's one, and the step it's at now. Nothing Cue doesn't have.
  const sid = deskHolderSid(i), crew = i.holder?.kind === "crew" ? i.holder.name : "";
  const who = sid ? deskHolderName(sid, i.holder?.kind === "crew" ? "" : i.holder?.name) : i.holder?.name || "";
  const live = at >= 0 && !["done", "dropped"].includes(i.state) ? [DESK_PROG_STEPS[at], i.act?.now ? esc(i.act.now) : "", st[at] ? `<span data-ago="${st[at]}">${ago(st[at])}</span>` : ""].filter(Boolean).join(" · ") : "";
  const line = (k, v) => (v ? `<div class="dk2-who-r"><span class="dk2-k">${k}</span><span class="dk2-v">${v}</span></div>` : "");
  const whoBox = who || crew || live ? `<div class="dk2-who"><div class="dk2-who-h">Who is on it</div>${line("Session", esc(who))}${line("Crew", esc(crew))}${line("Now", live)}</div>` : "";
  // Its buttons: back to the lead's chat (filled), its session's chat, the one thing to do now; the rest under ⋯.
  const r = i.state === "needs_you" ? deskItemRequest(i) : null, perm = r?.kind === "permission";
  const main = perm ? `<button class="btn small primary" data-desk-decision="allow" data-id="${esc(r.id)}"${answering.has(r.id) ? " disabled" : ""}>Allow</button>`
    : i.state === "needs_you" ? "" : i.state === "held" ? `<button class="btn small primary" data-desk-act="start_now" data-id="${esc(i.id)}">Start</button>` : deskLooksTop(i);
  const open = deskV2More === i.id;
  const tabB = (k, label) => (tab === k ? "" : `<button class="dk2-mi" role="menuitem" data-desk-itab="${k}" data-id="${esc(i.id)}"><span class="dk2-mn">${label}</span></button>`);
  const more = `${perm ? `<button class="dk2-mi" role="menuitem" data-desk-decision="deny" data-id="${esc(r.id)}"><span class="dk2-mn">No, don't allow it</span></button><div class="dk2-msep"></div>` : ""}${tabB("thread", "Chat")}${tabB("record", "Record")}${tabB("decisions", "Decisions")}${tabB("files", "Files")}`;
  const acts = `<div class="dk2-acts"><button class="btn small primary dk2-lead" data-desk-lead-chat>‹ Lead chat</button>${sid && sessionOf(sid) ? `<button class="btn small" ${deskChatAttrs(sid, d)}>Open its chat</button>` : ""}${main}
    <span class="dk2-more"><button class="btn small dk2-moreb" data-desk2-more="${esc(i.id)}" aria-haspopup="menu" aria-expanded="${open}" aria-label="More">${DESK_V2_DOTS}</button>${open ? `<div class="dk2-menu" role="menu">${more}</div>` : ""}</span></div>`;
  return `<div class="dk2-top"><div class="dk2-steps">${steps}</div>${whoBox}${acts}</div>`;
}
/** A session's chat opened on the new page: ‹ Lead chat, and the item it's on a click away ("Its item ›"). */
function deskV2ChatBack(sid, from, d) {
  const i = (from?.kind === "item" && deskItem(d.project, from.id)) || deskSessItem(d, sid);
  return {
    back: `<button class="dk-back dk2-lead" data-desk-lead-chat>‹ Lead chat</button>`,
    its: i ? `<button class="btn small dk2-its" data-desk-chk-item="${esc(i.id)}">Its item <b>${esc(deskNum(i))}</b> ›</button>` : "",
  };
}

// ---------- clicks and keys ----------

deskClicks({
  desk2Set: (el) => { if (deskV2Set) { deskV2Close(); renderMain(); } else deskV2Open(el.dataset.desk2Set); },
  desk2Shut: () => { deskV2Close(); renderMain(); },
  desk2Chats: () => { deskV2Chats = !deskV2Chats; if (!deskV2Chats && deskAddingSession) deskAddingSession = null; renderMain(); },
  desk2Mode: (el) => { deskV2Mode = el.dataset.desk2Mode === "progress" ? "progress" : "list"; renderMain(); },
  desk2More: (el) => { deskV2More = deskV2More === el.dataset.desk2More ? null : el.dataset.desk2More; renderMain(); },
});
/** A click on the page (desk/act.js deskClick): an open menu closes on a click outside it, or on one of its choices. */
function deskV2Outside(e) {
  if (!deskV2) return;
  const t = e.target;
  const pick = t.closest?.(".dk2-menu [role=menuitem]") && !t.closest?.("[data-desk-add-session]");
  if (deskV2Chats && (!t.closest?.(".dk2-chats") || pick)) { deskV2Chats = false; queueMicrotask(renderMain); }
  if (deskV2More && (!t.closest?.(".dk2-more") || pick)) { deskV2More = null; queueMicrotask(renderMain); }
}
/** Esc on the page: an open menu closes, then Desk settings (How work lands asks first when you changed it). */
function deskV2Escape() {
  if (!deskV2) return false;
  if (deskV2Chats || deskV2More) { deskV2Chats = false; deskV2More = null; renderMain(); return true; }
  if (!deskV2Set) return false;
  if (typeof lands !== "undefined" && lands?.inDrawer) landsEscape();
  else { deskV2Close(); renderMain(); }
  return true;
}
