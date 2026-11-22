// Cue UI desk — the Desk page as option B2 of #652 (#666): today's page unless "Try the new Desk page" is on
// (desk/v2.js). The lead's chat card at the left is always the lead's; Now and Waits on you fold into one strip over its
// chat (the Working pill unfolds the sessions at work in place, a line each); Show panel brings back the panel. At the
// right one item list, a row of four columns per item, the long groups showing their first 3. An item, a session's
// chat or All sessions open on the left in the chat card's place, ‹ Items N or Esc going back to the lead's chat; the
// list stays, the open row lit in place. Drawn by desk/view.js deskView; styles/desk-b2.css.
// Loaded after desk/v2.js (index.html, mini.html).

/** Today's page is B2: everything but the new page (#628) when it's on. */
const deskB2On = () => !(typeof deskV2 !== "undefined" && deskV2);
/** Something open beside the lead's chat: never now (#666), what's open takes the chat card's place on the left. */
const deskB2Side = () => false;
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

/** Now or Waits on you, one of them (#666, so the strip never runs out of room): while anything waits on you, how many,
 *  a chip per item (the first 5, +N › for the rest) and Check on the first; with nothing waiting, the Working pill,
 *  unfolding a line per session at work and who else waits on you or has work queued. */
function deskB2Strip(d, waits, working, ask, taskApprovals) {
  const n = deskB2WaitN(d, taskApprovals);
  if (n || ask || waits.length) {
    const shown = waits.slice(0, ask ? 4 : 5);
    const words = `<span class="dk2b-wn">${n} wait${n === 1 ? "s" : ""} on you</span>`;
    const lead = ask ? `<button class="dk2b-chip ask" data-desk-tab="lead">${esc(deskLeadName(d))} asks</button>` : "";
    const chips = shown.map((i) => `<button class="dk2b-chip" data-desk-chk-item="${esc(i.id)}">#${esc(i.id)}</button>`).join("");
    const more = waits.length > shown.length ? `<button class="dk2b-more" data-desk2b-tab="you">+${waits.length - shown.length} ›</button>` : "";
    const check = waits.length ? `<button class="btn small primary dk2b-check" data-desk-chk-item="${esc(waits[0].id)}">Check #${esc(waits[0].id)}</button>` : "";
    return `<section class="dk2b-strip"><div class="dk2b-sr">${words}<span class="dk2b-chips">${lead}${chips}</span>${more}${check}</div></section>`;
  }
  const pill = `<button class="dk2b-work${deskB2Unfold ? " on" : ""}" data-desk2b-work aria-expanded="${deskB2Unfold}"><i class="dk2b-wdot" aria-hidden="true"></i>Working ${working.length}${DESK_B2_CARET}</button>`;
  return `<section class="dk2b-strip${deskB2Unfold ? " open" : ""}"><div class="dk2b-sr">${pill}</div>${deskB2Unfold ? deskB2Working(d, working) : ""}</section>`;
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

// ---------- the list: tabs, groups, a row per item in four columns ----------

/** The six steps an item shows on today's page, its bar and its page alike. */
const DESK_B2_STEPS = ["Picked up", "Working", "Tests", "Landing", "Ready to check", "Done"];
/** The step item `i` is at on those six (-1: not started; 6: done, all filled). Cue stamps Picked up, Building,
 *  Tests, Commit, Landing, To check (desk/progress.js): Commit and Landing are both Landing here. */
function deskB2At(i) {
  if (i.state === "done") return 6;
  if (i.state === "to_check") return 4;
  const at = [-1, 0, 1, 2, 3, 3, 4][deskProgAt(i) + 1];
  // Finishing is its DONE on the landing line, on main or waiting to install: at Landing, whatever it stamped.
  if (deskFinishing(i)) return Math.max(at, 3);
  return i.state === "working" ? Math.max(at, 1) : at;
}
/** An item's six steps as six short bars: done filled, the one it's at dotted, the rest faint. */
function deskB2Track(i) {
  const at = deskB2At(i);
  return `<span class="dk2b-trk" aria-hidden="true">${DESK_B2_STEPS.map((_, k) => `<i class="${k < at ? "done" : k === at ? "cur" : ""}"></i>`).join("")}</span>`;
}
/** Under the bar: "3/6 · Tests", or "not started". */
const deskB2StepWord = (i) => { const at = deskB2At(i); return at < 0 ? "not started" : at >= 6 ? "6/6 · Done" : `${at + 1}/6 · ${DESK_B2_STEPS[at]}`; };
/** The tabs over the list, plain words with their counts, All a filled pill when picked (Calls while away only while
 *  away). */
function deskB2Tabs(g, taskApprovals) {
  const b = ([k, label, n]) => `<button class="dk2b-tab${deskB2Tab === k ? " on" : ""}" role="tab" aria-selected="${deskB2Tab === k}" data-desk2b-tab="${k}">${label} <span class="n">${n}</span></button>`;
  return `<div class="dk2b-tabs" role="tablist" aria-label="Show">${deskB2TabCounts(g, taskApprovals).filter(([k]) => k !== "away" || g.d?.away?.on).map(b).join("")}</div>`;
}
/** The tabs over the list, each [key, label, count]. */
function deskB2TabCounts(g, taskApprovals) {
  const d = g.d, away = d?.away?.on ? deskAwayCalls(d).length : 0;
  if (deskB2Tab === "away" && !d?.away?.on) deskB2Tab = "all";
  return [["all", "All", deskB2Open(d)], ["you", "Waits on you", deskB2WaitN(d, taskApprovals)], ["next", "Up next", g.queued.length + g.held.length], ["done", "Done today", deskB2DoneToday(d).length], ["away", "Calls while away", away]];
}
/** The list's groups the tab shows (the new page's list, desk/list.js): all of them, or the tab's, and the Away line. */
function deskB2Shown(parts, top) {
  if (deskB2Tab === "all") return { parts, top };
  const keys = DESK_B2_TABS[deskB2Tab] || [];
  return { parts: parts.filter(([k]) => keys.includes(k)), top: deskB2Tab === "away" ? top : "" };
}
/** A long group's rows in the All tab: its first 3 and Show all N, till that's pressed. [rows, Show all's html]. */
function deskB2Cap(key, list) {
  if (deskB2Tab !== "all" || list.length <= DESK_B2_SHOW || deskB2Full.has(`${deskSelection()}|${key}`)) return [list, ""];
  // The open item stays in view, in its own place.
  const open = deskOpen?.kind === "item" ? list.find((i) => deskIdIs(i, deskOpen.id)) : null;
  const head = list.filter((x, k) => k < DESK_B2_SHOW || x === open);
  return [head, `<button class="dk2b-all" data-desk2b-all="${esc(key)}">Show all ${list.length}</button>`];
}
/** The line under a row's title: where it stands in a few words, the old row's words cut short (#666: the row has no
 *  other line, so every state says itself here; answering and Looks right are on its page). [html, whether it's a
 *  session's step (mono)]. */
function deskB2Status(i) {
  const since = (ms) => (ms ? ` · since ${deskQHm(ms)}` : "");
  const cap = (t) => t.charAt(0).toUpperCase() + t.slice(1);
  // In a circle of waits (#250): none of it moves till a link is cut, on its page.
  if (i.state !== "done" && deskCircleOf(i)) return ["Stuck in a circle of waits", false];
  // A big feature nobody holds (#282): how many of its pieces are done, and a thin bar of it; its pieces are on its page.
  if (deskFeat(i) && ["work", "q"].includes(deskFeatState(i)) && i.state !== "needs_you" && i.state !== "to_check") return [deskFeatBar(i), false];
  if (i.state === "to_check") {
    if (deskInstallWait(i)) return ["Waiting for install", false];
    if (deskUncommitted(i)) return ["Not on main yet", false];
    // Changes left on its branch, its mocks' canvas, or in the app since Cue checked it there with what to try: the
    // old row's check line (#436, #448, #526).
    if (i.finish?.unlanded || deskCanvas(i) || i.finish?.mocks || i.finish?.step === "checked") return [deskWhereLine(i).html, false];
    return [`Ready to check${since(deskMs(i.updated))}`, false];
  }
  if (i.state === "needs_you") {
    const r = deskItemRequest(i), [q] = r?.kind === "question" ? questions(r) : [];
    // A tool to allow, by its request or else by its ask's own kind (its request can be another, older one).
    if (r?.kind === "permission" || i.ask?.kind === "permission") {
      const w = deskAllowLine(i), lc = /^[A-Z][a-z]/.test(w) ? w.charAt(0).toLowerCase() + w.slice(1) : w;
      return [esc(w ? `Asks: Allow ${lc}?` : "Asks to use a tool") + since(deskMs(i.updated)), false];
    }
    const ask = q?.question || deskQuestion(i) || deskVerb(i)?.[1];
    return [esc(ask ? `Asks: ${deskOneLine(ask)}` : "Asks you something") + since(deskMs(i.updated)), false];
  }
  // Done, with what carries it on or what it came from (#606): "You picked B2 on #652; #666 builds it".
  if (i.state === "done") { const b = deskBuiltWords(i); return [`Done · ${clockAt(deskMs(i.done_at || i.updated))}${b ? ` · <span class="dk-built">${b}</span>` : ""}`, false]; }
  if (i.state === "held") return ["Parked: starts when you press Start", false];
  if (i.state === "paused") return ["Paused by the Desk lead", false];
  if (i.state === "needs_resume") return ["Stopped before it was done", false];
  if (deskClearable(i)) return ["Stopped · leftover edits in the main folder", false];
  if (i.stopped) return ["Stopped · the Desk lead is on it", false];
  // Its landing step, already html (escaped, with its install's countdown).
  if (deskFinishing(i)) return [deskFinStep(i), false];
  if (i.state === "working") {
    if (i.waiting) return [`Waits: ${esc(firstLine(i.waiting))}`, false];
    // On another machine that isn't answering, or waits for something there: that, over its last step.
    if (i.remote?.quiet_since > 0 || i.remote?.wait) return [esc(cap(deskRemoteLine(i))), false];
    const s = sessionOf(deskHolderSid(i));
    const doing = s?.compacting_ms ? "Compacting…" : s?.running ? `Running: ${s.running}` : s?.doing || i.act?.now || "Thinking…";
    const at = s?.compacting_ms || s?.doing_ms || s?.since_ms || i.act?.at_ms || 0;
    return [`${esc(doing)}${at ? ` · <span data-ago="${at}">${ago(at)}</span>` : ""}`, true];
  }
  // Its new session wouldn't start (#172): its page has why and Retry.
  if (!i.holder && i.start_failed && (i.state === "new" || i.state === "queued")) return ["Couldn't start", false];
  if (deskCopyWaits(i)) return [esc(deskCopyWaitLine(i)), false];
  // Its hand-off failed: why, so it doesn't read as just not given out yet.
  // (Given out since, as next for a session, it isn't failed any more.)
  if (i.route_failed && !i.holder && !i.next_for) return [esc(`Couldn't give it out · ${firstLine(i.route_failed)}`), false];
  const q = deskQueueLine(i);
  if (q) return [esc(cap(q)), false];
  if (i.state === "queued" && i.after) { const on = deskAfterName(i); return [esc(on.startsWith("#") ? `After ${on}` : `Waits for ${on}`), false]; }
  if (!i.holder) return ["Nobody has it yet", false];
  return ["Queued", false];
}
/** Its tests, in a word, at the right of a row at work: what Cue knows from its steps. */
function deskB2Tests(i) {
  if (i.finish?.check_failed || i.finish?.step === "check_failed") return "still fail";
  const at = deskProgAt(i);
  return at < 2 ? "not run yet" : at === 2 ? "running" : "all pass";
}
/** One item as a row of four: #id; its title (and the machine it runs or starts on, when not this Mac, #597) with
 *  where it stands under it; its six steps with "3/6 · Tests"; Check
 *  when it's ready to check, its tests while at work. Lit in place while its page is open on the left. */
function deskB2Row(i) {
  const on = deskOpen?.kind === "item" && deskIdIs(i, deskOpen.id);
  const [st, mono] = deskB2Status(i);
  const right = i.state === "to_check" && !deskInstallWait(i) ? `<button class="dk2b-chk" data-desk-chk-item="${esc(i.id)}">Check</button>`
    : i.state === "working" || deskFinishing(i) ? `<span class="dk2b-ok">${deskB2Tests(i)}</span>` : "<span></span>";
  return `<div class="dk-it dk2b-row${on ? " on" : ""}${deskFeat(i) ? " dk-feat" : ""}${i.state === "needs_you" ? " ask" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"><span class="dk2b-n">#${esc(i.id)}</span><div class="dk2b-t"><div class="dk2b-tt"><span>${esc(i.title)}</span>${deskMachTag(i)}</div><div class="dk2b-ts${mono ? " mono" : ""}">${st}</div></div><div class="dk2b-st">${deskB2Track(i)}<span>${deskB2StepWord(i)}</span></div>${right}</div>`;
}
/** A group: its head (WORKING 7) and its rows, the first 3 in the All tab and Show all N; each drawn by `row`. */
function deskB2Group(key, label, list, extra = "", n = list.length, row = deskB2Row) {
  if (!list.length && !extra) return "";
  const [rows, more] = deskB2Cap(key, list);
  return `<div class="dk2b-gh" data-desk2b-group="${key}">${label} <i>${n}</i></div>${rows.map(row).join("")}${more}${extra}`;
}
/** The folded groups that are open, by key ("done", "away"), the same on every Desk; kept across restarts (#430). */
const deskB2Folds = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.desk2bFolds") || "[]")); } catch { return new Set(); } })();
/** A folded group, one line with its count and ›: Done today, Calls while away. Opened, its rows under it. */
function deskB2Fold(key, label, n, body) {
  const open = deskB2Folds.has(key);
  return `<button class="dk2b-fold${open ? " open" : ""}" data-desk2b-fold="${key}" aria-expanded="${open}">${label} <i>${n}</i><span aria-hidden="true">›</span></button>${open ? body : ""}`;
}
/** Done today, folded to a line with today's count. Opened: every finished item, newest first (today's, then older
 *  ones), 10 more each time its end comes into view (list.js deskDoneWatch), as the old Done group did. */
function deskB2DoneFold(project, d, today) {
  if (!d) return "";
  const pg = deskDonePages.get(project);
  if (!pg) queueMicrotask(() => deskDoneFetch(project));
  const all = deskDoneList(d), shown = all.slice(0, Math.max(deskDoneShown.get(project) || DESK_DONE_STEP, today.length));
  if (!shown.length) return "";
  const end = all.length > shown.length || !pg?.end ? `<div class="dk-more" data-desk-done-more="${esc(project)}">Loading older…</div>` : "";
  return deskB2Fold("done", "DONE TODAY", today.length, shown.map(deskB2Row).join("") + end);
}
/** Today's Items card: Items, Add an item…, Away and Pause in one line; the tabs; then Waits on you (questions and what's
 *  ready to check), Working, Up next, and Done today and Calls while away folded to a line each. */
function deskB2List(project, taskApprovals) {
  if (deskCardsOn()) return deskCards(project, taskApprovals);
  const g = deskGroups(project, taskApprovals), d = g.d, t = deskB2Tab;
  const aw = deskAwayWaiting(d, taskApprovals);
  const waits = [...g.asks, ...g.check];
  const working = [...g.landing, ...g.install, ...g.working, ...g.resume, ...g.paused];
  const next = [...g.queued, ...g.held];
  const doneToday = deskB2DoneToday(d), calls = d?.away?.on ? deskAwayCalls(d) : [];
  const tabs = deskB2Tabs(g, taskApprovals);
  const you = deskB2Group("you", "WAITS ON YOU", waits, aw.html || "", waits.length + (aw.n || 0));
  const work = deskB2Group("working", "WORKING", working);
  // Queued rows drag into your order by their grip (#387, #424: list.js deskQRow, deskQPress); parked ones don't.
  const qRow = deskQRow(deskB2Row), queued = new Set(g.queued);
  const up = deskB2Group("queued", "UP NEXT", next, "", next.length, (i) => (queued.has(i) ? qRow(i) : deskB2Row(i)));
  const done = t === "done" ? deskB2Group("done", "DONE TODAY", doneToday) : deskB2DoneFold(project, d, doneToday);
  const away = !d?.away?.on ? "" : t === "away" ? deskAwayCallsHtml(d) : deskB2Fold("away", "CALLS WHILE AWAY", calls.length, deskAwayCallsHtml(d));
  // Dropped items' kept copies, above Done today as the old list had them; an add-on's own items after the rest.
  const kept = (d?.kept || []).length ? `<div class="dk2b-gh" data-desk2b-group="kept">KEPT COPIES <i>${d.kept.length}</i></div>${d.kept.map(deskKeptRow).join("")}` : "";
  let extra = [];
  try { extra = addOn.deskHolders?.(project) || []; } catch (e) { console.error(e); }
  const tail = extra.map((h) => deskHolderHead({ ...h, addon: true, key: h.key || h.name }, deskOpen?.kind === "addon" && deskOpen.key === (h.key || h.name)) + (h.items || []).map(deskB2Row).join("")).join("");
  const body = { all: you + work + up + kept + done + away + tail, you, next: up, done, away }[t] || "";
  const empty = !body ? `<div class="dk-empty">${t === "all" ? "No items yet." : DESK_B2_NONE[t] || "Nothing here."}</div>` : "";
  return `<div class="dk-card dk-items dk2b-list"><div class="dk2b-ih"><b>Items</b>${deskCardsSwitch()}${deskQuickAdd()}${deskAwaySwitch(d)}${deskPausedTag(d)}${deskPauseButton(d)}</div>${tabs}<div class="dk-scroll" data-scroll-key="desk-items:${esc(project)}">${body}${empty}</div></div>`;
}

// ---------- an item's page, on the left in the lead's chat's place ----------

/** Item `i`'s page: ‹ Items N, its number and title; its progress in six steps; who has it, what it's doing, its tests,
 *  what's next; what to try and its buttons while it waits on you (Close, Send back); its last messages; Open its chat
 *  and Changes. Its question's card, or the box, at its foot. */
function deskB2ItemPage(i, d, foot) {
  const at = deskB2At(i);
  const bar = DESK_B2_STEPS.map((n, k) => `<div class="dk2b-step ${k < at ? "done" : k === at ? "cur" : ""}"><i></i><span>${esc(n)}</span></div>`).join("");
  const sid = deskHolderSid(i), s = sid ? sessionOf(sid) : null;
  const since = s && i.state === "working" ? deskProgStamps(i)[0] || s.since_ms || 0 : 0;
  const who = i.holder ? `<b>${esc(deskHolderOf(i))}</b>${since ? ` · working for <span data-ago="${since}">${ago(since)}</span>` : ""}` : "Nobody has it yet";
  const [now, mono] = deskB2Status(i);
  const tests = i.finish?.check_failed ? "Still fail" : at >= 3 ? "All pass" : at === 2 ? "Running" : "Not run yet";
  const next = i.state === "done" ? "Nothing: it's done." : i.state === "to_check" ? "Waits on you to check it." : i.state === "needs_you" ? "Waits on your answer."
    : i.state === "held" ? "Starts when you press Start." : at < 0 ? esc(deskQueueLine(i) || "Starts when a session is free.")
    : at < 3 ? "Lands when the tests pass, then waits on you to check." : "Waits on you to check once it has landed.";
  const kv = (k, v, cls = "") => `<span class="dk2b-ik">${k}</span><div class="dk2b-iv${cls}">${v}</div>`;
  // What to try and its buttons (Close, Send back, Start…), as its panel had them, while there's something to do.
  const backing = deskBackFor === i.id;
  const acts = `${backing ? deskBackActs(i) : deskLooksTop(i) + deskPickBtns(i) + deskItemActs(i)}${deskBuiltLink(i)}${deskDesignsLink(i)}`;
  const check = i.state === "to_check" ? deskCheckInfo(i, d) : "";
  const todo = check || acts.trim() || backing ? `<div class="dk2b-todo">${check}${acts}${backing ? deskBackHtml(i.id) : ""}</div>` : "";
  const msgs = deskItemSteps(i, d).slice(-2).map((c) => `<div class="dk2b-msg"><div class="dk2b-mh"><b>${esc(c.by || "Cue")}</b><span class="grow"></span><span class="dk2b-age">${c.at ? esc(ago(deskMs(c.at))) : ""}</span></div>${esc(c.text)}</div>`).join("");
  const files = deskItemTabs.get(i.id) === "files", changed = files ? deskFilesChanged(i) : "";
  const btns = `<div class="dk2b-iacts">${s ? `<button class="btn small" ${deskChatAttrs(sid, d)}>Open its chat</button>` : ""}<button class="btn small" data-desk-itab="${files ? "thread" : "files"}" data-id="${esc(i.id)}" aria-pressed="${files}">Changes</button></div>`;
  return `<div class="dk-pg dk2b-ipage" data-desk-page="item"><div class="dk-ph dk2b-iph">${deskBack("")}<span class="dk2b-in">#${esc(i.id)}</span><b class="dk2b-ititle">${esc(i.title)}</b></div>
    <div class="dk-pane dk-ipane dk2b-ipb" data-scroll-key="item:${esc(i.id)}"><div><div class="dk2b-sec">PROGRESS</div><div class="dk2b-steps">${bar}</div></div>
    <div class="dk2b-kv">${kv("Who", who)}${kv("Doing now", now, mono ? " mono" : "")}${kv("Tests", tests)}${kv("Next", next)}</div>${todo}
    <div><div class="dk2b-sec">ITS LAST MESSAGES</div>${msgs ? `<div class="dk2b-msgs">${msgs}</div>` : `<p class="dk-note">Nothing said about it yet.</p>`}</div>
    ${btns}${files ? `<div><div class="dk2b-sec">CHANGES</div>${changed ? `<div class="dk-fcs">${changed}</div>` : `<p class="dk-note">No files on it yet.</p>`}</div>` : ""}</div>${foot}</div>`;
}

// ---------- clicks ----------

deskClicks({
  desk2bWork: () => { deskB2Unfold = !deskB2Unfold; renderMain(); },
  // A tab (or +N › in the strip, to Waits on you): the list in place of what was open.
  desk2bTab: (el) => { deskB2Tab = el.dataset.desk2bTab in DESK_B2_TABS ? el.dataset.desk2bTab : "all"; renderMain(); },
  desk2bAll: (el) => { deskB2Full.add(`${deskSelection()}|${el.dataset.desk2bAll}`); renderMain(); },
  desk2bFold: (el) => {
    const k = el.dataset.desk2bFold;
    if (!deskB2Folds.delete(k)) deskB2Folds.add(k);
    try { localStorage.setItem("cue.desk2bFolds", JSON.stringify([...deskB2Folds])); } catch { /* kept for this window only */ }
    renderMain();
  },
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
