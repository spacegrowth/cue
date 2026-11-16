// Cue UI desk_lead — who runs a Desk, and adding to it (ui/desk.js draws the rest). Each project's Desk has one lead:
// a normal session you pick ("Who runs this Desk?") or a new one Cue starts; you tell it, it does the work or hands it
// out. Here: the picker (zero, one or several sessions; "Keep <name>" when changing), "Run by <name> · Change", the
// ended lead's line, "Give to…" on an item no one holds, and the quick-add line at the top of Items (Enter adds; a
// kind switch that remembers the last; an optional Done when; several pasted lines add one item a line; ⌘N focuses
// it). Commands: desk_set_lead, desk_add, desk_give.

/** The project whose lead is being changed (the picker shows over the lead's chat, with Keep), or null. */
let deskPick = null;
/** A lead being set: the session's id ("" for a new one) while desk_set_lead runs, or null. */
let deskLeadBusy = null;
/** The item whose "Give to…" menu is open: { id, at: "row" | "pane" }, or null. */
let deskGiveFor = null;
/** The project whose quick-add line has its Done when open, or null. */
let deskQaDone = null;

/** The kind the quick-add line adds: the last one picked (remembered), else Task. */
function deskQaKind() {
  try { const k = localStorage.getItem("cue.deskKind"); if (DESK_KINDS.includes(k)) return k; } catch {}
  return "task";
}
function deskQaKindSet(k) {
  try { localStorage.setItem("cue.deskKind", k); } catch {}
}

// ---------- the lead ----------

/** Desk `d`'s lead ({ sid, name, state }), or null. */
const deskLead = (d) => (d?.lead?.sid ? d.lead : null);
/** The lead's name: what the hub named it, else its session's. */
const deskLeadName = (d) => { const l = deskLead(d); return l ? deskHolderName(l.sid, l.name) : ""; };
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
/** Who wrote an item: "you", the lead (old rows say "dispatch"), or a session by name. */
const deskFrom = (i, d) => (i.from === "you" ? "you" : !i.from || i.from === "dispatch" ? deskLeadName(d) || "the Desk" : i.from);

/** The project's sessions to run its Desk or take an item, leads first: [{ sid, name, sub, lead }]. */
function deskSessionsOf(project, d = deskOf(project)) {
  const rows = (state.sessions || []).filter((s) => s.project === project && s.session_id).map((s) => {
    const sid = s.session_id, crew = deskCrew(sid).length;
    const lead = crew > 0 || deskLead(d)?.sid === sid || d.items.some((i) => i.holder?.kind === "lead" && i.holder.sid === sid);
    return { sid, name: deskHolderName(sid), lead, sub: !lead ? "session" : crew ? `lead, crew of ${crew}` : "lead, on its own" };
  });
  return rows.sort((a, b) => (b.lead ? 1 : 0) - (a.lead ? 1 : 0));
}

/** "Run by <name> · Change" in the Items header; nothing without a lead. */
const deskRunBy = (d) => (deskLead(d) ? `<span class="dk-runby">Run by <b>${esc(deskLeadName(d))}</b> · <button class="dk-link" data-desk-change>Change</button></span>` : "");

/** The right card when no one runs the Desk yet, or when changing who does: "Who runs this Desk?", then a button
 *  (no sessions), "Let <name> run it" (one), or a row a session (several); "Start a new lead" under them. */
function deskPicker(d) {
  const cur = deskLead(d), rows = deskSessionsOf(d.project, d).filter((r) => r.sid !== cur?.sid), busy = deskLeadBusy;
  const label = (sid, text) => (busy === sid ? "Starting…" : text);
  const dis = busy !== null ? "disabled" : "";
  const fresh = (primary) => `<button class="${primary ? "btn primary" : "dk-link dk-fresh"}" data-desk-lead="" ${dis}>${label("", rows.length || cur ? "Start a new lead" : "Start a lead")}</button>`;
  let body;
  if (!rows.length) body = cur ? fresh(false) : fresh(true);
  else if (rows.length === 1 && !cur) body = `<div class="dk-acts dk-pick1"><button class="btn primary" data-desk-lead="${esc(rows[0].sid)}" ${dis}>${label(rows[0].sid, `Let ${esc(rows[0].name)} run it`)}</button>${fresh(false)}</div>`;
  else {
    const row = (r) => `<div class="dk-who"><i class="${r.lead ? "" : "idle"}"></i><div><div class="dk-lt">${esc(r.name)}</div><div class="dk-ls">${esc(r.sub)}</div></div><button class="btn small" data-desk-lead="${esc(r.sid)}" ${dis}>${label(r.sid, "Run it")}</button></div>`;
    body = `<div class="dk-whos">${rows.map(row).join("")}</div>${fresh(false)}`;
  }
  const keep = cur ? `<button class="dk-link dk-keep" data-desk-keep>Keep ${esc(deskLeadName(d))}</button>` : "";
  return `<div class="dk-card dk-right"><div class="dk-pick"><h2>Who runs this Desk?</h2><p>A lead takes what you bring here, does it or hands it out, and tells you why.</p>${body}${keep}</div></div>`;
}
/** The chat pane of a lead that has ended: one line, Resume it and Pick another. */
const deskEndedLine = (d) => `<div class="dk-endlead"><p><b>${esc(deskLeadName(d))}</b> has ended. Items stay here.</p><div class="dk-acts"><button class="btn small primary" data-desk-lead="${esc(deskLead(d).sid)}" ${deskLeadBusy !== null ? "disabled" : ""}>${deskLeadBusy === deskLead(d).sid ? "Resuming…" : "Resume it"}</button><button class="btn small" data-desk-change>Pick another</button></div></div>`;
/** "crew: a, b" beside the lead's name; "" with no crew. */
const deskCrewLine = (d) => { const c = deskLead(d) ? deskCrew(deskLead(d).sid).map((t) => t.name) : []; return c.length ? `<span class="dk-crewline">crew: ${esc(c.join(", "))}</span>` : ""; };

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
  const rows = open ? deskSessionsOf(i.project).map((r) => `<button role="menuitem" data-desk-give-to="${esc(r.sid)}" data-id="${esc(i.id)}"><span>${esc(r.name)}</span><span class="dk-ls">${esc(r.sub)}</span></button>`).join("") : "";
  const menu = open ? `<span class="dk-givemenu" role="menu">${rows}<button role="menuitem" data-desk-act="start_now" data-id="${esc(i.id)}"><span>New session</span></button></span>` : "";
  return `<span class="dk-givewrap"><button class="${at === "row" ? "dk-link dk-give" : "btn small"}" data-desk-give="${esc(i.id)}" data-at="${at}" aria-expanded="${open}">Give to…</button>${menu}</span>`;
}
async function deskGiveTo(id, sid) {
  deskGiveFor = null;
  try { await invoke("desk_give", { id, sid }); } catch (e) { toast(`Not given: ${e}`); }
  renderMain();
}

// ---------- the quick-add line ----------

/** The quick-add line's draft keys, a pair a project (what you half-typed stays on its own project's Desk). */
const deskQaKeys = (project = projFilter) => ({ line: `desk-qa:${project}`, done: `desk-qa-done:${project}` });
/** The line (or its Done when) on screen. */
const deskQaEl = (which = "line") => document.querySelector(`.dk-board .dk-qa [data-qa="${which}"]`);
const deskQaFocusEl = (which) => deskQaEl(which)?.focus();

/** At the top of Items: "Add an item…", the kind switch, "+ Done when" (or its line, open). */
function deskQuickAdd() {
  const kind = deskQaKind(), k = deskQaKeys();
  const kinds = DESK_KINDS.map((x) => `<button class="dk-qk ${x === kind ? "on" : ""}" data-desk-kind="${x}" aria-pressed="${x === kind}">${deskCap(x)}</button>`).join("");
  const done = deskQaDone === projFilter ? `<input class="dk-qa-in dk-qa-done" data-qa="done" data-text="${esc(k.done)}" placeholder="Done when… (optional)" spellcheck="false" value="${esc(draft(k.done).text)}"/>`
    : `<button class="dk-link dk-qa-more" data-desk-qa-done>+ Done when</button>`;
  return `<div class="dk-qa"><div class="dk-qa-row"><input class="dk-qa-in" data-qa="line" data-text="${esc(k.line)}" placeholder="Add an item…" spellcheck="false" value="${esc(draft(k.line).text)}"/><span class="dk-qks">${kinds}</span></div>${done}</div>`;
}
/** Add what's in the line (or `titles`, pasted lines): one item each, of the kind picked; Done when goes with a single
 *  one. The line keeps the cursor for the next. */
async function deskQaAdd(titles = null) {
  const project = projFilter, k = deskQaKeys(project);
  const list = (titles || [draft(k.line).text]).map((t) => t.trim()).filter(Boolean);
  if (!list.length) return deskQaFocusEl("line");
  const kind = deskQaKind(), doneWhen = titles ? "" : draft(k.done).text.trim();
  if (!titles) { delete drafts[k.line]; delete drafts[k.done]; deskQaDone = null; saveDrafts(); }
  renderMain();
  deskQaFocusEl("line");
  const failed = [];
  let why = "";
  for (const title of list) {
    try { await invoke("desk_add", doneWhen ? { project, kind, title, doneWhen } : { project, kind, title }); }
    catch (e) { failed.push(title); why = String(e); }
  }
  // What didn't go: the first back in the line (when it's empty), and how many.
  if (failed.length) {
    toast(failed.length === 1 ? `Not added: ${why}` : `${failed.length} not added: ${why}`);
    if (!draft(k.line).text) {
      draft(k.line).text = failed[0];
      // A single one's Done when comes back with it, its line open.
      if (doneWhen && !draft(k.done).text) { draft(k.done).text = doneWhen; deskQaDone = project; }
      saveDrafts();
    }
  }
  renderMain();
  deskQaFocusEl("line");
}
/** Keys in the quick-add line: Enter adds, ⇧Enter opens Done when, Esc closes it. True when it was the line's. */
function deskQaKey(e, key) {
  const k = deskQaKeys();
  if (key !== k.line && key !== k.done) return false;
  if (e.key === "Enter" && e.shiftKey && key === k.line) { deskQaDone = projFilter; renderMain(); deskQaFocusEl("done"); return true; }
  if (e.key === "Enter" && !e.shiftKey) { deskQaAdd(); return true; }
  if (e.key === "Escape" && deskQaDone && !draft(k.done).text) { deskQaDone = null; renderMain(); deskQaFocusEl("line"); return true; }
  return false;
}
/** ⌘N on a project's Desk: the quick-add line. */
function deskQaFocus() {
  const el = deskQaEl("line");
  if (!el) return false;
  el.focus();
  return true;
}

/** The Desk's clicks for the lead and adding, merged into desk.js's. */
const DESK_LEAD_CLICKS = {
  deskLead: (el) => deskSetLead(projFilter, el.dataset.deskLead),
  // The picker takes the right card: whatever's open there closes.
  deskChange: () => { deskPick = projFilter; deskGiveFor = null; deskOpen = null; deskDropArm = null; renderMain(); },
  deskKeep: () => { deskPick = null; renderMain(); },
  deskGive: (el) => { const at = el.dataset.at; deskGiveFor = deskGiveFor?.id === el.dataset.deskGive && deskGiveFor.at === at ? null : { id: el.dataset.deskGive, at }; renderMain(); },
  deskGiveTo: (el) => deskGiveTo(el.dataset.id, el.dataset.deskGiveTo),
  deskKind: (el) => { deskQaKindSet(el.dataset.deskKind); renderMain(); deskQaFocusEl("line"); },
  deskQaDone: () => { deskQaDone = projFilter; renderMain(); deskQaFocusEl("done"); },
};

if (typeof document !== "undefined") {
  // Several lines pasted into the quick-add line: an item a line.
  document.addEventListener("paste", (e) => {
    if (e.target?.dataset?.qa !== "line") return;
    const lines = (e.clipboardData?.getData("text/plain") || "").split(/\r?\n/).map((l) => l.replace(/^\s*(?:[-*•]|\d+[.)])\s+/, "").trim()).filter(Boolean);
    if (lines.length < 2) return;
    e.preventDefault();
    deskQaAdd(lines);
  }, true);
}
