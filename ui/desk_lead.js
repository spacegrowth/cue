// Cue UI desk_lead — who runs a Desk, and adding to it (ui/desk.js draws the rest). Each project's Desk has one lead:
// a normal session you pick ("Who runs this Desk?") or a new one Cue starts; you tell it, it does the work or hands it
// out. Here: the picker (zero, one or several sessions; "Keep <name>" when changing), ⋯ beside "Runs this Desk", the
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
/** The hand-off cards in the lead's chat whose why you folded ("id>who"); open by default, a redraw keeps them folded. */
const deskWhyShut = new Set();
/** The title of the last ITEM line drawn: a ROUTE "#new" after it is that item (parse.rs reads it so). */
let deskSaidItem = "";

// ---------- the lead ----------

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
    return { sid, name: deskHolderName(sid), lead, sub: !lead ? "session" : crew ? `lead, crew of ${crew}` : "lead, on its own" };
  });
  return rows.sort((a, b) => (b.lead ? 1 : 0) - (a.lead ? 1 : 0));
}

/** The chat card when no one runs the Desk yet, or when changing who does: "Who runs this Desk?", then a button
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
  deskWhy: (el) => { const k = el.dataset.deskWhy; if (!deskWhyShut.delete(k)) deskWhyShut.add(k); renderMain(); },
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
const DESK_SAY = /^[-*+ ]*(ITEM|ROUTE|DONE|CONTEXT)\**\s*:(.*)$/;
const DESK_KINDS = ["bug", "feature", "task"];
/** `line` of the lead's reply as one quiet line of the Desk's (md's `lines` hook): ITEM "New item · BUG title · done
 *  when …"; ROUTE "Bug #2 title → who  Why ⌃", the why under it; DONE "Done · Bug #2 title"; CONTEXT "who → keep · why". "" for
 *  any other line, or one parse.rs wouldn't act on (an empty or "<…>" body). */
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
  const row = (head, sub) => `<div class="dk-say">${head}${sub ? `<span class="dk-say-s"> · ${sub}</span>` : ""}</div>`;
  // An item named by kind and number ("Bug #2") when the Desk has it, as its row says it.
  const known = (x) => (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, x));
  const ref = (x) => `<span class="dk-say-id">${deskKindIcon(known(x))}${esc(known(x) ? deskNum(known(x)) : `#${x}`)}</span>`;
  // The same, with its title when the Desk has it.
  const named = (x) => `${ref(x)}${known(x)?.title ? ` · <span class="dk-say-t">${esc(known(x).title)}</span>` : ""}`;
  if (m[1] === "ITEM") {
    const first = p[0].toLowerCase(), kind = DESK_KINDS.includes(first) ? first : "task";
    const rest = DESK_KINDS.includes(first) ? p.slice(1) : p;
    const title = rest.find((s) => lab(s, "done when") === null) || "";
    if (!title) return "";
    deskSaidItem = title;
    const done = rest.map((s) => lab(s, "done when")).find((v) => v !== null) || "";
    return row(`<span class="dk-say-h"><span class="dk-say-w">New item</span> ${deskKindIcon(kind)}<span class="dk-say-t">${inline(title)}</span></span>`, done ? `done when ${inline(done)}` : "");
  }
  if (m[1] === "ROUTE") {
    const ht = arrow(p[0]), at = ht && id(ht[0]), to = ht && name(ht[1]);
    if (!at || !to) return "";
    const after = id(find("after")), why = find("why"), crew = name(find("crew")), kind = find("kind").replace(/["`*.]/g, "").toLowerCase();
    const sub = [after ? `after ${ref(after)}` : "", why ? inline(why) : ""].filter(Boolean).join(" · ");
    // One line: its kind's icon, number and title (opens the item) → who; why open under it, "Why ⌃" folds it.
    // "#new" is the ITEM just above it, once the Desk has it; "→ new" (or "me") the session Cue started for it, once there is one.
    const it = at === "new" ? deskNewlySaid(deskSaidItem) : known(at), key = `${at}>${to}`, open = !deskWhyShut.has(key);
    const fresh = /^(new|me)$/i.test(to) && it?.holder;
    const who = fresh ? deskHolderName(it.holder.sid, it.holder.name) : to;
    const what = at === "new" && !it ? `<span class="dk-rt-n">New item</span>`
      : `<span class="dk-rt-n">${esc(it ? deskNum(it) : `#${at}`)}</span>${it?.title ? `<span class="dk-rt-t">${esc(it.title)}</span>` : ""}`;
    const head = it ? `<button class="dk-rt-it" data-desk-chk-item="${esc(it.id)}">${deskKindIcon(it)}${what}</button>` : `<span class="dk-rt-it">${deskKindIcon(kind)}${what}</span>`;
    const fold = sub ? `<button class="dk-rt-why" data-desk-why="${esc(key)}">Why ${open ? "⌃" : "⌄"}</button>` : "";
    return `<div class="dk-route${open ? " open" : ""}"><div class="dk-rt-row">${head}<span class="dk-rt-ar">→</span><span class="dk-rt-to">${esc(who)}${crew ? ` · ${esc(crew)}` : ""}</span>${fold}</div>${sub && open ? `<div class="dk-rt-why-s">${sub}</div>` : ""}</div>`;
  }
  if (m[1] === "DONE") {
    const ids = body.split(/[, ]/).filter((w) => w.trim().startsWith("#")).map(id).filter(Boolean);
    return ids.length ? row(`<span class="dk-say-h"><span class="dk-say-w">Done</span> ${ids.map(named).join(", ")}</span>`, "") : "";
  }
  const ht = arrow(p[0]), who = ht && name(ht[0]), what = ht && (ht[1].trim().split(/\s+/)[0] || "").replace(/["`*.]/g, "").toLowerCase();
  if (!who || !what) return "";
  const why = find("why");
  return row(`<span class="dk-say-h"><span class="dk-say-w">Context</span> <span class="dk-say-to"><b>${esc(who)}</b> → ${esc(what)}</span></span>`, why ? inline(why) : "");
}
/** The newest item titled `title` (as the lead's ITEM line said it), or undefined. */
const deskNewlySaid = (title) => (title ? (state.desk?.projects || []).flatMap((d) => d.items).filter((i) => i.title === title).sort((a, b) => (b.created ?? 0) - (a.created ?? 0))[0] : undefined);
/** Cue's message to the lead as a small card, when it's one of Cue's own (new items, or a DONE it couldn't act on);
 *  null for any other (chat/render.js chatHtml `you`). */
const deskCueNote = (e) => deskNewItems(e) ?? deskDoneNotes(e);
/** What Cue told the lead of a DONE it couldn't act on (hub/desk.rs lead_done: "#69 not closed: it waits on your
 *  check", a line each, then its words for the lead, not shown): a card an item, its kind, number and title (a click
 *  opens it) and why in a quiet line. null for any other message. */
function deskDoneNotes(e) {
  const lines = String(e.text || "").split("\n").map((l) => l.trim()).filter(Boolean);
  if (lines.length < 2 || !/^Only the person closes an item that waits on their check/.test(lines[lines.length - 1])) return null;
  const notes = lines.slice(0, -1).map((l) => /^#([^\s:]+) not closed: (.*)$/.exec(l));
  if (notes.some((m) => !m)) return null;
  const known = (x) => (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, x));
  return `<div class="dk-news">${notes.map(([, id, why]) => {
    const it = known(id);
    const head = it ? `${deskKindIcon(it)}<button class="dk-chk-a dk-new-t" data-desk-chk-item="${esc(it.id)}">${esc(`${deskNum(it)} · ${it.title}`)}</button>` : `<span class="dk-new-t">#${esc(id)}</span>`;
    return `<div class="dk-new"><span class="dk-say-w">Not closed</span> ${head}<span class="dk-say-s"> · ${esc(why)}</span></div>`;
  }).join("")}<div class="cv-meta">From Cue · ${ago(e.at_ms)} ago</div></div>`;
}
/** What Cue told the lead of new items (desk/mod.rs new_items_words: "New item #72 (bug): title | done when: … |
 *  pictures: … | from x", a line each, then its "Route it now…" words for the lead) as a small card an item: its kind,
 *  number and title (a click opens it), and done when, pictures and who it's from in a quiet line; the words for the
 *  lead aren't shown. null for any other message (chat/render.js chatHtml `you`). */
function deskNewItems(e) {
  const lines = String(e.text || "").split("\n").map((l) => l.trim()).filter(Boolean);
  const said = lines.filter((l) => !/^Route (it|each) now:/.test(l));
  const items = said.map((l) => /^New item #([^\s:(]+)(?: \((bug|feature|task)\))?: (.*)$/.exec(l));
  if (!said.length || said.length === lines.length || items.some((m) => !m)) return null;
  return `<div class="dk-news">${items.map(([, id, kind = "", rest]) => {
    const p = rest.split(" | "), title = p[0], tail = p.slice(1).map((x) => {
      const done = /^done when: (.*)$/.exec(x), pics = /^pictures: (.*)$/.exec(x);
      if (done) return `done when ${esc(done[1])}`;
      if (pics) { const n = pics[1].split("` `").length; return n === 1 ? "1 picture" : `${n} pictures`; }
      return esc(x);
    });
    return `<div class="dk-new"><span class="dk-say-w">New item</span> ${deskKindIcon(kind)}<button class="dk-chk-a dk-new-t" data-desk-chk-item="${esc(id)}">${esc(`${deskNum({ id, kind })} · ${title}`)}</button>${tail.length ? `<span class="dk-say-s"> · ${tail.join(" · ")}</span>` : ""}</div>`;
  }).join("")}<div class="cv-meta">From Cue · ${ago(e.at_ms)} ago</div></div>`;
}
