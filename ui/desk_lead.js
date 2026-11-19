// Cue UI desk_lead — who runs a Desk, and adding to it (ui/desk.js draws the rest). Each project's Desk has one lead:
// a normal session you pick ("Who runs this Desk?") or a new one Cue starts; you tell it, it does the work or hands it
// out. Here: the picker (zero, one or several sessions; "Keep <name>" when changing), ⋯ in the head strip, the
// ended lead's line, "Give to…" on an item no one holds, and the quick-add line at the top of Items (Enter adds; an
// optional Done when; several pasted lines add one item a line; ⌘N focuses it). An item you add has no kind: the
// lead says whether it's a bug, a feature or a task as it routes it. Commands: desk_set_lead, desk_add, desk_give.

/** The project whose lead is being changed (the picker shows over the lead's chat, with Keep), or null. */
let deskPick = null;
/** A lead being set: the session's id ("" for a new one) while desk_set_lead runs, or null. */
let deskLeadBusy = null;
/** The item whose "Give to…" menu is open: { id, at: "row" | "pane" }, or null. */
let deskGiveFor = null;
/** The project whose quick-add line has its Done when open, or null. */
let deskQaDone = null;
/** The lead's lines in its chat you opened (their word and body): shut by default, a redraw keeps them open (#157). */
const deskLnOpen = new Set();
/** The title of the last ITEM line drawn: a ROUTE "#new" after it is that item (parse.rs reads it so). */
let deskSaidItem = "";

// ---------- the lead ----------

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
  return `<div class="dk-chip-line"><button class="dk-chip" data-inbox-desk="${esc(d.project)}">${deskKindIcon("tray")}<span class="dk-chip-k">Desk</span><span class="dk-chip-t">${esc(deskName(d))}</span><span aria-hidden="true">›</span></button></div>`;
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
/** Who wrote an item: "you", the lead (old rows say "dispatch"), or a session by name. */
const deskFrom = (i, d) => (i.from === "you" ? "you" : !i.from || i.from === "dispatch" ? deskLeadName(d) || "the Desk" : i.from);

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
 *  that opens in place (data-desk-older). `rows` carry `work`, as deskSessionsOf's do. */
function deskWithOlder(key, rows, draw, cls = "dk-sfold dk-older") {
  const now = rows.filter((r) => r.work), older = rows.filter((r) => !r.work), open = deskOlderOpen.has(key);
  const fold = older.length ? `<button class="${cls}" data-desk-older="${esc(key)}" aria-expanded="${open}">${older.length} older · ${open ? "hide" : "show"}</button>${open ? older.map(draw).join("") : ""}` : "";
  return now.map(draw).join("") + fold;
}

/** The chat card when no one runs the Desk yet, or when changing who does: "Who runs this Desk?", then a button
 *  (no sessions), "Let <name> run it" (one), or a row a session (several); "Start a new dispatcher" under them. */
function deskPicker(d) {
  const cur = deskLead(d), rows = deskSessionsOf(d.project, d).filter((r) => r.sid !== cur?.sid), busy = deskLeadBusy;
  const label = (sid, text) => (busy === sid ? "Starting…" : text);
  const dis = busy !== null ? "disabled" : "";
  const fresh = (primary) => `<button class="${primary ? "btn primary" : "dk-link dk-fresh"}" data-desk-lead="" ${dis}>${label("", rows.length || cur ? "Start a new dispatcher" : "Start a dispatcher")}</button>`;
  let body;
  if (!rows.length) body = cur ? fresh(false) : fresh(true);
  else if (rows.length === 1 && !cur) body = `<div class="dk-acts dk-pick1"><button class="btn primary" data-desk-lead="${esc(rows[0].sid)}" ${dis}>${label(rows[0].sid, `Let ${esc(rows[0].name)} run it`)}</button>${fresh(false)}</div>`;
  else {
    const row = (r) => `<div class="dk-who"><i class="${r.lead ? "" : "idle"}"></i><div><div class="dk-lt">${esc(r.name)}</div><div class="dk-ls">${esc(r.sub)}</div></div><button class="btn small" data-desk-lead="${esc(r.sid)}" ${dis}>${label(r.sid, "Run it")}</button></div>`;
    body = `<div class="dk-whos">${deskWithOlder(`lead:${d.project}`, rows, row)}</div>${fresh(false)}`;
  }
  const keep = cur ? `<button class="dk-link dk-keep" data-desk-keep>Keep ${esc(deskLeadName(d))}</button>` : "";
  return `<div class="dk-card dk-right"><div class="dk-pick"><h2>Who runs this Desk?</h2><p>A dispatcher takes what you bring here, hands each item to a session, and tells you why.</p>${body}${keep}</div></div>`;
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
    if (/no folder/i.test(String(e)) && typeof lvAct === "function") { toast("No folder known for this project yet: pick one for the new dispatcher"); sheet = lightbox = null; liveSpot = true; lvAct("new", {}); }
    else toast(`Couldn't set the dispatcher: ${e}`);
  } finally { deskLeadBusy = null; renderMain(); }
}

// ---------- Give to… ----------

/** "Give to…" and, open, its menu: the project's sessions (leads first) and New session. */
function deskGive(i, at) {
  const open = deskGiveFor?.id === i.id && deskGiveFor.at === at;
  const draw = (r) => `<button role="menuitem" data-desk-give-to="${esc(r.sid)}" data-id="${esc(i.id)}"><span>${esc(r.name)}</span><span class="dk-ls">${esc(r.sub)}</span></button>`;
  const rows = open ? deskWithOlder(`give:${i.id}`, deskSessionsOf(i.project), draw, "dk-older-mi") : "";
  const menu = open ? `<span class="dk-givemenu" role="menu">${rows}<button role="menuitem" data-desk-act="start_now" data-id="${esc(i.id)}"><span>New session</span></button></span>` : "";
  if (open) queueMicrotask(deskGivePlace);
  return `<span class="dk-givewrap"><button class="${at === "row" ? "dk-link dk-give" : "btn small"}" data-desk-give="${esc(i.id)}" data-at="${at}" aria-expanded="${open}">Give to…</button>${menu}</span>`;
}
/** The open menu, whole inside the window wherever its button sits (#152): under the button from its left edge, moved
 *  left as far as it must, above it when there's more room there, and scrolling inside when even that is too short.
 *  Fixed to the window so no column cuts it; it follows its button as the list scrolls or the window resizes. */
function deskGivePlace() {
  const m = document.querySelector(".dk-givemenu"), b = m?.previousElementSibling;
  if (!m || !b) return;
  const r = b.getBoundingClientRect(), pad = 8, W = innerWidth, H = innerHeight, top = m.scrollTop;
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

/** The quick-add line's draft keys, a pair a project (what you half-typed stays on its own project's Desk). */
const deskQaKeys = (project = deskSelection()) => ({ line: `desk-qa:${project}`, done: `desk-qa-done:${project}` });
/** The line (or its Done when) on screen. */
const deskQaEl = (which = "line") => document.querySelector(`.dk-board .dk-qa [data-qa="${which}"]`);
const deskQaFocusEl = (which) => deskQaEl(which)?.focus();

/** At the top of Items: "Add an item…" with its picture button (or paste or drop one on it), the pictures added,
 *  "+ Done when" (or its line, open). */
function deskQuickAdd() {
  const k = deskQaKeys();
  const done = deskQaDone === deskSelection() ? `<input class="dk-qa-in dk-qa-done" data-qa="done" data-text="${esc(k.done)}" placeholder="Done when… (optional)" spellcheck="false" value="${esc(draft(k.done).text)}"/>`
    : `<button class="dk-link dk-qa-more" data-desk-qa-done>+ Done when</button>`;
  return `<div class="dk-qa" data-drop="${esc(k.line)}"><div class="dk-qa-row"><input class="dk-qa-in" data-qa="line" data-text="${esc(k.line)}" placeholder="Add an item…" spellcheck="false" value="${esc(draft(k.line).text)}"/><button class="icon-btn" data-attach="${esc(k.line)}" aria-label="Add an image">${IMAGE_ICON}</button></div>${draftChips(k.line)}${done}</div>`;
}
/** Add what's in the line (or `titles`, pasted lines): one item each, of no kind (the lead says); Done when and the
 *  pictures go with a single one. The line keeps the cursor for the next. */
async function deskQaAdd(titles = null) {
  const project = deskSelection(), k = deskQaKeys(project);
  const list = (titles || [draft(k.line).text]).map((t) => t.trim()).filter(Boolean);
  if (!list.length) return deskQaFocusEl("line");
  const kind = "", doneWhen = titles ? "" : draft(k.done).text.trim(), images = titles ? [] : draft(k.line).images;
  if (!titles) { delete drafts[k.line]; delete drafts[k.done]; deskQaDone = null; saveDrafts(); }
  renderMain();
  deskQaFocusEl("line");
  const failed = [];
  let why = "";
  for (const title of list) {
    try { await invoke("desk_add", { project, kind, title, ...(doneWhen && { doneWhen }), ...(images.length && { images }) }); }
    catch (e) { failed.push(title); why = String(e); }
  }
  // What didn't go: the first back in the line (when it's empty), and how many.
  if (failed.length) {
    toast(failed.length === 1 ? `Not added: ${why}` : `${failed.length} not added: ${why}`);
    if (!draft(k.line).text) {
      draft(k.line).text = failed[0];
      if (images.length && !draft(k.line).images.length) draft(k.line).images = images;
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
  if (e.key === "Enter" && e.shiftKey && key === k.line) { deskQaDone = deskSelection(); renderMain(); deskQaFocusEl("done"); return true; }
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
  deskLead: (el) => deskSetLead(deskSelection(), el.dataset.deskLead),
  // The picker takes the chat card: whatever's open there closes.
  deskChange: () => { deskPick = deskSelection(); deskGiveFor = null; deskOpen = null; deskDropArm = null; renderMain(); },
  deskKeep: () => { deskPick = null; renderMain(); },
  deskGive: (el) => { const at = el.dataset.at; deskGiveFor = deskGiveFor?.id === el.dataset.deskGive && deskGiveFor.at === at ? null : { id: el.dataset.deskGive, at }; renderMain(); },
  deskGiveTo: (el) => deskGiveTo(el.dataset.id, el.dataset.deskGiveTo),
  deskWhy: (el) => { const k = el.dataset.deskWhy; if (!deskLnOpen.delete(k)) deskLnOpen.add(k); renderMain(); },
  deskQaDone: () => { deskQaDone = deskSelection(); renderMain(); deskQaFocusEl("done"); },
};

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
/** A Desk line as desk/parse.rs reads it: its word in capitals, a list mark or bold around it; ASK has its own card
 *  (deskLeadAsk) and DECIDED md's own block, so neither is here. */
const DESK_SAY = /^[-*+ ]*(ITEM|ROUTE|DONE|CONTEXT|DESIGNS|TITLE|MERGE)\**\s*:(.*)$/;
const DESK_KINDS = ["bug", "feature", "task"];
/** `line` of a Desk chat's reply as one short plain sentence at the left (md's `lines` hook; #157, #173): ROUTE "Gave
 *  Task #2 to who" (or "Task #2 waits on #1"); ITEM "Added Bug #2: title"; DONE "Bug #2 is done"; TITLE "Renamed Bug
 *  #2"; MERGE "Folded #3 into Bug #2"; CONTEXT "Kept who's context"; DESIGNS "Open the mocks ↗", a link in the accent colour. An item's number is a
 *  link that opens it; a click on the rest of the line opens what's longer under it (deskLnOpen: the title, why, done
 *  when). No arrow, no chevron. "" for any other line, or one parse.rs wouldn't act on (an empty or "<…>" body). */
function deskLineHtml(line, inline) {
  const m = DESK_SAY.exec(line.trim());
  if (!m) return "";
  const body = m[2].trim().replace(/^\*\*/, "").trim().replace(/\*+$/, "").trim();
  if (!body || body.startsWith("<")) return "";
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
    return it ? `<button class="dk-say-id" data-desk-chk-item="${esc(it.id)}">${bare ? "" : deskKindIcon(it)}${esc(t)}</button>` : `<b class="dk-say-id">${esc(t)}</b>`;
  };
  // Its title under the line, a click opening it.
  const titled = (it, t = it?.title) => (!t ? "" : it ? `<button class="dk-sl-it" data-desk-chk-item="${esc(it.id)}">${esc(t)}</button>` : `<span class="dk-sl-tt">${esc(t)}</span>`);
  // The line, and what opens under it (none: a plain line). The line isn't a button: its item's number is one.
  const ln = (head, more) => {
    const key = `${m[1]}:${body}`, open = !!more && deskLnOpen.has(key);
    const row = more ? `<div class="dk-sl-r why" role="button" tabindex="0" data-desk-why="${esc(key)}" aria-expanded="${open}"><span class="dk-sl-h">${head}</span></div>` : `<span class="dk-sl-r"><span class="dk-sl-h">${head}</span></span>`;
    return `<div class="dk-say dk-sl${open ? " open" : ""}">${row}${open ? `<div class="dk-sl-d">${more}</div>` : ""}</div>`;
  };
  const who = (s) => `<b class="dk-rt-to">${esc(s)}</b>`;
  const dot = (xs) => xs.filter(Boolean).join(" · ");
  const and = (xs) => (xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);
  if (m[1] === "ITEM") {
    const first = p[0].toLowerCase();
    const kind = DESK_KINDS.includes(first) ? first : "";
    const rest = kind ? p.slice(1) : p;
    const title = rest.find((s) => lab(s, "done when") === null) || "";
    if (!title) return "";
    deskSaidItem = title;
    const done = rest.map((s) => lab(s, "done when")).find((v) => v !== null) || "";
    const it = deskNewlySaid(title);
    const what = it ? ref(it.id, it) : kind ? `a ${kind}` : "";
    return ln(`Added ${what ? `${what}: ` : ""}<span class="dk-say-t">${inline(title)}</span>`, done ? `Done when ${inline(done)}` : "");
  }
  if (m[1] === "ROUTE") {
    const ht = arrow(p[0]), at = ht && id(ht[0]), to = ht && name(ht[1]);
    if (!at || !to) return "";
    const after = id(find("after")), why = find("why"), crew = name(find("crew"));
    // "#new" is the ITEM just above it, once the Desk has it; "→ new" (or "me") the session Cue started for it, once there is one.
    const it = at === "new" ? deskNewlySaid(deskSaidItem) : known(at);
    const fresh = /^(new|me)$/i.test(to) && it?.holder;
    const to2 = fresh ? who(deskHolderName(it.holder.sid, it.holder.name)) : /^new$/i.test(to) ? "a new session" : who(to);
    const to3 = crew ? `${esc(crew)} on ${to2}'s crew` : to2;
    const what = at === "new" && !it ? "the new item" : ref(at, it);
    const head = after ? `${what} waits on ${ref(after, undefined, true)}` : `Gave ${what} to ${to3}`;
    const title = it?.title || (at === "new" ? deskSaidItem : "");
    return ln(head, dot([titled(it, title), after ? `Then to ${to3}` : "", why ? inline(why) : ""]));
  }
  if (m[1] === "DONE") {
    const ids = body.split(/[, ]/).filter((x) => x.trim().startsWith("#")).map(id).filter(Boolean);
    return ids.length ? ln(`${and(ids.map((x) => ref(x)))} ${ids.length > 1 ? "are" : "is"} done`, dot(ids.map((x) => titled(known(x))))) : "";
  }
  if (m[1] === "TITLE") {
    const at = p[0].startsWith("#") && id(p[0]), t = (p[1] || "").replace(/^["`*]+|["`*]+$/g, "").trim();
    return at && t ? ln(`Renamed ${ref(at)}`, titled(known(at), t)) : "";
  }
  if (m[1] === "MERGE") {
    // "#320 into #319" (or an arrow), as parse.rs merge: the copy is gone once Cue folds it, so it's its number alone.
    const pr = /^(#\S+)\s+(?:into|->|→|=>)\s+(#\S+)/i.exec(p[0]), from = pr && id(pr[1]), to = pr && id(pr[2]);
    if (!from || !to || from === to) return "";
    const why = find("why");
    return ln(`Folded ${ref(from, undefined, true)} into ${ref(to)}`, dot([titled(known(to)), why ? inline(why) : ""]));
  }
  if (m[1] === "DESIGNS") {
    // "#135 https://…" (as parse.rs designs): the link alone, under the words, as the item page has it.
    const url = /^#\S+\s.*?(https?:\/\/[^\s>)\]"`*]+)/.exec(body)?.[1]?.replace(/[.,;]+$/, "");
    return url ? `<div class="dk-say dk-dsay">${deskDesignsA(url)}</div>` : "";
  }
  const ht = arrow(p[0]), whom = ht && name(ht[0]), what = ht && (ht[1].trim().split(/\s+/)[0] || "").replace(/["`*.]/g, "").toLowerCase();
  if (!whom || !what) return "";
  const why = find("why");
  const said = /^(me|lead)$/i.test(whom) ? "my" : `${who(whom)}'s`;
  const verb = { keep: "Kept", clear: "Cleared", drop: "Dropped", compact: "Compacted" }[what];
  return ln(verb ? `${verb} ${said} context` : `${said === "my" ? "My" : said} context: ${esc(what)}`, why ? inline(why) : "");
}
/** The newest item titled `title` (as the lead's ITEM line said it), or undefined. */
const deskNewlySaid = (title) => (title ? (state.desk?.projects || []).flatMap((d) => d.items).filter((i) => i.title === title).sort((a, b) => (b.created ?? 0) - (a.created ?? 0))[0] : undefined);
/** Cue's message to the lead as quiet centred lines, when it's one of Cue's own (new items, or a DONE it couldn't act on);
 *  null for any other (chat/render.js chatHtml `you`). */
const deskCueNote = (e) => deskNewItems(e) ?? deskDoneNotes(e) ?? deskStopNote(e);
/** What Cue told the lead of an item that stopped (hub/desk_stall.rs stop_words: "#224 (title) stopped[, not committed]:
 *  holder ended its turn on it with no DONE…(files)… Its last line: …", then its words for the lead from "Settle it:
 *  ROUTE", not shown) or that it couldn't start a session for (hub/desk_lead.rs start_failed), as a card from Cue (#244):
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
  const what = fail ? "Couldn't start" : loose ? "Stopped, not committed" : "Stopped";
  const plain = fail ? `Cue couldn't start a session for it: ${esc(fail[3])}.`
    : `<b class="dk-sn-who">${esc(stop[4])}</b> · Ended its turn with no DONE${loose ? ", and its work isn't in your app yet." : " and no question for you."}`;
  const last = stop?.[6]?.trim();
  return `<div class="dk-news"><div class="dk-sn"><div class="dk-sn1"><span class="dk-sn-cue"><i></i>Cue</span><span>·</span><span class="dk-sn-w${fail || loose ? " red" : ""}">${what}</span><span class="dk-cl-age">${ago(e.at_ms)}</span></div>`
    + `<div class="dk-sn2">${deskItemRef(id, it?.kind || "", deskNum(it || { id }), it?.title || said)}</div>`
    + `<div class="dk-sn3">${plain}</div>`
    + (last ? `<div class="dk-sn-last">Its last line: <span>${esc(last)}</span></div>` : "")
    + (files.length ? `<details class="dk-sn-f"><summary>${files.length} file${files.length > 1 ? "s" : ""} not committed</summary><div class="dk-sn-fl">${files.map(esc).join("<br>")}</div></details>` : "")
    + `</div></div>`;
}
/** What Cue told the lead of a DONE it couldn't act on (hub/desk.rs lead_done: "#69 not closed: it's done already",
 *  a line each, then its words for the lead, not shown, ending "Don't say DONE for these again." as older notes did):
 *  a centred line an item, "Cue · not closed [task] Task #69 title · why", its number and title opening it. null for
 *  any other message. */
function deskDoneNotes(e) {
  const lines = String(e.text || "").split("\n").map((l) => l.trim()).filter(Boolean);
  if (lines.length < 2 || !/Don't say DONE for these again\.$/.test(lines[lines.length - 1])) return null;
  const notes = lines.slice(0, -1).map((l) => /^#([^\s:]+) not closed: (.*)$/.exec(l));
  if (notes.some((m) => !m)) return null;
  const known = (x) => (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, x));
  return `<div class="dk-news">${notes.map(([, id, why]) => {
    const it = known(id);
    const head = it ? deskItemRef(it.id, it.kind, deskNum(it), it.title) : `<b class="dk-new-t">#${esc(id)}</b>`;
    return deskCueLine(`<span class="dk-say-w">not closed</span> ${head}<span class="dk-say-s"> · ${esc(why)}</span>`, e.at_ms);
  }).join("")}</div>`;
}
/** What Cue told the lead of new items (desk/mod.rs new_items_words: "New item #72 (bug): title | done when: … |
 *  pictures: `a.png` | from x", a line each, then one line of words for the lead, none of it shown: sometimes "If one
 *  of yours holds two separate problems, SPLIT it first…", always "Route it now…" or "Route each now…", sometimes "Say
 *  what one without a kind is…". Cue never sends new items without the Route words, so they mark the message) as a small
 *  card an item: its kind's icon, number and title, a muted line under them (done when, who it's from), then its
 *  pictures as thumbnails; a click on the card opens the item. null for any other message (chat/render.js chatHtml
 *  `you`). */
function deskNewItems(e) {
  const lines = String(e.text || "").split("\n").map((l) => l.trim()).filter(Boolean);
  const NEW = /^New item #([^\s:(]+)(?: \((bug|feature|task)\))?: (.*)$/;
  const n = lines.findIndex((l) => !NEW.test(l));
  // Cue's own: New item lines first, then its words for the lead, which always ask for a ROUTE.
  if (n < 1 || !lines.slice(n).some((l) => /\bRoute (it|each) now:/.test(l)) || lines.slice(n).some((l) => NEW.test(l))) return null;
  const known = (x) => (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, x));
  return `<div class="dk-news">${lines.slice(0, n).map((l) => deskNewCard(NEW.exec(l), known, e.at_ms)).join("")}</div>`;
}
/** One new item's card (deskNewItems), design B (#200, #203): "[icon] Feature #72  1h" on one line, the short title
 *  under it, then the words it was added with, plain and lighter (once the lead gave it a shorter title), a quiet
 *  line (who it's from, done when), and its pictures as a small row. */
function deskNewCard([, id, said = "", rest], known, at) {
  const p = rest.split(" | "), it = known(id), kind = it?.kind || said;
  // The line holds the title it was added with; the item may have a shorter one since (TITLE:), its words kept.
  const title = it?.title || p[0], words = it?.words || (p[0] !== title ? p[0] : "");
  let pics = [];
  const sub = p.slice(1).flatMap((x) => {
    const done = /^done when: (.*)$/.exec(x), pc = /^pictures: (.*)$/.exec(x);
    if (pc) { pics = [...pc[1].matchAll(/`([^`]+)`/g)].map((m) => m[1]); return []; }
    return [done ? `done when ${esc(done[1])}` : esc(x)];
  });
  return `<div class="dk-nc" role="button" tabindex="0" data-desk-chk-item="${esc(id)}"><div class="dk-nc1">${deskKindIcon(kind)}<b class="dk-inum">${esc(deskNum({ id, kind }))}</b><span class="dk-cl-age">${ago(at)}</span></div>`
    + `<div class="dk-nc-t">${esc(title)}</div>${words ? `<div class="dk-nc-w">${esc(words)}</div>` : ""}`
    + `${sub.length ? `<div class="dk-nc2">${sub.join(" · ")}</div>` : ""}${thumbs(pics)}</div>`;
}
/** An item in a Desk chat's line: its kind's icon and number in bold, then its title (a click on either opens it). */
const deskItemRef = (id, kind, num, title) => `<button class="dk-chk-a dk-new-t" data-desk-chk-item="${esc(id)}">${deskKindIcon(kind)}<b class="dk-inum">${esc(num)}</b>${title ? ` <span class="dk-new-tt">${esc(title)}</span>` : ""}</button>`;
/** One of Cue's own lines in the lead's chat, centred between thin rules: "Cue · <what> <age>". */
const deskCueLine = (html, at) => `<div class="dk-new dk-cl"><span class="dk-cl-t"><b>Cue</b> · ${html} <span class="dk-cl-age">${ago(at)}</span></span></div>`;

// ---------- the Desk's hand-off, as a card in the chat of the session it went to (#174) ----------
/** The Desk's hand-off (desk/mod.rs handed_words: "Desk item #171 (task): title", then "Done when:", "Pictures with
 *  it", "Why you:", then Cue's standing words to the session, not shown) as a card on the right: "From <the Desk's
 *  lead> · Task #171" (its number opening the item), its title, Done when as short lines, Why. Lines before it (Cue's
 *  "Done? Write DONE: #171…") stay as a quiet line over it. null for any other message. */
function deskHandHtml(e, meta) {
  const text = String(e.text || "");
  const at = /(?:^|\n)(?:For your teammate (.+?): )?Desk item #([^\s:(]+)(?: \((bug|feature|task)\))?: (.*)/.exec(text);
  if (!at) return null;
  const [, mate, id, kind = "", title] = at;
  const pre = text.slice(0, at.index).trim();
  const rest = text.slice(at.index + at[0].length).split("\n");
  // Its labelled parts, each running to the next label; Cue's standing words end it.
  const parts = {};
  let cur = null;
  for (const l of rest) {
    const m = /^(Done when|Pictures with it \(open each to see it\)|Why you): ?(.*)$/.exec(l);
    if (m) { cur = m[1][0]; parts[cur] = m[2]; continue; }
    if (/^(Fixes and next steps for it stay in|If you put its designs on a canvas|Say DONE: #)/.test(l)) { cur = null; continue; }
    if (cur) parts[cur] += `\n${l}`;
  }
  const projects = state.desk?.projects || [];
  const d = projects.find((x) => x.items.some((i) => deskIdIs(i, id) && i.title === title)) || projects.find((x) => x.items.some((i) => deskIdIs(i, id)));
  const it = d?.items.find((i) => deskIdIs(i, id));
  const lead = d ? deskLeadName(d) : "";
  const num = deskKindIcon(kind || it?.kind) + esc(deskNum({ id, kind: kind || it?.kind }));
  const ref = d ? `<button class="dk-hand-n" data-desk-hand="${esc(d.project)}" data-id="${esc(it?.id ?? id)}">${num}</button>` : `<b>${num}</b>`;
  const pics = (parts.P || "").split(",").map((x) => x.trim()).filter(Boolean).length;
  return `<div class="cv-you dk-hand">${pre ? `<div class="dk-hand-pre">${esc(pre)}</div>` : ""}<div class="dk-hand-c">
    <div class="dk-hand-h">${lead ? `From <b>${esc(lead)}</b> · ` : "From the Desk · "}${ref}${mate ? ` · for ${esc(mate)}` : ""}</div>
    <div class="dk-hand-t">${esc(title)}</div>
    ${parts.D?.trim() ? `<div class="dk-hand-l">Done when</div>${deskDoneWhenHtml(parts.D.trim())}` : ""}
    ${pics ? `<div class="dk-hand-p">${pics} picture${pics === 1 ? "" : "s"} with it</div>` : ""}
    ${parts.W?.trim() ? `<div class="dk-hand-l">Why</div><div class="dk-hand-w">${esc(parts.W.trim())}</div>` : ""}
  </div><div class="cv-meta">${meta ?? `Cue · ${ago(e.at_ms)} ago`}</div></div>`;
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
