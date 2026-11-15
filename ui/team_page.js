// ---------- a lead's crew on a page of its own (team.js has the drawer this grows from) ----------
// "Open full ›" on the crew's tray line opens it across the window, in the board's and the chat's place (the top bar and
// tabs stay): ‹ Back to the board (or Esc, or Open chat ›) goes back to the lead's chat. It fits the window: a head line;
// the goal on one line (All of it › opens its four boxes in place); then the left reads and the right picks. Left, as
// wide as on the board: the board's chat pane itself (chat/pane.js activePane: its title line, btw, its chat with the
// waiting line, its box with the same draft), what the lead decided on its own placed in its chat by time, and Needs
// you over its box (one row, "N more ›" for the rest) in the crew tray's place. Right, the rest of the width: The lead, then the crew's cards (one to a line) in the order the
// lead started them (a card never moves for its state), the finished ones folded into "N done ›" (the drawer's fold).
// The optional add-on may add to it (core.js addOn.crewPage*): its own head line, a band under The lead, rows under the
// goal, rows in Needs you and decisions in the chat.

/** The lead whose crew's page is open; null when none is. It closes when that session goes away, or another opens. */
let crewPageSid = null;
/** The view the page was opened from and shows within (its tab stays selected), and what its Back button names:
 *  the board's unless the add-on opened it from one of its views. */
let crewPageFrom = "board", crewPageBack = "the board";

/** Open the crew's page for lead `sid`, within view `from` (Back names `back`). */
function crewPageOpen(sid, from = "board", back = "the board") {
  crewPageSid = sid;
  crewPageFrom = from;
  crewPageBack = back;
  mateOpenLeads.delete(sid);
  mateMenu = null;
  if (typeof crewMenuFor !== "undefined") crewMenuFor = null;
  view = from;
  // Its chat is drawn afresh: it opens at its end, as the chat does.
  drawAtChatEnd();
}
/** The page for lead `sid` from elsewhere (the add-on's): its chat becomes the one open first, so the board shows it
 *  after. `from`: the add-on's view it opens within (that tab stays selected; Back and Esc return to it), else the
 *  board; `back`: that view's name for the Back button. */
addOn.openCrewPage = (sid, { from = "board", back = "" } = {}) => {
  if (!sessionOf(sid)) return;
  if (active?.sid !== sid) setActive(null, sid);
  crewPageOpen(sid, from, back || (from === "board" ? "the board" : from));
};
/** The add-on's way out: the page closes if it was opened within view `from` (any, when not given); true when it did. */
addOn.closeCrewPage = (from) => {
  if (!crewPageSid || (from && crewPageFrom !== from)) return false;
  crewPageSid = null;
  crewPageFrom = "board";
  return true;
};
/** Back to where it was opened from: the board, the lead's box keeping the cursor, or the add-on's view. `chat`: to
 *  the lead's chat on the board, wherever it was opened from (Open chat ›). */
function crewPageClose(chat = false) {
  const from = crewPageFrom;
  crewPageSid = null;
  crewPageFrom = "board";
  mateMenu = null;
  if (chat) view = "board";
  drawAtChatEnd();
  if (chat || from === "board") focusComposer();
}
/** The page across the board's place (columns and chat pane); null when it isn't open, or its lead is gone or no
 *  longer the open one: then it closes. Another tab shows that tab; back on the tab it opened within, the page is
 *  still there. */
function crewPageShown() {
  if (!crewPageSid || view !== crewPageFrom) return null;
  if (!sessionOf(crewPageSid) || (active?.sid && active.sid !== crewPageSid)) { crewPageSid = null; return null; }
  return `<div class="board cp-board">${crewPageView(crewPageSid)}</div>`;
}

/** A member's decisions in the crew's log, oldest first: by its Agent call when the entry has one (a member started
 *  again under the same name is another call), else by its name. */
const crewEntriesOf = (sid, t) => (sessionOf(sid)?.crew_log || []).filter((d) => d.call ? d.call === t.call : d.who === t.name).sort((a, b) => (a.at_ms || 0) - (b.at_ms || 0));
/** The latest of them. */
const crewEntryOf = (sid, t) => crewEntriesOf(sid, t).at(-1) || null;
/** What its review found, in a few words: "2 should-fix", "1 note", "clean". */
function crewFound(t) {
  const n = (t.findings || []).length, fix = fixCount(t.findings);
  return [fix ? `${fix} should-fix` : "", n - fix ? plural(n - fix, "note") : ""].filter(Boolean).join(", ") || "clean";
}
/** What comes next for a reviewed member: who it waits on. */
function crewNext(sid, t) {
  if (mateNeeds(sid, t)) return "waits for you";
  if (mateCueHandles(sid, t)) return mateReadyAct(t) === "fixes" ? "Cue sends it back" : "Cue commits it";
  if (mateOnLead(t)) return mateFixesKept(t) ? "fixes wait for the lead" : "its lead commits it";
  return "waits for you";
}
/** A step's words with what it runs or edits as code: "Running: npm test" → "Running `npm test`", "Editing a/b.js" →
 *  "Editing `a/b.js`". */
const crewStepCode = (step) => String(step || "").replace(/^Running:\s*(.+)$/, "Running `$1`").replace(/^(Editing|Writing|Reading|Creating) (\S+)$/, "$1 `$2`");
/** A finding's words without its number and kind: "hub/handoff.rs:40 — the list is cut at 8". */
const crewFindingText = (line) => String(line).replace(/^\d+\.\s*/, "").replace(FIX_KIND, " — ").replace(/^\s*—\s*/, "");
/** What a lane says a member is doing now, all from its own data (its state, step, ask, findings, the crew's log),
 *  never more: `now` one plain sentence (`code` in backticks; `ask`: it waits on you), `sub` a dim line of context,
 *  `fix` the finding it's fixing (its kind, its words), and its word, colour and meter as team.js crewMark says. */
function crewNow(sid, t) {
  const ask = mateAsk(sid, t), log = crewEntryOf(sid, t), last = log ? [log.head, ...(log.steps || [])].filter(Boolean).at(-1) : "";
  const logLine = last && log.at_ms ? `${last} at ${hhmm(log.at_ms)}` : "";
  const fixes = (t.findings || []).filter((l) => ["should-fix", "must-fix", "blocker"].includes(findingKind(l)));
  const working = t.state === "working", step = crewStepCode(t.step);
  const { at, look, word } = crewMark(sid, t);
  let now = step || word, sub = "", fix = null;
  if (working) {
    if (t.quiet === "stuck") now = `Quiet for ${mateQuietMins(t)}m — looks stuck`;
    else if (t.fix_rounds && fixes.length) { now = "Fixing what review found"; fix = [findingKind(fixes[0]), crewFindingText(fixes[0])]; }
    else if (!step) now = t.fix_rounds ? "Fixing what review found" : "Working";
    if (!fix) sub = logLine || (t.step && t.step_ms ? `on it since ${hhmm(t.step_ms)}` : "");
  } else if (["queued", "reviewing"].includes(t.state)) {
    now = t.state === "queued" ? "Its review waits for the lead" : "Being reviewed";
    sub = logLine;
  } else if (t.state === "reviewed") {
    now = `Reviewed: ${crewFound(t)}`;
    if (fixes.length) fix = [findingKind(fixes[0]), crewFindingText(fixes[0])];
    sub = crewNext(sid, t);
  } else if (t.state === "committed") {
    const said = log && [log.head, ...(log.steps || [])].filter(Boolean).find((x) => /^committed\b/i.test(x));
    now = [said ? said.replace(/^c/, "C") : "Committed", t.commit ? t.commit.slice(0, 7) : ""].filter(Boolean).join(" ");
    sub = [t.recommendation || (t.findings || []).length ? `review: ${crewFound(t)}` : "", t.ended_ms ? `ended ${hhmm(t.ended_ms)}` : ""].filter(Boolean).join(" · ");
  } else {
    // Done, stopped, set aside: what it said last.
    now = { aside: "Set aside", stopped: "Stopped" }[t.state] || "Done";
    sub = t.summary || "";
  }
  if (ask) { now = `${verb(ask).replace(/^./, (c) => c.toUpperCase())} \`${summary(ask)}\``; sub = logLine || (step && step !== `Running \`${summary(ask)}\`` ? step : ""); }
  else if (t.needs) now = t.needs;
  return { now, sub, fix, at, look, word, ask: !!ask || !!t.needs };
}
/** The meter: five small bars, those before its stage done, its stage as `look` says; its stage in a word after. */
function crewMeter(n) {
  const bars = CREW_STAGES.map((_, i) => `<b class="${i < n.at ? "done" : i === n.at ? n.look : ""}"></b>`).join("");
  return `<span class="cp-meter"><span class="cp-m" aria-hidden="true">${bars}</span><span class="cp-ml">${esc(n.word)}</span></span>`;
}
/** A finding's kind as a small pill: should-fix (and worse) amber, the rest outlined. */
const crewKind = (kind) => `<span class="cp-f ${["should-fix", "must-fix", "blocker"].includes(kind) ? "fix" : ""}">${esc(kind)}</span>`;
/** The leads whose goal is open in full (All of it ›): its four boxes in place of its line. */
const crewGoalOpen = new Set();
/** The goal on one line, its job first, cut at the end; All of it › opens the four boxes (team.js crewGoalParts). */
function crewGoalLine(sid, handles) {
  const open = crewGoalOpen.has(sid);
  const btn = `<button class="mt-lk cp-all" data-cp-goal="${esc(sid)}" aria-expanded="${open}">${open ? "Less ‹" : "All of it ›"}</button>`;
  if (open) return `<div class="cp-goal open"><span class="cp-lab">Goal</span>${crewGoalHtml(sid, handles)}${btn}</div>`;
  const words = crewGoalParts(sid, handles).map(([, short, text], n) => (n ? `${short}: ${crewCode(text)}.` : `<b>${crewCode(text.replace(/\.?$/, "."))}</b>`)).join(" ");
  return `<div class="cp-goal"><span class="cp-lab">Goal</span><span class="cp-gt">${words}</span>${btn}</div>`;
}

/** Its last steps, oldest first, up to three: from its own feed when the lead's chat has it (chat/helpers.js helpersOf:
 *  each step's time and words, the one under way bold), else what it's on now. */
function crewCardSteps(sid, t) {
  const h = t.agent_id && typeof helpersOf === "function" ? helpersOf(sid).find((x) => x.id === t.agent_id) : null;
  const steps = (h?.items || []).filter((x) => x.t === "step").slice(-3).map((x) => ({ at: x.at_ms, html: `${esc(x.verb || "")} ${x.subject ? (x.mono ? `<code>${esc(x.subject)}</code>` : esc(x.subject)) : ""}`.trim(), now: x.done === false }));
  if (!steps.length && t.step) steps.push({ at: t.step_ms, html: crewCode(crewStepCode(t.step)), now: t.state === "working" });
  return steps.map((x) => `<span class="cp-st ${x.now ? "now" : ""}"><span class="cp-t">${x.at ? esc(hhmm(x.at)) : ""}</span><span class="cp-sw">${x.html}</span></span>`).join("");
}
/** What its review found: a line per finding (its kind as a pill, its words), two at most and "N more"; "clean" when
 *  it found nothing; "not yet" before it has one. */
function crewCardReview(t) {
  const f = t.findings || [];
  if (!f.length) return `<span class="cp-dim">${t.recommendation || ["reviewed", "committed"].includes(t.state) ? "clean" : ["queued", "reviewing"].includes(t.state) ? "waiting for its review" : "not yet"}</span>`;
  return f.slice(0, 2).map((l) => `<span class="cp-fl">${crewKind(findingKind(l))}${crewCode(crewFindingText(l))}</span>`).join("") + (f.length > 2 ? `<span class="cp-dim">${f.length - 2} more</span>` : "");
}
/** Its files on one line, each by its own name (the folder dropped), cut at the end; its size when Cue read it. */
function crewCardFiles(t) {
  const files = t.files || [];
  if (!files.length) return "";
  const names = files.map((f) => `<code>${esc(String(f).split("/").pop())}</code>`).join(" ");
  return `<span class="cp-fn">${names}</span>${t.stat ? `<span class="cp-dim cp-stat">${esc(t.stat.replace(/^\d+ files? /, ""))}</span>` : ""}`;
}
/** A labelled part of a card: its label in small caps on the left, its lines on the right. */
const crewCardPart = (label, html) => html ? `<div class="cp-part"><span class="cp-pl">${label}</span><span class="cp-pv">${html}</span></div>` : "";

/** One member's card in the list: its name with its stage in a word and its meter; what it was asked (two lines);
 *  what it's doing now (red when it waits on you) and a line of context; then its last steps, what its review found
 *  and its files; at its foot how many files and how long, and the one action the drawer's row has (team.js
 *  mateOneAct: Stop, Resume first when stuck, Skip while its review waits, else Diff). Its top is one button: a click
 *  opens its menu (team.js mateMenuHtml: its steps, its review, its Diff), and what that opened shows under the card's
 *  parts. A committed or set-aside one is dimmed. */
function crewListRow(sid, t) {
  const n = crewNow(sid, t), k = mateKey(sid, t), past = ["committed", "aside"].includes(t.state), files = (t.files || []).length;
  // The finding it's fixing is in its Review part: under what it's doing, who it waits on, if anyone.
  const sub = crewCode(n.fix ? (t.state === "reviewed" ? crewNext(sid, t) : "") : n.sub);
  const size = [files ? plural(files, "file") : "", t.started_ms ? mateSpan(t) : ""].filter(Boolean).join(" · ");
  const act = mateOneAct(sid, t);
  const top = `<button class="cp-row" data-key="cp:${esc(k)}" data-mt-more="${esc(k)}" aria-expanded="${mateMenu === k}">`
    + `<span class="cp-l1"><span class="cp-nm">${esc(t.name)}</span>${crewMeter(n)}</span>`
    + `<span class="cp-ask1">${crewCode(t.asked || t.what || "")}</span>`
    + `<span class="cp-now ${n.ask ? "q" : ""}">${crewCode(n.now)}</span>`
    + `${sub ? `<span class="cp-x">${sub}</span>` : ""}</button>`;
  const parts = crewCardPart("Steps", crewCardSteps(sid, t)) + crewCardPart("Review", crewCardReview(t)) + crewCardPart("Files", crewCardFiles(t));
  return `<div class="cp-card ${past ? "past" : ""} ${n.ask ? "q" : ""} ${mateMenu === k ? "on" : ""}" data-key="cpc:${esc(k)}">${top}<div class="cp-parts">${parts}</div>`
    + `<div class="cp-acts"><span class="cp-r">${esc(size)}</span>${act}</div>${mateUnderHtml(sid, t)}</div>`;
}
/** The finished members (committed, set aside) at the list's end: one line, "N done ›", folded until you click it (the
 *  drawer's fold, team.js mateDoneOpen, so it's open in both or neither); unfolded, their cards under it, dimmed, in the
 *  crew's order. Nothing above it moves. */
function crewDoneRows(sid) {
  const past = (sessionOf(sid)?.team || []).filter((t) => ["committed", "aside"].includes(t.state));
  if (!past.length) return "";
  const open = mateDoneOpen.has(sid);
  const line = `<button class="mt-lk mt-done-line" data-mt-done="${esc(sid)}" aria-expanded="${open}">${past.length} done ${open ? "⌄" : "›"}</button>`;
  return `<div class="cp-done ${open ? "open" : ""}" data-mt-for="done:${esc(sid)}">${line}${open ? `<div class="cp-cards">${past.map((t) => crewListRow(sid, t)).join("")}</div>` : ""}</div>`;
}
/** The crew's cards in the order the lead started them, whatever their state (a card never moves for it), one to a
 *  line; the finished ones folded at the end. */
const crewRowsHtml = (sid) => `<div class="cp-cards">${mateList(sid).map((t) => crewListRow(sid, t)).join("")}</div>` + crewDoneRows(sid);
/** The list's head: "The crew · 5", what needs you first (in red), then who's at work, in review and committed; Handle
 *  it / Ask me (team_send.js crewMode, its menu opening downward here). */
function crewStatusLine(sid, needs) {
  const team = sessionOf(sid)?.team || [], count = (...st) => team.filter((t) => st.includes(t.state)).length;
  const rest = [[count("working"), "working"], [count("queued", "reviewing"), "in review"], [count("committed"), "committed"]].filter(([n]) => n).map(([n, w]) => esc(`${n} ${w}`));
  const need = needs ? `<span class="cp-q">${needs} need${needs === 1 ? "s" : ""} you</span>` : "";
  return `<div class="cp-status"><span class="mt-grp">The crew · ${team.length}</span><span class="cp-count">${[need, ...rest].filter(Boolean).join(" · ")}</span><span class="grow"></span>${crewMode(sid)}</div>`;
}
/** The lead's own ask, if it has one (not a teammate's). */
const crewLeadAsk = (sid) => state.items.find((i) => i.session_id === sid && i.kind !== "waiting" && !mateOfAsk(i)) || null;
/** The lead's row on top of the list, lit while the left shows its chat (always, for now): what the left shows, and its
 *  state at the right ("working · 12s", "your turn", "asked 14:39"). Never "needs you": Needs you says that. */
function crewLeadRow(sid) {
  const s = sessionOf(sid), ask = crewLeadAsk(sid);
  const words = { waiting: "your turn", deciding: "deciding", agent: s.helpers ? "on its agents" : "on its lead", stopped: "stopped", limited: "out of usage" };
  const st = ask ? esc(`asked ${hhmm(ask.created_ms)}`) : s.state === "working" ? `working · <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span>` : esc(words[s.state] || s.state || "");
  return `<div class="cp-lead on"><span class="badge h-${esc(s.harness || "other")}">${agentLetter(s.harness)}</span><b>The lead</b><span class="cp-ld">the chat</span><span class="cp-lr">${st}</span></div>`;
}

/** The leads whose Needs you shows every row (N more ›), not just its first. */
const crewNeedsAll = new Set();
/** Needs you over the box: its first row only (`rows`, each one member's or one of the add-on's), "N more ›" drawing the
 *  rest in place, scrolling inside past what's left it. Nothing at all when nothing needs you. `count`: how many wait
 *  (more than the rows when the add-on gave its rows as one piece of html, which then counts as one row). */
function crewNeedsBox(sid, rows, count = rows.length) {
  if (!count) return "";
  const all = crewNeedsAll.has(sid) && rows.length > 1;
  const more = rows.length > 1 ? `<button class="mt-lk cp-nmore" data-cp-needs="${esc(sid)}" aria-expanded="${all}">${all ? "Fewer ‹" : `${count - 1} more ›`}</button>` : "";
  return `<div class="cp-needs ${all ? "all" : ""}"><div class="cp-nh"><span class="mt-grp red">Needs you · ${count}</span><span class="grow"></span>${more}</div><div class="cp-nrows">${(all ? rows : rows.slice(0, 1)).join("")}</div></div>`;
}
/** The left: the board's chat pane itself, as the board draws it (chat/pane.js activePane: its title line with its ⋯
 *  menu and btw, its folder, timeline and tabs, the chat with its waiting line, its box), with what the lead (or Cue,
 *  or the add-on: `added`, addOn.crewPageDecided) decided on its own placed in its chat by time, and `needs` (Needs
 *  you) in the crew tray's place. */
function crewPageLeft(sid, added, needs) {
  const s = sessionOf(sid);
  // Each decision one keyed row of the chat (a row and what it opened under it), so the chat patches it in place.
  const row = (key, html) => `<div class="cp-dec" data-key="${esc(key)}">${html}</div>`;
  const decided = (s.crew_log || []).map((d) => ({ at: d.at_ms || 0, html: row(`cd:${d.id}`, crewLogRow(sid, d, true)) }));
  const theirs = added.items ? added.items.map((x, n) => ({ at: x.at_ms || 0, html: row(`ad:${n}:${x.at_ms || 0}`, x.html) }))
    : added.html ? [{ at: -Infinity, html: row("ad", added.html) }] : [];
  // The item the chat stands on: its waiting card (the reply it stopped on), else the lead's own ask; never a
  // teammate's (those are in Needs you).
  const own = state.items.filter((i) => i.session_id === sid && !mateOfAsk(i));
  const it = own.find((i) => i.kind === "waiting") || own[0] || null;
  return activePane({ sid, it, extra: decided.concat(theirs), needs });
}

/** A plain crew's state in one word, for its head's pill: needs you (the lead or a member asks), working (the lead or a
 *  member at work), else done. [word, look] as the add-on's head gives them. */
function crewPill(sid) {
  const s = sessionOf(sid), team = sessionOf(sid)?.team || [];
  if (crewLeadAsk(sid) || team.some((t) => mateNeeds(sid, t))) return ["needs you", "need"];
  if (s.state === "working" || team.some((t) => t.state === "working")) return ["working", "run"];
  return ["done", "off"];
}
/** The head: ‹ Back, then the add-on's head line (`head`), else the lead's name, "Cue Crew" and the crew's state in a
 *  word; Open chat › at its right. What the chat pane's own title line has (⋯, btw, Stop, the folder) isn't repeated. */
function crewPageHead(sid, head) {
  const s = sessionOf(sid), [word, look] = crewPill(sid), oc = `<button class="cp-oc" data-cp-chat>Open chat ›</button>`;
  return `<div class="cp-head"><button class="cp-back" data-cp-back>‹ Back to ${esc(crewPageBack)}</button>${head ? head + oc
    : `<span class="cp-name">${esc(nameOf(sid, s.project))}</span><span class="cp-kind">Cue Crew</span><span class="cp-pill ${look}">${word}</span><span class="grow"></span>${oc}`}</div>`;
}

/** The page: its head (crewPageHead), the goal line and the add-on's rows; then the
 *  left (the lead's chat, Needs you, its box), as wide as the board's chat pane (board.js boardChatWidth), beside the
 *  list, which has the rest: The lead, the add-on's band, the crew's cards. */
function crewPageView(sid) {
  const s = sessionOf(sid), handles = crewHandles(sid), needs = crewNeedRows(sid, crewGroups(sid), true);
  // The add-on's part, each empty without it: its head line, its band, rows under the goal, rows in Needs you, decisions.
  const head = addOn.crewPageHead?.(sid) || "", band = addOn.crewPageBand?.(sid) || "", rowsAdded = addOn.crewPageRows?.(sid) || "";
  const needsAdded = addOn.crewPageNeeds?.(sid) || { items: [], count: 0 }, decidedAdded = addOn.crewPageDecided?.(sid) || { html: "", count: 0 };
  // Needs you, a row each: the crew's, then the add-on's (its items; an add-on that still gives one piece of html, as one).
  const added = needsAdded.items ? needsAdded.items.map((x) => x.html) : needsAdded.count && needsAdded.html ? [needsAdded.html] : [];
  const needRows = needs.items.concat(added), needCount = needs.list.length + (needsAdded.items ? added.length : needsAdded.count || 0);
  return `<div class="crew-page" data-crew-page="${esc(sid)}">
    ${crewPageHead(sid, head)}
    <div class="cp-upper" data-scroll-key="cpu:${esc(sid)}"><div class="cp-top">${crewGoalLine(sid, handles)}${rowsAdded}</div></div>
    <div class="cp-body" style="--cp-chat-w:${typeof boardChatWidth === "function" ? boardChatWidth() : "50%"}"><div class="cp-left">${crewPageLeft(sid, decidedAdded, crewNeedsBox(sid, needRows, needCount))}</div>
      <aside class="cp-list">${crewLeadRow(sid)}${band ? `<div class="cp-band" data-scroll-key="cpb:${esc(sid)}">${band}</div>` : ""}${crewStatusLine(sid, needCount)}<div class="cp-scroll" data-scroll-key="cpl:${esc(sid)}">${crewRowsHtml(sid)}</div></aside></div>
    ${mateMenuOf(sid)}</div>`;
}

/** Esc on the page: the mode menu or a member's menu closes first, then a background agent or a reply in the reader
 *  open on the left (back to the chat), then the page; not while you type in a box that has text (Esc is the box's
 *  there) or something is open over the window. True when it took the key. */
function crewPageEsc(e) {
  if (e.key !== "Escape" || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.isComposing) return false;
  if (!crewPageSid || sheet || lightbox || !document.querySelector(".crew-page")) return false;
  const f = document.activeElement;
  if (f?.matches?.("textarea, input") && f.value) return false;
  if (typeof crewMenuFor !== "undefined" && crewMenuFor) { crewMenuFor = null; renderMain(); }
  else if (mateMenu) { mateMenu = null; renderMain(); }
  else if (helperView?.sid === crewPageSid) closeHelperView();
  else if (readerView?.sid === crewPageSid) closeReader();
  else crewPageClose();
  return true;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (crewPageEsc(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", (e) => {
    const el = e.target.closest?.("[data-cp-open], [data-cp-back], [data-cp-chat], [data-cp-goal], [data-cp-needs]");
    if (!el) return;
    e.stopPropagation();
    if (el.dataset.cpOpen) return crewPageOpen(el.dataset.cpOpen);
    if (el.matches("[data-cp-back], [data-cp-chat]")) return crewPageClose(el.matches("[data-cp-chat]"));
    if (el.dataset.cpNeeds) { const sid = el.dataset.cpNeeds; if (!crewNeedsAll.delete(sid)) crewNeedsAll.add(sid); return renderMain(); }
    const sid = el.dataset.cpGoal;
    if (!crewGoalOpen.delete(sid)) crewGoalOpen.add(sid);
    renderMain();
  }, true);
}

