// ---------- a lead's crew on a page of its own (team.js has the drawer this grows from) ----------
// "Open full ›" on the crew's tray line opens it across the window, in the board's and the chat's place (the top bar and
// tabs stay): ‹ Back to the board (or Esc, or Open chat ›) goes back to the lead's chat. It fits the window: a head line;
// the goal on one line (All of it › opens its four boxes in place); the crew, one lane per member still in it, read
// down in columns (who, with a five-stage meter; what it was asked; what it's doing now; its work; its links), a click
// on its name opening its steps, findings and files in place, the finished ones folded into "N done ›" (the drawer's fold);
// at the bottom the lead's stream (what it said and what it decided on its own, newest last, your own messages small on
// the right, the lead's box under it: the chat's draft, so what you type carries over) beside Needs you, given at
// least 45% of the page's height. What's above it (the add-on's band, the goal, the lanes) scrolls inside itself, as the
// stream does. The optional add-on may add to it (core.js addOn.crewPage*): its own head line,
// a band under it, rows under the goal, rows in Needs you and decisions in the stream.

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
  renderMain();
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
  renderMain();
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
/** A member's steps so far, oldest first, as short words (`code` in backticks): its decision's head and what came of
 *  it, when the log has one; else what its row in the drawer says (at work, reviewed: what it found, what's next). */
function crewSteps(sid, t) {
  const d = crewEntryOf(sid, t);
  if (d?.head) return [d.head, ...(d.steps || [])];
  const out = [];
  if (t.state === "working") out.push(t.fix_rounds ? "sent back" : "", t.fix_rounds ? "fixing" : "working");
  else if (t.state === "queued") out.push("review waits for the lead");
  else if (t.state === "reviewing") out.push("reviewing");
  else if (t.state === "reviewed") out.push(`reviewed: ${crewFound(t)}`, crewNext(sid, t));
  else if (t.state === "committed") out.push(t.commit ? `committed \`${t.commit.slice(0, 7)}\`` : "committed");
  else if (t.state === "aside") out.push("set aside");
  else if (t.state === "stopped") out.push("stopped");
  else out.push("done");
  if (mateAsk(sid, t)) out.push("asks you");
  return out.filter(Boolean);
}
/** A member's state in a word, and its pill's look: "need" (red), "live" (dark: at work, or what Cue does next). */
function crewStateWord(sid, t) {
  if (mateAsk(sid, t)) return ["asks you", "need"];
  if (mateNeeds(sid, t)) return ["needs you", "need"];
  if (t.state === "working" && t.fix_rounds) return ["sent back", "live"];
  const words = { working: ["working", "live"], queued: ["queued", ""], reviewing: ["reviewing", "live"], reviewed: ["reviewed", ""], committed: ["committed", ""], aside: ["set aside", ""], stopped: ["stopped", ""] };
  return words[t.state] || ["done", ""];
}

/** The five stages of a lane's meter, in order. */
const CREW_STAGES = ["working", "checking", "reviewed", "fixing", "committed"];
/** A step of its own that checks its work: a test, lint, type check or build run. */
const CREW_CHECKING = /^Running\b.*\b(test|tests|check|lint|clippy|tsc|build|pytest|jest|vitest|playwright)\b/i;
/** A step's words with what it runs or edits as code: "Running: npm test" → "Running `npm test`", "Editing a/b.js" →
 *  "Editing `a/b.js`". */
const crewStepCode = (step) => String(step || "").replace(/^Running:\s*(.+)$/, "Running `$1`").replace(/^(Editing|Writing|Reading|Creating) (\S+)$/, "$1 `$2`");
/** A finding's words without its number and kind: "hub/handoff.rs:40 — the list is cut at 8". */
const crewFindingText = (line) => String(line).replace(/^\d+\.\s*/, "").replace(FIX_KIND, " — ").replace(/^\s*—\s*/, "");
/** What a lane says a member is doing now, all from its own data (its state, step, ask, findings, the crew's log),
 *  never more: `now` one plain sentence (`code` in backticks; `ask`: it waits on you), `sub` a dim line of context,
 *  `fix` the finding it's fixing (its kind, its words), and its meter: at stage `at` (CREW_STAGES), `look` "now" (on
 *  it), "need" (it waits on you there) or "done" (that far and stopped), `word` the stage in a word. */
function crewNow(sid, t) {
  const ask = mateAsk(sid, t), log = crewEntryOf(sid, t), last = log ? [log.head, ...(log.steps || [])].filter(Boolean).at(-1) : "";
  const logLine = last && log.at_ms ? `${last} at ${hhmm(log.at_ms)}` : "";
  const fixes = (t.findings || []).filter((l) => ["should-fix", "must-fix", "blocker"].includes(findingKind(l)));
  const working = t.state === "working", step = crewStepCode(t.step);
  let at = 0, look = "done", word = crewStateWord(sid, t)[0], now = step || word, sub = "", fix = null;
  if (working) {
    look = "now";
    if (t.fix_rounds) { at = 3; word = "fixing"; }
    else if (CREW_CHECKING.test(t.step || "")) { at = 1; word = "checking"; }
    else word = "working";
    if (t.quiet === "stuck") now = `Quiet for ${mateQuietMins(t)}m — looks stuck`;
    else if (t.fix_rounds && fixes.length) { now = "Fixing what review found"; fix = [findingKind(fixes[0]), crewFindingText(fixes[0])]; }
    else if (!step) now = t.fix_rounds ? "Fixing what review found" : "Working";
    if (!fix) sub = logLine || (t.step && t.step_ms ? `on it since ${hhmm(t.step_ms)}` : "");
  } else if (["queued", "reviewing"].includes(t.state)) {
    at = 2; look = "now"; word = t.state === "queued" ? "queued" : "reviewing";
    now = t.state === "queued" ? "Its review waits for the lead" : "Being reviewed";
    sub = logLine;
  } else if (t.state === "reviewed") {
    at = 2; word = "reviewed"; now = `Reviewed: ${crewFound(t)}`;
    if (fixes.length) fix = [findingKind(fixes[0]), crewFindingText(fixes[0])];
    sub = crewNext(sid, t);
  } else if (t.state === "committed") {
    at = 4; word = "committed";
    const said = log && [log.head, ...(log.steps || [])].filter(Boolean).find((x) => /^committed\b/i.test(x));
    now = [said ? said.replace(/^c/, "C") : "Committed", t.commit ? t.commit.slice(0, 7) : ""].filter(Boolean).join(" ");
    sub = [t.recommendation || (t.findings || []).length ? `review: ${crewFound(t)}` : "", t.ended_ms ? `ended ${hhmm(t.ended_ms)}` : ""].filter(Boolean).join(" · ");
  } else {
    // Done, stopped, set aside: what it said last.
    now = { aside: "Set aside", stopped: "Stopped" }[t.state] || "Done";
    sub = t.summary || "";
  }
  if (ask) { look = "need"; word = "asked you"; now = `${verb(ask).replace(/^./, (c) => c.toUpperCase())} \`${summary(ask)}\``; sub = logLine || (step && step !== `Running \`${summary(ask)}\`` ? step : ""); }
  else if (t.needs) { look = "need"; word = "needs you"; now = t.needs; }
  return { now, sub, fix, at, look, word, ask: !!ask || !!t.needs };
}
/** The meter: five small bars, those before its stage done, its stage as `look` says; its stage in a word after. */
function crewMeter(n) {
  const bars = CREW_STAGES.map((_, i) => `<b class="${i < n.at ? "done" : i === n.at ? n.look : ""}"></b>`).join("");
  return `<span class="cp-meter"><span class="cp-m" aria-hidden="true">${bars}</span><span class="cp-ml">${esc(n.word)}</span></span>`;
}
/** A finding's kind as a small pill: should-fix (and worse) amber, the rest outlined. */
const crewKind = (kind) => `<span class="cp-f ${["should-fix", "must-fix", "blocker"].includes(kind) ? "fix" : ""}">${esc(kind)}</span>`;
/** Its work: its files and +/− (so far, at work), then how long and since when. */
function crewWork(t) {
  const files = (t.files || []).length;
  const size = t.stat || (files ? `${plural(files, "file")}${t.state === "working" ? " so far" : ""}` : "");
  return `<div class="cp-work">${size ? `<span>${esc(size)}</span>` : ""}<span>${esc([mateSpan(t), t.started_ms ? `since ${hhmm(t.started_ms)}` : ""].filter(Boolean).join(" · "))}</span></div>`;
}
/** A member's links: Answer when it asks you (its row in Needs you), Review (its full review under its lane, or a
 *  review asked for once it's done), Diff, Talk (the chat's box with "@name " typed), and Stop (Resume first when
 *  stuck) at work, or Skip while its review waits. */
function crewLaneLinks(sid, t) {
  const k = mateKey(sid, t);
  const answer = mateNeeds(sid, t) ? `<button class="mt-lk" data-cp-answer="${esc(k)}">Answer</button>` : "";
  const review = t.review ? `<button class="mt-lk ${mateUnder.get(k) === "review" ? "on" : ""}" data-mt-full="${esc(k)}">Review</button>`
    : ["done", "stopped"].includes(t.state) ? mateBtn(sid, t, "review", "Review", "link") : "";
  const live = ["working", "queued"].includes(t.state) ? mateOneAct(sid, t) : "";
  return answer + review + mateBtn(sid, t, "diff", t.state === "working" ? "Diff so far" : "Diff", "link") + `<button class="mt-lk" data-cp-talk="${esc(t.name)}">Talk</button>` + live;
}
/** The lanes open in place (a click on the name), by member key; the others stay where they are. */
const crewLaneOpen = new Set();
/** Lanes whose task is shown whole (Full task ›): a lead's brief can run long, so even an open lane keeps it to two
 *  lines and leads with its steps. */
const crewAskFull = new Set();
/** An open lane's steps, oldest first, each [time or 0, words, now]: when it started; each of its decisions in the
 *  crew's log (its first words when noted, what came of it, the last at when it last grew), when it finished; at
 *  work, the step it's on. Without a log, what its row says (crewSteps). */
function crewStepTimes(sid, t) {
  const out = [];
  if (t.started_ms) out.push([t.started_ms, t.what ? `Started: ${t.what}` : "Started", false]);
  const log = crewEntriesOf(sid, t), ended = !["working", "queued", "reviewing"].includes(t.state) || log.length;
  if (t.ended_ms && ended) out.push([t.ended_ms, t.summary ? `Finished: ${t.summary}` : "Finished", false]);
  for (const d of log) {
    const lines = [d.head || d.what, ...(d.head ? d.steps || [] : [])].filter(Boolean), first = +(String(d.id).match(/^d(\d+)-/)?.[1] || 0);
    lines.forEach((x, n) => out.push([n === lines.length - 1 ? d.at_ms || 0 : n ? 0 : first, x.replace(/^./, (c) => c.toUpperCase()), false]));
  }
  if (!log.length) for (const x of crewSteps(sid, t)) if (!["working", "done", "fixing", "asks you"].includes(x)) out.push([0, x.replace(/^./, (c) => c.toUpperCase()), false]);
  // In time order, those without a time kept after the one before them.
  let at = 0;
  const timed = out.map((s, n) => { if (s[0]) at = s[0]; return { s, at, n }; }).sort((a, b) => a.at - b.at || a.n - b.n).map((x) => x.s);
  const ask = mateAsk(sid, t);
  if (ask) timed.push([ask.created_ms || 0, `Asks you: ${verb(ask)} \`${summary(ask)}\``, true]);
  else if (t.state === "working" && t.step) timed.push([t.step_ms || 0, crewStepCode(t.step), true]);
  return timed;
}
/** What review found, each with what came of it when the log says: a should-fix "sent back", a note "left as is". */
function crewFoundRows(sid, t) {
  const said = crewEntriesOf(sid, t).flatMap((d) => [d.head, ...(d.steps || []), d.what]).join(" · ");
  const sent = /sent (the fix )?back/i.test(said) || t.fix_rounds > 0, left = /left as is/i.test(said);
  return (t.findings || []).map((l) => {
    const kind = findingKind(l), fix = ["should-fix", "must-fix", "blocker"].includes(kind), done = fix ? (sent ? "sent back" : "") : left ? "left as is" : "";
    return `<div class="co-fd">${crewKind(kind)}<span>${crewCode(crewFindingText(l))}</span><span class="co-fs">${done}</span></div>`;
  }).join("");
}
/** Under an open lane: its steps with their times; what review found and its files. */
function crewLaneBody(sid, t) {
  const steps = crewStepTimes(sid, t).map(([at, x, on]) => `<div class="co-st ${on ? "now" : ""}"><span class="co-tm">${at ? hhmm(at) : ""}</span><i></i><span class="co-tx">${crewCode(x)}</span></div>`).join("");
  const found = (t.findings || []).length ? `<div class="mt-grp">Review found · ${t.findings.length}</div>${crewFoundRows(sid, t)}` : "";
  const files = (t.files || []).length ? `<div class="mt-grp ${found ? "co-g2" : ""}">Files · ${t.files.length}${t.stat ? ` <span>${esc(t.stat)}</span>` : ""}</div>${t.files.map((f) => `<div class="co-fl"><code>${esc(f)}</code></div>`).join("")}` : "";
  return `<div class="co-body"><div class="co-col"><div class="mt-grp">Steps</div>${steps}</div><div class="co-col">${found}${files}</div></div>`;
}
/** One member's lane, read across in columns: who (its name opening the lane in place, its state, its meter), what it
 *  was asked (two lines; open, Full task › shows the rest), what it's doing now and its context, its work, its links. What its
 *  links opened (its full review) under it. A committed or set-aside one is dimmed. */
function crewLane(sid, t) {
  const [word, look] = crewStateWord(sid, t), past = ["committed", "aside"].includes(t.state), k = mateKey(sid, t), open = crewLaneOpen.has(k);
  const n = crewNow(sid, t), asked = t.asked || t.what || "";
  const sub = n.fix ? `${crewKind(n.fix[0])} ${crewCode(n.fix[1])}` : crewCode(n.sub);
  return `<div class="cp-lane ${past ? "past" : ""} ${open ? "open" : ""}" data-key="cp:${esc(k)}">`
    + `<div class="cp-who"><button class="mt-name" data-cp-lane="${esc(k)}" aria-expanded="${open}">${esc(t.name)} <span class="cp-car">${open ? "⌄" : "›"}</span></button><span class="cp-st ${look}">${word}</span>${crewMeter(n)}</div>`
    + `<div class="cp-askc"><div class="cp-ask ${crewAskFull.has(k) ? "full" : ""}">${crewCode(asked)}</div>${open && asked.length > 160 ? `<button class="mt-lk cp-askx" data-cp-ask="${esc(k)}" aria-expanded="${crewAskFull.has(k)}">${crewAskFull.has(k) ? "Less ⌃" : "Full task ›"}</button>` : ""}</div>`
    + `<div class="cp-now"><div class="cp-big ${n.ask ? "q" : ""}">${crewCode(n.now)}</div>${sub ? `<div class="cp-small">${sub}</div>` : ""}</div>`
    + `${crewWork(t)}<div class="cp-links">${crewLaneLinks(sid, t)}</div>${open ? crewLaneBody(sid, t) : ""}${mateUnderHtml(sid, t)}</div>`;
}
/** The members still in the crew as their lanes stand: those that ask or need you, then those at work (working,
 *  reviewing), then the rest (queued, reviewed, stopped, done), each group in the crew's own order. Only a change of
 *  state moves a lane; opening one doesn't. */
function crewLaneOrder(sid) {
  const rank = (t) => (mateNeeds(sid, t) ? 0 : ["working", "reviewing"].includes(t.state) ? 1 : 2);
  return mateList(sid).map((t, n) => ({ t, n, r: rank(t) })).sort((a, b) => a.r - b.r || a.n - b.n).map((x) => x.t);
}
/** The crew's line over its lanes: "The crew · 5", then "2 committed · 1 working · 1 needs you · 1h 20m since it
 *  started", and Handle it / Ask me (team_send.js crewMode, its menu opening downward here). */
function crewStatusLine(sid, needs) {
  const team = sessionOf(sid)?.team || [], count = (st) => team.filter((t) => t.state === st).length;
  const start = Math.min(...team.map((t) => t.started_ms || Infinity));
  const parts = [count("committed") ? `${count("committed")} committed` : "", count("working") ? `${count("working")} working` : "",
    needs ? `${needs} need${needs === 1 ? "s" : ""} you` : "", Number.isFinite(start) ? `${tookText(Math.max(0, now() - start))} since it started` : ""];
  return `<div class="cp-status"><span class="mt-grp">The crew · ${team.length}</span><span class="cp-count">${parts.filter(Boolean).map(esc).join(" · ")}</span><span class="grow"></span>${crewMode(sid)}</div>`;
}
/** The finished members (committed, set aside) at the end of the lanes: one line, "N done ›", folded until you click
 *  it (the drawer's fold, team.js mateDoneOpen, so it's open in both or neither); unfolded, their lanes, dimmed. */
function crewDoneLanes(sid) {
  const past = (sessionOf(sid)?.team || []).filter((t) => ["committed", "aside"].includes(t.state));
  if (!past.length) return "";
  const open = mateDoneOpen.has(sid);
  const line = `<button class="mt-lk mt-done-line" data-mt-done="${esc(sid)}" aria-expanded="${open}">${past.length} done ${open ? "⌄" : "›"}</button>`;
  return `<div class="cp-done ${open ? "open" : ""}" data-mt-for="done:${esc(sid)}">${line}${open ? past.map((t) => crewLane(sid, t)).join("") : ""}</div>`;
}
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

/** The lead's replies shown whole in the stream (Show all ›), by `<lead>:<at_ms>`; a long one is cut at about six lines. */
const crewSaidOpen = new Set();
/** A reply's first whole blocks, up to about `max` lines' worth (a line of prose about 100 characters; a table row, a
 *  code line, a list item's line one each), never splitting a table, a code block or a list item: a first block that
 *  alone is too tall shows its first rows, lines or items whole (a table keeps its head, a code block its fences).
 *  `cut`: something was left out. */
function crewCutReply(text, max = 6) {
  const FENCE = /^\s*(```|~~~)/, lines = String(text).split("\n"), blocks = [];
  let cur = [], fence = false;
  for (const l of lines) {
    if (FENCE.test(l)) fence = !fence;
    if (!fence && !l.trim()) { if (cur.length) blocks.push(cur); cur = []; } else cur.push(l);
  }
  if (cur.length) blocks.push(cur);
  const weight = (l) => (FENCE.test(l) ? 0 : /^\s*\|/.test(l) ? (/^\s*\|?[\s:|-]+\|?\s*$/.test(l) ? 0 : 1) : Math.max(1, Math.ceil(l.length / 100)));
  const out = [];
  let used = 0;
  for (const b of blocks) {
    const w = b.reduce((n, l) => n + weight(l), 0);
    if (used + w <= max) { out.push(b.join("\n")); used += w; continue; }
    if (out.length) return { text: out.join("\n\n"), cut: true };
    // The first block alone is too tall: its head, whole rows or items, up to what's left.
    if (FENCE.test(b[0])) {
      const body = b.slice(1, FENCE.test(b.at(-1)) ? -1 : undefined);
      return { text: [b[0], ...body.slice(0, Math.max(1, max - 1)), b[0].trim().slice(0, 3)].join("\n"), cut: true };
    }
    if (/^\s*\|/.test(b[0])) return { text: b.slice(0, (/^\s*\|?[\s:|-]+\|?\s*$/.test(b[1] || "") ? 2 : 1) + Math.max(1, max - 1)).join("\n"), cut: true };
    // By parts: a code block fence to fence and a table's rows each one part, a list by its items (an item with the
    // lines under it), else prose by its lines; one at least.
    const ITEM = /^\s*([-*•+]|\d+[.)])\s/, ROW = /^\s*\|/, list = ITEM.test(b[0]), parts = [];
    let inCode = false;
    for (const l of b) {
      const prev = parts.at(-1), fence = FENCE.test(l);
      if (inCode) { prev.push(l); if (fence) inCode = false; }
      else if (fence) { parts.push([l]); inCode = true; parts.at(-1).code = true; }
      else if (ROW.test(l) && prev?.table) prev.push(l);
      else if (ROW.test(l)) { parts.push([l]); parts.at(-1).table = true; }
      else if (list && prev && !prev.code && !prev.table && !ITEM.test(l)) prev.push(l);
      else parts.push([l]);
    }
    const keep = [];
    let n = 0;
    for (const p of parts) { const pw = p.reduce((k, l) => k + weight(l), 0); if (keep.length && n + pw > max) break; keep.push(...p); n += pw; }
    return { text: keep.join("\n"), cut: true };
  }
  return { text: out.join("\n\n"), cut: false };
}
/** One reply of the lead's in the stream: a long one cut at its first whole blocks (crewCutReply), a soft fade over its
 *  end and Show all › under it opening it in place. */
function crewSaid(sid, e, who) {
  const text = addOn.plain(stripStatus(e.text)), k = `${sid}:${e.at_ms}`, open = crewSaidOpen.has(k), part = crewCutReply(text);
  const more = part.cut ? `<button class="mt-lk cp-more" data-cp-more="${esc(k)}" aria-expanded="${open}">${open ? "Less ‹" : "Show all ›"}</button>` : "";
  return `<div class="cv-agent" data-key="a:${e.at_ms}"><div class="cv-meta">${who} · ${ago(e.at_ms)} ago</div><div class="msg cv-text ${part.cut && !open ? "cut" : ""}">${md(open ? text : part.text)}</div>${more}</div>`;
}
/** The lead's stream, oldest first: its replies and your messages from its chat (not Cue's own, nor the reviews it
 *  wrote for Cue, which the lanes have, nor other sessions'), each decision of the crew's log and the add-on's (`added`,
 *  crewPageDecided), in time order; then what you sent that hasn't gone in yet. */
function crewStream(sid, added) {
  const s = sessionOf(sid), who = esc(agentName(s.harness));
  // What the chat shows (chat/thread.js conversation): its earlier messages once loaded, and the reply it stopped on
  // when it waits on you (from its waiting card), else the lead's own ask.
  const own = state.items.filter((i) => i.session_id === sid && !mateOfAsk(i));
  const it = own.find((i) => i.kind === "waiting") || own[0] || null;
  const thread = conversation(it, s);
  const { reviews } = reviewFolds(thread, s.team || []);
  const shown = (e, n) => e.text?.trim() && !reviews.has(n) && (e.role === "agent" || (e.role === "you" && !cueSaid(e.text)));
  const rows = thread.map((e, n) => !shown(e, n) ? ""
    : e.role === "you" ? `<div class="cv-you" data-key="y:${e.at_ms}">${yourBubble(e, `${sid}:${e.at_ms}`)}<div class="cv-meta">You · ${ago(e.at_ms)} ago</div></div>`
    : crewSaid(sid, e, who));
  const decided = (s.crew_log || []).map((d) => ({ at: d.at_ms || 0, html: crewLogRow(sid, d, true) }));
  const theirs = added.items ? added.items.map((x) => ({ at: x.at_ms || 0, html: x.html })) : added.html ? [{ at: -Infinity, html: added.html }] : [];
  // Then, as under the chat, what waits to go: kept in Cue, sent at a time, queued in the lead.
  const later = [s.held ? heldRow(s) : "", s.send_at ? sendAtRow(s) : "", s.queued ? queuedRow(s) : ""].join("");
  // As the chat draws them (chat/render.js): each turn's steps by time among the messages and decisions, and under it
  // all the chat's own status line (working on…, waiting on you, its helpers), so the page never says less than the chat.
  return withSteps(sid, s, thread, rows, decided.concat(theirs)).join("") + later + statusLine(it, s);
}
/** The stream's box: the lead's own ask's card when it asks you something (its teammates' asks are in Needs you), else
 *  the chat's box (the same draft, s:<sid>). */
function crewPageFoot(sid) {
  const s = sessionOf(sid), ask = state.items.find((i) => i.session_id === sid && i.kind !== "waiting" && !mateOfAsk(i));
  if (ask) return askFoot(ask, sid, s.harness);
  return `<div class="foot">${box_(`s:${sid}`, "Message the lead… or @name a teammate", s.state === "working" ? "Queue" : "Send", `data-act="send-to" data-sid="${esc(sid)}"`)}</div>`;
}

/** The page: its head (‹ Back to the board, the lead's name and project), the add-on's band, the goal line and the
 *  add-on's rows, the crew's line and lanes, then the lead's stream with its box beside Needs you (outlined red). */
function crewPageView(sid) {
  const s = sessionOf(sid), handles = crewHandles(sid), groups = crewGroups(sid), needs = crewNeedRows(sid, groups, true);
  // The add-on's part, each empty without it: its head line, its band, rows under the goal, rows in Needs you, decisions.
  const head = addOn.crewPageHead?.(sid) || "", band = addOn.crewPageBand?.(sid) || "", rowsAdded = addOn.crewPageRows?.(sid) || "";
  const needsAdded = addOn.crewPageNeeds?.(sid) || { html: "", count: 0 }, decidedAdded = addOn.crewPageDecided?.(sid) || { html: "", count: 0 };
  const needCount = needs.list.length + (needsAdded.count || 0), needRows = needs.rows + (needsAdded.count ? needsAdded.html : "");
  const decided = (s.crew_log || []).length + (decidedAdded.count || 0);
  // Still in the crew: what needs you first, then those at work, then the rest, each in the crew's order; committed and
  // set aside folded at the end.
  const lanes = crewLaneOrder(sid).map((t) => crewLane(sid, t)).join("") + crewDoneLanes(sid);
  const needBox = `<div class="cp-needs ${needCount ? "red" : ""}">${needCount ? `<div class="mt-grp red">Needs you · ${needCount}</div>${needRows}` : `<div class="mt-grp">Needs you · nothing now</div><div class="crew-none">${CREW_NONE}</div>`}</div>`;
  const items = crewStream(sid, decidedAdded);
  const stream = `<div class="cp-stream"><div class="cp-sh"><span class="mt-grp">The lead <span>what it said and decided, newest last · ${decided} decided</span></span><span class="grow"></span><button class="mt-lk" data-cp-chat>Open chat ›</button></div>
    <div class="cp-items" data-scroll-key="cps:${esc(sid)}"><div class="cp-items-in">${items || `<div class="crew-none">Nothing yet: what the lead says, and what it settles by itself, shows here.</div>`}</div></div>${crewPageFoot(sid)}</div>`;
  return `<div class="crew-page" data-crew-page="${esc(sid)}">
    <div class="cp-head"><button class="cp-back" data-cp-back>‹ Back to ${esc(crewPageBack)}</button>${head || `<span class="cp-name">${esc(nameOf(sid, s.project))}</span><span class="grow"></span>${s.project && s.project !== nameOf(sid, s.project) ? `<span class="cp-proj">${esc(s.project)}</span>` : ""}`}</div>
    <div class="cp-body" data-scroll-key="cp:${esc(sid)}"><div class="cp-upper" data-scroll-key="cpu:${esc(sid)}">${band}
      <div class="cp-top">${crewGoalLine(sid, handles)}${rowsAdded}</div>
      <div class="cp-crew">${crewStatusLine(sid, needCount)}<div class="cp-scroll" data-scroll-key="cpl:${esc(sid)}"><div class="cp-lanes"><div class="cp-hd"><span>Who</span><span>What it was asked</span><span>Now</span><span>Its work</span><span></span></div>${lanes}</div></div></div></div>
      <div class="cp-split">${stream}${needBox}</div>
    </div>${mateMenuOf(sid)}</div>`;
}

/** Talk on a lane: the lead's box (the stream's) with "@name " typed (after what's there), the cursor at its end. */
function crewTalk(name) {
  const key = `s:${crewPageSid}`, d = draft(key), at = `@${name} `;
  if (!d.text.includes(at)) d.text = d.text.trim() ? `${d.text.replace(/\s*$/, " ")}${at}` : at;
  saveDrafts();
  renderMain();
  const el = document.querySelector(`.crew-page textarea[data-text="${CSS.escape(key)}"]`);
  if (el) { el.focus(); el.setSelectionRange(el.value.length, el.value.length); }
}

/** Answer on a lane: its row in Needs you scrolled into view, its first button focused. */
function crewAnswer(k) {
  const row = document.querySelector(`.crew-page .cp-needs [data-key="mt:${CSS.escape(k)}"]`);
  if (!row) return;
  row.scrollIntoView({ block: "nearest" });
  row.querySelector("button:not(.mt-name), textarea, input")?.focus();
}

/** Esc on the page: the mode menu or a member's menu closes first, then the page; not while you type in a box that
 *  has text (Esc is the box's there) or something is open over the window. True when it took the key. */
function crewPageEsc(e) {
  if (e.key !== "Escape" || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.isComposing) return false;
  if (!crewPageSid || sheet || lightbox || !document.querySelector(".crew-page")) return false;
  const f = document.activeElement;
  if (f?.matches?.("textarea, input") && f.value) return false;
  if (typeof crewMenuFor !== "undefined" && crewMenuFor) { crewMenuFor = null; renderMain(); }
  else if (mateMenu) { mateMenu = null; renderMain(); }
  else crewPageClose();
  return true;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (crewPageEsc(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", (e) => {
    const el = e.target.closest?.("[data-cp-open], [data-cp-back], [data-cp-chat], [data-cp-talk], [data-cp-goal], [data-cp-more], [data-cp-lane], [data-cp-ask], [data-cp-answer]");
    if (!el) return;
    e.stopPropagation();
    if (el.dataset.cpOpen) return crewPageOpen(el.dataset.cpOpen);
    if (el.matches("[data-cp-back], [data-cp-chat]")) return crewPageClose(el.matches("[data-cp-chat]"));
    if (el.dataset.cpMore) { const k = el.dataset.cpMore; if (!crewSaidOpen.delete(k)) crewSaidOpen.add(k); return renderMain(); }
    if (el.dataset.cpAsk) { const k = el.dataset.cpAsk; if (!crewAskFull.delete(k)) crewAskFull.add(k); return renderMain(); }
    if (el.dataset.cpLane) { const k = el.dataset.cpLane; mateMenu = null; if (!crewLaneOpen.delete(k)) crewLaneOpen.add(k); return renderMain(); }
    if (el.dataset.cpAnswer) return crewAnswer(el.dataset.cpAnswer);
    if (el.dataset.cpGoal) { const sid = el.dataset.cpGoal; if (!crewGoalOpen.delete(sid)) crewGoalOpen.add(sid); return renderMain(); }
    crewTalk(el.dataset.cpTalk);
  }, true);
}
