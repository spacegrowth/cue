// Cue UI desk — Away (#361, #373, #415) and Pause (#366): the lines that show the lead keep the Desk moving while you're away, its rules and its clicks; and the Pause/Resume button.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (and after desk/list.js).
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
  // In a narrow column (#412) the until time stays whole and what it spent shortens first.
  const [until, ...rest] = on ? deskAwayStatus(deskAwayOf(d)).split(" · ") : [];
  const st = on ? `<span class="dk-aw-st${d.away.ended ? " ended" : ""}"><span class="dk-aw-until">${esc(until)}</span>${rest.length ? `<span class="dk-aw-spent"> · ${esc(rest.join(" · "))}</span>` : ""}</span>` : "";
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
 *  permissions) from its sessions not already on an item }, leaving out sessions paused on a usage limit, a lead's
 *  question about another Desk (#613: a lead on two Desks asks each about its own), and a lead's question about an
 *  item that has finished since (done, landed, dropped: the hub closes those too, desk_close_finished_asks). */
function deskAwayNeeds(d, taskApprovals = deskTaskApprovals(d)) {
  const waits = deskWaitingItems(d, taskApprovals).filter((i) => !deskPausedTill(deskHolderSid(i)));
  const placed = new Set([...taskApprovals.values()].map((r) => r.id));
  for (const i of d.items) if (i.ask?.item_id) placed.add(i.ask.item_id);
  const asks = state.items.filter((it) => !placed.has(it.id) && deskHasSession(d, it.session_id) && it.status === "pending" && it.kind !== "waiting"
    && !answering.has(it.id) && !deskPausedTill(it.session_id) && deskAskIsHere(it, d));
  return { waits, asks };
}
/** Whether open ask `it` is Desk `d`'s: a lead's question names its Desk (desk_id) and the item it's about (desk_item),
 *  which must still be open on it. */
function deskAskIsHere(it, d) {
  if (!it.desk) return true;
  const dk = it.tool_input?.desk_id, about = it.tool_input?.desk_item;
  if (dk && dk !== d.project && dk !== d.id) return false;
  if (!about) return true;
  const i = d.items.find((x) => x.id === about);
  return !!i && i.state !== "done" && i.state !== "dropped";
}
/** The same as one list for "Before you go" (#618): the lead's asks first, newest first, then the other sessions' asks,
 *  then the items that wait; each { key, desk, back (the lead's own ASK: it goes back to the lead once Away is on),
 *  stale (a session's ask whose items on this Desk have all finished: it no longer needs you), at (when asked, ms; 0 for
 *  an item), html }. */
function deskAwayFirstList(d) {
  const { waits, asks } = deskAwayNeeds(d), lead = deskLead(d)?.sid;
  const at = (it) => it.created_ms || it.at_ms || deskMs(it.created) || 0;
  const sorted = [...asks].sort((x, y) => (y.session_id === lead) - (x.session_id === lead) || at(y) - at(x));
  const stale = (sid) => { const its = sid && sid !== lead ? d.items.filter((i) => deskHolderSid(i) === sid) : []; return its.length > 0 && its.every((i) => i.state === "done" || i.state === "dropped"); };
  return [...sorted.map((it) => ({ key: `a:${it.id}`, desk: d.project, back: !!it.desk && it.session_id === lead, stale: stale(it.session_id), at: at(it), html: () => deskAwayAsk(it, d) })),
    ...[...waits].filter((i) => !deskAwaySent.has(`i:${i.id}`)).sort((x, y) => deskMs(y.updated) - deskMs(x.updated)).map((i) => ({ key: `i:${i.id}`, desk: d.project, at: 0, html: () => deskAwayItem(i, d) }))];
}
/** An item that waits on you, whole (#618: not the Desk list's short row, cut to a line): its number and who holds it,
 *  its full title and You'll see, then what it waits on you for, answerable right here: a question's card with its
 *  choices, an option to pick, a question in its holder's words with a box to answer it, or Looks right. Nothing in it
 *  opens a page. */
function deskAwayItem(i, d) {
  const who = deskHolderOf(i);
  const head = `<div class="dk-aw-ih">${deskKindIcon(i)}<b>${esc(deskNum(i))}</b>${who ? `<span aria-hidden="true">·</span><span>${esc(who)}</span>` : ""}</div>`
    + `<div class="dk-aw-it">${esc(i.title)}</div>${i.see ? `<div class="dk-aw-see">You'll see: ${esc(i.see)}</div>` : ""}`;
  const request = deskItemRequest(i);
  let body;
  if (request) body = deskAwayAsk(request, d);
  else if (deskPickOptions(i).length) body = `<p class="dk-aw-q">${esc(deskPickLine(i))}</p>${deskPickBtns(i)}`;
  else if (i.state === "needs_you") {
    const q = deskQuestion(i);
    // To its holder, about it (deskSendItem works out where it goes); no holder: to the lead, about it.
    const foot = i.holder ? deskItemFoot(i, d) : (() => { const key = deskBoxKey({ project: d.project, item: i.id }), line = deskToLine(deskLeadName(d), i); return boxHtml({ key, top: deskPreHtml(line.pre), placeholder: line.ph, send: { attrs: `data-desk-send="${esc(key)}"` } }); })();
    body = `${q ? `<div class="dk-aw-q dk-md">${deskMd(q)}</div>` : ""}${foot}`;
  } else {
    const where = deskWhereLine(i)?.html || "";
    body = `${where ? `<p class="dk-aw-q">${where}</p>` : ""}<div class="row-btns"><button class="btn primary" data-desk-act="looks_right" data-id="${esc(i.id)}">Looks right</button></div>`;
  }
  return `<div class="dk-aw-item" data-away-item="${esc(i.id)}">${head}${body}</div>`;
}
/** Items you wrote to from "Before you go" (their key, "i:<id>"): answered, so it moves on, though the item waits on
 *  you until its holder reads it. Kept while the sheet is open (or set aside), cleared when Away opens afresh. */
const deskAwaySent = new Set();
/** Box `to` sent (desk/act.js deskSendBox): an item answered from the sheet counts as answered there. */
function deskAwayNoteSent(to) {
  if (to?.item && (deskAwayEdit || deskAwayAside) && document.querySelector(`.dk-aw-dialog [data-away-item="${CSS.escape(to.item)}"]`)) deskAwaySent.add(`i:${to.item}`);
}
/** An open ask, answerable where it is: who asks and what, a single question's choices, then the controls its card
 *  has everywhere (decisionButtons: Allow / No, Decline / Type answer…, a form's fields). */
function deskAwayAsk(it, d, deny = "No") {
  const qs = it.kind === "question" ? questions(it) : [];
  // A question: its whole card (board/question.js questionHtml), its suggestions picked; Send is in its buttons.
  const picks = qs.length ? questionHtml(it) : "";
  const acts = ["permission", "question", "input"].includes(it.kind) ? decisionButtons(it, deny) : `<div class="row-btns"><button class="btn" ${deskChatAttrs(it.session_id, d)}>Open its chat</button></div>`;
  const cmd = it.tool_input?.command;
  const what = it.kind === "input" ? formBody(it) : cmd ? cmdCut(it.id, cmd) : qs.length ? "" : `<p>${esc(summary(it))}</p>`;
  return `<div class="dk-request dk-aw-ask" data-desk-request="${esc(it.id)}"><div class="dk-request-who">${esc(deskHolderName(it.session_id))}${it.agent_name ? ` · ${esc(it.agent_name)}` : ""}</div>${what}${picks}${acts}</div>`;
}
/** Away (#415, calls 1): the lead's calls since Away went on sit in one line at the top of Items, "4 calls while away ·
 *  since 16:00", with till, spend and a spend bar; open, each call with its time, why and Undo. Folded, one line; the
 *  fold is remembered per Desk. Rows stay where they are. */
const DESK_AWAY_ICON = `<svg class="dk-aw-ic" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5Z"/></svg>`;
/** The lead's calls since Away went on, oldest first (the list reads down in the order they were made); none while off. */
function deskAwayCalls(d) {
  const a = d?.away;
  if (!a?.on) return [];
  // Calls are kept to the second: one made in Away's first second counts.
  const since = Math.floor(deskMs(a.since) / 1000) * 1000;
  return deskCalls(d).filter((c) => deskMs(c.at) >= since).sort((x, y) => deskMs(x.at) - deskMs(y.at));
}
/** The Desks whose calls line you folded (kept in this window: a convenience, fine to lose). */
let deskAwayFolded = new Set();
try { deskAwayFolded = new Set(JSON.parse(localStorage.getItem("cue.desk.awayFolded") || "[]")); } catch {}
function deskAwayFold(project) {
  if (!deskAwayFolded.delete(project)) deskAwayFolded.add(project);
  try { localStorage.setItem("cue.desk.awayFolded", JSON.stringify([...deskAwayFolded])); } catch {}
  renderMain();
}
/** One call in the open list: its time, what was decided (its item) · why, Undo. */
function deskAwayMark(c, d) {
  const undo = c.undo === "can" ? `<button class="dk-aw-undo" data-desk-undo="${esc(c.id)}">Undo</button>` : c.undo === "asked" ? `<span class="dk-u">Undo asked</span>` : "";
  // Its item ("Task #287"), unless the call already names it.
  const item = c.item_id && !String(c.text || "").includes(`#${c.item_id}`) ? deskNum((d?.items || []).find((i) => i.id === c.item_id) || { id: c.item_id }) : "";
  const why = [item, c.why ? deskOneLine(c.why) : ""].filter(Boolean).join(" · ");
  // Decided in place of a question you'd have been asked (#471): the question, under it.
  const asked = c.asked ? `<span class="dk-aw-asked">Instead of asking you: ${esc(deskOneLine(c.asked))}</span>` : "";
  return `<div class="dk-aw-mark" data-desk-call="${esc(c.id)}"><span class="dk-aw-tm">${c.at ? esc(deskHm(deskMs(c.at))) : ""}</span><span class="dk-aw-mt"><b>${esc(c.text)}</b>${why ? `<span class="dk-aw-why"> · ${esc(why)}</span>` : ""}${asked}</span>${undo}</div>`;
}
/** The calls line at the top of Items while Away is on: the count and since when, then till, spend and its bar (or why
 *  it paused); a click folds or opens the list under it. */
function deskAwayCallsHtml(d) {
  const a = d?.away;
  if (!a?.on) return "";
  const calls = deskAwayCalls(d), open = calls.length > 0 && !deskAwayFolded.has(d.project);
  const n = calls.length ? `${calls.length} call${calls.length === 1 ? "" : "s"} while away` : "No calls yet";
  const since = a.since ? `<span class="dk-aw-w dk-aw-since">since ${esc(deskHm(deskMs(a.since)))}</span>` : "";
  const st = a.ended ? `<span class="dk-aw-w ended">${esc(deskAwayStatus(a))}</span>`
    : `<span class="dk-aw-w">${esc([a.until ? `till ${deskHm(a.until)}` : "", a.budget != null ? `${deskMoney(a.spent)} of ${deskMoney(a.budget)}` : ""].filter(Boolean).join(" · "))}</span>`;
  const pct = a.budget > 0 ? Math.min(100, Math.round((Number(a.spent) || 0) / a.budget * 100)) : null;
  const bar = pct == null ? "" : `<span class="dk-aw-meter" role="img" aria-label="${pct}% of the budget spent"><i style="width:${pct}%"></i></span>`;
  const chev = calls.length ? `<span class="dk-aw-chev${open ? " open" : ""}" aria-hidden="true">›</span>` : "";
  const head = `<button class="dk-aw-fold" data-desk-away-fold="${esc(d.project)}"${calls.length ? ` aria-expanded="${open}"` : " disabled"}>${chev}${DESK_AWAY_ICON}<b>${esc(n)}</b>${since}<span class="grow"></span>${st}${bar}</button>`;
  return `<div class="dk-aw-calls${open ? " open" : ""}">${head}${open ? `<div class="dk-aw-list">${calls.map((c) => deskAwayMark(c, d)).join("")}</div>` : ""}</div>`;
}
/** The dispatcher's Away tab (#416): each past Away stretch, newest first (desk/hub/away/away_log.rs). Folded, one line:
 *  when, how long, spend and counts; open, the items that moved, the lead's calls (Undo while it can still be undone)
 *  and the steps held for you, allowed or refused. Which ones are open is kept in this window only. */
const deskAwayLogOpen = new Set();
function deskAwayLogToggle(id) {
  if (!deskAwayLogOpen.delete(id)) deskAwayLogOpen.add(id);
  renderMain();
}
/** An item's state in plain words. */
const DESK_STATE_WORDS = { new: "new", queued: "queued", working: "working", finishing: "finishing", needs_you: "waiting on you", to_check: "ready to check", done: "done", dropped: "dropped", paused: "paused", needs_resume: "stopped", held: "parked" };
const deskStateWord = (s) => DESK_STATE_WORDS[s] || String(s || "").replace(/_/g, " ");
/** How long: "8h 50m", "45m", "under a minute". */
function deskAwayLong(ms) {
  const m = Math.round(Math.max(0, ms) / 60000);
  if (m < 1) return "under a minute";
  const h = Math.floor(m / 60);
  return h ? `${h}h${m % 60 ? ` ${m % 60}m` : ""}` : `${m}m`;
}
/** When a stretch ran: "07:00 – 15:50" today, "Nov 18, 22:10 – 07:00" before (the end's day too when it's another
 *  day than the start's). */
function deskAwayRan(since, end) {
  const day = (ms) => { const t = new Date(ms); return t.toDateString() === new Date(now()).toDateString() ? "" : `${t.toLocaleDateString([], { month: "short", day: "numeric" })}, `; };
  const same = new Date(since).toDateString() === new Date(end).toDateString();
  return `${day(since)}${deskHm(since)} – ${same ? "" : day(end)}${deskHm(end)}`;
}
const deskCount = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
/** One stretch: its line, and open, what happened. */
function deskAwayStretch(r, d) {
  const open = deskAwayLogOpen.has(r.id);
  const items = r.items || [], calls = r.calls || [], held = r.held || [];
  const counts = [items.length ? `${items.length} moved` : "", calls.length ? deskCount(calls.length, "call") : "", held.length ? `${held.length} held` : ""].filter(Boolean);
  const facts = [deskAwayLong(r.end - r.since), r.spent > 0 || r.budget != null ? deskMoney(r.spent) : "", ...(counts.length ? counts : ["nothing happened"]),
    r.ended === "budget" ? "budget spent" : r.ended === "until" ? "ran to its end time" : ""].filter(Boolean).join(" · ");
  const head = `<button class="dk-aw-fold dk-al-h" data-desk-away-log="${esc(r.id)}" aria-expanded="${open}"><span class="dk-aw-chev${open ? " open" : ""}" aria-hidden="true">›</span>${DESK_AWAY_ICON}<span class="dk-al-tx"><b>${esc(deskAwayRan(r.since, r.end))}</b> <span class="dk-aw-w">${esc(facts)}</span></span></button>`;
  if (!open) return `<div class="dk-al" data-key="al:${esc(r.id)}">${head}</div>`;
  const sec = (t, rows) => rows ? `<div class="dk-al-s">${esc(t)}</div>${rows}` : "";
  const moved = items.map((i) => {
    const there = (d.items || []).some((x) => x.id === i.id);
    const num = `<b class="dk-inum">#${esc(i.id)}</b>`;
    const what = `${num} <span class="dk-al-t">${esc(i.title)}</span><span class="dk-aw-why"> · ${esc(i.from ? `${deskStateWord(i.from)} → ${deskStateWord(i.to)}` : `added · ${deskStateWord(i.to)}`)}</span>`;
    return there ? `<button class="dk-aw-mark dk-al-row" data-desk-chk-item="${esc(i.id)}"><span class="dk-aw-mt">${what}</span></button>` : `<div class="dk-aw-mark dk-al-row"><span class="dk-aw-mt">${what}</span></div>`;
  }).join("");
  const answer = (h) => h.answer === "allow" ? "Allowed" : h.answer === "no" ? "Refused" : "Not answered yet: under Waiting on you";
  const heldRows = held.map((h) => `<div class="dk-aw-mark dk-al-held" data-held-log="${esc(h.id)}"><span class="dk-aw-tm">${esc(deskHm(deskMs(h.at)))}</span><span class="dk-aw-mt"><b>${esc(h.who || "A session")} wanted to ${esc(h.what)}</b><span class="dk-aw-why"> · ${esc(answer(h))}</span><code class="dk-al-cmd">${esc(h.command)}</code></span></div>`).join("");
  const body = sec("Items", moved) + sec("Calls", calls.map((c) => deskAwayMark(c, d)).join("")) + sec("Held for you", heldRows);
  return `<div class="dk-al open" data-key="al:${esc(r.id)}">${head}<div class="dk-aw-list">${body || `<div class="dk-aw-none">Nothing moved, no calls, nothing held.</div>`}</div></div>`;
}
function deskAwayLog(d) {
  const log = d.away_log || [];
  const body = log.length ? log.map((r) => deskAwayStretch(r, d)).join("") : `<div class="dk-empty">Nothing yet. Each time Away goes off, what happened while you were away is kept here: the items that moved, ${esc(deskLeadName(d) || "the lead")}'s calls and the steps held for you.</div>`;
  return `<div class="dk-pane dk-away-log" data-scroll-key="awaylog:${esc(d.project)}">${body}</div>`;
}
/** Away on a Desk now: the head says so (#415); the window keeps its own light or dark (#459). */
const deskAwayOnDesks = () => (state.desk?.projects || []).filter((d) => d.away?.on);
/** The Desks the head speaks for (#643): on one Desk's page only that Desk, so another Desk's Away never shows there;
 *  anywhere else (the Board, All projects…) every Desk away. */
function deskAwayShown() {
  const on = deskAwayOnDesks(), p = view === "desk" && typeof deskSelection === "function" ? deskSelection() : "";
  return p ? on.filter((d) => d.project === p) : on;
}
/** The head's mark: "Away until 18:00 · $3.40 of $10" (or why it paused); with several Desks away, how many. Off a
 *  Desk's own page, one Desk away is named: "cue: Away until 18:00". */
function deskAwayHeadMark() {
  const on = deskAwayShown();
  if (!on.length) return "";
  const a = deskAwayOf(on[0]);
  const own = view === "desk" && typeof deskSelection === "function" && !!deskSelection();
  const main = on.length > 1 ? `Away on ${on.length} Desks` : `${own ? "" : `${deskName(deskOf(on[0].project))}: `}${a.ended ? "Away" : a.until ? `Away until ${deskHm(a.until)}` : "Away"}`;
  const rest = on.length > 1 ? "" : a.ended ? deskAwayStatus(a) : a.budget != null ? `${deskMoney(a.spent)} of ${deskMoney(a.budget)}` : "";
  return `<span class="top-away" data-tauri-drag-region>${DESK_AWAY_ICON.replace('width="12" height="12"', 'width="14" height="14"')}<span>${esc(main)}</span>${rest ? `<span class="top-away-m">· ${esc(rest)}</span>` : ""}</span>`;
}
/** What Away adds under Waiting on you: open asks from the Desk's sessions not already on an item (a permission with
 *  Allow / Deny right there; the lead never grants one), and sessions on a usage limit as one quiet line. */
function deskAwayWaiting(d, taskApprovals) {
  // Steps held while you were away (#406): listed until you answer them, Away on or not.
  const held = deskHeldRows(d);
  if (!d?.away?.on) return { n: held.n, html: held.html };
  const { asks } = deskAwayNeeds(d, taskApprovals);
  const askRow = (it) => it.kind === "permission" ? deskAwayAsk(it, d, "Deny") : `<button class="dk-prow" ${deskChatAttrs(it.session_id, d)}><i class="dk-pdot ask" aria-hidden="true"></i><span class="dk-prt"><b>${esc(deskHolderName(it.session_id))}</b> <span class="dk-prs">asks you · ${esc(deskOneLine(summary(it)))}</span></span></button>`;
  const paused = [d.lead?.sid, ...(d.members || [])].filter((sid, k, all) => sid && all.indexOf(sid) === k && deskPausedTill(sid));
  const till = paused.length ? Math.max(...paused.map(deskPausedTill)) : 0;
  const quiet = paused.length ? `<div class="dk-aw-quiet">${esc(paused.map((sid) => deskHolderName(sid)).join(", "))} paused${till > 1 ? ` till ${deskHm(till)}` : ""} · Cue carries on then</div>` : "";
  return { n: held.n + asks.length, html: held.html + (asks.length ? `<div class="dk-prows dk-aw-asks">${asks.map(askRow).join("")}</div>` : "") + quiet };
}

/** Desk `d`'s held steps (desk/hub/away/away_holds.rs): who, what it wants to do in plain words, the exact command, Allow and No.
 *  Only what you allow runs, in the order the session asked. */
function deskHeldRows(d) {
  const held = d?.held || [];
  if (!held.length) return { n: 0, html: "" };
  const row = (h) => `<div class="dk-request dk-aw-ask dk-held" data-held-row="${esc(h.id)}"><div class="dk-request-who">${esc(h.who || deskHolderName(h.sid))} wants to ${esc(h.what)} · held while you were away</div>${cmdCut(`held:${h.id}`, h.command)}<div class="row-btns"><button class="btn deny flex" data-desk-held="${esc(h.id)}" data-allow="0">No</button><button class="btn primary flex" data-desk-held="${esc(h.id)}" data-allow="1">Allow</button></div></div>`;
  return { n: held.length, html: `<div class="dk-prows dk-aw-asks">${held.map(row).join("")}</div>` };
}
/** Your Allow or No to held step `id`: gone from the list at once; the session is told once its earlier ones are answered. */
async function deskHeldAnswer(id, allow) {
  const d = (state.desk?.projects || []).find((x) => (x.held || []).some((h) => h.id === id));
  if (d) d.held = d.held.filter((h) => h.id !== id);
  renderMain();
  try { await invoke("desk_held_answer", { id, allow }); } catch (err) { toast(`Couldn't answer the held step: ${err}`); }
}

/** The rules card, open: { project (the Desk it was opened on), on (Away is on there), until "HH:MM", budget "10",
 *  rules, busy, desks: every Desk as { project, desk, was (Away on as it opened), on (its switch), fresh (turned on
 *  just now: its lead's pass and "Before you go" are shown), swept (its lead's pass was asked for, or it needs none),
 *  ran (a pass was sent) }, shown: "Before you go", fixed once every pass is over, as { key, desk, back },
 *  step: the one showing, live: the keys still open at the last render, stale: how many were left out, open: the
 *  rules unfolded }. One switch for every Desk: Done sets Away on each Desk switched on, off on each switched off. */
let deskAwayEdit = null;
const deskAwayDesks = () => (state.desk?.projects || []);
/** The Away switch: off, one click turns it on (#612); on, it opens its rules. */
function deskAwayOpen(project) {
  deskAwayAside = null; deskAwaySent.clear();
  const d = deskOf(project), a = deskAwayOf(d);
  if (!a.on) return deskAwayQuick(project);
  const desks = deskAwayDesks().map((x) => { const was = !!deskAwayOf(x).on; return { project: x.project, desk: x.id || x.project, was, on: was, fresh: false, swept: true, ran: false }; });
  deskAwayEdit = { project, on: true, until: a.until ? deskHm(a.until) : "", budget: a.budget != null ? String(a.budget) : "",
    rules: { ...DESK_AWAY_DEFAULTS, ...(a.rules || {}) }, busy: false, shown: null, step: 0, live: new Set(), stale: 0, open: false, desks };
  renderMain();
}
/** One click from off (#612): this Desk goes Away, and only it (#643: other Desks stay as they were), with its last
 *  settings (07:00 and $10 the first time). Each lead goes through its open items only when something changed on its Desk since its last pass (the hub
 *  decides: desk_away_sweep). The card opens only when there's something to see: a pass going on, or what you could
 *  answer before you go; answering is optional, Away is already on. */
let deskAwayTurning = false;
async function deskAwayQuick(project) {
  if (deskAwayTurning) return;
  deskAwayAside = null; deskAwaySent.clear();
  const a = deskAwayOf(deskOf(project)), used = !!a.since;
  const e = { project, on: true, until: used ? (a.until ? deskHm(a.until) : "") : "07:00", budget: used ? (a.budget != null ? String(a.budget) : "") : "10",
    rules: { ...DESK_AWAY_DEFAULTS, ...(a.rules || {}) }, busy: false, shown: null, step: 0, live: new Set(), stale: 0, open: false,
    desks: deskAwayDesks().map((x) => { const was = !!deskAwayOf(x).on, me = x.project === project; return { project: x.project, desk: x.id || x.project, was, on: me || was, fresh: me && !was, swept: !me || was, ran: false }; }) };
  deskAwayTurning = true;
  try {
    // Only this Desk is set: another Desk keeps its own Away, on or off, and its own settings.
    const failed = await deskAwaySetAll({ ...e, desks: e.desks.filter((r) => r.project === project) }, true);
    if (failed.length) return toast(`Couldn't turn Away on: ${failed.join("; ")}`);
    for (const r of e.desks) r.was = r.on;
    renderMain();
    await Promise.all(e.desks.filter((r) => r.fresh).map((r) => deskAwaySweep(r, e)));
  } finally { deskAwayTurning = false; }
  if (deskAwayEdit) return;
  if (e.desks.some((r) => r.ran) || deskAwayFirst(e).length) { deskAwayEdit = e; renderMain(); }
  // Nothing to show: say it's on, with what it runs on, so the click isn't silent.
  else toast(deskAwayOnLine(e), 5000);
}
/** "Away is on until 07:00 · budget $10. Click Away again to change it or turn it off." */
function deskAwayOnLine(e) {
  const b = String(e.budget).trim();
  return `Away is on${e.until ? ` until ${e.until}` : ""} · ${b === "" ? "no budget limit" : `budget $${b}`}. Click Away again to change it or turn it off.`;
}
/** Ask Desk row `r`'s lead to sort its items out before Away (only when its Desk changed since its last pass: the hub
 *  says whether it sent it). Sent means the hub now has it sweeping (desk_away_sweep sets it before it returns): kept
 *  here too, so a state that hasn't caught up yet doesn't fix "Before you go" before the lead has gone through. */
function deskAwaySweep(r, e) {
  r.swept = false;
  const sent = (ran) => {
    const dd = deskAwayDesks().find((x) => x.project === r.project);
    if (ran && dd) dd.away = { ...deskAwayOf(dd), sweeping: true };
    r.swept = true;
    r.ran = r.ran || ran;
    if (deskAwayEdit === e) renderMain();
    return ran;
  };
  return invoke("desk_away_sweep", { desk: r.desk, rules: { ...e.rules } }).then((ran) => sent(ran === true), (err) => { toast(`${deskName(deskOf(r.project))}'s lead couldn't go through the items: ${err}`); return sent(false); });
}
/** What "Before you go" could hold now, across the Desks just turned on: the lead's asks, the others', items that wait. */
function deskAwayFirst(e) {
  const seen = new Set();
  return e.desks.filter((r) => r.on && r.fresh).flatMap((r) => { const d = deskOf(r.project); return d ? deskAwayFirstList(d) : []; }).filter((x) => !x.stale && !seen.has(x.key) && seen.add(x.key));
}
/** The rules in a line, for the folded row. */
const DESK_AWAY_SHORT = { route: "routing", how: "“how” questions", stopped: "stopped items", order: "order", what: "“what” questions", outside: "pulling in sessions" };
function deskAwayRulesLine(rules) {
  const may = Object.keys(DESK_AWAY_SHORT).filter((k) => rules[k]).map((k) => DESK_AWAY_SHORT[k]);
  if (!may.length) return "The lead decides nothing alone; everything waits for you";
  const list = may.length > 1 ? `${may.slice(0, -1).join(", ")} and ${may[may.length - 1]}` : may[0];
  return `The lead may decide ${list}; the rest waits for you`;
}
/** Where the ones you leave go: "Left unanswered: 1 goes back to its lead, 2 wait in Waiting on you." */
function deskAwayLeftLine(left) {
  if (!left.length) return "";
  const back = left.filter((x) => x.back).length, rest = left.length - back;
  const parts = [back ? `${back} ${back === 1 ? "goes back to its lead" : "go back to their leads"}` : "", rest ? `${rest} wait${rest === 1 ? "s" : ""} in Waiting on you` : ""].filter(Boolean);
  return `Left unanswered: ${parts.join(", ")}.`;
}
/** A question asked more than this long ago goes under "Older", folded (#613). */
const DESK_AWAY_OLD_MS = 24 * 3600000;
/** "Before you go" (#618, pick C): one question at a time, "1 of 5", with Skip; marks per Desk show where you are and
 *  which are answered, a click on one jumps there. Answering moves on by itself. Never holds Done. Each says which
 *  Desk it's from; this Desk's come first; questions older than a day wait under "Older", folded (#613). */
function deskAwayFirstHtml(e, going, many) {
  // This Desk's first, then the other Desks turned on with it.
  const order = [...going].sort((x, y) => (y.project === e.project) - (x.project === e.project));
  const seen = new Set(), all = order.flatMap((r) => { const d = deskOf(r.project); return d ? deskAwayFirstList(d) : []; }).filter((x) => !seen.has(x.key) && seen.add(x.key));
  const list = all.filter((x) => !x.stale), live = new Map(list.map((x) => [x.key, x]));
  if (!e.shown) {
    const old = (x) => x.at > 0 && now() - x.at > DESK_AWAY_OLD_MS;
    e.shown = list.filter((x) => !old(x)).map(({ key, desk, back }) => ({ key, desk, back: !!back }));
    e.older = list.filter(old).map(({ key, desk, back, at }) => ({ key, desk, back: !!back, at }));
    e.step = 0; e.live = new Set(live.keys()); e.stale = all.length - list.length;
  }
  const olderLive = (e.older || []).filter((x) => live.has(x.key));
  const olderHtml = deskAwayOlderHtml(e, olderLive, live);
  // The one showing was answered since the last render (Send, Allow, No…): on to the next open one.
  const at = e.shown[e.step];
  if (at && !live.has(at.key) && e.live.has(at.key)) {
    const k = e.shown.findIndex((x, n) => n > e.step && live.has(x.key));
    e.step = k < 0 ? e.shown.length : k;
  }
  e.live = new Set(live.keys());
  const left = [...e.shown.filter((x) => live.has(x.key)), ...olderLive];
  const staleLine = e.stale ? `<div class="dk-aw-more">Not shown: ${e.stale} that no longer need${e.stale === 1 ? "s" : ""} you</div>` : "";
  if (!e.shown.length) return { html: olderLive.length ? `<div class="dk-aw-oneh"><div class="dk-aw-h">Before you go</div></div>${olderHtml}${staleLine}` : staleLine, left };
  const groups = [...new Set(e.shown.map((x) => x.desk))].map((p) => {
    const pips = e.shown.map((x, k) => {
      if (x.desk !== p) return "";
      const done = !live.has(x.key), cur = k === e.step;
      return `<button class="dk-aw-pip${cur ? " cur" : done ? " ans" : ""}" data-away-step="${k}" aria-label="Question ${k + 1}${done ? ", answered" : ""}"${cur ? ' aria-current="step"' : ""}></button>`;
    }).join("");
    return `<span class="dk-aw-sg">${many ? `<span class="dk-aw-sl">${esc(deskName(deskOf(p)))}</span>` : ""}${pips}</span>`;
  }).join("");
  const head = `<div class="dk-aw-oneh"><div class="dk-aw-h">Before you go</div><span class="grow"></span><div class="dk-aw-steps">${groups}</div></div>`;
  const cur = e.shown[e.step], open = cur && live.get(cur.key);
  let body;
  if (open) {
    const ifLeft = cur.back ? "If you leave it, its lead decides" : "If you leave it, it waits for you";
    body = `<div class="dk-aw-nav"><span class="dk-aw-from">${esc(deskAwayFrom(cur.desk))}</span><span aria-hidden="true">·</span><span>${e.step + 1} of ${e.shown.length}</span><span aria-hidden="true">·</span><span>${ifLeft}</span><span class="grow"></span><button class="btn dk-aw-skip" data-away-skip>Skip</button></div><div class="dk-aw-first">${open.html()}</div>`;
  } else {
    const end = cur ? "Answered." : left.length ? `That's all of them. ${left.length} left for later.` : `All answered. ${e.on ? "Nothing waits on you." : "Turn on Away when you're ready."}`;
    body = `<div class="dk-aw-end">${esc(end)}</div>`;
  }
  return { html: head + body + olderHtml + staleLine, left };
}
/** "Cue Desk": where a question comes from. */
const deskAwayFrom = (project) => `${deskName(deskOf(project)) || project} Desk`;
/** "Older · 3", folded: questions asked more than a day ago, each with its Desk and when, answerable in place. */
function deskAwayOlderHtml(e, older, live) {
  if (!older.length) return "";
  const head = `<button class="dk-aw-fold dk-aw-older" data-away-older aria-expanded="${!!e.olderOpen}"><span class="dk-aw-chev${e.olderOpen ? " open" : ""}" aria-hidden="true">›</span><b>Older</b><span class="dk-aw-w">${older.length}</span></button>`;
  if (!e.olderOpen) return head;
  const day = (ms) => new Date(ms).toLocaleDateString([], { day: "numeric", month: "short" });
  const rows = older.map((x) => `<div class="dk-aw-oldrow"><div class="dk-aw-nav"><span class="dk-aw-from">${esc(deskAwayFrom(x.desk))}</span><span aria-hidden="true">·</span><span>asked ${esc(day(x.at))}</span></div>${live.get(x.key).html()}</div>`).join("");
  return `${head}<div class="dk-aw-first dk-aw-olderlist">${rows}</div>`;
}
function deskAwayHtml() {
  const e = deskAwayEdit;
  if (!e) return "";
  const tg = (on) => `<span class="dk-tg${on ? " on" : ""}" aria-hidden="true"></span>`;
  const row = (k, label, on) => `<button class="dk-aw-row" role="switch" aria-checked="${on}" data-away-rule="${k}"><span class="grow">${esc(label)}</span>${tg(on)}</button>`;
  // Just turned on: the lead's pass when one runs, then what you could answer before you go, one at a time (#618).
  // All optional (#612): Away is on either way, Done never waits; what you leave goes back to its lead or waits.
  const going = e.desks.filter((r) => r.on && r.fresh), many = e.desks.length > 1, manyGoing = going.length > 1;
  const sweeping = going.filter((r) => !r.swept || deskOf(r.project)?.away?.sweeping);
  let first = "", left = [];
  if (sweeping.length) {
    const who = manyGoing ? `${sweeping.length === 1 ? `${deskName(deskOf(sweeping[0].project))}'s lead is` : `${sweeping.length} leads are`}` : "The lead is";
    first = `<div class="dk-aw-sweep"><i class="dk-aw-spin" aria-hidden="true"></i>${esc(who)} going through the open items…</div>`;
  } else if (going.length) {
    ({ html: first, left } = deskAwayFirstHtml(e, going, manyGoing));
    if (e.shown?.length) first += `<div class="dk-aw-opt">Optional: Away is already on. Skip any you'd rather leave.</div>`;
  }
  // Says which Desks are away (#643): one Desk on is "this Desk" (or named, when it's another), never "each Desk".
  const onRows = e.desks.filter((r) => r.on);
  const keeps = onRows.length > 1 ? "Each Desk's lead keeps it moving while you're away."
    : onRows.length && onRows[0].project !== e.project ? `${deskName(deskOf(onRows[0].project))}'s lead keeps it moving while you're away.`
    : "The lead keeps this Desk moving while you're away.";
  const lead = e.desks.some((r) => r.fresh) ? `Away is on. ${keeps}` : keeps;
  const may = DESK_AWAY_RULES.map(([k, l]) => row(k, l, !!e.rules[k])).join("");
  const desks = many ? `<div class="dk-aw-h">Desks</div><div class="dk-aw-rows">${e.desks.map((r) => `<button class="dk-aw-row" role="switch" aria-checked="${r.on}" data-away-desk="${esc(r.project)}"><span class="grow">${esc(deskName(deskOf(r.project)))}</span>${tg(r.on)}</button>`).join("")}</div>` : "";
  const lock = DESK_AWAY_LOCKED.map((l) => `<div class="dk-aw-row locked"><span class="grow">${esc(l)}</span><span class="dk-aw-lock">${DESK_LOCK}</span></div>`).join("")
    + DESK_AWAY_WAITS.map(([k, l]) => row(k, l, !e.rules[k])).join("");
  // With several Desks, which are on sits beside Until and Budget; a click opens the switches.
  const onDesks = e.desks.filter((r) => r.on).map((r) => deskName(deskOf(r.project)));
  const deskFld = many ? `<button class="dk-aw-fld" data-away-rules aria-expanded="${e.open}">Desks<b>${esc(onDesks.join(", ") || "None")}</b></button>` : "";
  const rules = e.open ? `${desks}<div class="dk-aw-h">The lead may decide</div><div class="dk-aw-rows">${may}</div>
    <div class="dk-aw-h">Always waits for you</div><div class="dk-aw-rows">${lock}</div>
    <div class="dk-aw-foot">One notification only when nothing can move without you.</div>` : "";
  return `<div class="move-scrim dk-aw-scrim" data-away="scrim"><div class="move-dialog dk-aw-dialog" role="dialog" aria-labelledby="dk-aw-title">
    <div class="move-title" id="dk-aw-title">Away</div>
    <div class="end-text">${lead}</div>
    <div class="dk-aw-flds"><label>Until<input type="time" data-away-field="until" value="${esc(e.until)}"/></label><label>Budget ($)<input type="text" inputmode="decimal" data-away-field="budget" value="${esc(e.budget)}" placeholder="No limit"/></label>${deskFld}</div>
    ${first}
    <button class="dk-aw-sum" data-away-rules aria-expanded="${e.open}"><span class="grow">${esc(deskAwayRulesLine(e.rules))}</span><span class="dk-aw-sum-m">${e.open ? "Hide" : "Change"}</span></button>
    ${rules}
    <div class="row-btns"><span class="dk-aw-left">${esc(deskAwayLeftLine(left))}</span><span class="grow"></span><button class="btn" data-away="off"${e.busy ? " disabled" : ""}>Turn Away off</button><button class="btn primary" data-away="done"${e.busy ? " disabled" : ""}>Done</button></div>
  </div></div>`;
}
/** The sheet set aside (#618): something in it opened a page (a session's chat, an item). It isn't thrown away: a bar
 *  at the top of the Desk, "‹ Back to Away · 2 of 5", brings it back where you left off, with Until, Budget and your
 *  answers kept; Esc does too; × lets it go. */
let deskAwayAside = null;
function deskAwayBackHtml() {
  const e = deskAwayAside;
  if (!e || view !== "desk") return "";
  const n = e.shown?.length || 0, at = n ? Math.min(e.step, n - 1) + 1 : 0;
  return `<div class="dk-aw-back" role="group" aria-label="Away"><button class="dk-aw-backb" data-away-back>‹ Back to Away${n ? ` · ${at} of ${n}` : ""}</button><button class="dk-aw-backx" data-away-back-x aria-label="Close">×</button></div>`;
}
/** The middle card with the bar at its top right, where nothing else sits. */
function deskAwayBackIn(html) {
  const bar = deskAwayBackHtml();
  // On today's page (B2, #666) the chat card or the open page beside it, whichever is the dk-right one.
  return bar ? html.replace(/^(<div class="dk-card(?: [^"]*)? dk-right(?: [^"]*)?">)/, `$1${bar}`) : html;
}
/** Set the sheet aside, to come back to (deskAwayBackHtml). */
function deskAwaySetAside() {
  deskAwayAside = deskAwayEdit;
  deskAwayEdit = null;
}
/** Back to the sheet where you left off. */
function deskAwayBack() {
  if (!deskAwayAside) return;
  deskAwayEdit = deskAwayAside;
  deskAwayAside = null;
  renderMain();
}
/** Away set on every Desk of `e` as its switch says (`on` false: off on all); a Desk that was off and stays off isn't
 *  touched. Each Desk's view kept at once. Returns what failed, in words. */
async function deskAwaySetAll(e, on) {
  const b = String(e.budget).trim(), budget = b === "" ? null : Number(b);
  const until = deskNextAt(e.until);
  const failed = [];
  for (const r of e.desks) {
    const now = on && r.on;
    if (!now && !r.was) continue;
    try {
      const view = await invoke("desk_away_set", { desk: r.desk, away: { on: now, until, budget, rules: { ...e.rules } } });
      const d = deskAwayDesks().find((x) => x.project === r.project);
      if (view && typeof view === "object" && d) d.away = { ...view, sweeping: d.away?.sweeping };
    } catch (err) { failed.push(`${deskName(deskOf(r.project))}: ${err}`); }
  }
  return failed;
}
async function deskAwaySave(on) {
  const e = deskAwayEdit;
  if (!e || e.busy) return;
  const b = String(e.budget).trim();
  if (b !== "" && !(Number(b) >= 0)) return toast("The budget is a number of dollars");
  e.busy = true;
  renderMain();
  // Every Desk switched on gets these settings; one switched off (or all, for Turn Away off) goes off; one that was
  // off and stays off isn't touched.
  const failed = await deskAwaySetAll(e, on);
  if (failed.length) { e.busy = false; toast(`Couldn't change Away on ${failed.join("; ")}`); }
  else {
    if (deskAwayEdit === e) deskAwayEdit = null;
    // Turned off (on every Desk): say so, as turning it on does.
    if (!on || e.desks.every((r) => !r.on)) toast("Away is off. Everything waits for you again.", 5000);
  }
  renderMain();
}
/** The Desk's own buttons in the rules card (an item's Allow / No, a choice, Looks right): as on the Desk. One that
 *  opens a page closes the card first. */
const DESK_AWAY_GOES = ["deskItem", "deskChkItem", "deskChatOf", "deskLeadChat", "deskCrew"];
document.addEventListener("click", (ev) => {
  const back = ev.target.closest?.("[data-away-back], [data-away-back-x]");
  if (back && !deskAwayEdit) {
    ev.stopPropagation();
    if ("awayBackX" in back.dataset) { deskAwayAside = null; deskAwaySent.clear(); return renderMain(); }
    return deskAwayBack();
  }
  if (!deskAwayEdit) return;
  const own = ev.target.closest?.(".dk-aw-dialog") && ev.target.closest?.(deskSel());
  if (own && !own.closest("[data-away]:not([data-away=scrim]), [data-away-rule]")) {
    ev.stopPropagation();
    const k = Object.keys(DESK_CLICKS).find((x) => x in own.dataset);
    // One that opens a page sets the sheet aside: "Back to Away" brings it back (#618).
    if (DESK_AWAY_GOES.includes(k)) deskAwaySetAside();
    DESK_CLICKS[k](own);
    if (!deskAwayEdit) renderMain();
    return;
  }
  const r = ev.target.closest?.("[data-away-rule]"), b = ev.target.closest?.("[data-away]:not([data-away=scrim])") || (ev.target.dataset?.away === "scrim" ? ev.target : null);
  const dk = ev.target.closest?.("[data-away-desk]");
  const nav = ev.target.closest?.(".dk-aw-dialog [data-away-step], .dk-aw-dialog [data-away-skip], .dk-aw-dialog [data-away-rules], .dk-aw-dialog [data-away-older]");
  if (!r && !b && !dk && !nav) return;
  ev.stopPropagation();
  if (nav) {
    const e = deskAwayEdit;
    if ("awayRules" in nav.dataset) e.open = !e.open;
    else if ("awayOlder" in nav.dataset) e.olderOpen = !e.olderOpen;
    else if ("awayStep" in nav.dataset) e.step = Number(nav.dataset.awayStep);
    // Skip: on to the next one still open after it, else the end.
    else { const k = e.shown.findIndex((x, n) => n > e.step && e.live.has(x.key)); e.step = k < 0 ? e.shown.length : k; }
    return renderMain();
  }
  if (dk) {
    const row = deskAwayEdit.desks.find((x) => x.project === dk.dataset.awayDesk);
    if (!row) return;
    row.on = !row.on;
    // Switched on from off: its lead goes through its items too when its Desk changed, and "Before you go" shows what
    // it leaves. Done turns it on.
    if (row.on && !row.was && !row.fresh) { row.fresh = true; deskAwayEdit.shown = null; deskAwaySweep(row, deskAwayEdit); }
    // Switched off: its questions leave "Before you go" (it's built again from the Desks still going on).
    else if (!row.on) { row.fresh = false; deskAwayEdit.shown = null; }
    return renderMain();
  }
  if (r) {
    const k = r.dataset.awayRule;
    deskAwayEdit.rules[k] = !deskAwayEdit.rules[k];
    return renderMain();
  }
  const act = b.dataset.away;
  if (act === "scrim" || act === "cancel") { deskAwayEdit = null; deskAwaySent.clear(); return renderMain(); }
  if (act === "off") return deskAwaySave(false);
  if (act === "done" && !b.disabled) return deskAwaySave(true);
}, true);
document.addEventListener("input", (ev) => {
  const f = ev.target.closest?.("[data-away-field]");
  if (f && deskAwayEdit) deskAwayEdit[f.dataset.awayField] = f.value;
}, true);
/** Esc closes the Away sheet (app/keys.js KEY_FIRST), and only it: kept from the Desk's Esc and the Board's. On a page
 *  opened from the sheet, Esc goes back to the sheet first (#618). */
function deskAwayKey(ev) {
  if (ev.key !== "Escape") return false;
  if (!deskAwayEdit && deskAwayAside && view === "desk") { ev.stopPropagation(); ev.preventDefault(); deskAwayBack(); return true; }
  if (!deskAwayEdit) return false;
  ev.stopPropagation(); ev.preventDefault();
  deskAwayEdit = null; deskAwayAside = null; deskAwaySent.clear();
  renderMain();
  // Taken: the Desk under it doesn't see this Esc (#521).
  return true;
}

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
