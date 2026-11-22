// Cue UI desk — the Desk page as option B2 of #652 (#666): today's page unless "Try the new Desk page" is on
// (desk/v2.js). The lead's chat card at the left is always the lead's; Now and Waits on you fold into one strip over its
// chat (the Working pill unfolds the sessions at work in place, a line each); Show panel brings back the panel. At the
// right one item list with tabs by group, the long groups showing their first 3; an item, a session's chat or All
// sessions open there in the list's place, ‹ Items N going back. Drawn by desk/view.js deskView; styles/desk-b2.css.
// Loaded after desk/v2.js (index.html, mini.html).

/** Today's page is B2: everything but the new page (#628) when it's on. */
const deskB2On = () => !(typeof deskV2 !== "undefined" && deskV2);
/** Something is open on the right in its own page, which then is `.dk-right` (the card that shows what's open, the
 *  lead's chat card otherwise); All sessions is a list, not a page, so the chat card keeps it. */
const deskB2Side = () => deskB2On() && !!deskOpen && deskOpen.kind !== "sessions";
/** The Working pill in the strip is unfolded. */
let deskB2Unfold = false;
/** The list's tab: "all", "you" (Waits on you), "next" (Up next), "done" (Done today), "away" (Calls while away). */
let deskB2Tab = "all";
/** The groups shown whole after Show all, as "project|group". */
const deskB2Full = new Set();
/** How many rows a long group shows before Show all. */
const DESK_B2_SHOW = 3;
/** Each tab's groups (deskList's keys); Calls while away is the Away line over the list. */
const DESK_B2_TABS = { you: ["you", "check"], next: ["queued", "held"], done: ["done"], away: [] };
const DESK_B2_CARET = `<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>`;

/** Desk `d`'s open items, as the lead's line and the All tab count them. */
const deskB2Open = (d) => (d?.items || []).filter((i) => i.state !== "done" && i.state !== "dropped").length;
/** How many wait on you on Desk `d`, the same in the strip's words and on the Waits on you tab: what that tab lists (its
 *  questions, what's ready to check, the steps and asks held while away) and the lead's own ask. */
function deskB2WaitN(d, taskApprovals) {
  const g = deskGroups(d.project, taskApprovals), ask = deskLeadEnded(d) ? null : deskLeadAsk(d);
  return g.asks.length + g.check.length + deskAwayWaiting(d, taskApprovals).n + (ask ? 1 : 0);
}
/** Desk `d`'s finished items done today: the Done today tab's rows and its count. */
const deskB2DoneToday = (d) => deskDoneList(d).filter(deskDoneToday);
/** What an empty tab says in its list's place. */
const DESK_B2_NONE = { you: "Nothing waits on you.", next: "Nothing up next.", done: "Nothing done today.", away: "No calls while away." };
/** The way back to the list: "Items 40". */
const deskB2ItemsWord = (d) => `Items ${deskB2Open(d)}`;
/** "a", "a and b", "a, b and c". */
const deskB2Names = (ns) => (ns.length < 2 ? ns.join("") : `${ns.slice(0, -1).join(", ")} and ${ns.at(-1)}`);

// ---------- the strip over the lead's chat ----------

/** Now and Waits on you in one line: the Working pill, how many wait on you, a chip per item that waits (+N › for
 *  the rest), Check on the first; unfolded, a line per session at work and who else waits on you or has work queued. */
function deskB2Strip(d, waits, working, ask, taskApprovals) {
  const n = deskB2WaitN(d, taskApprovals), shown = waits.slice(0, 9);
  const pill = `<button class="dk2b-work${deskB2Unfold ? " on" : ""}" data-desk2b-work aria-expanded="${deskB2Unfold}"><i class="dk2b-wdot" aria-hidden="true"></i>Working ${working.length}${DESK_B2_CARET}</button>`;
  const words = `<span class="dk2b-wn">${n ? `${n} wait${n === 1 ? "s" : ""} on you` : "Nothing waits on you"}</span>`;
  const lead = ask ? `<button class="dk2b-chip ask" data-desk-tab="lead">${esc(deskLeadName(d))} asks</button>` : "";
  const chips = shown.map((i) => `<button class="dk2b-chip" data-desk-chk-item="${esc(i.id)}">#${esc(i.id)}</button>`).join("");
  const more = waits.length > shown.length ? `<button class="dk2b-more" data-desk2b-tab="you">+${waits.length - shown.length} ›</button>` : "";
  const check = waits.length ? `<button class="btn small primary dk2b-check" data-desk-chk-item="${esc(waits[0].id)}">Check #${esc(waits[0].id)}</button>` : "";
  return `<section class="dk2b-strip${deskB2Unfold ? " open" : ""}"><div class="dk2b-sr">${pill}${words}<span class="dk2b-chips">${lead}${chips}</span>${more}${check}</div>${deskB2Unfold ? deskB2Working(d, working) : ""}</section>`;
}
/** The unfolded Working list: a line per session at work (its name, its item, what it's running and since when, the
 *  item's steps); a click opens its item on the right (its chat when it holds none). Then a line naming the sessions
 *  that wait on you or have work queued, and All N sessions. */
function deskB2Working(d, working) {
  const o = deskOpen;
  const rows = working.map(({ sid, name, on }) => {
    const s = sessionOf(sid);
    const doing = s?.compacting_ms ? "Compacting…" : s?.running ? `Running: ${s.running}` : s?.doing || "Thinking…";
    const at = s?.compacting_ms || s?.doing_ms || s?.since_ms || 0;
    const lit = on ? o?.kind === "item" && deskIdIs(on, o.id) : o?.kind === "chat" && o.sid === sid;
    const go = on ? `data-desk-chk-item="${esc(on.id)}"` : deskChatAttrs(sid, d);
    const step = on && on.state !== "done" ? Math.max(0, deskProgAt(on)) + 1 : 0;
    return `<button class="dk2b-wrow${lit ? " on" : ""}" ${go} data-desk2b-sid="${esc(sid)}"><i class="dk2b-wdot" aria-hidden="true"></i><b class="dk2b-wnm">${esc(name)}</b><span class="dk2b-wid">${on ? `#${esc(on.id)}` : ""}</span><span class="dk2b-wdo">${esc(doing)}${at ? ` · <span data-ago="${at}">${ago(at)}</span>` : ""}</span>${on ? deskB2Track(on) : "<span></span>"}<span class="dk2b-wst">${step ? `${step}/6` : ""}</span></button>`;
  }).join("");
  // Who else: the sessions whose items wait on you, and those with an item queued for them.
  const waitNames = [...new Set(d.items.filter(deskWaits).map((i) => (i.holder ? deskHolderOf(i) : "")).filter(Boolean))];
  const queued = d.items.filter((i) => i.state === "queued" && (i.holder || i.next_for)).slice(0, 2).map((i) => `${i.holder ? deskHolderOf(i) : deskHolderOf({ holder: i.next_for })} has #${i.id} queued`);
  const say = [waitNames.length ? `${deskB2Names(waitNames.slice(0, 3))}${waitNames.length > 3 ? ` and ${waitNames.length - 3} more` : ""} ${waitNames.length === 1 ? "waits" : "wait"} on you` : "", ...queued].filter(Boolean).join(" · ");
  const all = deskB2SessionCount(d);
  return `<div class="dk2b-wlist">${rows || `<div class="dk2b-wnone">No session is at work right now.</div>`}<div class="dk2b-wfoot"><span class="dk2b-wsay">${esc(say)}</span><span class="grow"></span><button class="dk2b-alls" data-desk2b-sessions>All ${all} session${all === 1 ? "" : "s"} ›</button></div></div>`;
}
/** The sessions All N sessions lists: the lead and the Desk's sessions in its folder. */
const deskB2SessionCount = (d) => (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && s.session_id && deskHasSession(d, s.session_id)).length;

// ---------- the list: tabs, caps, a step track per row ----------

/** The tabs over the list: All, Waits on you, Up next, Done today, Calls while away, each with its count (those
 *  with none left out, but All and Waits on you). */
function deskB2Tabs(g, taskApprovals) {
  const tabs = deskB2TabCounts(g, taskApprovals);
  const b = ([k, label, n]) => `<button class="dk2b-tab${deskB2Tab === k ? " on" : ""}" role="tab" aria-selected="${deskB2Tab === k}" data-desk2b-tab="${k}">${label} <span class="n">${n}</span></button>`;
  return `<div class="dk2b-tabs" role="tablist" aria-label="Show">${tabs.filter(([k, , n]) => k === "all" || k === "you" || n || deskB2Tab === k).map(b).join("")}</div>`;
}
/** The tabs over the list, each [key, label, count]. */
function deskB2TabCounts(g, taskApprovals) {
  const d = g.d, away = d?.away?.on ? deskAwayCalls(d).length : 0;
  if (deskB2Tab === "away" && !d?.away?.on) deskB2Tab = "all";
  return [["all", "All", deskB2Open(d)], ["you", "Waits on you", deskB2WaitN(d, taskApprovals)], ["next", "Up next", g.queued.length + g.held.length], ["done", "Done today", deskB2DoneToday(d).length], ["away", "Calls while away", away]];
}
/** The list's groups the tab shows: all of them, or the tab's ([key, html] pairs from deskList), and the Away line. */
function deskB2Shown(parts, top) {
  if (deskB2Tab === "all") return { parts, top };
  const keys = DESK_B2_TABS[deskB2Tab] || [];
  return { parts: parts.filter(([k]) => keys.includes(k)), top: deskB2Tab === "away" ? top : "" };
}
/** A long group's rows in the All tab: its first 3 and Show all N, till that's pressed. [rows, Show all's html]. */
function deskB2Cap(key, list) {
  if (deskB2Tab !== "all" || list.length <= DESK_B2_SHOW || deskB2Full.has(`${deskSelection()}|${key}`)) return [list, ""];
  // The open item stays in view.
  const open = deskOpen?.kind === "item" ? list.find((i) => deskIdIs(i, deskOpen.id)) : null;
  const head = list.slice(0, DESK_B2_SHOW);
  if (open && !head.includes(open)) head.push(open);
  return [head, `<button class="dk2b-all" data-desk2b-all="${esc(key)}">Show all ${list.length}</button>`];
}
/** An item's six steps as six short dashes: done filled, the one it's at in its look, the rest faint. */
function deskB2Track(i) {
  const at = i.state === "done" ? 6 : deskProgAt(i), look = deskProgLook(i);
  return `<span class="dk2b-trk" aria-hidden="true">${DESK_PROG_STEPS.map((_, k) => `<i class="${k < at ? "done" : k === at ? `cur ${look}` : ""}"></i>`).join("")}</span>`;
}

// ---------- an item's page on the right ----------

/** Over an item's tabs: its progress (six steps), who has it, what it's doing, its tests, what's next; Open its chat
 *  and Changes (its Files tab). */
function deskB2ItemTop(i, d) {
  const look = deskProgLook(i), steps = deskProgSteps(i), at = i.state === "done" ? 6 : deskProgAt(i);
  const bar = DESK_PROG_STEPS.map((n, k) => {
    const st = i.state === "done" ? "done" : steps[k].state;
    return `<div class="dk2b-step ${st}${st === "cur" ? ` ${look}` : ""}"><i></i><span>${esc(n)}</span></div>`;
  }).join("");
  const sid = deskHolderSid(i), s = sid ? sessionOf(sid) : null;
  const since = s && i.state === "working" ? (deskProgStamps(i)[0] || s.since_ms || 0) : 0;
  const who = i.holder ? `<b>${esc(deskHolderOf(i))}</b>${since ? ` · working for <span data-ago="${since}">${ago(since)}</span>` : ""}` : "Nobody has it yet";
  const tests = i.state === "done" || at > 2 ? "Done" : at === 2 ? (s?.running ? `Running: ${esc(s.running)}` : "Running") : "Not run yet";
  const next = i.state === "done" ? "Nothing: it's done" : i.state === "to_check" ? "Waits on you to check it" : i.state === "needs_you" ? "Waits on your answer"
    : i.state === "held" ? "Starts when you press Start" : at < 0 ? esc(deskQueueLine(i) || "Starts when a session is free")
    : `Then ${DESK_PROG_STEPS.slice(at + 1).join(", ")}`;
  // Its question is in its chat just below (#418): said once, there, not again up here.
  const now = i.state === "needs_you" ? "Asks you something: its question is in the chat below" : deskProgNow(i);
  const line = (k, v) => `<div class="dk2b-ir"><span class="dk2b-ik">${k}</span><span class="dk2b-iv">${v}</span></div>`;
  const acts = `${s ? `<button class="btn small" ${deskChatAttrs(sid, d)}>Open its chat</button>` : ""}<button class="btn small" data-desk-itab="files" data-id="${esc(i.id)}">Changes</button>`;
  return `<div class="dk2b-itop"><div class="dk2b-ih">Progress</div><div class="dk2b-steps">${bar}</div>
    <div class="dk2b-irows">${line("Who", who)}${line("Doing now", now)}${line("Tests", tests)}${line("Next", next)}</div><div class="dk2b-iacts">${acts}</div></div>`;
}

// ---------- clicks ----------

deskClicks({
  desk2bWork: () => { deskB2Unfold = !deskB2Unfold; renderMain(); },
  // A tab (or +N › in the strip, to Waits on you): the list in place of what was open.
  desk2bTab: (el) => { deskB2Tab = el.dataset.desk2bTab in DESK_B2_TABS ? el.dataset.desk2bTab : "all"; deskShow(null); },
  desk2bAll: (el) => { deskB2Full.add(`${deskSelection()}|${el.dataset.desk2bAll}`); renderMain(); },
  desk2bSessions: () => deskShow({ kind: "sessions" }),
});

/** Desk settings for `project` (the header's Desks menu, head_view.js): today's settings screen (lands.js), or the
 *  new page's drawer when that page is on. */
function deskSettingsOpen(project) {
  if (!deskB2On()) return deskV2Open(project);
  const d = deskOf(project);
  // Every Desk the hub sends has how its work lands (desk/lands.rs desk_view), what it asks before on the same screen.
  landsChange(d?.id || d?.project || project);
}
