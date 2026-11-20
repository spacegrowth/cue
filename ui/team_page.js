// ---------- a lead's crew on a page of its own (team.js has the drawer this grows from) ----------
// "Open full ›" on the crew's tray line opens it across the window, in the board's and the chat's place (the top bar and
// tabs stay): ‹ Back to the board (or Esc, or Open chat ›) goes back to the lead's chat. It fits the window: a head line;
// the goal on one line (All of it › opens its four boxes in place); then the left reads and the right picks. Left, as
// wide as on the board: the board's chat pane itself (chat/pane.js activePane: its title line, btw, its chat with the
// waiting line, its box with the same draft), what the lead decided on its own placed in its chat by time, and Needs
// you over its box (one bar, one ask at a time: team_bar.js) in the crew tray's place. Right, the rest of the width: The lead, then the crew's cards (one to a line) in the order the
// lead started them (a card never moves for its state), the finished ones folded into "N done ›" (the drawer's fold).
// The optional add-on may add to it (core/addon.js addOn.crewPage*): its own head line, a band under The lead, rows under the
// goal, rows in Needs you and decisions in the chat.

/** The lead whose crew's page is open; null when none is. It closes when that session goes away, or another opens. */
let crewPageSid = null;
/** The open crew page becomes lead `sid`'s (null: none); the add-on hears of each change (addOn.crewPageChanged). */
function crewPageSet(sid) {
  if (crewPageSid === sid) return;
  crewPageSid = sid;
  try { addOn.crewPageChanged(sid); } catch (e) { console.error(e); }
}
/** The view the page was opened from and shows within (its tab stays selected), and what its Back button names:
 *  the board's unless the add-on opened it from one of its views. */
let crewPageFrom = "board", crewPageBack = "the board";

/** Whether lead `sid`'s crew page is what the window shows. */
const crewPageOn = (sid) => !!sid && crewPageSid === sid && view === crewPageFrom;
/** Open the crew's page for lead `sid`, within view `from` (Back names `back`). */
function crewPageOpen(sid, from = "board", back = "the board") {
  crewPageSet(sid);
  crewPageFrom = from;
  crewPageBack = back;
  mateOpenLeads.delete(sid);
  mateMenu = null;
  if (typeof crewMenuFor !== "undefined") crewMenuFor = null;
  // It opens on the lead's chat.
  crewLeft.clear();
  crewChip = null;
  view = from;
  // Its chat is drawn afresh: it opens at its end, as the chat does.
  drawAtChatEnd();
}
/** The page for lead `sid` from elsewhere (the add-on's): its chat becomes the one open first, so the board shows it
 *  after. `from`: the add-on's view it opens within (that tab stays selected; Back and Esc return to it), else the
 *  board; `back`: that view's name for the Back button. */
addOn.openCrewPage = (sid, { from = "board", back = "" } = {}) => {
  if (!sessionOf(sid) && !addOn.crewPageRecord?.(sid)) return;
  // A record (the add-on's, its lead gone) has no chat to open first.
  if (sessionOf(sid) && active?.sid !== sid) setActive(null, sid);
  crewPageOpen(sid, from, back || (from === "board" ? "the board" : from));
};
/** The add-on's way out: the page closes if it was opened within view `from` (any, when not given); true when it did. */
addOn.closeCrewPage = (from) => {
  if (!crewPageSid || (from && crewPageFrom !== from)) return false;
  crewPageSet(null);
  crewPageFrom = "board";
  return true;
};
/** Back to where it was opened from: the board, the lead's box keeping the cursor, or the add-on's view. `chat`: to
 *  the lead's chat on the board, wherever it was opened from (Open chat ›). */
function crewPageClose(chat = false) {
  const from = crewPageFrom;
  crewPageSet(null);
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
  const rec = !sessionOf(crewPageSid) && addOn.crewPageRecord?.(crewPageSid);
  if (rec) return `<div class="board cp-board">${crewRecordView(crewPageSid, rec)}</div>`;
  if (!sessionOf(crewPageSid) || (active?.sid && active.sid !== crewPageSid)) { crewPageSet(null); return null; }
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

/** What matters about a member now, in one plain sentence: what it wants of you (it needs you), what it's on (at
 *  work), what its review found and who settles it ("1 should-fix · Cue sends it back"), else its last words. */
function crewRowSays(sid, t, n = crewNow(sid, t)) {
  if (n.ask) return n.now;
  if (t.state === "reviewed") return `${crewFound(t)} · ${crewNext(sid, t)}`;
  if (["done", "stopped", "aside"].includes(t.state) && t.summary) return t.summary;
  return n.now;
}
/** One member's row in the list: calm, two lines, a thin line between rows (no box). Line 1: its name, its stage in a
 *  word and its meter, how long, and the one action the drawer's row has at the right (team.js mateOneAct: Stop,
 *  Resume first when stuck, Skip while its review waits, else Diff). Line 2: one sentence (crewRowSays), red when it
 *  needs you. The row is one button: a click picks it, and the left shows it whole (team_mate.js: what it was asked,
 *  its files and their diff), its row lit, its steps and what its review found unfolding under it (crewMateExp). What
 *  its last button said shows under it; a committed or set-aside one is dimmed. */
function crewListRow(sid, t) {
  const n = crewNow(sid, t), k = mateKey(sid, t), past = ["committed", "aside"].includes(t.state), lit = crewMateLit(sid, k);
  const top = `<button class="cp-row" data-key="cp:${esc(k)}" data-cp-mate="${esc(k)}" aria-pressed="${lit}">`
    + `<span class="cp-l1"><span class="cp-nm">${esc(t.name)}</span>${crewMeter(n)}<span class="cp-tm">${t.started_ms ? esc(mateSpan(t)) : ""}</span></span>`
    + `<span class="cp-now ${n.ask ? "q" : ""}">${crewCode(crewRowSays(sid, t, n))}</span></button>`;
  const exp = lit && typeof crewMateExp === "function" ? crewMateExp(sid, t) : "";
  return `<div class="cp-card ${past ? "past" : ""} ${n.ask ? "q" : ""} ${lit ? "on" : ""}" data-key="cpc:${esc(k)}">${top}<span class="cp-act">${mateOneAct(sid, t)}</span>${mateUnderHtml(sid, t)}${exp}</div>`;
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
/** The lead's row on top of the list, lit while the left shows its chat, and its state at the right ("working · 12s",
 *  "your turn", "asked 14:39"); while the left shows something else, the first words of its latest reply, and a dark
 *  dot when it said something since the left left its chat. A click picks it: the chat, where you left it. Never
 *  "needs you": the bar says that. */
function crewLeadRow(sid) {
  const s = sessionOf(sid), ask = crewLeadAsk(sid), on = !crewLeftOf(sid);
  const words = { waiting: "your turn", deciding: "deciding", agent: s.helpers ? "on its agents" : "on its lead", stopped: "stopped", limited: "out of usage" };
  const st = ask ? esc(`asked ${hhmm(ask.created_ms)}`) : s.state === "working" ? `working · <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span>` : esc(words[s.state] || s.state || "");
  const last = (s.thread || []).filter(askLnSaid).at(-1);
  const said = on ? "the chat" : `“${esc(plain(firstLine(last?.text || "")))}”`;
  const dot = !on && crewLeadSaid(sid) ? `<i class="cp-said" role="img" aria-label="It said something since"></i>` : "";
  return `<button class="cp-lead ${on ? "on" : ""}" data-cp-lead="${esc(sid)}" aria-pressed="${on}"><span class="badge h-${esc(s.harness || "other")}">${agentLetter(s.harness)}</span><b>The lead</b><span class="cp-ld">${said}</span>${dot}<span class="cp-lr">${st}</span></button>`;
}

/** What the left shows for lead `sid` other than its chat, the page's one left: { kind: "needs" } (the Needs you page,
 *  team_bar.js), { kind: "mate", key, tab, file } (a teammate, team_mate.js), or { kind: "addon", key, html } (the
 *  add-on's own page, addOn.crewPageLeft: it keeps that state itself, and while it gives one it is what the left
 *  shows); null: the lead's chat. Cleared when the page opens. */
const crewLeft = new Map();
function crewLeftOf(sid) {
  const theirs = addOn.crewPageLeft?.(sid);
  return theirs ? { kind: "addon", key: theirs.key, html: theirs.html } : crewLeft.get(sid) || null;
}
/** Whether teammate `key`'s row is lit: the left shows it. */
const crewMateLit = (sid, key) => crewLeftOf(sid)?.kind === "mate" && crewLeftOf(sid).key === key;
/** Where the lead's chat was scrolled when the left last left it, put back when it returns: { top, end }. */
let crewChatAt = null;
/** Show `what` on the left (null: the lead's chat, where you left it). */
function crewLeftSet(sid, what) {
  const was = crewLeftOf(sid);
  if (!was && what) {
    const c = document.querySelector(".cp-left [data-chat]");
    crewChatAt = c ? { top: c.scrollTop, end: boxAtEnd(c) } : null;
    crewLeftChatAt = Date.now();
  }
  // The add-on's page gives way to whatever the left shows next.
  if (was?.kind === "addon") addOn.crewPageLeftClose?.(sid);
  // A teammate or Needs you takes the pane's head with it, btw too: it closes, not left open out of sight.
  if (["needs", "mate"].includes(what?.kind) && btwFor === sid) btwFor = null;
  if (what) crewLeft.set(sid, what); else crewLeft.delete(sid);
  if (!was || what) return;
  renderMain();
  const c = document.querySelector(".cp-left [data-chat]"), at = crewChatAt;
  crewChatAt = null;
  if (c && at) { if (at.end) boxStickToEnd(c, false); else c.scrollTop = at.top; }
}
/** Back to the lead's chat, the cursor in its box. */
function crewToLead(sid) {
  crewLeftSet(sid, null);
  renderMain();
  focusComposer();
}
/** The head line of what the left shows in the chat's place: ‹ The lead's chat (Esc), then its own words. */
const crewViewHead = (sid, inner) => `<div class="cpv-head"><button class="cpv-back" data-cp-lead="${esc(sid)}">‹ The lead's chat</button><span class="esc-hint">esc</span><span class="cpv-sep"></span>${inner}</div>`;
/** The left: the board's chat pane itself, as the board draws it (chat/pane.js activePane: its title line with its ⋯
 *  menu and btw, its folder, timeline and tabs, the chat with its waiting line, its box), with what the lead (or Cue,
 *  or the add-on: `added`, addOn.crewPageDecided) decided on its own placed in its chat by time, and `needs` (the
 *  Needs you bar, team_bar.js) in the crew tray's place; the box held while the lead itself asks. Not the chat (the
 *  Needs you page, a teammate): that in the chat's place, the bar and the box under it. */
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
  // Not the chat: Needs you or a teammate in the pane's place, its head line their own; the add-on's page in the
  // chat's place only, under the pane's head. The bar and the box stay under either.
  const left = crewLeftOf(sid), view = left?.kind === "needs" ? crewNeedsPage(sid) : left?.kind === "mate" && typeof crewMateView === "function" ? crewMateView(sid, left) : "";
  if (left && left.kind !== "addon" && !view) crewLeft.delete(sid);
  return activePane({ sid, it, extra: decided.concat(theirs), needs, view, page: left?.kind === "addon" ? left : null, foot: crewFoot(sid, !view) });
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
  // A record (its lead gone) has no chat to open.
  const s = sessionOf(sid), oc = s ? `<button class="cp-oc" data-cp-chat>Open chat ›</button>` : "", back = `<button class="cp-back" data-cp-back>‹ Back to ${esc(crewPageBack)}</button>`;
  if (head || !s) return `<div class="cp-head">${back}${head}${oc}</div>`;
  const [word, look] = crewPill(sid);
  return `<div class="cp-head">${back}<span class="cp-name">${esc(nameOf(sid, s.project))}</span><span class="cp-kind">Cue Crew</span><span class="cp-pill ${look}">${word}</span><span class="grow"></span>${oc}</div>`;
}

/** The page: its head (crewPageHead), the goal line and the add-on's rows; then the
 *  left (the lead's chat, Needs you, its box), as wide as the board's chat pane (board/columns.js boardChatWidth), beside the
 *  list, which has the rest: The lead, the add-on's band, the crew's cards. While the left shows a teammate (`mate`),
 *  the left takes all but a narrow list, so its diff has the room. */
function crewPageView(sid) {
  const handles = crewHandles(sid), needCount = crewAsks(sid).filter((x) => !x.lead).length;
  // The add-on's part, each empty without it: its head line, its band, rows under the goal, decisions (its asks are in
  // the bar's list, team_bar.js crewAsks).
  const head = addOn.crewPageHead?.(sid) || "", band = addOn.crewPageBand?.(sid) || "", rowsAdded = addOn.crewPageRows?.(sid) || "";
  const decidedAdded = addOn.crewPageDecided?.(sid) || { html: "", count: 0 };
  const mate = crewLeftOf(sid)?.kind === "mate";
  return `<div class="crew-page" data-crew-page="${esc(sid)}">
    ${crewPageHead(sid, head)}
    <div class="cp-upper" data-scroll-key="cpu:${esc(sid)}"><div class="cp-top">${addOn.crewPageGoal?.(sid) ?? crewGoalLine(sid, handles)}${rowsAdded}</div></div>
    <div class="cp-body ${mate ? "mate" : ""}" style="--cp-chat-w:${typeof boardChatWidth === "function" ? boardChatWidth() : "50%"}"><div class="cp-left">${crewPageLeft(sid, decidedAdded, crewBarHtml(sid))}</div>
      <aside class="cp-list">${crewLeadRow(sid)}${band ? `<div class="cp-band" data-scroll-key="cpb:${esc(sid)}">${band}</div>` : ""}${crewStatusLine(sid, needCount)}<div class="cp-scroll" data-fit data-scroll-key="cpl:${esc(sid)}">${crewRowsHtml(sid)}</div><button class="cp-jump" data-cp-jump="" hidden></button></aside></div>
    ${mateMenuOf(sid)}</div>`;
}
/** A record's page (addOn.crewPageRecord `rec`: something whose lead is gone): the add-on's head and rows, its page on
 *  the left (no box, no Needs you), and in the list its band, then its crew by name. */
function crewRecordView(key, rec) {
  const left = addOn.crewPageLeft?.(key), band = addOn.crewPageBand?.(key) || "";
  return `<div class="crew-page cp-record" data-crew-page="${esc(key)}">
    ${crewPageHead(key, addOn.crewPageHead?.(key) || "")}
    <div class="cp-upper" data-scroll-key="cpu:${esc(key)}"><div class="cp-top">${addOn.crewPageRows?.(key) || ""}</div></div>
    <div class="cp-body" style="--cp-chat-w:${typeof boardChatWidth === "function" ? boardChatWidth() : "50%"}"><div class="cp-left"><div class="cp-addon-left" data-scroll-key="cpx:${esc(left?.key || key)}">${left?.html || ""}</div></div>
      <aside class="cp-list">${band ? `<div class="cp-band" data-scroll-key="cpb:${esc(key)}">${band}</div>` : ""}<div class="cp-scroll" data-scroll-key="cpl:${esc(key)}">${rec.mates || ""}</div></aside></div></div>`;
}

// ---------- a big crew: the list scrolls, a chip for an ask out of view, ↑ / ↓ through the list ----------
/** The list's row that has the cursor ("lead", or a teammate's key), kept across draws; null when it's elsewhere. */
let crewListFocus = null;
/** After each draw (app/render.js placeFloating) and on resize: the cursor back on its row, and the jump chip. A big
 *  crew keeps its two-line rows and the list scrolls; the chip points at an ask out of view. */
function crewListPlace() {
  const box = crewPageSid && document.querySelector(".crew-page .cp-scroll[data-fit]");
  if (!box) return;
  // The cursor was on a row of the list (↑ / ↓): a draw built the row afresh, so it goes back to it.
  if (crewListFocus && (!document.activeElement || document.activeElement === document.body)) {
    document.querySelector(crewListFocus === "lead" ? ".crew-page .cp-lead" : `.crew-page .cp-row[data-cp-mate="${CSS.escape(crewListFocus)}"]`)?.focus();
  }
  crewJumpPlace(box);
}
/** The chip at the list's edge for a red row (one that needs you) out of view: "↓ webhooks needs you", two or more
 *  "↓ 2 need you"; at the bottom edge when any is below, else the top. A click scrolls the list to it; nothing is
 *  picked. */
function crewJumpPlace(box) {
  const chip = box.parentElement?.querySelector(".cp-jump");
  if (!chip) return;
  const top = box.scrollTop, bottom = top + box.clientHeight;
  const red = [...box.querySelectorAll(".cp-card.q")].map((el) => ({ el, y: el.offsetTop - box.offsetTop, h: el.offsetHeight }));
  const below = red.filter((r) => r.y + r.h / 2 > bottom), above = red.filter((r) => r.y + r.h / 2 < top);
  const off = below.length ? below : above, down = !!below.length;
  chip.hidden = !off.length;
  if (!off.length) return;
  chip.classList.toggle("up", !down);
  chip.dataset.cpJump = off[0].el.dataset.key;
  chip.textContent = `${down ? "↓" : "↑"} ${off.length > 1 ? `${off.length} need you` : `${off[0].el.querySelector(".cp-nm")?.textContent || ""} needs you`}`;
}
/** ↑ / ↓ with the cursor in the list: the pick moves along its fixed order (The lead, then each row shown; folded done
 *  rows are skipped until the fold is open); Home: The lead. The cursor stays in the list. */
function crewListKey(e) {
  const at = document.activeElement;
  if (!crewPageSid || !at?.matches?.(".crew-page .cp-lead, .crew-page .cp-row") || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return false;
  if (!["ArrowUp", "ArrowDown", "Home"].includes(e.key)) return false;
  const order = [...document.querySelectorAll(".crew-page .cp-lead, .crew-page .cp-scroll .cp-row")];
  const n = order.indexOf(at), to = e.key === "Home" ? order[0] : order[Math.max(0, Math.min(order.length - 1, n + (e.key === "ArrowDown" ? 1 : -1)))];
  if (!to || to === at) return true;
  const sid = crewPageSid, key = to.dataset.cpMate;
  if (key) crewMatePick(sid, key, true); else { crewLeftSet(sid, null); renderMain(); }
  const again = key ? document.querySelector(`.crew-page .cp-row[data-cp-mate="${CSS.escape(key)}"]`) : document.querySelector(".crew-page .cp-lead");
  again?.focus();
  again?.scrollIntoView({ block: "nearest" });
  return true;
}

/** Esc on the page, nearest thing first, one step a press: a menu closes (Handle it, a member's, ⋯, Always…; a box's
 *  own "/" or "@" list is the box's); btw closes; the cursor leaves a box that has text (its draft stays); ↪'s chip
 *  comes off the box; what the left shows other than the chat (a teammate, Needs you, a background agent, a reply in
 *  the reader) goes back to the chat; then the page closes, back to where it was opened from. Never a Stop: that's the
 *  pane's button here. Not while something is open over the window. True when it took the key. */
function crewPageEsc(e) {
  if (e.key !== "Escape" || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.isComposing) return false;
  if (!crewPageSid || sheet || lightbox || deleteAsk || !document.querySelector(".crew-page")) return false;
  const sid = crewPageSid, f = document.activeElement, key = f?.dataset?.text;
  if (key && cmdMatches(key).length) return false;
  if (typeof crewMenuFor !== "undefined" && crewMenuFor) crewMenuFor = null;
  else if (mateMenu) mateMenu = null;
  else if (moreFor || menuFor || redirectFor) moreFor = menuFor = redirectFor = null;
  // btw is the pane head's: drawn only while the pane's head is (the chat, or the add-on's page under it).
  else if (btwFor && !["needs", "mate"].includes(crewLeftOf(sid)?.kind)) btwFor = null;
  // The add-on's own step (an edit it has open, then its page on the left back to the chat).
  else if (addOn.crewPageEsc?.(sid)) { /* it took it */ }
  else if (f?.matches?.("textarea, input") && f.value) { f.blur(); return true; }
  else if (crewChip?.sid === sid) crewChip = null;
  // (A record's left is all it has: Esc leaves the page.)
  else if (sessionOf(sid) && crewLeftOf(sid)) { crewToLead(sid); return true; }
  else if (helperView?.sid === sid) { closeHelperView(); return true; }
  else if (readerView?.sid === sid) { closeReader(); return true; }
  else { crewPageClose(); return true; }
  renderMain();
  return true;
}

if (typeof document !== "undefined") {
  document.addEventListener("focusin", (e) => {
    const row = e.target?.closest?.(".crew-page .cp-lead, .crew-page .cp-row");
    crewListFocus = row ? (row.dataset.cpMate || "lead") : null;
  });
  document.addEventListener("scroll", (e) => { if (e.target?.matches?.(".crew-page .cp-scroll")) crewJumpPlace(e.target); }, true);
  document.defaultView?.addEventListener("resize", () => crewListPlace());
  document.addEventListener("click", (e) => {
    const el = e.target.closest?.("[data-cp-open], [data-cp-back], [data-cp-chat], [data-cp-goal], [data-cp-lead], [data-cp-jump]");
    if (!el) return;
    e.stopPropagation();
    if (el.matches("[data-cp-jump]")) return document.querySelector(`.crew-page .cp-card[data-key="${CSS.escape(el.dataset.cpJump)}"]`)?.scrollIntoView({ block: "center" });
    if (el.dataset.cpOpen) return crewPageOpen(el.dataset.cpOpen);
    if (el.matches("[data-cp-back], [data-cp-chat]")) return crewPageClose(el.matches("[data-cp-chat]"));
    if (el.dataset.cpLead) return crewToLead(el.dataset.cpLead);
    const sid = el.dataset.cpGoal;
    if (!crewGoalOpen.delete(sid)) crewGoalOpen.add(sid);
    renderMain();
  }, true);
}

