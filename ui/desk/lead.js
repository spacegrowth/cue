// Cue UI desk/lead — who runs a Desk, and adding to it (ui/desk/desk.js draws the rest). Each project's Desk has one lead:
// a normal session you pick ("Who runs this Desk?") or a new one Cue starts; you tell it, it does the work or hands it
// out. Here: the picker (zero, one or several sessions; "Keep <name>" when changing), ⋯ in the head strip, the
// ended lead's line, "Give to…" on an item no one holds, and the quick-add line at the top of Items (Enter adds;
// several pasted lines add one item a line; ⌘N focuses it). An item you add has no kind: the lead says whether it's
// a bug, a feature or a task as it routes it, and writes its You'll see (#382, #400). Commands: desk_set_lead, desk_add, desk_give.

/** The project whose lead is being changed (the picker shows over the lead's chat, with Keep), or null. */
let deskPick = null;
/** A lead being set: the session's id ("" for a new one) while desk_set_lead runs, or null. */
let deskLeadBusy = null;
/** The item whose "Give to…" menu is open: { id, at: "row" | "pane" }, or null. */
let deskGiveFor = null;
/** The lead's lines in its chat you opened (their word and body): shut by default, a redraw keeps them open (#157). */
const deskLnOpen = new Set();
/** The title of the last ITEM line drawn: a ROUTE "#new" after it is that item (parse.rs reads it so). */
let deskSaidItem = "";

// ---------- the lead ----------

/** A message another Desk's session sent this lead (#472; #640 option B): the band in that Desk's colour
 *  (deskBand), headed by the Desk, the session and the item it holds there. Null for any other message: one from the
 *  lead's own Desk, from no Desk, or in a chat that isn't a lead's (chat/render.js draws those as today). The sender
 *  is found by its session id (the hub notes it when the message came through its hook), else by its name. */
function deskPeerBand(sid, e) {
  const own = (state.desk?.projects || []).find((d) => deskLead(d)?.sid === sid);
  if (!own) return null;
  const from = e.from_sid || (state.sessions || []).find((x) => e.from && nameOf(x.session_id, x.project) === e.from)?.session_id;
  const d = from && from !== sid ? deskForSession(from) : null;
  if (!d || d.project === own.project) return null;
  const it = deskSessItem(d, from);
  return deskBand({ d, label: deskName(d), who: nameOf(from, e.from) || e.from || "a session", id: it?.id, title: it?.title, at: e.at_ms, text: e.text,
    foot: `<span class="dk-band-st">${esc(deskLeadName(own) || "The lead")} answers it below; you don't need to.</span>` });
}
/** The head of a band Cue sent this lead for another Desk's lead (#540, desk/tell.rs `band_text`): its Desk's name, its
 *  lead's, the item it's about; then the words, and the line on how to answer, which the band leaves out. */
const DESK_TOLD = /^\[From another Desk: (.+?) · lead (.*?)(?: · about #([A-Za-z0-9]+))?\]\n([\s\S]*?)(?:\n\n\(To answer: TELL DESK: [^\n]*\))?$/;
/** A TELL DESK, TELL DESKS or HOLD from another Desk's lead, as Cue delivered it to this lead (#540, #640 option B):
 *  the band in that Desk's colour, its lead as the sender, and whether this lead has answered it (the hub's bands,
 *  desk/tell.rs `Band.answered`). Null for any other message, or outside a lead's chat. */
function deskToldBand(sid, e) {
  const desks = state.desk?.projects || [];
  const own = desks.find((d) => deskLead(d)?.sid === sid);
  const m = own && DESK_TOLD.exec(deskSaid(e.text).trim());
  if (!m) return null;
  const d = desks.find((x) => x.project !== own.project && deskName(x) === m[1]) || null;
  const from = d && deskLead(d)?.sid, words = m[4].trim();
  const b = deskBandOf(own, (x) => x.to === own.project && (!d || x.from === d.project) && x.text.trim() === words, e.at_ms);
  const me = esc(deskLeadName(own) || "the lead");
  const st = b?.answered ? `<span class="dk-band-ok"><i></i>Answered by ${me} · ${ago(b.answered)}</span>`
    : b && !b.ask ? `<span class="dk-band-st">A heads-up; nothing to answer</span>` : `<span class="dk-band-st">${me} answers it below; you don't need to.</span>`;
  const it = d && m[3] ? (d.items || []).find((i) => deskIdIs(i, m[3])) : null;
  return deskBand({ d, label: m[1], who: (from && nameOf(from, m[2])) || m[2] || "its lead", id: m[3], title: it?.title, at: e.at_ms, text: words, foot: st });
}
/** The band Desk `own` sent or got that `ok` picks (the hub's bands, desk/hub/core/view.rs), the one nearest `at`. */
function deskBandOf(own, ok, at) {
  const all = (state.desk?.projects || []).flatMap((d) => d.bands || []).filter(ok);
  return all.sort((a, b) => Math.abs(a.at - at) - Math.abs(b.at - at))[0] || null;
}
/** A lead's own TELL DESK / TELL DESKS line in its reply (#640): the same band, in the colour of the Desk it went to,
 *  "To Loops Desk", the words, and whether that lead has answered yet (the hub's bands). "" when no Desk by that name
 *  is loaded and the hub kept no band of it (the line stays as the lead wrote it). */
function deskLineTell(k) {
  const { m, body } = k;
  const desks = state.desk?.projects || [];
  const many = /DESKS$/.test(m[1]);
  const to = many ? "" : body.slice(0, body.indexOf("|")).trim().replace(/^["`*]+|["`*]+$/g, "");
  const words = many ? body : body.slice(body.indexOf("|") + 1).trim();
  if (!many && (!body.includes("|") || !to)) return "";
  if (!words) return "";
  const named = (d) => d.project === to || deskName(d).toLowerCase() === to.toLowerCase();
  const sent = desks.flatMap((d) => d.bands || []).filter((b) => b.text.trim() === words && (many || desks.some((d) => d.project === b.to && named(d)) || b.to === to));
  const targets = many ? [...new Set(sent.map((b) => b.to))].map((p) => desks.find((d) => d.project === p)).filter(Boolean) : desks.filter(named).slice(0, 1);
  const d = targets.length === 1 ? targets[0] : null;
  if (!many && !d && !sent.length) return "";
  const label = many ? (targets.length ? targets.map(deskName).join(", ") : "every other Desk on this code") : d ? deskName(d) : to;
  const b = sent.at(-1);
  const answered = sent.filter((x) => x.answered);
  const st = !b ? `<span class="dk-band-st">Sending…</span>`
    : !b.ask ? `<span class="dk-band-st">A heads-up; nothing to answer</span>`
    : answered.length === sent.length ? `<span class="dk-band-ok"><i></i>Answered${many ? "" : ` by ${esc(d ? deskLeadName(d) || "its lead" : "its lead")}`} · ${ago(Math.max(...answered.map((x) => x.answered)))}</span>`
    : `<span class="dk-band-st">Not answered yet${answered.length ? ` · ${answered.length} of ${sent.length} answered` : ""}</span>`;
  const item = b?.item, it = item ? desks.flatMap((x) => x.items || []).find((i) => deskIdIs(i, item)) : null;
  return deskBand({ d, label, to: true, id: item, title: it?.title, at: b?.at ?? null, text: words, foot: st });
}
/** The band between Desks (#640 option B): across the whole chat in Desk `d`'s colour (its project's, styles
 *  desk-lead.css .dk-band; the app's own when `d` isn't loaded), headed by a tile and the Desk ("Loops Desk", or "To
 *  Loops Desk" for one this lead sent, `to`), `who` sent it, the item it's about (`id`, its `title`), its age; then
 *  the words; then the foot (`foot`: whether it was answered) and Open <Desk>. */
function deskBand({ d, label, to = false, who = "", id = "", title = "", at = null, text = "", foot = "" }) {
  const name = `${esc(label)}${/desks?$/i.test(label) || label.includes(",") ? "" : " Desk"}`;
  const tile = `<span class="dk-band-tile" aria-hidden="true"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 8h14l-3-3"/><path d="M20 16H6l3 3"/></svg></span>`;
  const sub = [who ? `<b class="dk-band-who">${esc(who)}</b>` : "", id ? `<span class="dk-band-sub">#${esc(id)}${title ? ` ${esc(title)}` : ""}</span>` : ""].filter(Boolean).join(" · ");
  const head = `${tile}<span class="dk-band-desk">${to ? "To " : ""}${name}</span>${sub ? `<span class="dk-band-by">· ${sub}</span>` : ""}${at != null ? `<span class="dk-band-age">${ago(at)}</span>` : ""}`;
  const open = d ? `<button class="dk-band-open" data-desk-hand="${esc(d.project)}">Open ${esc(deskName(d))} Desk</button>` : "";
  return `<div class="dk-band${to ? " out" : ""}" data-band="${to ? "to" : "from"}" data-from-desk="${esc(d ? d.project : label)}"><div class="dk-band-h">${head}</div><div class="dk-band-t dk-md">${md(text || "")}</div>${foot || open ? `<div class="dk-band-f">${foot}${open}</div>` : ""}</div>`;
}
/** One message of the family (#501, option B, ui/styles/messages.css): the card on panel-2, its head row (Cue's mark
 *  with `cue`, another Desk's `chip`, who → to, the TAG, the item's chip, the age at the right), then `body` (its
 *  .mg-t / .mg-line / .mg-full rows) and the foot (`foot`: how it went, actions). `kind` names it for the page and
 *  its tests (data-mg). */
function deskMg({ kind, cls = "", attrs = "", cue = false, chip = "", who = "", to = "", tag = "", id = "", age = null, body = "", foot = "" }) {
  const mark = cue && typeof CUE_MARK_SM === "string" ? CUE_MARK_SM.replace("<svg ", '<svg class="mg-mk" ') : "";
  const head = `${mark}${chip}${who ? `<span class="mg-who">${who}</span>` : ""}${to ? `<span class="mg-to">→ ${to}</span>` : ""}`
    + `${tag ? `<span class="mg-tag">${esc(tag[0] + tag.slice(1).toLowerCase())}</span>` : ""}${id}${age != null ? `<span class="mg-age">${ago(age)}</span>` : ""}`;
  return `<div class="mg${cls ? ` ${cls}` : ""}" data-mg="${kind}"${attrs}><div class="mg-h">${head}</div>${body}${foot ? `<div class="mg-f">${foot}</div>` : ""}</div>`;
}
/** An item's chip in a message (messages.css .mg-id): its kind's icon and number ("Bug #12"), a button that opens it. */
const deskMgId = (id, kind, num) => `<button class="mg-id" data-desk-chk-item="${esc(id)}">${deskKindIcon(kind)}${esc(num)}</button>`;

/** Desks a session runs, has work on, or belongs to, in that order. */
function deskForSession(sid) {
  if (!sid) return null;
  const desks = state.desk?.projects || [];
  const held = (d) => (d.items || []).some((i) => i.state !== "done" && i.state !== "dropped" && (i.holder?.sid === sid || i.holder?.lead_sid === sid));
  return desks.find((d) => d.lead?.sid === sid) || desks.find(held) || desks.find((d) => (d.members || []).includes(sid)) || null;
}
const deskWorksFor = (sid) => !!deskForSession(sid);
/** The session's actual Desk role, using the same type icons as the Desk. */
function deskNameMark(sid) {
  const d = deskForSession(sid);
  if (!d) return "";
  const kind = deskSessKind(sid, d), label = DESK_MARKS[kind], name = deskName(d);
  return `<span class="dk-name-ic" role="img" data-desk-type="${kind}" aria-label="${esc(`${label} · ${name}`)}">${deskKindIcon(kind)}</span>`;
}
/** The Desk link under a session's name on the Board: one quiet line, the tray, "Desk", its name, › (#260). */
function deskCardLine(sid) {
  const d = deskForSession(sid);
  if (!d) return "";
  return `<div class="dk-chip-line"><button class="dk-chip" data-desk-hand="${esc(d.project)}">${deskKindIcon("tray")}<span class="dk-chip-k">Desk</span><span class="dk-chip-t">${esc(deskName(d))}</span><span aria-hidden="true">›</span></button></div>`;
}
/** Desk `d`'s lead ({ sid, name, state }), or null. */
const deskLead = (d) => (d?.lead?.sid ? d.lead : null);
/** The lead's name: one you just gave it (before the hub's next update has it), else what the hub named it, else its
 *  session's. */
const deskLeadName = (d) => { const l = deskLead(d); return l ? deskHolderName(l.sid, renamedNow.get(l.sid)?.name || l.name) : ""; };
/** Whether the lead has ended (its session is gone for good). */
const deskLeadEnded = (d) => deskLead(d)?.state === "ended";
/** The lead's session for its chat: a live one, else a parked one's kept messages. */
function deskLeadSession(d) {
  const l = deskLead(d);
  if (!l) return null;
  const s = sessionOf(l.sid);
  if (s) return s;
  const p = (state.parked || []).find((x) => x.session_id === l.sid);
  return p ? { session_id: l.sid, project: p.project, harness: p.harness || "claude", thread: p.thread || [] } : null;
}

/** The project's sessions to run its Desk or take an item, leads first: [{ sid, name, sub, lead }]. */
function deskSessionsOf(project, d = deskOf(project)) {
  const rows = (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && s.session_id && !(state.desk?.projects || []).some((other) => other.project !== project && other.lead?.sid === s.session_id)).map((s) => {
    const sid = s.session_id, crew = deskCrew(sid).length;
    const lead = crew > 0 || deskLead(d)?.sid === sid || d.items.some((i) => i.holder?.kind === "lead" && i.holder.sid === sid);
    return { sid, name: deskHolderName(sid), lead, work: deskHasWork(d, sid), sub: !lead ? "session" : crew ? `lead, crew of ${crew}` : "lead, on its own" };
  });
  return rows.sort((a, b) => (b.lead ? 1 : 0) - (a.lead ? 1 : 0));
}

/** Whether session `sid` has work on Desk `d` (#363): the hub's list of the ones a Desk's lists show (its lead, one
 *  holding an item, or whose item was done in the last day, or busy on the Desk now). */
const deskHasWork = (d, sid) => !d.with_work || d.with_work.includes(sid);
/** Older lines open now, by where they are ("give:<id>", "lead:<desk>", "add:<desk>", "side:<desk>"): shut by default,
 *  and shut again when the window opens. */
const deskOlderOpen = new Set();
/** A list's `rows` with work, each by `draw`; then the rest (idle, no recent item) folded into one "N older" line
 *  (`word`: "cold" on the Desk's left column, #428) that opens in place (data-desk-older). `rows` carry `work`, as
 *  deskSessionsOf's do. */
function deskWithOlder(key, rows, draw, cls = "dk-sfold dk-older", word = "older") {
  const now = rows.filter((r) => r.work), older = rows.filter((r) => !r.work), open = deskOlderOpen.has(key);
  const fold = older.length ? `<button class="${cls}" data-desk-older="${esc(key)}" aria-expanded="${open}">${older.length} ${word} · ${open ? "hide" : "show"}</button>${open ? older.map(draw).join("") : ""}` : "";
  return now.map(draw).join("") + fold;
}

/** A picker row's line (#479): what it is (lead or session), its size, when it was last active ("working now" while it
 *  works), from its card's numbers (insCache, asked for by deskInsAsk); and whether it's warm: at work, or its cache
 *  still alive. With no numbers in yet, a session with work on the Desk counts as warm. */
function deskPickInfo(r) {
  if (typeof deskInsAsk === "function") deskInsAsk(r.sid);
  const c = typeof insCache !== "undefined" ? insCache.get(r.sid) : null, s = sessionOf(r.sid);
  const busy = !!s && typeof deskSessWithHelpers === "function" && deskSessWithHelpers(s)[0] === "";
  const when = busy ? "working now" : c?.last_ms ? `active <span data-ago="${c.last_ms}">${ago(c.last_ms)}</span> ago` : "";
  const line = [esc(r.sub), c?.tokens && typeof insTok === "function" ? esc(insTok(c.tokens)) : "", when].filter(Boolean).join(" · ");
  const warm = busy || (c?.until_ms ? c.until_ms > now() : r.work);
  return { line, warm, at: busy ? Infinity : c?.last_ms || s?.since_ms || 0 };
}

/** The chat card when no one runs the Desk yet, or when changing who does: "Who runs this Desk?", then a button
 *  (no sessions), "Let <name> run it" (one), or a row a session (several): warm ones first, the most recently active
 *  on top, cold ones folded under "Older"; "Start a new lead" under them. Each row is the box's full width: its name
 *  (wrapping, never cut) and its line, with Run it at the right (#479). */
function deskPicker(d) {
  const cur = deskLead(d), rows = deskSessionsOf(d.project, d).filter((r) => r.sid !== cur?.sid), busy = deskLeadBusy;
  const label = (sid, text) => (busy === sid ? "Starting…" : text);
  const dis = busy !== null ? "disabled" : "";
  const fresh = (primary) => `<button class="${primary ? "btn primary" : "dk-link dk-fresh"}" data-desk-lead="" ${dis}>${label("", rows.length || cur ? "Start a new lead" : "Start a lead")}</button>`;
  let body;
  if (!rows.length) body = cur ? fresh(false) : fresh(true);
  else if (rows.length === 1 && !cur) body = `<div class="dk-acts dk-pick1"><button class="btn primary" data-desk-lead="${esc(rows[0].sid)}" ${dis}>${label(rows[0].sid, `Let ${esc(rows[0].name)} run it`)}</button>${fresh(false)}</div>`;
  else {
    const all = rows.map((r) => ({ ...r, ...deskPickInfo(r) })).sort((a, b) => b.at - a.at);
    const row = (r) => `<div class="dk-whorow"><i class="${r.warm ? "" : "cold"}"></i><div class="dk-whotx"><div class="dk-lt">${esc(r.name)}</div><div class="dk-ls">${r.line}</div></div><button class="btn small" data-desk-lead="${esc(r.sid)}" ${dis}>${label(r.sid, "Run it")}</button></div>`;
    const warm = all.filter((r) => r.warm), cold = all.filter((r) => !r.warm), key = `lead:${d.project}`, open = deskOlderOpen.has(key) || !warm.length;
    const fold = cold.length ? `<button class="dk-older" data-desk-older="${esc(key)}" aria-expanded="${open}">Older · ${cold.length}<span class="dk-chev${open ? " open" : ""}" aria-hidden="true">›</span></button>${open ? cold.map(row).join("") : ""}` : "";
    body = `<div class="dk-whos">${warm.map(row).join("")}${fold}</div>${fresh(false)}`;
  }
  const keep = cur ? `<button class="dk-link dk-keep" data-desk-keep>Keep ${esc(deskLeadName(d))}</button>` : "";
  return `<div class="dk-card dk-right"><div class="dk-pick"><h2>Who runs this Desk?</h2><p>A lead takes what you bring here, hands each item to a session, and tells you why.</p>${body}${keep}</div></div>`;
}
/** The chat pane of a lead that has ended: one line, Resume it and Pick another. */
const deskEndedLine = (d) => `<div class="dk-endlead"><p><b>${esc(deskLeadName(d))}</b> has ended. Items stay here.</p><div class="dk-acts"><button class="btn small primary" data-desk-lead="${esc(deskLead(d).sid)}" ${deskLeadBusy !== null ? "disabled" : ""}>${deskLeadBusy === deskLead(d).sid ? "Resuming…" : "Resume it"}</button><button class="btn small" data-desk-change>Pick another</button></div></div>`;

/** Session `sid` (or a new lead: no `sid`) runs project's Desk. With no folder known for a new one, + New's folder
 *  flow opens instead. */
async function deskSetLead(project, sid) {
  if (deskLeadBusy !== null) return;
  deskLeadBusy = sid || "";
  renderMain();
  try {
    await invoke("desk_set_lead", sid ? { project, sid } : { project });
    deskPick = null;
    deskChatEnd = true;
  } catch (e) {
    if (/no folder/i.test(String(e)) && typeof lvAct === "function") { toast("No folder known for this project yet: pick one for the new lead"); sheet = lightbox = null; liveSpot = true; lvAct("new", {}); }
    else toast(`Couldn't set the lead: ${e}`);
  } finally { deskLeadBusy = null; renderMain(); }
}

// ---------- Give to… ----------

/** "Give to…" and, open, its menu: the project's sessions (leads first) and New session. */
function deskGive(i, at) {
  const open = deskGiveFor?.id === i.id && deskGiveFor.at === at;
  const draw = (r) => `<button role="menuitem" data-desk-give-to="${esc(r.sid)}" data-id="${esc(i.id)}"><span>${esc(r.name)}</span><span class="dk-ls">${esc(r.sub)}</span></button>`;
  const rows = open ? deskWithOlder(`give:${i.id}`, deskSessionsOf(i.project), draw, "dk-older-mi") : "";
  const menu = open ? `<span class="dk-givemenu" role="menu">${rows}<button role="menuitem" data-desk-act="start_now" data-id="${esc(i.id)}"><span>New session</span></button></span>` : "";
  // Drawn again while open: where you'd scrolled it, read off the menu still on screen. The draw places the menu
  // before the list's scroll is put back, which moves its button: sized for the wrong place, the browser cuts its
  // scroll short. Placed again once all is back (this microtask), from the place noted here (#308).
  if (open) {
    deskGiveTop = document.querySelector(".dk-givemenu")?.scrollTop ?? null;
    queueMicrotask(() => { deskGivePlace(); deskGiveTop = null; });
  }
  return `<span class="dk-givewrap"><button class="${at === "row" ? "dk-link dk-give" : "btn small"}" data-desk-give="${esc(i.id)}" data-at="${at}" aria-expanded="${open}">Give to…</button>${menu}</span>`;
}
/** The open menu, whole inside the window wherever its button sits (#152): under the button from its left edge, moved
 *  left as far as it must, above it when there's more room there, and scrolling inside when even that is too short.
 *  Fixed to the window so no column cuts it; it follows its button as the list scrolls or the window resizes. */
let deskGiveTop = null;
function deskGivePlace() {
  const m = document.querySelector(".dk-givemenu"), b = m?.previousElementSibling;
  if (!m || !b) return;
  const r = b.getBoundingClientRect(), pad = 8, W = innerWidth, H = innerHeight, top = deskGiveTop ?? m.scrollTop;
  Object.assign(m.style, { position: "fixed", right: "auto", maxWidth: `${W - 2 * pad}px`, maxHeight: "", overflowY: "" });
  const w = m.offsetWidth, h = m.scrollHeight, below = H - r.bottom - 4 - pad, above = r.top - 4 - pad;
  const up = h > below && above > below, room = up ? above : below;
  m.style.left = `${Math.max(pad, Math.min(r.left, W - pad - w))}px`;
  if (h > room) Object.assign(m.style, { maxHeight: `${room}px`, overflowY: "auto" });
  m.style.top = `${up ? r.top - 4 - Math.min(h, room) : r.bottom + 4}px`;
  m.scrollTop = top;   // measuring it unscrolled dropped where you'd scrolled it to (#308)
}
addEventListener("scroll", (e) => { if (deskGiveFor && !e.target?.closest?.(".dk-givemenu")) deskGivePlace(); }, true);
addEventListener("resize", () => { if (deskGiveFor) deskGivePlace(); });
async function deskGiveTo(id, sid) {
  deskGiveFor = null;
  try { await invoke("desk_give", { id, sid }); } catch (e) { toast(`Not given: ${e}`); }
  renderMain();
}

// ---------- the quick-add line ----------

/** The quick-add line's draft key, one a project (what you half-typed stays on its own project's Desk). */
const deskQaKeys = (project = deskSelection()) => ({ line: `desk-qa:${project}` });
/** The line on screen. */
const deskQaEl = (which = "line") => document.querySelector(`.dk-board .dk-qa [data-qa="${which}"]`);
const deskQaFocusEl = (which) => deskQaEl(which)?.focus();

/** At the top of Items: "Add an item…" with its picture button (or paste or drop one on it), and the pictures added.
 *  Nothing more: the lead writes what you'll see, and the session's check (#382, #400). */
function deskQuickAdd() {
  const k = deskQaKeys();
  return `<div class="dk-qa" data-drop="${esc(k.line)}"><div class="dk-qa-row"><input class="dk-qa-in" data-qa="line" data-text="${esc(k.line)}" placeholder="Add an item…" spellcheck="false" value="${esc(draft(k.line).text)}"/><button class="icon-btn" data-attach="${esc(k.line)}" aria-label="Add an image">${IMAGE_ICON}</button></div>${draftChips(k.line)}</div>`;
}
/** Add what's in the line (or `titles`, pasted lines): one item each, of no kind (the lead says); the pictures go
 *  with a single one. The line keeps the cursor for the next. */
async function deskQaAdd(titles = null) {
  const project = deskSelection(), k = deskQaKeys(project);
  const list = (titles || [draft(k.line).text]).map((t) => t.trim()).filter(Boolean);
  if (!list.length) return deskQaFocusEl("line");
  const kind = "", images = titles ? [] : draft(k.line).images;
  if (!titles) { delete drafts[k.line]; saveDrafts(); }
  renderMain();
  deskQaFocusEl("line");
  const failed = [];
  let why = "";
  for (const title of list) {
    try { await invoke("desk_add", { project, kind, title, ...(images.length && { images }) }); }
    catch (e) { failed.push(title); why = String(e); }
  }
  // What didn't go: the first back in the line (when it's empty), and how many.
  if (failed.length) {
    toast(failed.length === 1 ? `Not added: ${why}` : `${failed.length} not added: ${why}`);
    if (!draft(k.line).text) {
      draft(k.line).text = failed[0];
      if (images.length && !draft(k.line).images.length) draft(k.line).images = images;
      saveDrafts();
    }
  }
  renderMain();
  deskQaFocusEl("line");
}
/** Keys in the quick-add line: Enter adds. True when it was the line's. */
function deskQaKey(e, key) {
  if (key !== deskQaKeys().line) return false;
  if (e.key === "Enter") { deskQaAdd(); return true; }
  return false;
}
/** ⌘N on a project's Desk: the quick-add line. */
function deskQaFocus() {
  const el = deskQaEl("line");
  if (!el) return false;
  el.focus();
  return true;
}

/** The Desk's clicks for the lead and adding (desk/clicks.js). */
deskClicks({
  deskLead: (el) => deskSetLead(deskSelection(), el.dataset.deskLead),
  // The picker takes the chat card: whatever's open there closes.
  deskChange: () => { deskPick = deskSelection(); deskGiveFor = null; deskOpen = null; deskDropArm = null; renderMain(); },
  deskKeep: () => { deskPick = null; renderMain(); },
  // An item's ⋯ menu (#669): open and shut as Give to…'s, under the same one-open-at-a-time note.
  deskMore: (el) => { const at = el.dataset.at || "more"; deskGiveFor = deskGiveFor?.id === el.dataset.deskMore && deskGiveFor.at === at ? null : { id: el.dataset.deskMore, at }; renderMain(); },
  deskGive: (el) => { const at = el.dataset.at; deskGiveFor = deskGiveFor?.id === el.dataset.deskGive && deskGiveFor.at === at ? null : { id: el.dataset.deskGive, at }; renderMain(); },
  deskGiveTo: (el) => deskGiveTo(el.dataset.id, el.dataset.deskGiveTo),
  deskWhy: (el) => { const k = el.dataset.deskWhy; if (!deskLnOpen.delete(k)) deskLnOpen.add(k); renderMain(); },
});

if (typeof document !== "undefined") {
  // Leaving the Desk's name box (its Sessions head) keeps what's in it, as Enter does; Esc has closed it already.
  // Checked a moment later: a redraw puts the cursor back in the new box, and that isn't leaving it.
  document.addEventListener("focusout", (e) => {
    if (!e.target?.matches?.(".dk-sh .pj-name-in")) return;
    const key = e.target.dataset.text;
    setTimeout(() => {
      if (!pjRenaming || `pjname:${pjRenaming}` !== key || document.activeElement?.dataset?.text === key) return;
      saveProjName(pjRenaming);
    }, 0);
  }, true);
  // Several lines pasted into the quick-add line: an item a line.
  document.addEventListener("paste", (e) => {
    if (e.target?.dataset?.qa !== "line") return;
    const lines = (e.clipboardData?.getData("text/plain") || "").split(/\r?\n/).map((l) => l.replace(/^\s*(?:[-*•]|\d+[.)])\s+/, "").trim()).filter(Boolean);
    if (lines.length < 2) return;
    e.preventDefault();
    deskQaAdd(lines);
  }, true);
}

// ---------- the lead's own lines, drawn in its chat ----------
/** A card's words in the lead's chat cut to two lines under its one-line head (#452: at most three lines a card), with
 *  More to open the rest in place and Less to fold it (core/md.js reqCut, the permission cards' own). `key`: what keeps it
 *  open through redraws; `html` the words drawn, `text` them plain (how long they are). */
const deskCut = (key, cls, html, text) => reqCut(`dk:${key}`, `<div class="${cls} req-t">${html}</div>`, String(text ?? "").replace(/<[^>]*>/g, ""), 2, 140);
/** A Desk line as desk/parse.rs reads it: its word in capitals, a list mark or bold around it. DECIDED is md's own
 *  block, so not here; ASK is left out of the text (#427): Cue files it as a question, which the chat shows once as a
 *  small card ("Asked: …", your answer beside it once you give it) and the pop-up answers. NOTE (#449) is a small card
 *  too: "Note to cue-387 · Feature #418", the message under it. */
const DESK_SAY = /^[-*+ ]*(ITEM|ROUTE|DONE|CONTEXT|DESIGNS|TITLE|SEE|MERGE|ASK|NOTE|TELL DESKS|TELL DESK|START|CLOSE|PARK|HOLD|UNPARK|DROP|PAUSE|RESUME|TAKE COPY|LAND FIRST|COPY FIRST)\**\s*:(.*)$/;
const DESK_KINDS = ["bug", "feature", "task"];
/** `line` of a Desk chat's reply as one short plain sentence at the left (md's `lines` hook; #157, #173): ROUTE "Gave
 *  Task #2 to who" (or "Task #2 waits on #1"); ITEM "Added Bug #2: title"; DONE "Bug #2 is done"; TITLE "Renamed Bug
 *  #2"; MERGE "Folded #3 into Bug #2"; CONTEXT "Kept who's context"; DESIGNS "Open the mocks ↗", a link in the accent colour. An item's number is a
 *  link that opens it; a click on the rest of the line opens what's longer under it (deskLnOpen: the title, why, done
 *  when). No arrow, no chevron. "" for any other line, or one parse.rs wouldn't act on (an empty or "<…>" body). */
function deskLineHtml(line, inline) {
  const m = DESK_SAY.exec(line.trim()) || DESK_SESS.exec(line.trim());
  if (!m) return "";
  const body = m[2].trim().replace(/^\*\*/, "").trim().replace(/\*+$/, "").trim();
  if (!body || body.startsWith("<")) return "";
  if (m[1] === "ASK") return MD_DROP;
  const k = deskLineKit(m, body, inline);
  const kinds = { ITEM: deskLineItem, SEE: deskLineSee, ROUTE: deskLineRoute, DONE: deskLineDone, TITLE: deskLineTitle,
    MERGE: deskLineMerge, NOTE: deskLineNote, DESIGNS: deskLineDesigns, "TELL DESK": deskLineTell, "TELL DESKS": deskLineTell,
    CHECK: deskLineCheck, "IF PICKED": deskLinePicked, "TO LEAD": deskLineSaid, "USER VERIFY": deskLineSaid, "NOT DONE": deskLineSaid,
    "DECIDED (minor)": deskLineMinor };
  const run = DESK_ACTS[m[1]] ? deskLineAct : kinds[m[1]] || deskLineContext;
  const html = run(k);
  // Marked as the Desk's own (desk/runs.js): it leaves the chat for Activity (a count line says it), unless it's for you.
  return html && html !== MD_DROP ? dkMark(html, deskSignOf(m[1])) : html;
}
/** An item named in a message: "#826", or with its kind before it ("Bug #826"); not "&#39;" (an escaped quote) or a
 *  path's "/#x". A session named: a word with a dash in it ("cue-826"), linked only when it is a session's name. */
const DESK_REF = /(^|[^\w&/#-])(?:((?:bug|feature|task)\s+)?#([a-z0-9]+)\b|([a-z0-9][\w.]*-[\w.-]*\w))/gi;
/** The lead chat's words (md's `refs`, html with its links made): each item number a link that opens that item, each
 *  session's name a link that opens its chat. Only what exists: a number no item has, a word no session is named, stays
 *  as it is. Never inside a link, a button or inline code already there. */
function deskRefs(h) {
  if (!/#|-/.test(h)) return h;
  const p = typeof deskSelection === "function" ? deskSelection() : "";
  let names = null;
  const sessBy = (n) => {
    names ??= new Map(state.sessions.map((s) => [nameOf(s.session_id, s.project).toLowerCase(), s.session_id]));
    return names.get(n.toLowerCase());
  };
  const swap = (t) => t.replace(DESK_REF, (m, pre, kind, id, name) => {
    if (id) {
      const it = deskItemAny(p, id.toLowerCase());
      // Its project too: a link outside the Desk (the Board's chat) opens the item on its own Desk (app/clicks_chat.js).
      const of = (state.desk?.projects || []).find((d) => (d.items || []).includes(it))?.project || p;
      return it ? `${pre}<a href="#" class="dk-ref" data-desk-chk-item="${esc(it.id)}" data-desk-project="${esc(of)}">${kind || ""}#${id}</a>` : m;
    }
    const sid = sessBy(name);
    return sid ? `${pre}<a href="#" class="dk-ref" data-desk-session="${esc(sid)}">${name}</a>` : m;
  });
  // Tags as they are; text between them changed only outside <a>, <button> and <code>.
  let deep = 0;
  return h.split(/(<[^>]*>)/).map((part) => {
    const tag = /^<(\/?)(a|button|code)\b/i.exec(part);
    if (tag) { deep = Math.max(0, deep + (tag[1] ? -1 : 1)); return part; }
    return part.startsWith("<") || deep ? part : swap(part);
  }).join("");
}
/** A session's own Desk lines (#685: CHECK, IF PICKED, TO LEAD…), drawn as the lead's are, each with its sign. */
const DESK_SESS = /^[-*+ ]*(CHECK|IF PICKED|TO LEAD|USER VERIFY|NOT DONE|DECIDED \(minor\))\**\s*:(.*)$/;
/** A Desk line's kind as its sign and mark name it: "TAKE COPY" → "take-copy", "DECIDED (minor)" → "minor". */
const deskSignOf = (word) => (word === "DECIDED (minor)" ? "minor" : word.toLowerCase().replace(/ /g, "-"));
/** Each kind's sign (option A of #676): a 14px stroke drawing in a small round tile. */
const DESK_SIGNS = {
  route: "M4 12h15M13 6l6 6-6 6", item: "M12 5v14M5 12h14", merge: "M6 4v6a6 6 0 0 0 6 6h6M14 12l4 4-4 4", note: "M4 5h16v11H9l-5 4z",
  minor: "M12 8v8M8 12h8", "to-lead": "M12 19V5M6 11l6-6 6 6", check: "M6 4l14 8-14 8z", done: "M5 12.5l4.5 4.5L19 7",
  "if-picked": "M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
  designs: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
  title: "M4 20h4L19 9l-4-4L4 16zM13 7l4 4", see: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4z",
  context: "M5 7h14M5 12h14M5 17h9", "user-verify": "M6 4l14 8-14 8z", "not-done": "M7 7l10 10M17 7L7 17",
  start: "M7 5l11 7-11 7z", close: "M5 12.5l4.5 4.5L19 7", act: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
};
/** Kind `kind`'s sign: its tile and drawing (the done ones filled in the accent, desk-lead.css .dk-sg). */
function deskSign(kind) {
  const d = DESK_SIGNS[kind] || DESK_SIGNS.act, w = kind === "done" || kind === "close" ? 2.6 : kind === "minor" ? 1.8 : 2;
  return `<span class="dk-sg" data-sg="${esc(kind)}" aria-hidden="true"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"><path d="${d}"/></svg></span>`;
}
/** A CHECK line (a session's, deskLineHtml): "How to check Feature #685", its parts (Where, Do, Expect…) in a box under it. */
function deskLineCheck(k) {
  const { inline, p, id, known, ref, ln } = k;
  const at = p[0].startsWith("#") && id(p[0]);
  if (!at) return "";
  const rows = p.slice(1).filter(Boolean).map((s) => { const l = /^([A-Za-z][A-Za-z' ]{1,18}):\s*([\s\S]*)$/.exec(s); return `<div class="dk-opt-r">${l ? `<b>${esc(l[1])}</b> ${inline(l[2])}` : inline(s)}</div>`; });
  return ln(`<span class="mg-v">How to check</span> ${ref(at, known(at))}`, "", rows.length ? `<div class="dk-opt">${rows.join("")}</div>` : "");
}
/** An IF PICKED line (deskLineHtml): "If you pick B", what follows it in a box under it. */
function deskLinePicked(k) {
  const { inline, p, ln } = k;
  const rest = p.slice(1).filter(Boolean);
  return ln(`<span class="mg-v">If you pick</span> <b>${inline(p[0])}</b>`, "", rest.length ? `<div class="dk-opt">${rest.map((s) => `<div class="dk-opt-r">${inline(s)}</div>`).join("")}</div>` : "");
}
/** A TO LEAD, USER VERIFY or NOT DONE line (deskLineHtml): its words after a quiet "To the lead:", "For you to try:",
 *  "Not done:". */
function deskLineSaid(k) {
  const { m, body, inline, ln } = k;
  const say = { "TO LEAD": "To the lead:", "USER VERIFY": "For you to try:", "NOT DONE": "Not done:" }[m[1]];
  return ln(`<span class="mg-v">${say}</span> <span class="dk-ln-w">${inline(body)}</span>`);
}
/** A DECIDED (minor) line (deskLineHtml): "Small call: what", why under it once opened. */
function deskLineMinor(k) {
  const { body, inline, ln } = k;
  const d = /\s+(?:—|–|--)\s+/.exec(body), what = d ? body.slice(0, d.index) : body, why = d ? body.slice(d.index + d[0].length) : "";
  return ln(`<span class="mg-v">Small call:</span> <span class="dk-ln-w">${inline(what)}</span>`, why ? inline(why) : "");
}
/** What deskLineHtml's kinds share: the line's match `m`, its `body` and parts `p`, and the helpers that read and draw
 *  them. */
function deskLineKit(m, body, inline) {
  const p = body.split("|").map((x) => x.trim());
  // A part without its label ("done when:", any case), as parse.rs labelled(); null without it.
  const lab = (s, k) => (s.slice(0, k.length).toLowerCase() === k ? s.slice(k.length).trimStart().replace(/^:/, "").trim() : null);
  const find = (k) => p.slice(1).map((s) => lab(s, k)).find((v) => v !== null) ?? "";
  const id = (s) => (/^#?([a-z0-9]+)/i.exec(s.trim())?.[1] || "").toLowerCase();
  const name = (s) => s.trim().replace(/^@/, "").replace(/^["`*]+|["`*]+$/g, "").trim();
  const arrow = (s) => { const a = /\s*(->|→|=>)\s*/.exec(s); return a ? [s.slice(0, a.index), s.slice(a.index + a[0].length)] : null; };
  const known = (x) => (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, x));
  // An item as "Bug #2", a link that opens it (just "#2" when `bare`); plain until the Desk has it.
  const ref = (x, it = known(x), bare) => {
    const t = bare || !it ? `#${it?.id ?? x}` : deskNum(it);
    return it ? `<button class="mg-id" data-desk-chk-item="${esc(it.id)}">${bare ? "" : deskKindIcon(it)}${esc(t)}</button>` : `<span class="mg-id">${esc(t)}</span>`;
  };
  // Its title under the line, a click opening it.
  const titled = (it, t = it?.title) => (!t ? "" : it ? `<button class="dk-ln-it" data-desk-chk-item="${esc(it.id)}">${esc(t)}</button>` : `<span>${esc(t)}</span>`);
  // The line (messages.css .mg-line, its verbs quiet in .mg-v), and what opens under it as .mg-full (none: a plain
  // line). A click on the line opens it; its item's chip opens the item.
  // Its kind's sign at its left (#685, option A of #676); `box`: what shows under it always (a CHECK's parts).
  const ln = (head, more, box = "") => {
    const key = `${m[1]}:${body}`, open = !!more && deskLnOpen.has(key);
    const row = more ? `<div class="mg-line" role="button" tabindex="0" data-desk-why="${esc(key)}" aria-expanded="${open}"><span class="mg-lh">${head}</span></div>` : `<div class="mg-line"><span class="mg-lh">${head}</span></div>`;
    return `<div class="dk-ln dk-sgl${open ? " open" : ""}">${deskSign(deskSignOf(m[1]))}${row}${box}${open ? deskCut(key, "mg-full", more, more) : ""}</div>`;
  };
  const who = (s) => `<b class="mg-who">${esc(s)}</b>`;
  const dot = (xs) => xs.filter(Boolean).join(" · ");
  const and = (xs) => (xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);
  return { m, body, inline, p, lab, find, id, name, arrow, known, ref, titled, ln, who, dot, and };
}
/** An ITEM line (deskLineHtml): "Added Bug #2: title". */
function deskLineItem(k) {
  const { inline, p, lab, ref, ln } = k;
  const first = p[0].toLowerCase();
  const kind = DESK_KINDS.includes(first) ? first : "";
  const rest = kind ? p.slice(1) : p;
  const title = rest.find((s) => lab(s, "done when") === null && deskSeeOf(s) === null && !/^why\s*:/i.test(s)) || "";
  if (!title) return "";
  deskSaidItem = title;
  // What you'll see (#382); its done when is the session's check, not shown.
  const see = rest.map(deskSeeOf).find((v) => v !== null) || "";
  const it = deskNewlySaid(title);
  const what = it ? ref(it.id, it) : kind ? `a ${kind}` : "";
  return ln(`<span class="mg-v">Added</span> ${what ? `${what}: ` : ""}${inline(title)}`, see ? `You'll see ${inline(see)}` : "");
}
/** A SEE line (deskLineHtml): "Said what you'll see for Bug #12". */
function deskLineSee(k) {
  const { inline, p, lab, id, known, ref, ln } = k;
  // "SEE: #12 | <what you'll see> | done when: <check>" (#382): "Said what you'll see for Bug #12", the words under it.
  if (!p[0].startsWith("#")) return "";
  const it = known(id(p[0]));
  const see = p.slice(1).map((s) => deskSeeOf(s) ?? (lab(s, "done when") === null ? s : null)).find((v) => v) || "";
  return ln(`<span class="mg-v">Said what you'll see for</span> ${ref(id(p[0]), it)}`, see ? `You'll see ${inline(see)}` : "");
}
/** A ROUTE line (deskLineHtml): "Gave Task #2 to who", or "Task #2 waits on #1". */
function deskLineRoute(k) {
  const { inline, p, find, id, name, arrow, known, ref, titled, ln, who, dot } = k;
  const ht = arrow(p[0]), at = ht && id(ht[0]), to = ht && name(ht[1]);
  if (!at || !to) return "";
  const after = id(find("after")), why = find("why"), crew = name(find("crew"));
  // "#new" is the ITEM just above it, once the Desk has it; "→ new" (or "me") the session Cue started for it, once there is one.
  const it = at === "new" ? deskNewlySaid(deskSaidItem) : known(at);
  const fresh = /^(new|me)$/i.test(to) && it?.holder;
  const to2 = fresh ? who(deskHolderName(it.holder.sid, it.holder.name)) : /^new$/i.test(to) ? "a new session" : who(to);
  const to3 = crew ? `${esc(crew)} on ${to2}'s crew` : to2;
  const what = at === "new" && !it ? "the new item" : ref(at, it);
  const head = after ? `${what} <span class="mg-v">waits on</span> ${ref(after, undefined, true)}` : `<span class="mg-v">Gave</span> ${what} <span class="mg-v">to</span> ${to3}`;
  const title = it?.title || (at === "new" ? deskSaidItem : "");
  return ln(head, dot([titled(it, title), after ? `Then to ${to3}` : "", why ? inline(why) : ""]));
}
/** A DONE line (deskLineHtml): "Bug #2 is done". */
function deskLineDone(k) {
  const { body, id, known, ref, titled, ln, dot, and } = k;
  const ids = body.split(/[, ]/).filter((x) => x.trim().startsWith("#")).map(id).filter(Boolean);
  return ids.length ? ln(`${and(ids.map((x) => ref(x)))} <span class="mg-v">${ids.length > 1 ? "are" : "is"} done</span>`, dot(ids.map((x) => titled(known(x))))) : "";
}
/** A TITLE line (deskLineHtml): "Renamed Bug #2". */
function deskLineTitle(k) {
  const { p, id, known, ref, titled, ln } = k;
  const at = p[0].startsWith("#") && id(p[0]), t = (p[1] || "").replace(/^["`*]+|["`*]+$/g, "").trim();
  return at && t ? ln(`<span class="mg-v">Renamed</span> ${ref(at)}`, titled(known(at), t)) : "";
}
/** A MERGE line (deskLineHtml): "Folded #3 into Bug #2". */
function deskLineMerge(k) {
  const { inline, p, find, id, known, ref, titled, ln, dot } = k;
  // "#320 into #319" (or an arrow), as parse.rs merge: the copy is gone once Cue folds it, so it's its number alone.
  const pr = /^(#\S+)\s+(?:into|->|→|=>)\s+(#\S+)/i.exec(p[0]), from = pr && id(pr[1]), to = pr && id(pr[2]);
  if (!from || !to || from === to) return "";
  const why = find("why");
  return ln(`<span class="mg-v">Folded</span> ${ref(from, undefined, true)} <span class="mg-v">into</span> ${ref(to)}`, dot([titled(known(to)), why ? inline(why) : ""]));
}
/** A NOTE line (deskLineHtml): the family's card, "lead → who NOTE Feature #418", the words under it. */
function deskLineNote(k) {
  const { body, inline, p, id, known, ref, who } = k;
  // "NOTE: #418 | words" (desk/hub/turn/route.rs note): who it went to, as Cue noted it ("note to cue-387"), else who holds it.
  const at = p[0].startsWith("#") && !p[0].includes(" ") && id(p[0]), words = body.slice(body.indexOf("|") + 1).trim();
  if (!body.includes("|")) return "";
  if (!at || !words) return "";
  const it = known(at), d = it && deskOf(it.project);
  const sent = (d?.decisions || []).find((x) => x.item_id === it.id && x.text === words && /^note to /.test(x.why || ""));
  const to = sent ? sent.why.slice(8) : it?.holder ? deskHolderName(it.holder.sid, it.holder.name) : "";
  // The family's card (#501): "cue-lead → cue-481 NOTE Feature #418", the words under it.
  return deskMg({ kind: "note", who: esc(d ? deskLeadName(d) : "The lead"), to: to ? esc(to) : "", tag: "NOTE", id: ref(at, it),
    body: deskCut(`note:${body}`, "mg-t whole", inline(words), words) });
}
/** A DESIGNS line (deskLineHtml): the mocks' link alone. */
function deskLineDesigns(k) {
  const { body } = k;
  // "#135 https://…" (as parse.rs designs): the link alone, under the words, as the item page has it.
  const url = /^#\S+\s.*?(https?:\/\/[^\s>)\]"`*]+)/.exec(body)?.[1]?.replace(/[.,;]+$/, "");
  return url ? `<div class="dk-ln dk-sgl">${deskSign("designs")}<div class="mg-line dk-dsay">${deskDesignsA(url)}</div></div>` : "";
}
/** What the lead's lines that act on an item say (#622; desk/parse.rs unstick, hold, whole_desk): START "Started Bug
 *  #612", the why under it. PAUSE and RESUME without an item are the whole Desk's. */
const DESK_ACTS = { START: "Started", CLOSE: "Closed", PARK: "Parked", HOLD: "Parked", UNPARK: "Unparked", DROP: "Dropped",
  PAUSE: "Paused", RESUME: "Resumed", "TAKE COPY": "Took back the copy of", "LAND FIRST": "Put first to land", "COPY FIRST": "Put first for a copy" };
/** A START, CLOSE, PARK… line (deskLineHtml): "Started Bug #612", its why (and a CLOSE's commit) under it. */
function deskLineAct(k) {
  const { m, inline, lab, id, known, ref, titled, ln, dot } = k;
  // UNPARK's why may follow a dash ("#12 — why: …", parse.rs dashed); the others have it after a "|", label or not.
  const body = k.body.replace(/^(#\S+)\s+[—–-]\s*/, "$1 | ");
  const p = body.split("|").map((x) => x.trim());
  const verb = DESK_ACTS[m[1]];
  const why = p.slice(1).map((s) => lab(s, "why")).find((v) => v !== null) ?? p.slice(1).find((s) => lab(s, "commit") === null && lab(s, "area") === null && lab(s, "till") === null) ?? "";
  if (!p[0].startsWith("#") && !/^\S+\s*#\S+$/.test(p[0])) {
    // The whole Desk (PAUSE: why: …), or a line Cue won't act on: its words alone, no capitals.
    if (m[1] !== "PAUSE" && m[1] !== "RESUME") return "";
    const w = lab(body, "why") ?? body;
    return ln(`<span class="mg-v">${verb} this Desk</span>`, w ? inline(w) : "");
  }
  const at = p[0].startsWith("#") ? id(p[0]) : "", it = at && known(at);
  // Once Cue did it, its own line says so at its time (desk/page.js deskActRows: "Started #612 now", the why under it):
  // the lead's line isn't drawn twice.
  const acts = (deskOf(it?.project)?.decisions || []).filter((x) => x.act && x.item_id === it?.id);
  if (it && acts.some((x) => (x.why || "").trim() === why.trim())) return MD_DROP;
  const what = at ? ref(at, it) : `<span class="mg-id">${esc(p[0].replace(/\s+/g, ""))}</span>`;
  const area = m[1] === "HOLD" ? p.slice(1).map((s) => lab(s, "area")).find((v) => v !== null) : null;
  const head = area ? `<span class="mg-v">Held</span> ${esc(area)} <span class="mg-v">for</span> ${what}` : `<span class="mg-v">${verb}</span> ${what}`;
  const commit = m[1] === "CLOSE" ? (p.slice(1).map((s) => lab(s, "commit")).find((v) => v !== null) || "").replace(/`/g, "") : "";
  return ln(head, dot([titled(it), commit ? `On main as ${esc(commit.slice(0, 9))}` : "", why ? inline(why) : ""]));
}
/** A CONTEXT line (deskLineHtml): "Kept who's context". */
function deskLineContext(k) {
  const { inline, p, find, name, arrow, ln, who } = k;
  const ht = arrow(p[0]), whom = ht && name(ht[0]), what = ht && (ht[1].trim().split(/\s+/)[0] || "").replace(/["`*.]/g, "").toLowerCase();
  if (!whom || !what) return "";
  const why = find("why");
  const said = /^(me|lead)$/i.test(whom) ? "my" : `${who(whom)}'s`;
  const verb = { keep: "Kept", clear: "Cleared", drop: "Dropped", compact: "Compacted" }[what];
  return ln(verb ? `<span class="mg-v">${verb}</span> ${said} <span class="mg-v">context</span>` : `${said === "my" ? "My" : said} <span class="mg-v">context:</span> ${esc(what)}`, why ? inline(why) : "");
}
/** The newest item titled `title` (as the lead's ITEM line said it), or undefined. */
const deskNewlySaid = (title) => (title ? (state.desk?.projects || []).flatMap((d) => d.items).filter((i) => i.title === title).sort((a, b) => (b.created ?? 0) - (a.created ?? 0))[0] : undefined);
/** Cue's message to the lead as the family's card (#501), when it's one of Cue's own (new items, or a DONE it couldn't act on);
 *  null for any other (chat/render.js chatHtml `you`). */
const deskCueNote = (e) => deskNewItems(e) ?? deskDoneNotes(e) ?? deskStopNote(e) ?? deskFromSession(e) ?? deskAskAnswered(e);
/** The lead's own chat (#616): deskCueNote's cards, and any other message Cue wrote it (Away's rules, what you sent
 *  back, the Desk's changes, a reply to its lines: desk/cue_said.js CUE_MSGS) as one short line, folded. */
const deskLeadNote = (e) => deskCueNote(e) ?? deskCueLine(e);
/** Cue's message `e` to the lead as one short line at the left saying what it is: "+ Cue · Desk changes  3m", then
 *  "#528 finished · #535 sent back" (two lines at most in a narrow chat). A click on it opens what matters in it under it (#625, cueFoldHtml: whose words, the item, what's next), as + on a voice message does (the
 *  lead still got all of it); the item's chip opens the item. null for a message that isn't one of Cue's. */
function deskCueLine(e) {
  const c = typeof cueMsgOf === "function" && cueMsgOf(e.text);
  if (!c) return null;
  const key = `lead:${e.at_ms}:cue`, open = openAsks.has(key), full = deskSaid(String(e.text ?? "")).trim();
  const it = c.id ? (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, c.id)) : null;
  const chip = c.id ? ` ${deskMgId(c.id, it?.kind || "", deskNum(it || { id: c.id }))}` : "";
  const what = c.what || c.verb;
  const words = `${esc(what[0].toUpperCase() + what.slice(1))}${c.more ? ` <span class="mg-v">· ${esc(c.more)}</span>` : ""}`;
  const st = e.unsent ? " · not sent" : e.pending ? " · not read yet" : "";
  return `<div class="dk-ln dk-cue${open ? " open" : ""}"><div class="mg-line"><button class="dk-cue-t" data-ask-open="${esc(key)}" aria-expanded="${open}">`
    + `<span class="dk-cue-pm">${open ? "−" : "+"}</span>${cueMark()}<b class="mg-who">Cue</b> <span class="mg-v">· ${esc(c.tag)}${st}</span></button>${chip}`
    + `${c.go ? `<button class="dk-cue-go" data-desk-tab="${esc(c.go[0])}">${esc(c.go[1])} ›</button>` : ""}`
    + `${e.at_ms ? `<span class="mg-age">${ago(e.at_ms)}</span>` : ""}</div><button class="dk-cue-w" data-ask-open="${esc(key)}" aria-expanded="${open}">${words}</button>`
    + `${open ? cueFoldHtml(c, full, c.mate || "", it?.title || "") : ""}</div>`;
}
/** Cue telling the lead your answer to its ASK (desk/hub/core/desk.rs desk_answered: "Your ASK[ about #12]: question\nThe answer:
 *  …"): not drawn (#427), the question's card above already has your answer beside it. null for any other message.
 *  SHORTCUT: queued (the lead busy when it was sent) it shows as an empty "Queued" row; rare, a lead waits idle on its
 *  ASK. Upgrade: laterOwn skips a hidden note. */
const deskAskAnswered = (e) => (/^Your ASK(?: about #\S+)?: [^\n]*\nThe answer: [^\n]*$/.test(String(e.text || "").replace(/^\s*<desk\b[\s\S]*?<\/desk>\s*/, "").trim()) ? `<i hidden></i>` : null);
/** What Cue told the lead of an item that stopped (desk/hub/stall/stall.rs stop_words: "#224 (title) stopped[, not committed]:
 *  holder ended its turn on it with no DONE…(files)… Its last line: …", then its words for the lead from "Settle it:
 *  ROUTE", not shown) or that it couldn't start a session for (desk/hub/lead/run.rs start_failed), as a card from Cue (#244):
 *  what happened, the item's icon, number and title (a click opens it), its holder and one plain line, its last line, and
 *  the files not committed folded. null for any other message. */
function deskStopNote(e) {
  const text = String(e.text || "").replace(/^\s*<desk\b[\s\S]*?<\/desk>\s*/, "").trim();
  const stop = /^#(\S+) \(([\s\S]*)\) stopped(, not committed)?: ([\s\S]+?) ended its turn on it with no DONE(?:, and its work isn't committed \(([\s\S]*)\), so it isn't in the user's app| and no question for the user)\. Its last line: ([\s\S]*?)\nSettle it: ROUTE #\1 /.exec(text);
  const fail = !stop && /^Cue couldn't start a new session for #(\S+) \(([\s\S]*)\): ([\s\S]*)\. It's still [^;\n]*; ROUTE it to a session, or to new to try again\.$/.exec(text);
  if (!stop && !fail) return null;
  const [, id, said] = stop || fail;
  const it = (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, id));
  const loose = !!stop?.[3], files = loose ? stop[5].split(", ").filter(Boolean) : [];
  const tag = fail ? "COULDN'T START" : loose ? "STOPPED, NOT COMMITTED" : "STOPPED";
  const plain = fail ? `Cue couldn't start a session for it: ${esc(fail[3])}.`
    : `<b class="mg-who">${esc(stop[4])}</b> ended its turn with no DONE${loose ? ", and its work isn't in your app yet." : " and no question for you."}`;
  const last = stop?.[6]?.trim();
  const num = deskNum(it || { id });
  const folded = files.length ? `<details class="dk-mfiles mg-st"><summary>${files.length} file${files.length > 1 ? "s" : ""} not committed</summary><div class="dk-mfl">${files.map(esc).join("<br>")}</div></details>` : "";
  return deskMg({ kind: "stop", attrs: ' data-dk="stopped"', cue: true, who: "Cue", tag, id: deskMgId(id, it?.kind || "", num), age: e.at_ms,
    body: `<div class="mg-t whole">${esc(it?.title || said)}</div><div class="mg-t whole dk-mq">${plain}</div>`
      + (last ? deskCut(`last:${id}:${e.at_ms}`, "mg-full", `Its last line: ${esc(last)}`, `Its last line: ${last}`) : ""),
    foot: `${folded}<span class="mg-acts"><button class="mg-btn" data-desk-chk-item="${esc(id)}">Open #${esc(id)}</button></span>` });
}
/** A session's words for the lead (#474: its TO LEAD: line, desk/hub/turn/heard.rs to_lead_said: "From <name> on #<id> (title):\n
 *  <words>\nAnswer it with NOTE: …", or "From <name>:\n<words>\nIt holds no item here…" when it's on none) as a card:
 *  who it's from, the item it's about (a click opens it), its words. Cue's last line, for the lead, isn't shown. A
 *  message that joined several (Cue keeps what comes while the lead is busy, one message when its turn ends): a card
 *  each. In a message that holds your words too (#812), these cards are Activity's; your words stay in the chat
 *  (deskRelaySplit). null for any other message. */
function deskFromSession(e) {
  const split = deskRelaySplit(e.text);
  if (!split) return null;
  return split.relays.map(({ who, id, said, words }, k) => {
    const it = id ? (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, id)) : null;
    return deskMg({ kind: "from", who: esc(who), tag: "TO LEAD", id: id ? deskMgId(id, it?.kind || "", deskNum(it || { id })) : "", age: e.at_ms,
      body: `${id ? `<div class="mg-t whole">${esc(it?.title || said)}</div>` : ""}${deskCut(`from:${e.at_ms}${k ? `:${k}` : ""}`, "mg-t whole", esc(words).replace(/\n/g, "<br>"), words)}` });
  }).join("");
}
/** A session's words for the lead as Cue sent them (to_lead_said), each between blank lines: one or more of them. */
const DESK_RELAY = /(^|\n\n)From ([^\n]+?)(?: on #(\S+) \(([^\n]*)\))?:\n([\s\S]+?)\n(?:Answer it with NOTE: #\S+ \| <your words>\.|It holds no item here, so a NOTE can't reach it\.)(?=\n\n|$)/g;
/** Message `text` to the lead taken apart (#812): the sessions' words Cue passed on in it ({ who, id, said: the item's
 *  title then, words }), and `yours`, whatever else it holds: your words, when you wrote while the lead was busy and
 *  Cue sent them in one message with a session's. null when it passes on none. */
function deskRelaySplit(text) {
  const t = deskSaid(String(text ?? "")).trim();
  const relays = [...t.matchAll(DESK_RELAY)].map(([, , who, id, said, words]) => ({ who, id, said, words: words.trim() }));
  if (!relays.length) return null;
  return { relays, yours: t.replace(DESK_RELAY, "$1").replace(/\n{3,}/g, "\n\n").trim() };
}
/** What Cue told the lead of a DONE it couldn't act on (desk/hub/core/desk.rs lead_done: "#69 not closed: it's done already",
 *  a line each, then its words for the lead, not shown, ending "Don't say DONE for these again." as older notes did):
 *  one card from Cue (NOT CLOSED), a line an item: "[task] Task #69 title · why", its number opening it. null for any
 *  other message. */
function deskDoneNotes(e) {
  const lines = String(e.text || "").split("\n").map((l) => l.trim()).filter(Boolean);
  if (lines.length < 2 || !/Don't say DONE for these again\.$/.test(lines[lines.length - 1])) return null;
  const notes = lines.slice(0, -1).map((l) => /^#([^\s:]+) not closed: (.*)$/.exec(l));
  if (notes.some((m) => !m)) return null;
  const known = (x) => (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, x));
  return deskMg({ kind: "done", cue: true, who: "Cue", tag: "NOT CLOSED", age: e.at_ms, body: notes.map(([, id, why]) => {
    const it = known(id);
    const head = it ? `${deskMgId(it.id, it.kind, deskNum(it))} ${esc(it.title)}` : `<span class="mg-id">#${esc(id)}</span>`;
    return `<div class="mg-line"><span class="mg-lh">${head} <span class="mg-v">· ${esc(why)}</span></span></div>`;
  }).join("") });
}
/** `part` without its "you'll see:" label (or "you will see:", "see:"), as parse.rs see_of(); null without it. */
function deskSeeOf(part) {
  const m = /^(?:you['’]ll see|you will see|see)\s*:(.*)$/i.exec(String(part).trim());
  return m ? m[1].trim() : null;
}
/** What Cue told the lead of new items (desk/mod.rs new_items_words: "New item #72 (bug): title | done when: … |
 *  pictures: `a.png` | from x", a line each, then one line of words for the lead, none of it shown: sometimes "If one
 *  of yours holds two separate problems, SPLIT it first…", always "Route it now…" or "Route each now…", sometimes "Say
 *  what one without a kind is…". Cue never sends new items without the Route words, so they mark the message) as one
 *  card (NEW ITEMS), a line an item: its kind's icon, number and title, a muted line under them (you'll see, who it's
 *  from), then its pictures as thumbnails; a click on the item opens it. null for any other message (chat/render.js chatHtml
 *  `you`). */
function deskNewItems(e) {
  const lines = String(e.text || "").split("\n").map((l) => l.trim()).filter(Boolean);
  const NEW = /^New item #([^\s:(]+)(?: \((bug|feature|task)\))?: (.*)$/;
  const n = lines.findIndex((l) => !NEW.test(l));
  // Cue's own: New item lines first, then its words for the lead, which always ask for a ROUTE.
  if (n < 1 || !lines.slice(n).some((l) => /\bRoute (it|each) now:/.test(l)) || lines.slice(n).some((l) => NEW.test(l))) return null;
  const known = (x) => (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, x));
  return deskMg({ kind: "new", who: "You", tag: "NEW ITEMS", age: e.at_ms, body: lines.slice(0, n).map((l) => deskNewCard(NEW.exec(l), known)).join("") });
}
/** One new item in the NEW ITEMS card (deskNewItems, #501): "[icon] Feature #72 title" as one line, then the words it was
 *  added with, plain and lighter (once the lead gave it a shorter title), a quiet
 *  line (what you'll see, who it's from: not its done when, the session's check, #382), and its pictures as a small row. */
function deskNewCard([, id, said = "", rest], known) {
  const p = rest.split(" | "), it = known(id), kind = it?.kind || said;
  // The line holds the title it was added with; the item may have a shorter one since (TITLE:), its words kept.
  const title = it?.title || p[0], words = it?.words || (p[0] !== title ? p[0] : "");
  let pics = [];
  const sub = p.slice(1).flatMap((x) => {
    const see = /^you'll see: (.*)$/.exec(x), pc = /^pictures: (.*)$/.exec(x);
    if (pc) { pics = [...pc[1].matchAll(/`([^`]+)`/g)].map((m) => m[1]); return []; }
    // Its done when is the session's check (#382): not on the card.
    if (/^done when: /.test(x)) return [];
    return [see ? `you'll see ${esc(see[1])}` : esc(x)];
  });
  return `<div class="dk-ni" role="button" tabindex="0" data-desk-chk-item="${esc(id)}"><div class="mg-line"><span class="mg-lh"><span class="mg-id">${deskKindIcon(kind)}${esc(deskNum({ id, kind }))}</span> <span class="dk-ni-t">${esc(title)}</span></span></div>`
    + `${words ? deskCut(`nc:${id}`, "dk-ni-w", esc(words), words) : ""}`
    + `${sub.length ? `<div class="dk-ni-s">${sub.join(" · ")}</div>` : ""}${thumbs(pics)}</div>`;
}

// ---------- the Desk's hand-off, as a card in the chat of the session it went to (#174) ----------
/** The Desk's hand-off (desk/mod.rs handed_words: "Desk item #171 (task): title", then "You'll see:", "Done when:", "Pictures with
 *  it", "Why you:", then Cue's standing words to the session, not shown) as a card on the right: "From <the Desk's
 *  lead> · Task #171" (its number opening the item), its title, You'll see as short lines (not its Done when: the
 *  session's check is in its message, #382), Why. Lines before it (Cue's
 *  "Done? Write DONE: #171…") stay as a quiet line over it. null for any other message. */
function deskHandHtml(e, meta) {
  const text = String(e.text || "");
  // Cue's other messages about an item start the same way ("Desk item #d5: the Desk lead took back its copy…"): a
  // short line, not a hand-off (desk/cue_said.js, #478).
  if (typeof cueMsgOf === "function" && cueMsgOf(text)) return null;
  const at = /(?:^|\n)(?:For your teammate (.+?): )?Desk item #([^\s:(]+)(?: \((bug|feature|task)\))?: (.*)/.exec(text);
  if (!at) return null;
  const [, mate, id, kind = "", title] = at;
  const pre = text.slice(0, at.index).trim();
  const rest = text.slice(at.index + at[0].length).split("\n");
  // Its labelled parts, each running to the next label; Cue's standing words end it.
  const parts = {};
  let cur = null;
  for (const l of rest) {
    const m = /^(You'll see|Done when|Pictures with it \(open each to see it\)|Why you): ?(.*)$/.exec(l);
    if (m) { cur = m[1] === "You'll see" ? "S" : m[1][0]; parts[cur] = m[2]; continue; }
    if (/^(Fixes and next steps for it stay in|If you put its designs on a canvas|Say DONE: #)/.test(l)) { cur = null; continue; }
    if (cur) parts[cur] += `\n${l}`;
  }
  const projects = state.desk?.projects || [];
  const d = projects.find((x) => x.items.some((i) => deskIdIs(i, id) && i.title === title)) || projects.find((x) => x.items.some((i) => deskIdIs(i, id)));
  const it = d?.items.find((i) => deskIdIs(i, id));
  const lead = d ? deskLeadName(d) : "";
  const num = deskKindIcon(kind || it?.kind) + esc(deskNum({ id, kind: kind || it?.kind }));
  const ref = d ? `<button class="mg-id" data-desk-hand="${esc(d.project)}" data-id="${esc(it?.id ?? id)}">${num}</button>` : `<span class="mg-id">${num}</span>`;
  const pics = (parts.P || "").split(",").map((x) => x.trim()).filter(Boolean).length;
  // The family's card (#501): "web-lead HAND-OFF Bug #d5 for x", the title, You'll see, Why, the pictures counted.
  const card = deskMg({ kind: "hand", cue: !lead, who: esc(lead || "The Desk"), tag: "HAND-OFF", id: ref, to: mate ? esc(mate) : "",
    body: `<div class="mg-t whole dk-hand-t"><b>${esc(title)}</b></div>`
      + (parts.S?.trim() ? `<div class="mg-f">You'll see</div>${deskDoneWhenHtml(parts.S.trim())}` : "")
      + (parts.W?.trim() ? `<div class="mg-f">Why</div><div class="mg-t whole dk-hand-w">${esc(parts.W.trim())}</div>` : ""),
    foot: pics ? `<span class="mg-st dk-hand-p">${pics} picture${pics === 1 ? "" : "s"} with it</span>` : "" });
  return `<div class="mg-you mg-hand">${pre ? `<div class="mg-f dk-hand-pre">${esc(pre)}</div>` : ""}${card}`
    + `${/^<div /.test(meta || "") ? meta : `<div class="mg-f"><span class="mg-st">${meta ?? `Cue · ${ago(e.at_ms)} ago`}</span></div>`}</div>`;
}
if (typeof document !== "undefined") {
  // A hand-off's item number: that item on its Desk, from any chat.
  document.addEventListener("click", (e) => {
    const el = e.target.closest?.("[data-desk-hand]");
    if (!el) return;
    e.stopPropagation();
    openInDesk(el.dataset.deskHand, el.dataset.id);
  }, true);
}
