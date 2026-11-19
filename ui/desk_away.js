// Cue UI desk — Away (#361, #373) and Pause (#366): the lines that show the lead keep the Desk moving while you're away, its rules and its clicks; and the Pause/Resume button.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (and after desk_list.js).
// ---------- Away (#361, pick B): the lead keeps the Desk moving while you're away ----------

/** The rules (desk_away_set): true = the lead may decide it alone. The last two show under "Always waits for you",
 *  where a switch on means it waits (the rule false). */
const DESK_AWAY_RULES = [
  ["route", "Route, split and merge items; wake or start sessions (up to 3 new)"],
  ["how", "“How” questions inside an item, when easy to change later"],
  ["stopped", "Send back a stopped item with what to do next"],
  ["order", "Which item goes first when two change the same files"],
];
const DESK_AWAY_WAITS = [
  ["what", "“What” questions: scope, taste, what you want"],
  ["outside", "Pulling in a session that isn't on the Desk"],
];
const DESK_AWAY_LOCKED = ["Ask me before: push to main, release, deleting files", "Picking a mock", "Looks right / Send back on a finished item"];
const DESK_AWAY_DEFAULTS = { route: true, how: true, stopped: true, order: true, what: false, outside: false };
const DESK_LOCK = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>`;
const deskHm = (ms) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
const deskMoney = (n) => `$${Number(n || 0).toFixed(Number.isInteger(Number(n)) ? 0 : 2)}`;
/** "HH:MM" → its next time from now, in ms. */
function deskNextAt(hm) {
  const [h, m] = String(hm || "").split(":").map(Number);
  if (!Number.isFinite(h) || !Number.isFinite(m)) return null;
  const t = new Date(now()); t.setHours(h, m, 0, 0);
  if (t.getTime() <= now()) t.setDate(t.getDate() + 1); // by the calendar: a clock change keeps the time
  return t.getTime();
}
/** A Desk's Away now (desk_view), or off. */
const deskAwayOf = (d) => d?.away || { on: false, active: false, ended: "", since: 0, until: null, budget: null, spent: 0, rules: { ...DESK_AWAY_DEFAULTS } };
/** Its line: "until 07:00 · $3.20 of $10", or why it paused. */
function deskAwayStatus(a) {
  if (a.ended === "budget") return "Paused: budget reached";
  if (a.ended === "until") return "Paused: its time is up";
  return [a.until ? `until ${deskHm(a.until)}` : "", a.budget != null ? `${deskMoney(a.spent)} of ${deskMoney(a.budget)}` : ""].filter(Boolean).join(" · ") || "on";
}
/** The switch in the Items head: Away, lit while on. A click opens its rules. */
function deskAwaySwitch(d) {
  if (!d?.lead) return "";
  const on = !!d.away?.on;
  // On: until when and what it spent, beside its name ("Away · until 18:00 · $3.40 of $10"), or why it paused.
  const st = on ? `<span class="dk-aw-st${d.away.ended ? " ended" : ""}">${esc(deskAwayStatus(deskAwayOf(d)))}</span>` : "";
  return `<button class="dk-aw-sw${on ? " on" : ""}" data-desk-away="${esc(d.project)}" aria-pressed="${on}"><span class="dk-knob" aria-hidden="true"></span>Away${st}</button>`;
}
/** Session `sid` stopped on a usage limit Cue carries on from by itself (send_at's carry-on): the reset's ms (1 when
 *  it isn't known), else 0. Not yours to do: it goes on when the limit resets. */
function deskPausedTill(sid) {
  const s = sid && sessionOf(sid), l = s?.limit;
  if (!l || (l.resets_ms && l.resets_ms <= now()) || s.send_at?.failed) return 0;
  return l.resets_ms || 1;
}
/** What waits on you on Desk `d`: { waits: its items that wait, asks: open asks (the lead's questions, Ask me before
 *  permissions) from its sessions not already on an item }, leaving out sessions paused on a usage limit. */
function deskAwayNeeds(d, taskApprovals = deskTaskApprovals(d)) {
  const waits = deskWaitingItems(d, taskApprovals).filter((i) => !deskPausedTill(deskHolderSid(i)));
  const placed = new Set([...taskApprovals.values()].map((r) => r.id));
  for (const i of d.items) if (i.ask?.item_id) placed.add(i.ask.item_id);
  const asks = state.items.filter((it) => !placed.has(it.id) && deskHasSession(d, it.session_id) && it.status === "pending" && it.kind !== "waiting"
    && !answering.has(it.id) && !deskPausedTill(it.session_id));
  return { waits, asks };
}
/** The same as one list for "Before you go": the lead's asks first, newest first, then the other sessions' asks,
 *  then the items that wait; each { key, html }. */
function deskAwayFirstList(d) {
  const { waits, asks } = deskAwayNeeds(d), lead = deskLead(d)?.sid;
  const at = (it) => it.created_ms || it.at_ms || deskMs(it.created) || 0;
  const sorted = [...asks].sort((x, y) => (y.session_id === lead) - (x.session_id === lead) || at(y) - at(x));
  return [...sorted.map((it) => ({ key: `a:${it.id}`, html: () => deskAwayAsk(it, d) })),
    ...[...waits].sort((x, y) => deskMs(y.updated) - deskMs(x.updated)).map((i) => ({ key: `i:${i.id}`, html: () => deskItemRow(i, false) }))];
}
/** An open ask, answerable where it is: who asks and what, a single question's choices, then the controls its card
 *  has everywhere (decisionButtons: Allow / No, Decline / Type answer…, a form's fields). */
function deskAwayAsk(it, d, deny = "No") {
  const qs = it.kind === "question" ? questions(it) : [];
  // A question: its whole card (board.js questionHtml), its suggestions picked; Send is in its buttons.
  const picks = qs.length ? questionHtml(it) : "";
  const acts = ["permission", "question", "input"].includes(it.kind) ? decisionButtons(it, deny) : `<div class="row-btns"><button class="btn" ${deskChatAttrs(it.session_id, d)}>Open its chat</button></div>`;
  const cmd = it.tool_input?.command;
  const what = it.kind === "input" ? formBody(it) : cmd ? `<pre>${esc(cmd)}</pre>` : qs.length ? "" : `<p>${esc(summary(it))}</p>`;
  return `<div class="dk-request dk-aw-ask" data-desk-request="${esc(it.id)}"><div class="dk-request-who">${esc(deskHolderName(it.session_id))}${it.agent_name ? ` · ${esc(it.agent_name)}` : ""}</div>${what}${picks}${acts}</div>`;
}
/** Away (#373, pick C): no column. Each call the lead made since Away went on is a small line under the item it changed
 *  (deskAwayMark), with Undo; what waits on you joins Waiting on you (deskAwayWaiting). */
const DESK_AWAY_ICON = `<svg class="dk-aw-ic" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5Z"/></svg>`;
/** The lead's calls since Away went on, oldest first (a row's lines read down in the order they were made); none while off. */
function deskAwayCalls(d) {
  const a = d?.away;
  if (!a?.on) return [];
  const since = deskMs(a.since);
  return deskCalls(d).filter((c) => deskMs(c.at) >= since).sort((x, y) => deskMs(x.at) - deskMs(y.at));
}
/** One call as a line under its item: the Away icon, what was decided · why, Undo (in words: it's the line's one act). */
function deskAwayMark(c) {
  const undo = c.undo === "can" ? `<button class="dk-aw-undo" data-desk-undo="${esc(c.id)}">Undo</button>` : c.undo === "asked" ? `<span class="dk-u">Undo asked</span>` : "";
  return `<div class="dk-aw-mark" data-desk-call="${esc(c.id)}">${DESK_AWAY_ICON}<span class="dk-aw-mt">${esc(c.text)}${c.why ? `<span class="dk-aw-why"> · ${esc(deskOneLine(c.why))}</span>` : ""}</span>${undo}</div>`;
}
/** What Away adds under Waiting on you: open asks from the Desk's sessions not already on an item (a permission with
 *  Allow / Deny right there; the lead never grants one), and sessions on a usage limit as one quiet line. */
function deskAwayWaiting(d, taskApprovals) {
  if (!d?.away?.on) return { n: 0, html: "" };
  const { asks } = deskAwayNeeds(d, taskApprovals);
  const askRow = (it) => it.kind === "permission" ? deskAwayAsk(it, d, "Deny") : `<button class="dk-prow" ${deskChatAttrs(it.session_id, d)}><i class="dk-pdot ask" aria-hidden="true"></i><span class="dk-prt"><b>${esc(deskHolderName(it.session_id))}</b> <span class="dk-prs">asks you · ${esc(deskOneLine(summary(it)))}</span></span></button>`;
  const paused = [d.lead?.sid, ...(d.members || [])].filter((sid, k, all) => sid && all.indexOf(sid) === k && deskPausedTill(sid));
  const till = paused.length ? Math.max(...paused.map(deskPausedTill)) : 0;
  const quiet = paused.length ? `<div class="dk-aw-quiet">${esc(paused.map((sid) => deskHolderName(sid)).join(", "))} paused${till > 1 ? ` till ${deskHm(till)}` : ""} · Cue carries on then</div>` : "";
  return { n: asks.length, html: (asks.length ? `<div class="dk-prows dk-aw-asks">${asks.map(askRow).join("")}</div>` : "") + quiet };
}

/** The rules card, open: { project (the Desk it was opened on), on (Away is on there), until "HH:MM", budget "10",
 *  rules, busy, desks: every Desk as { project, desk, was (Away on as it opened), on (its switch), swept (its lead's
 *  sweep was sent, or it needs none) }, shown: the keys "Before you go" holds, fixed once every sweep is over }.
 *  One switch for every Desk: Done sets Away on each Desk switched on, off on each switched off. */
let deskAwayEdit = null;
const DESK_AWAY_FIRST = 5;
const deskAwayDesks = () => (state.desk?.projects || []);
function deskAwayOpen(project) {
  const d = deskOf(project), a = deskAwayOf(d);
  const desks = deskAwayDesks().map((x) => { const was = !!deskAwayOf(x).on; return { project: x.project, desk: x.id || x.project, was, on: a.on ? was : true, swept: was }; });
  deskAwayEdit = { project, on: !!a.on, until: a.on ? (a.until ? deskHm(a.until) : "") : "07:00",
    budget: a.on ? (a.budget != null ? String(a.budget) : "") : "10", rules: { ...DESK_AWAY_DEFAULTS, ...(a.rules || {}) }, busy: false, shown: null, desks };
  // Each Desk going from off to on: its lead goes through every open item first and settles what it can.
  for (const r of desks) if (r.on && !r.swept) deskAwaySweep(r);
  renderMain();
}
/** Ask Desk row `r`'s lead to sort its items out before Away. "Before you go" is fixed only once every sweep was sent:
 *  what the sweeps leave is what it shows. Sent means the hub now has it sweeping (desk_away_sweep sets it before it
 *  returns): kept here too, so a state that hasn't caught up yet doesn't fix the list before the lead has gone through. */
function deskAwaySweep(r) {
  const e = deskAwayEdit;
  r.swept = false;
  const sent = (on) => {
    if (deskAwayEdit !== e) return;
    const dd = deskAwayDesks().find((x) => x.project === r.project);
    if (on && dd) dd.away = { ...deskAwayOf(dd), sweeping: true };
    r.swept = true;
    renderMain();
  };
  invoke("desk_away_sweep", { desk: r.desk, rules: { ...e.rules } }).then(() => sent(true), (err) => { toast(`${deskName(deskOf(r.project))}'s lead couldn't go through the items: ${err}`); sent(false); });
}
function deskAwayHtml() {
  const e = deskAwayEdit;
  if (!e) return "";
  const tg = (on) => `<span class="dk-tg${on ? " on" : ""}" aria-hidden="true"></span>`;
  const row = (k, label, on) => `<button class="dk-aw-row" role="switch" aria-checked="${on}" data-away-rule="${k}"><span class="grow">${esc(label)}</span>${tg(on)}</button>`;
  // Turning it on: the lead goes through the open items first (desk_away_sweep); then up to 5 that truly need you,
  // answered in place. Those shown hold Done; the rest wait in Waiting on you.
  // The Desks going from off to on: their leads sweep, then "Before you go" holds what they left, across all of them.
  const going = e.desks.filter((r) => r.on && !r.was), many = e.desks.length > 1;
  const sweeping = going.filter((r) => !r.swept || deskOf(r.project)?.away?.sweeping);
  let first = "", block = 0;
  if (sweeping.length) {
    const who = many ? `${sweeping.length === 1 ? `${deskName(deskOf(sweeping[0].project))}'s lead is` : `${sweeping.length} leads are`}` : "The lead is";
    first = `<div class="dk-aw-sweep"><i class="dk-aw-spin" aria-hidden="true"></i>${esc(who)} going through the open items…</div>`;
    block = 1;
  } else if (going.length) {
    const seen = new Set(), list = going.flatMap((r) => { const d = deskOf(r.project); return d ? deskAwayFirstList(d) : []; }).filter((x) => !seen.has(x.key) && seen.add(x.key));
    if (!e.shown) e.shown = list.slice(0, DESK_AWAY_FIRST).map((x) => x.key);
    const shown = list.filter((x) => e.shown.includes(x.key)), more = list.length - shown.length;
    block = shown.length;
    if (shown.length || more) first = `<div class="dk-aw-h">Before you go: ${shown.length} to answer</div><div class="dk-aw-first">${shown.map((x) => x.html()).join("")}</div>`
      + (more ? `<div class="dk-aw-more">${more} more will wait in Waiting on you</div>` : "");
  }
  const may = DESK_AWAY_RULES.map(([k, l]) => row(k, l, !!e.rules[k])).join("");
  const desks = many ? `<div class="dk-aw-h">Desks</div><div class="dk-aw-rows">${e.desks.map((r) => `<button class="dk-aw-row" role="switch" aria-checked="${r.on}" data-away-desk="${esc(r.project)}"><span class="grow">${esc(deskName(deskOf(r.project)))}</span>${tg(r.on)}</button>`).join("")}</div>` : "";
  const lock = DESK_AWAY_LOCKED.map((l) => `<div class="dk-aw-row locked"><span class="grow">${esc(l)}</span><span class="dk-aw-lock">${DESK_LOCK}</span></div>`).join("")
    + DESK_AWAY_WAITS.map(([k, l]) => row(k, l, !e.rules[k])).join("");
  return `<div class="move-scrim dk-aw-scrim" data-away="scrim"><div class="move-dialog dk-aw-dialog" role="dialog" aria-labelledby="dk-aw-title">
    <div class="move-title" id="dk-aw-title">Away</div>
    <div class="end-text">${many ? "Each Desk's lead keeps it moving while you're away." : "The lead keeps this Desk moving while you're away."}</div>${first}
    <div class="dk-aw-flds"><label>Until<input type="time" data-away-field="until" value="${esc(e.until)}"/></label><label>Budget ($)<input type="number" min="0" step="1" data-away-field="budget" value="${esc(e.budget)}" placeholder="No limit"/></label></div>
    ${desks}<div class="dk-aw-h">The lead may decide</div><div class="dk-aw-rows">${may}</div>
    <div class="dk-aw-h">Always waits for you</div><div class="dk-aw-rows">${lock}</div>
    <div class="dk-aw-foot">One notification only when nothing can move without you.</div>
    <div class="row-btns">${e.on ? `<button class="btn" data-away="off"${e.busy ? " disabled" : ""}>Turn Away off</button>` : `<button class="btn" data-away="cancel">Cancel</button>`}<span class="grow"></span><button class="btn primary" data-away="done"${e.busy || block ? " disabled" : ""}>${block && !sweeping.length ? "Answer these first" : "Done"}</button></div>
  </div></div>`;
}
async function deskAwaySave(on) {
  const e = deskAwayEdit;
  if (!e || e.busy) return;
  const b = String(e.budget).trim(), budget = b === "" ? null : Number(b);
  if (budget != null && !(budget >= 0)) return toast("The budget is a number of dollars");
  const until = deskNextAt(e.until);
  e.busy = true;
  renderMain();
  // Every Desk switched on gets these settings; one switched off (or all, for Turn Away off) goes off; one that was
  // off and stays off isn't touched.
  const failed = [];
  for (const r of e.desks) {
    const now = on && r.on;
    if (!now && !r.was) continue;
    try {
      const view = await invoke("desk_away_set", { desk: r.desk, away: { on: now, until, budget, rules: { ...e.rules } } });
      const d = deskAwayDesks().find((x) => x.project === r.project);
      if (view && typeof view === "object" && d) d.away = view;
    } catch (err) { failed.push(`${deskName(deskOf(r.project))}: ${err}`); }
  }
  if (failed.length) { e.busy = false; toast(`Couldn't change Away on ${failed.join("; ")}`); }
  else if (deskAwayEdit === e) deskAwayEdit = null;
  renderMain();
}
/** The Desk's own buttons in the rules card (an item's Allow / No, a choice, Looks right): as on the Desk. One that
 *  opens a page closes the card first. */
const DESK_AWAY_GOES = ["deskItem", "deskChkItem", "deskChatOf", "deskLeadChat", "deskCrew"];
document.addEventListener("click", (ev) => {
  if (!deskAwayEdit) return;
  const own = ev.target.closest?.(".dk-aw-dialog") && ev.target.closest?.(DESK_SEL);
  if (own && !own.closest("[data-away]:not([data-away=scrim]), [data-away-rule]")) {
    ev.stopPropagation();
    const k = Object.keys(DESK_CLICKS).find((x) => x in own.dataset);
    if (DESK_AWAY_GOES.includes(k)) deskAwayEdit = null;
    DESK_CLICKS[k](own);
    if (!deskAwayEdit) renderMain();
    return;
  }
  const r = ev.target.closest?.("[data-away-rule]"), b = ev.target.closest?.("[data-away]:not([data-away=scrim])") || (ev.target.dataset?.away === "scrim" ? ev.target : null);
  const dk = ev.target.closest?.("[data-away-desk]");
  if (!r && !b && !dk) return;
  ev.stopPropagation();
  if (dk) {
    const row = deskAwayEdit.desks.find((x) => x.project === dk.dataset.awayDesk);
    if (!row) return;
    row.on = !row.on;
    // Switched on from off: its lead sweeps too, and "Before you go" waits for it.
    if (row.on && !row.was && !row.swept) { deskAwayEdit.shown = null; deskAwaySweep(row); }
    return renderMain();
  }
  if (r) {
    const k = r.dataset.awayRule;
    deskAwayEdit.rules[k] = !deskAwayEdit.rules[k];
    return renderMain();
  }
  const act = b.dataset.away;
  if (act === "scrim" || act === "cancel") { deskAwayEdit = null; return renderMain(); }
  if (act === "off") return deskAwaySave(false);
  if (act === "done" && !b.disabled) return deskAwaySave(true);
}, true);
document.addEventListener("input", (ev) => {
  const f = ev.target.closest?.("[data-away-field]");
  if (f && deskAwayEdit) deskAwayEdit[f.dataset.awayField] = f.value;
}, true);
document.addEventListener("keydown", (ev) => {
  if (ev.key !== "Escape" || !deskAwayEdit) return;
  ev.stopPropagation(); ev.preventDefault();
  deskAwayEdit = null;
  renderMain();
}, true);

// ---------- Pause (#366): every session on the Desk stops at a safe point; Resume picks them all back up ----------

/** "Paused" by the Items title while the Desk is paused (desk_pause). */
function deskPausedTag(d) {
  return d?.paused?.on ? `<span class="dk-pz-tag">Paused</span>` : "";
}
/** Pause, or Resume while paused. */
function deskPauseButton(d) {
  if (!d) return "";
  const on = !!d.paused?.on, busy = deskPausing.has(d.project);
  return `<button class="btn small dk-pz${on ? " on" : ""}" data-desk-pause="${esc(d.project)}"${busy ? " disabled" : ""}>${on ? "Resume" : "Pause"}</button>`;
}
const deskPausing = new Set();
async function deskPauseToggle(project) {
  const d = deskOf(project);
  if (!d || deskPausing.has(project)) return;
  const on = !d.paused?.on;
  deskPausing.add(project); renderMain();
  try {
    const view = await invoke("desk_pause", { desk: d.id || d.project, on });
    if (view && typeof view === "object") d.paused = view;
  } catch (err) { toast(`Couldn't ${on ? "pause" : "resume"} the Desk: ${err}`); }
  deskPausing.delete(project); renderMain();
}
