// Cue UI desk — the new Desk layout (#738, "C, reviewed" of the #726 mocks), tried with "Try the new Desk layout" in
// Desk settings: off by default, remembered across launches. Today's page (B2, desk/b2.js) with three columns on the
// green: at the left the Desk's sessions right on the green, grouped by machine, the lead on top with a thin line under
// it; in the middle the lead's chat (or what's open in its place, as on B2), its panel only Now and Activity; at the
// right Items, one white card for what waits on you and what's working (a row per item with its progress as a strip,
// or a line along its foot (#751), the step it's at in the Board's waiting stripes), and under it Up next and Done today side by side on the green, each folding to its
// heading. One count everywhere: Waiting on you's heading is the Desk's own. Drawn by desk/view.js deskView;
// everything here sits under `.dk-c` (styles/desk-c.css). Loaded after desk/b2.js (index.html, mini.html).

const DESK_C_KEY = "cue.deskC";
/** The new Desk layout is on. Kept in this window's storage, like the Desk's other choices of look. */
let deskC = (() => { try { return localStorage.getItem(DESK_C_KEY) === "1"; } catch { return false; } })();
/** Up next and Done today folded to their headings, by key ("next", "done"); kept across restarts. */
const deskCFolds = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskCFolds") || "[]")); } catch { return new Set(); } })();

/** Turn the new layout on or off, at once. It's today's page underneath, so the new Desk page (#628) goes off with it. */
function deskCTurn(on) {
  deskC = on;
  try { localStorage.setItem(DESK_C_KEY, on ? "1" : "0"); } catch { /* not kept: on for this run only */ }
  if (on && typeof deskV2 !== "undefined" && deskV2) return deskV2Turn(false);
  renderMain();
}
/** The switch, by Desk settings' title beside the new Desk page's. */
const deskCToggle = () => `<button class="dk2-sw${deskC ? " on" : ""}" role="switch" aria-checked="${deskC}" data-desk-c-try><span class="dk2-knob" aria-hidden="true"></span>Try the new Desk layout</button>`;
if (typeof document !== "undefined") {
  document.addEventListener("click", (e) => {
    if (!e.target.closest?.("[data-desk-c-try]")) return;
    e.stopPropagation();
    deskCTurn(!deskC);
  }, true);
}
/** The new layout is what the Desk shows now: on, and not the new Desk page. */
const deskCOn = () => deskC && !(typeof deskV2 !== "undefined" && deskV2);

const DESK_C_MAC = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="5" width="16" height="11" rx="1.5"/><path d="M2 19h20"/></svg>`;
const DESK_C_CREW = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="9" cy="8" r="3"/><path d="M3 20a6 6 0 0 1 12 0"/><circle cx="17" cy="9" r="2.4"/><path d="M15.5 14.5A5 5 0 0 1 21 19"/></svg>`;
/** Drawers you just opened: key -> when, for their rows' slide in (DESK_C_OPEN_MS, as desk-c.css's dkn-open). */
const deskCOpened = new Map();
const DESK_C_OPEN_MS = 260;
const DESK_C_ARROW = (open) => `<span class="dkn-fa" aria-hidden="true">${open ? "▾" : "▸"}</span>`;

// ---------- the sessions column, on the green ----------

/** How a session stands in the column: [dot, words]. dot: "ask" (asks you), "work", or "idle". */
function deskCSessWord(d, s) {
  const sid = s.session_id, it = deskSessItem(d, sid), [dot] = deskSessWithHelpers(s);
  const pre = it ? `#${it.id} · ` : "";
  if (dot === "ask" || it?.state === "needs_you") return ["ask", `${pre}<span class="dkn-ask">asks you</span>`];
  // Ready to check is the Items card's (on white, with its ✓); here it's a session that finished and sits still.
  if (it?.state === "to_check" && dot !== "") return ["idle", `${esc(pre)}idle${s.since_ms ? ` <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span>` : ""}`];
  if (dot === "" && it) {
    const st = deskProgStamps(it), at = deskCAt(it), t = at >= 0 ? st[at] : 0;
    return ["work", `${esc(pre)}${esc(DESK_PROG_STEPS[Math.max(0, at)])}${t ? ` · <span data-ago="${t}">${ago(t)}</span>` : ""}`];
  }
  if (dot === "") return ["work", esc(s.running ? `Running: ${s.running}` : s.doing || "working")];
  if (it) return ["idle", `${esc(pre)}${deskB2Status(it)[0]}`];
  return ["idle", `free · idle${s.since_ms ? ` <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span>` : ""}`];
}
/** A crew member of `sid` as a line under it, while it's in play. */
function deskCMate(project, sid, t, o) {
  const key = t.agent_id || t.call;
  const ask = mateNeeds(sid, t), busy = deskMateBusy(t);
  const on = o?.kind === "crew" && o.lead === sid && o.agent === key;
  const word = ask ? `<span class="dkn-ask">asks you</span>` : esc(busy ? "working" : t.state || "");
  const line = t.step || t.what || "";
  return `<button class="dkn-cr${on ? " on" : ""}" data-desk-crew="${esc(key)}" data-lead="${esc(sid)}" aria-label="${esc(t.name)}"><span class="dkn-n"><i class="dkn-d ${ask ? "ask" : busy ? "work" : "idle"}" aria-hidden="true"></i>${DESK_C_CREW}<span class="dkn-nm">${esc(t.name)}</span><span class="dkn-st">${word}</span></span>${line ? `<span class="dkn-s">${esc(line)}</span>` : ""}</button>`;
}
/** The Desk's sessions at the left, on the green: its name and how many work on how many machines; the lead pinned on top, a thin line under
 *  it; then a heading per machine ("THIS MAC · 3 of 4 busy") over its sessions, each with its crew in play. A click
 *  opens that session's chat in the middle; the lead's brings its chat back. All N sessions › at the foot opens the
 *  full list (add, remove, close the old ones). In a narrow window the column is a strip of dots (styles/desk-c.css). */
function deskCSessions(project) {
  const d = deskOf(project), o = deskOpen, leadSid = deskLead(d)?.sid;
  if (!d) return "";
  const all = (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && s.session_id);
  const leadS = all.find((s) => s.session_id === leadSid);
  const mine = all.filter((s) => s !== leadS && deskHasSession(d, s.session_id));
  // At work, or holding an open item, or still warm: shown. Cold ones holding nothing are only in All sessions.
  const busy = (s) => deskSessWithHelpers(s)[0] !== "idle";
  const shown = mine.filter((s) => busy(s) || !deskSessOlder(d, s));
  const working = mine.filter(busy).length;
  const machines = [...new Set(all.map((s) => s.machine || ""))];
  const lit = (sid) => ((o?.kind === "chat" || o?.kind === "holder") && o.sid === sid) || (o?.kind === "item" && deskHolderSid(deskItem(project, o.id)) === sid);
  // The lead: its name, Lead, how it stands; lit while its chat is what's in the middle.
  let lead = "";
  if (leadSid) {
    const s = sessionOf(leadSid), [dot, word] = s ? deskSessWithHelpers(s, false) : ["idle", "ended"];
    const on = !o && deskPick !== project;
    lead = `<button class="dkn-lead${on ? " on" : ""}" data-desk-c-lead aria-label="${esc(deskLeadName(d))}, the Desk lead"><span class="dkn-n">${deskKindIcon("desk")}<span class="dkn-nm">${esc(deskLeadName(d))}</span><span class="dkn-lp">Lead</span><span class="dkn-st">${dot === "" ? '<i class="dkn-d work" aria-hidden="true"></i>' : ""}${esc(dot === "ask" ? "asks you" : word.split(" · ")[0])}${on ? " ›" : ""}</span></span></button>`;
  }
  // A machine's busy count: the Desk's items at work there, of as many as it runs at once (Desk settings, #711).
  const items = d.items || [];
  const atOnce = (m) => (m ? (d.machines?.machines || []).find((x) => x.name === m || machShort(x.name) === machShort(m))?.at_once : d.lands?.set?.at_once) || 0;
  const busyOn = (m) => items.filter((i) => (i.state === "working" || deskFinishing(i)) && (sessionOf(deskHolderSid(i))?.machine || "") === m).length;
  const row = (s) => {
    const sid = s.session_id, [dot, line] = deskCSessWord(d, s), on = lit(sid);
    const team = (sessionOf(sid)?.team || []).filter((t) => deskMateLive(sid, t));
    const crew = team.map((t) => deskCMate(project, sid, t, o)).join("");
    return `<button class="dkn-ss${on ? " on" : ""}" data-desk-chat-of="${esc(sid)}" aria-label="${esc(deskHolderName(sid))}"><span class="dkn-n"><i class="dkn-d ${dot}" aria-hidden="true"></i><span class="dkn-nm">${esc(deskHolderName(sid))}</span>${on ? '<span class="dkn-chev" aria-hidden="true">›</span>' : ""}</span><span class="dkn-s">${line}</span></button>${crew}`;
  };
  const groups = machines.filter((m) => shown.some((s) => (s.machine || "") === m) || !m).map((m) => {
    // The ones at work first, in their order; then the rest, the one that stopped last first.
    const list = shown.filter((s) => (s.machine || "") === m).sort((a, b) => busy(b) - busy(a) || (busy(a) ? 0 : (b.since_ms || 0) - (a.since_ms || 0)));
    const of = atOnce(m), n = busyOn(m);
    const head = `<div class="dkn-mg">${DESK_C_MAC}<b>${esc(m ? machShort(m).toUpperCase() : "THIS MAC")}</b><span>· ${of ? `${n} of ${of} busy` : `${n} busy`}</span></div>`;
    return head + (list.map(row).join("") || `<div class="dkn-none">No sessions here yet.</div>`);
  }).join("");
  // How many are at work, and on how many machines; how many sessions in all is on All N sessions at its foot.
  const count = [`${working} working`, machines.length > 1 ? `${machines.length} machines` : ""].filter(Boolean).join(" · ");
  return `<nav class="dkn-side" aria-label="Sessions"><div class="dkn-head"><div class="dkn-dn">${deskKindIcon("tray")}<b>${esc(deskName(d))}</b></div><div class="dkn-cnt">${esc(count)}</div></div>${lead}<div class="dkn-rule" aria-hidden="true"></div><div class="dkn-list" data-scroll-key="desk-n-sessions:${esc(project)}">${groups}</div><div class="dkn-foot"><button class="dkn-alls" data-desk2b-sessions aria-label="All ${mine.length + (leadS ? 1 : 0)} sessions">All<span class="dkn-w"> ${mine.length + (leadS ? 1 : 0)} sessions</span> ›</button></div></nav>`;
}

// ---------- Items: one white card, then Up next and Done today on the green ----------

/** How an item's progress shows in Items (#751, K and T of #745's canvas): "strip" (K: a strip in its own column, each
 *  step's piece as long as it took, its time under it) or "line" (T: the same along the row's foot, the times on it,
 *  the step it's at and the time in all in Now). The Strip · Line switch in Items' head; Strip unless you switch. */
let deskCLine = (() => { try { return localStorage.getItem("cue.deskCLine") === "line" ? "line" : "strip"; } catch { return "strip"; } })();
/** How long each of item `i`'s six steps took, in ms (0: not reached, or no time kept); the one it's at runs till now.
 *  Picked up took till its first round's next step, as deskProgSteps reads it. */
function deskCStepMs(i) {
  const st = deskProgStamps(i), at = deskProgAt(i);
  return st.map((t, k) => {
    if (!t) return 0;
    const next = (k === 0 && i.backs?.length ? i.backs[0].steps || [] : st).slice(k + 1).find(Boolean);
    return Math.max(0, next ? next - t : k === at ? now() - t : 0);
  });
}
/** Item `i`'s six pieces: state ("done", "cur", "todo"), weight (the square root of its seconds: long steps longer, short
 *  ones still seen) and its time (HTML; the one it's at counts up, tickAges). */
function deskCPieces(i) {
  const at = deskCAt(i), ms = deskCStepMs(i), took = deskProgSteps(i);
  return ms.map((m, k) => ({ state: k < at ? "done" : k === at ? "cur" : "todo", w: Math.sqrt(m / 1000), took: took[k].took }));
}
/** The step it's at and the time in all, in words: "<b>Tests</b> 12m · 35m in all" (HTML), "" before it starts. */
function deskCSoFar(i) {
  const at = deskCAt(i), st = deskProgStamps(i), first = st.find(Boolean);
  if (at < 0) return "";
  const t = deskProgSteps(i)[at]?.took;
  return `<b>${esc(DESK_PROG_STEPS[at])}</b>${t ? ` ${t}` : ""}${first ? ` · <span data-ago="${first}">${ago(first)}</span> in all` : ""}`;
}
/** What the strip or line reads out: "Step 3 of 6, Tests", or "Not started". */
const deskCLineSay = (i) => { const at = deskCAt(i); return at < 0 ? "Not started" : `Step ${at + 1} of 6, ${DESK_PROG_STEPS[at]}`; };
/** K: a strip of six pieces, each as long as its step took (done and current never under 18px, steps to come 8px
 *  stubs), the one it's at in the Board's waiting stripes (moving while it's worked on), each time under its piece. */
function deskCStrip(i) {
  const live = deskProgLook(i) === "work";
  const ps = deskCPieces(i).map(({ state, w, took }, k) => {
    const grow = state === "todo" ? "" : ` style="flex-grow:${(state === "cur" ? Math.max(w, 1) : w).toFixed(2)}"`;
    return `<span class="dkn-kp ${state}${state === "cur" && live ? " live" : ""}"${grow}><i></i><span class="dkn-kt">${state === "todo" ? "" : took || "·"}</span>${deskCStepPop(k, state, took)}</span>`;
  }).join("");
  return `<div class="dkn-ks" role="img" aria-label="${esc(deskCLineSay(i))}">${ps}</div>`;
}
/** A strip piece's card on hover: its step's name and how long it took ("so far" for the one it's at, "not yet" for
 *  one to come). Kept in the piece, shown over the page by the mouseover below (a list scrolls, and would cut it). */
const deskCStepPop = (k, state, took) => `<span class="dkn-kpop" hidden><b>${esc(DESK_PROG_STEPS[k])}</b> ${state === "todo" ? "not yet" : `${took || "under a second"}${state === "cur" ? " so far" : ""}`}</span>`;
if (typeof document !== "undefined") {
  // One small card over the page while the pointer is on a strip's piece: that step and its time (deskCStepPop).
  let pop = null, from = null;
  document.addEventListener("mouseover", (e) => {
    const ks = e.target.closest?.(".dk-c .dkn-kp");
    if (ks === from) return;
    from = ks;
    pop?.remove(); pop = null;
    const src = ks?.querySelector(".dkn-kpop");
    if (!src) return;
    pop = document.createElement("div");
    pop.className = "dkn-steps-pop";
    pop.innerHTML = src.innerHTML;
    document.body.appendChild(pop);
    const r = ks.getBoundingClientRect(), h = pop.offsetHeight, w = pop.offsetWidth;
    const top = r.bottom + 8 + h > innerHeight - 8 ? r.top - 8 - h : r.bottom + 8;
    pop.style.top = `${Math.max(8, top)}px`;
    pop.style.left = `${Math.max(8, Math.min(r.left + r.width / 2 - w / 2, innerWidth - 8 - w))}px`;
  });
}
/** T: the line along the row's foot, each step's piece as long as it took and its time on it (left off a sliver too
 *  short to hold it; the step it's at always says its time), steps to come short stubs. */
function deskCFoot(i) {
  const live = deskProgLook(i) === "work", pieces = deskCPieces(i);
  const sum = pieces.reduce((s, p) => s + (p.state === "todo" ? 0 : Math.max(p.w, 1)), 0) || 1;
  const ps = pieces.map(({ state, w, took }, k) => {
    const grow = Math.max(w, 1), say = state === "cur" || (state === "done" && grow / sum >= 0.065);
    const flex = state === "todo" ? "" : ` style="flex-grow:${grow.toFixed(2)}"`;
    return `<span class="dkn-tp ${state}${state === "cur" && live ? " live" : ""}"${flex}><span class="dkn-tl">${say ? `<span class="dkn-tn">${esc(DESK_PROG_STEPS[k])} </span>${took}` : ""}</span><i></i></span>`;
  }).join("");
  return `<div class="dkn-tline" role="img" aria-label="${esc(deskCLineSay(i))}">${ps}</div>`;
}
/** The Strip · Line switch in Items' head. */
const deskCLineSwitch = () => {
  const b = (k, label) => `<button class="dkn-lsb${deskCLine === k ? " on" : ""}" aria-pressed="${deskCLine === k}" data-desk-c-line="${k}">${label}</button>`;
  return `<span class="dkn-lsw" role="group" aria-label="Progress as">${b("strip", "Strip")}${b("line", "Line")}</span>`;
};
/** The step item `i` is at on Cue's six (Picked up, Building, Tests, Commit, Landing, To check; -1 not started): the
 *  furthest it stamped, and no less than its state says (at work: building; landing; to check), as today's bar reads
 *  it (b2.js deskB2At). */
function deskCAt(i) {
  const at = deskProgAt(i);
  if (i.state === "to_check") return 5;
  if (deskFinishing(i)) return Math.max(at, 4);
  return i.state === "working" ? Math.max(at, 1) : at;
}
/** What's under its title: the machine it runs on, who has it, its kind. */
function deskCSub(i) {
  return [deskCNum(i), i.holder ? esc(deskHolderOf(i)) : "", deskCMach(i)].filter(Boolean).join(" · ");
}
/** Its kind as the Desk's small icon (bug, feature, task) before its number, in place of the word; the word is still read out. */
const deskCKind = (i) => (deskKindIcon(i) ? `<span class="dkn-k" role="img" aria-label="${esc(i.kind)}">${deskKindIcon(i)}</span>` : "");
/** Its kind's icon and number, at the head of its second line ("🐞 #586"): rows have no number column, so titles get the room. */
const deskCNum = (i) => `<span class="dkn-id">${deskCKind(i)}#${esc(i.id)}</span>`;
/** Where it runs, only when that isn't this Mac (most items run here, and the sessions column says This Mac once). */
const deskCMach = (i) => (i?.machine === "This Mac" ? "" : deskMachTag(i));
/** The Now cell: Asks you, Ready to check with the ✓ (Looks right, as today's rows), else where it stands. */
function deskCNow(i) {
  // What it asks opens with the row, as Check did: no Choose… button fighting the words for room.
  if (i.state === "needs_you") return `<span class="dkn-nask">Asks you</span>`;
  if (i.state === "to_check" && !deskInstallWait(i)) return deskLooksSoon.has(i.id) || deskLeaving.has(i.id) ? `<span class="dk-ok on" aria-hidden="true"></span><span class="dk-soon">Done · <button class="dk-undo-c" data-desk-act="looks_undo" data-id="${esc(i.id)}">Undo</button></span>` : `<span class="dkn-nw">Ready to check</span><button class="dk-ok" data-desk-act="looks_right" data-id="${esc(i.id)}" aria-label="Looks right"></button>`;
  const [st, mono] = deskB2Status(i);
  return `<span class="dkn-nw${mono ? " mono" : ""}">${st}</span>`;
}
/** One item in the card: its title with its kind, number, who has it and where it runs; then, as the Strip · Line
 *  switch has it, its strip and Now (K), or Now with the step it's at and the time in all, over the line along the row's
 *  foot (T). Narrow, the strip goes under the title. Lit while its page is open. */
function deskCRow(i) {
  // Lit while its page is open, or its holder's chat (that session's chat is the item's chat).
  const o = deskOpen, on = (o?.kind === "item" && deskIdIs(i, o.id)) || ((o?.kind === "chat" || o?.kind === "holder") && !!o.sid && deskHolderSid(i) === o.sid);
  return `<div class="dk-it dkn-row${on ? " on" : ""}${i.state === "needs_you" ? " ask" : ""}${i.state === "to_check" && deskLooksSoon.has(i.id) ? " soon" : ""}${deskLeaveStyle(i.id)}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"><div class="dkn-t"><span class="dkn-tt">${esc(i.title)}</span><span class="dkn-sub">${deskCSub(i)}</span></div>${deskCLine === "line" ? `<div class="dkn-now"><span class="dkn-nl">${deskCNow(i)}</span><span class="dkn-sofar">${deskCSoFar(i)}</span></div>${deskCFoot(i)}` : `<div class="dkn-pl">${deskCStrip(i)}</div><div class="dkn-now">${deskCNow(i)}</div>`}</div>`;
}
/** A row in Up next or Done today, on the green: #id and title (a grip on the ones you can drag), and a line under it. */
function deskCLow(i, line) {
  const on = deskOpen?.kind === "item" && deskIdIs(i, deskOpen.id);
  const mach = deskCMach(i);
  return `<div class="dk-it dkn-nx${on ? " on" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"><div class="dkn-a1"><span class="dkn-tx">${esc(i.title)}</span></div><div class="dkn-a2">${deskCNum(i)}${mach}${line ? `<span>${line}</span>` : ""}</div></div>`;
}
/** A done item's line: how long it took and when it landed. */
function deskCDoneLine(i) {
  const st = deskProgStamps(i), start = st.find(Boolean), end = deskMs(i.done_at || i.updated);
  return [start && end > start ? `took ${deskProgDur(end - start)}` : "", end ? `landed ${clockAt(end)}` : ""].filter(Boolean).join(" · ");
}
/** A list on the green with the Board's drawer head: the arrow at the left, its name and count; the whole line folds it
 *  to that line. */
function deskCDrawer(key, label, n, hint, rows, none) {
  const open = !deskCFolds.has(key);
  // Just opened: its rows slide in, already as far in as the time since (a redraw meanwhile doesn't start it again).
  const since = Date.now() - (deskCOpened.get(key) || 0), anim = open && since < DESK_C_OPEN_MS ? ` dkn-opening" style="animation-delay:-${since}ms` : "";
  return `<section class="dkn-dr${open ? "" : " fold"}" data-dkn-drawer="${key}"><button class="dkn-dh" data-desk-c-fold="${key}" aria-expanded="${open}">${DESK_C_ARROW(open)}${label} <span class="dkn-dn">${n}</span>${hint && open ? `<span class="dkn-hint">· ${hint}</span>` : ""}</button>${open ? `<div class="dkn-dl${anim}" data-dkn-list="${key}"${deskCFitStyle(key)}>${rows || `<div class="dkn-dnone">${none}</div>`}</div><button class="dkn-more" data-desk-c-all="${key}"${deskCFitBy.get(key) ? "" : " hidden"}></button>` : ""}</section>`;
}
/** Items' card, Up next and Done today each scroll, and at rest end on a whole row: a list's height is cut to the
 *  last row that fits (what's left goes below it). Under each, "↓ N more" says how many rows are below its edge as
 *  you scroll (a click scrolls a page); at the end it shows nothing but keeps its room, so the list doesn't jump.
 *  The cuts are measured once (one layout) and kept while the window's size and the lists' rows stay the same. */
let deskCFitKey = "", deskCFits = [];
/** Each list's cut as last measured, by list ("items", "next", "done"), drawn with it so a redraw lays it out at that
 *  height at once (a list scrolled to its end is put back there, and stays at its end). */
const deskCFitBy = new Map();
const deskCFitStyle = (name) => (deskCFitBy.get(name) ? ` style="max-height:${deskCFitBy.get(name)}px"` : "");
/** How many of Working's rows show before it scrolls within (7 to 10 at once happens). */
const DESK_C_WORK_ROWS = 4;
function deskCFitLists() {
  // Working first: cut to its first rows, so the card's own cut below counts it at that height.
  for (const el of document.querySelectorAll("#app .dkn-items [data-dkn-cap]")) {
    const r = el.children[DESK_C_WORK_ROWS - 1];
    if (r) el.style.maxHeight = `${Math.floor(r.getBoundingClientRect().bottom - el.getBoundingClientRect().top + el.scrollTop)}px`;
  }
  const lists = [...document.querySelectorAll("#app .dkn-items [data-dkn-list]:not([data-dkn-cap])")];
  const key = `${innerWidth}x${innerHeight}|${deskCLine}|${lists.map((el) => `${el.dataset.dknList}:${el.querySelectorAll(".dk-it").length}`).join(",")}`;
  if (key !== deskCFitKey) {
    // "↓ N more" measured with its words in, as it will be drawn (empty, it's a line shorter, and the cut a row too low).
    for (const el of lists) { el.style.maxHeight = ""; el.nextElementSibling.hidden = false; el.nextElementSibling.textContent ||= "↓ 0 more"; }
    deskCFits = lists.map((el) => {
      if (el.scrollHeight <= el.clientHeight + 1) return 0;
      const top = el.getBoundingClientRect().top - el.scrollTop;
      let fit = 0;
      // Row by row, into each group's band (option A wraps a group in one): a cut never falls mid-row.
      for (const r of el.querySelectorAll(":scope > :not(.dkn-band), :scope > .dkn-band > *")) {
        const bottom = r.getBoundingClientRect().bottom - top;
        if (bottom > el.clientHeight) break;
        fit = bottom;
      }
      return Math.floor(fit);
    });
    deskCFitKey = key;
    deskCFitBy.clear();
    lists.forEach((el, k) => deskCFitBy.set(el.dataset.dknList, deskCFits[k]));
  }
  lists.forEach((el, k) => {
    el.style.maxHeight = deskCFits[k] ? `${deskCFits[k]}px` : "";
    el.nextElementSibling.hidden = !deskCFits[k];
    // On its way after "↓ N more": a redraw meanwhile made it afresh where it had got to (its start, in the slide's
    // first frame), so it goes on from there to where the click sent it.
    const go = deskCGoing.get(el.dataset.dknList);
    if (go && Date.now() < go.until && Math.abs(el.scrollTop - go.top) > 1) el.scrollTo({ top: go.top, behavior: "smooth" });
    else deskCGoing.delete(el.dataset.dknList);
  });
  deskCMore();
}
/** Where a list's "↓ N more" is sending it, by list, for as long as its slide takes (deskCAll, deskCFitLists). */
const deskCGoing = new Map(), DESK_C_GOING_MS = 1000;
/** "↓ N more" under each list that scrolls: the rows still below its edge. */
function deskCMore() {
  for (const el of document.querySelectorAll("#app .dkn-items [data-dkn-list]")) {
    const more = el.nextElementSibling;
    if (more.hidden) continue;
    const edge = el.getBoundingClientRect().bottom;
    const n = [...el.querySelectorAll(":scope > .dk-it, :scope > .dkn-band > .dk-it, :scope > .dkn-band > .dk-prows")].filter((r) => r.getBoundingClientRect().bottom > edge + 1).length;
    more.textContent = `↓ ${n} more`;
    more.classList.toggle("end", !n);
  }
}
window.addEventListener("resize", () => { if (view === "desk" && deskCOn()) { deskCFitLists(); deskCFitNames(); } });
/** Line: a step's name goes with its time only where its piece has room for both; else just the time. */
function deskCFitNames() {
  for (const tl of document.querySelectorAll("#app .dk-c .dkn-tl")) {
    if (!tl.querySelector(".dkn-tn")) continue;
    tl.classList.remove("short");
    if (tl.scrollWidth > tl.parentElement.clientWidth + 1) tl.classList.add("short");
  }
}
document.addEventListener("scroll", (e) => { if (e.target?.dataset?.dknList) deskCMore(); }, true);
/** An item opened on the new layout: its session's chat (the same as clicking the session on the left), not a page of
 *  its own (#560's plan A, finished in #744: one item per session until it's done, so that chat is its story). The
 *  page's head already says the item it's on; over the chat, a bar with its strip, and ready to check, what to try
 *  with Looks right and Send back. null when no session
 *  has it yet (its page shows instead). */
function deskCItemChat(i, d, foot) {
  const sid = deskHolderSid(i);
  if (!sid || !sessionOf(sid)) return null;
  const check = i.state === "to_check" && !deskInstallWait(i);
  const acts = !check ? "" : deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id)
    : `<button class="btn small primary" data-desk-act="looks_right" data-id="${esc(i.id)}">Looks right</button><button class="btn small" data-desk-act="send_back" data-id="${esc(i.id)}">Send back</button>`;
  const see = check && (i.see || "").trim() ? `<div class="dkn-ibsee"><b>Try:</b> ${esc(deskOneLine(i.see))}</div>` : "";
  const bar = `<div class="dkn-ib${check ? " check" : ""}"><div class="dkn-ib2">${deskCStrip(i)}<div class="dkn-ibacts">${acts}</div></div>${see}</div>`;
  return deskCSessChat(sid, null, d, foot, bar);
}
/** Each session's open tab on the new layout ("chat" | "files"), by session: Chat unless you picked Files. */
const deskCSessTabs = new Map();
/** A session's chat on the new layout (a session clicked on the left, or the item it's on): back, then the Board's own
 *  top bar for it (its name, state, chips, Commit, ⋯, btw, Stop: chat/pane.js activeHead), the item it's on, the item's
 *  bar when there is one, then Chat and Files (what it changed this turn, each opening its changes) as tabs, and the box.
 *  No panel: Stop and the rest are in the bar, the files in their tab. */
/** A crew member's page in the new layout (desk/thread.js deskCrewPane): back, its name with its state and Stop it, ⋯;
 *  the line saying whose crew and what it's on; then Chat (its steps), Files (what it changed) and Activity (its lead's
 *  Desk lines) as tabs; and the box to its lead. No panel. */
function deskCCrewPage({ lead, key, t, it, leadName, back, word, dot, acts, main, files, d, foot }) {
  const k = `crew:${lead}:${key}`, tab = ["files", "activity"].includes(deskCSessTabs.get(k)) ? deskCSessTabs.get(k) : "chat";
  const rows = dkActivityOf(lead);
  const b = (id, label) => `<button class="dkn-stab${tab === id ? " on" : ""}" role="tab" aria-selected="${tab === id}" data-desk-c-stab="${id}" data-sid="${esc(k)}">${label}</button>`;
  const tabs = `<div class="dkn-stabs" role="tablist">${b("chat", "Chat")}${b("files", "Files")}${b("activity", "Activity")}</div>`;
  const body = tab === "files" ? `<div class="dk-pane dkn-sfiles" data-scroll-key="cfiles:${esc(k)}">${files || `<div class="dk-quiet">Nothing changed yet.</div>`}</div>`
    : tab === "activity" ? `<div class="dk-pane dkn-sfiles" data-scroll-key="cact:${esc(k)}">${rows.length ? `<div class="dk-prows">${rows.map((r) => deskActPanelRow({ at: r.at, line: r }, d)).join("")}</div>` : `<div class="dk-quiet">Nothing yet.</div>`}</div>`
    : main;
  const head = `<div class="dkn-chead"><i class="dk-pdot ${dot}" aria-hidden="true"></i>${DESK_C_CREW}<b class="dkn-cname">${esc(t.name)}</b><span class="dkn-cword">${esc(word)}</span><span class="grow"></span>${acts}</div>`;
  const meta = `<div class="dkn-smeta">in <b>${esc(leadName)}</b>'s crew${it ? ` · on ${deskItemLink(it)} · ${esc(it.title)}` : t.what ? ` · on ${esc(t.what)}` : ""}</div>`;
  return `<div class="dkn-sp dkn-crew" data-desk-page="crew"><div class="dkn-sback">${back}</div>${head}${meta}${tabs}<div class="dkn-spm">${body}${foot}</div></div>`;
}
/** Under this turn's files in a session's Files tab: each item it worked on, newest first, with the files that item
 *  changed (its commit's, or so far), as its page lists them (desk.js deskFilesChanged). */
const deskCSessTasks = (d, sid) => (d.items || []).filter((i) => i.holder && i.holder.kind !== "crew" && i.holder.sid === sid).sort((a, b) => (b.updated ?? 0) - (a.updated ?? 0));
function deskCSessTaskFiles(d, sid) {
  return deskCSessTasks(d, sid).map((i) => { const f = deskFilesChanged(i); return f ? deskPsec(`${deskNum(i)} · ${esc(i.title)}`, `<div class="dk-fcs">${f}</div>`, "dk-pfiles") : ""; }).join("");
}
function deskCSessChat(sid, from, d, foot, bar = "") {
  const s = sessionOf(sid);
  if (!s) return null;
  const card = (state.items || []).find((x) => x.session_id === sid) || null;
  const head = activeHead(paneArgs({ it: card, s }));
  const on = deskSessItems(d, sid)[0];
  const meta = on ? `<div class="dkn-smeta">on ${deskItemLink(on)} · ${esc(on.title)}</div>` : "";
  // This turn's files and each of its items' (its Files tab lists them all).
  const n = (inboxTurnDiff(sid)?.files || s.changes?.files || []).length + deskCSessTasks(d, sid).reduce((k, i) => k + (deskDiffs.get(deskDiffId(i))?.files || deskFilesOn(i)).length, 0);
  const tab = deskCSessTabs.get(sid) === "files" ? "files" : "chat";
  const t = (k, label) => `<button class="dkn-stab${tab === k ? " on" : ""}" role="tab" aria-selected="${tab === k}" data-desk-c-stab="${k}" data-sid="${esc(sid)}">${label}</button>`;
  const tabs = `<div class="dkn-stabs" role="tablist">${t("chat", "Chat")}${t("files", `Files <span class="n">${n}</span>`)}</div>`;
  let main;
  if (tab === "files") main = `<div class="dk-pane dkn-sfiles" data-scroll-key="files:${esc(sid)}">${deskSessFiles(sid) || deskPsec("Changed this turn", `<div class="dk-quiet">Nothing yet.</div>`, "dk-pfiles")}${deskCSessTaskFiles(d, sid)}</div>`;
  else {
    const lead = (state.desk?.projects || []).some((x) => deskLead(x)?.sid === sid);
    const rows = deskChatRows(s, lead ? { you: deskLeadNote } : undefined), where = whereNow(null, s);
    const empty = !rows && !where ? `<div class="dk-empty" data-key="empty">Nothing said yet.</div>` : "";
    main = `<div class="dk-chat" data-desk-chat="${esc(sid)}">${empty}${rows}${where ? keyed(where, "where") : ""}</div>`;
  }
  return `<div class="dkn-sp" data-desk-page="session"><div class="dkn-sback">${deskChatBack(from, d)}</div>${head}${meta}${bar}${tabs}<div class="dkn-spm">${main}${foot}</div></div>`;
}
/** Waiting on you's order as it was last drawn. */
let deskCWaitOrder = [];
/** Rows you just said Looks right to stay where they were while "Done · Undo" shows and they fold away: that moment
 *  moves them from asking to ready to check (list.js deskGroups), which would otherwise send them to the list's end. */
function deskCKeepPlace(list) {
  const stay = (i) => deskLooksSoon.has(i.id) || deskLeaving.has(i.id);
  const out = list.filter((i) => !stay(i));
  list.filter(stay).map((i) => [deskCWaitOrder.indexOf(i.id), i]).sort((a, b) => a[0] - b[0])
    .forEach(([at, i]) => out.splice(at < 0 ? out.length : Math.min(at, out.length), 0, i));
  deskCWaitOrder = out.map((i) => i.id);
  return out;
}
/** Items: one line at its head (Items, Add an item…, Away, Pause; no tabs: every group is on screen); the white card
 *  with the step names over Waiting on you and Working; then Up next and Done today side by side on the green. */
function deskCList(project, taskApprovals) {
  const g = deskGroups(project, taskApprovals), d = g.d;
  const aw = deskAwayWaiting(d, taskApprovals);
  const waits = deskCKeepPlace([...g.asks, ...g.check]);
  const working = [...g.landing, ...g.install, ...g.working, ...g.resume, ...g.paused];
  const queued = new Set(g.queued), next = [...g.queued, ...g.held];
  const doneToday = deskB2DoneToday(d);
  // Each group folds to its heading with a click on it, as Up next and Done today do (remembered with them).
  // Option A ("Items: working, waiting, asking", picked 23 Nov): each kind in its own band, the heading with
  // its count and a plain line saying what it means for you; asking you on the soft red, Working on white, the rest on
  // the card's grey.
  const GRP_SAY = { blocked: "nothing moves till you answer", working: "at work · nothing for you", you: "done · ready for you to check" };
  const grp = (key, label, list, extra = "", n = list.length, first = "") => {
    if (!list.length && !extra && !first) return "";
    const open = !deskCFolds.has(key);
    const rows = first + list.map(deskCRow).join("") + extra;
    // Working holds at most DESK_C_WORK_ROWS rows and scrolls within, with its own "↓ N more", so Waiting on you stays in sight.
    const body = key === "working" && list.length > DESK_C_WORK_ROWS ? `<div class="dkn-wl" data-dkn-list="working" data-dkn-cap>${rows}</div><button class="dkn-more dkn-wmore" data-desk-c-all="working"></button>` : rows;
    const say = GRP_SAY[key] ? `<em class="dkn-gsay">${GRP_SAY[key]}</em>` : "";
    return `<section class="dkn-band dkn-b-${key}"><button class="dkn-gh" data-desk2b-group="${key}" data-desk-c-fold="${key}" aria-expanded="${open}">${DESK_C_ARROW(open)}${label} <span>${n}</span>${say}</button>${open ? body : ""}</section>`;
  };
  // The lead's own ask waits on you too, as the header and the strip count it (deskB2WaitN): one count.
  const ask = d && !deskLeadEnded(d) ? deskLeadAsk(d) : null;
  const leadAsk = ask ? `<div class="dk-it dkn-row ask" role="button" tabindex="0" data-desk-c-lead><div class="dkn-t"><span class="dkn-tt">${esc(deskLeadName(d))}</span><span class="dkn-sub">The Desk lead</span></div>${deskCLine === "line" ? "" : `<div class="dkn-pl"></div>`}<div class="dkn-now"><span class="dkn-nask">${esc(deskOneLine(summary(ask) || "Asks you something"))}</span></div></div>` : "";
  // What's blocked on you goes first, over Working: nothing moves till you answer. Steps held for your OK, oldest first
  // (#749), with the lead's own one-way ask among them; then the lead asking you, then asks while you were away.
  const hf = deskHeldFirst(aw, waits, deskCRow, ask?.one_way && leadAsk ? [[ask.created_ms || 0, leadAsk]] : []);
  const blockedN = (aw.n || 0) + (ask ? 1 : 0) + (waits.length - hf.rest.length);
  const blocked = grp("blocked", "BLOCKED ON YOU", [], (ask?.one_way ? "" : leadAsk) + (aw.more || ""), blockedN, hf.html);
  const you = grp("you", "WAITING ON YOU", hf.rest);
  const work = grp("working", "WORKING", working);
  const kept = (d?.kept || []).length ? `<div class="dkn-gh" data-desk2b-group="kept">KEPT COPIES <span>${d.kept.length}</span></div>${d.kept.map(deskKeptRow).join("")}` : "";
  const calls = d?.away?.on ? deskAwayCalls(d) : [];
  const away = d?.away?.on ? deskB2Fold("away", "CALLS WHILE AWAY", calls.length, deskAwayCallsHtml(d)) : "";
  let extra = [];
  try { extra = addOn.deskHolders?.(project) || []; } catch (e) { console.error(e); }
  const tail = extra.map((h) => deskHolderHead({ ...h, addon: true, key: h.key || h.name }, deskOpen?.kind === "addon" && deskOpen.key === (h.key || h.name)) + (h.items || []).map(deskCRow).join("")).join("");
  // Working next: it's what's live, and it stays short (the Desk runs only so many at once); Waiting on you under it.
  const body = blocked + work + you + kept + away + tail;
  const head = `<div class="dkn-th" aria-hidden="true"><span>Item</span>${deskCLine === "line" ? "" : "<span>Progress</span>"}<span>Now</span></div>`;
  // Where it stands, in words only (deskB2Status is html, escaped already).
  const said = (i) => deskB2Status(i)[0].replace(/<(?!\/?span\b)[^>]*>/g, "");
  const qRow = deskQRow((i) => deskCLow(i, said(i))), lowRow = (i) => (queued.has(i) ? qRow(i) : deskCLow(i, said(i)));
  const up = deskCDrawer("next", "UP NEXT", next.length, next.length > 1 ? "drag to reorder" : "", next.map(lowRow).join(""), "Nothing up next.");
  const done = deskCDrawer("done", "DONE TODAY", doneToday.length, "", doneToday.map((i) => deskCLow(i, esc(deskCDoneLine(i)))).join(""), "Nothing done today.");
  requestAnimationFrame(() => { deskCFitLists(); deskCFitNames(); });
  return `<div class="dk-items dkn-items"><div class="dk-card dkn-card dkn-${deskCLine}"><div class="dk2b-ih dkn-ih"><b>Items</b>${deskCLineSwitch()}${deskQuickAdd()}${deskAwaySwitch(d)}${deskPausedTag(d)}${deskPauseButton(d)}</div>${body ? head : ""}<div class="dk-scroll dkn-body" data-scroll-key="desk-items:${esc(project)}" data-dkn-list="items"${deskCFitStyle("items")}>${body || `<div class="dk-empty">Nothing waits on you and nothing is working.</div>`}</div><button class="dkn-more" data-desk-c-all="items"${deskCFitBy.get("items") ? "" : " hidden"}></button></div><div class="dkn-low">${up}${done}</div></div>`;
}

// ---------- clicks ----------

deskClicks({
  // The lead (its block in the sessions column, its ask in Items, the header's count): its chat, whatever was open,
  // All sessions too.
  deskCLead: () => { deskPick = null; deskTab = "lead"; readerView = null; deskShow(null); },
  deskCLine: (el) => {
    deskCLine = el.dataset.deskCLine === "line" ? "line" : "strip";
    try { localStorage.setItem("cue.deskCLine", deskCLine); } catch { /* kept for this window only */ }
    renderMain();
  },
  deskCStab: (el) => { deskCSessTabs.set(el.dataset.sid, el.dataset.deskCStab); renderMain(); },
  deskCAll: (el) => {
    const list = el.previousElementSibling;
    if (!list) return;
    const top = Math.min(list.scrollTop + list.clientHeight, list.scrollHeight - list.clientHeight);
    deskCGoing.set(list.dataset.dknList, { top, until: Date.now() + DESK_C_GOING_MS });
    list.scrollTo({ top, behavior: "smooth" });
  },
  deskCFold: (el) => {
    const k = el.dataset.deskCFold;
    if (!deskCFolds.delete(k)) deskCFolds.add(k);
    else deskCOpened.set(k, Date.now());
    try { localStorage.setItem("cue.deskCFolds", JSON.stringify([...deskCFolds])); } catch { /* kept for this window only */ }
    renderMain();
  },
});
