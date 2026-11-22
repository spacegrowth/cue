// Cue UI Inbox (ui/inbox/list.js says what it is): the brief's side, and the brief put together.
// ---------- the brief's side: you asked, it did, what changed, its context ----------

/** Its latest turn in its steps (the chat's steps feed, read again when it's a few seconds old); null until read. */
function inboxTurn(sid) {
  const f = stepFeeds.get(sid);
  if (!f || Date.now() - f.at > 4000) loadSteps(sid);
  return f?.turns?.at(-1) || null;
}
/** What you asked it last: your latest message in its thread, else its latest turn's prompt, else its prompt.
 *  { text, at } (at 0: not known); null when there's nothing. */
function inboxYouAsked(r) {
  const you = (r.s?.thread || []).filter((e) => e.role === "you" && !askLnAnswer(e)).at(-1), turn = inboxTurn(r.sid);
  if (you?.text) return { text: you.text, at: you.at_ms };
  if (turn?.prompt) return { text: turn.prompt, at: turn.at_ms };
  return r.s?.prompt ? { text: r.s.prompt, at: 0 } : null;
}
/** A run of steps of one kind and verb as one line ("Read 6 files", "Edited 3 files" with their +/− summed). */
function inboxStepRun(run) {
  const x = run.at(-1), n = run.length, bad = run.some((s) => s.bad), live = !x.done;
  if (n === 1) return { what: `${esc(x.verb)} ${x.mono ? `<code>${esc(x.subject)}</code>` : esc(x.subject)}`, result: x.result, bad, live };
  const sum = run.map((s) => /^\+(\d+) −(\d+)$/.exec(s.result)).filter(Boolean);
  const many = { read: "files", edit: "files", run: "commands", search: "times", web: "pages" }[x.kind] || "times";
  const result = bad ? `${run.filter((s) => s.bad).length} failed` : sum.length ? `+${sum.reduce((a, m) => a + +m[1], 0)} −${sum.reduce((a, m) => a + +m[2], 0)}` : "";
  return { what: `${esc(x.verb)} ${n} ${many}`, result, bad, live };
}
/** Its steps this turn as lines, runs of the same kind and verb folded into one. */
function inboxStepLines(steps) {
  const runs = [];
  for (const x of steps) {
    const last = runs.at(-1)?.at(-1);
    if (last && last.kind === x.kind && last.verb === x.verb && last.done && x.done && x.kind !== "agent") runs.at(-1).push(x); else runs.push([x]);
  }
  return runs.map(inboxStepRun);
}
/** "It did": its steps this turn, folded, the latest few. Nothing when it has none (or they aren't read yet). */
function inboxDid(r) {
  const steps = (inboxTurn(r.sid)?.items || []).filter((x) => x.t === "step");
  if (!steps.length) return "";
  const lines = inboxStepLines(steps), shown = lines.slice(-INBOX_STEPS_MAX), more = lines.length - shown.length;
  const earlier = more ? `<div class="ib-step-more">${more} earlier</div>` : "";
  return `<div class="ib-blk"><div class="ib-k">It did · ${steps.length} step${steps.length === 1 ? "" : "s"}</div><div class="ib-steps">${earlier}${shown.map(inboxStepHtml).join("")}</div></div>`;
}
/** One line of its steps (a folded run is one): its dot (lit while it's on it), what, and how it went. */
const inboxStepHtml = (l) => `<div class="ib-step ${l.bad ? "bad" : ""} ${l.live ? "now" : ""}"><i></i><span class="ib-sv">${l.what}</span><span class="ib-sr">${l.live ? "" : esc(l.result || "")}</span></div>`;
/** The brief's side column, in the order that says why it asks: what's running now, You asked, It did, what changed,
 *  its context, and its full chat and diff as links. */
function inboxAside(r) {
  const you = inboxYouAsked(r), sid = esc(r.sid);
  // Cue's own message (a send-back, a reopen…, #478): what Cue asked, in one plain line, not its instructions.
  const cue = you && cueMsgPlainHtml(you.text, nameOf(r.sid, r.s?.project));
  const asked = you ? `<div class="ib-blk"><div class="ib-k">${cue ? "Cue asked" : "You asked"}${you.at ? ` · ${ago(you.at)} ago` : ""}</div><div class="ib-you${cue ? " mg-plain" : ""}">${cue || esc(plain(you.text))}</div></div>` : "";
  const ctx = inboxCtx(r.s) ? `<div class="ib-blk"><div class="ib-k">Context</div><div class="ib-ctx">${inboxMeter(r.s, "wide")}<span>${inboxCtx(r.s)}%</span></div></div>` : "";
  const links = `<div class="ib-links"><button class="ib-link" data-inbox-diff="${sid}">Diff</button></div>`;
  return `<aside class="ib-page-body ib-aside" data-scroll-key="inbox-aside">${inboxLive(r).html}${inboxOpenBlock()}${asked}${inboxDid(r)}${inboxChangedBlock(r)}${ctx}${links}</aside>`;
}

// ---------- the brief, put together ----------

/** What it's at, in a few words, for the brief's head ("asks you · 12s"). */
function inboxStatus(r) {
  const t = ago(r.at), n = r.kind === "ask" && r.it.kind === "question" ? questions(r.it).length : 0;
  if (r.kind === "ask") return `${n > 1 ? `${n} questions` : n ? "asks you" : verb(r.it)} · ${t}`;
  if (r.kind === "turn") return `${r.it.interrupted ? "interrupted" : "your turn"} · ${t}`;
  if (r.kind === "chore") return `context ${inboxCtx(r.s)}% full`;
  if (r.kind === "crew") return "its crew needs you";
  if (r.kind === "review") return "crew review available";
  if (r.kind === "run") return `working · ${ago(r.s?.since_ms || r.at)}`;
  return `finished · ${t}`;
}
/** "2 of 6 ›" to the next that needs you ("next ask ›" from one that doesn't), when there's another. */
function inboxStep(sid) {
  const all = inboxNeedOrder(), at = all.indexOf(sid);
  if (!all.some((x) => x !== sid)) return "";
  return `<button class="ib-next" data-inbox-next>${at >= 0 ? `${at + 1} of ${all.length} ›` : "next ask ›"}</button>`;
}
/** Close, as on the Board's chat (sessions.js closeBtn: Close, then Close? to be sure): not while it works or asks
 *  you something, where you'd lose the turn or the question. */
function inboxCloseBtn(r) {
  const it = r.it, st = it && it.kind !== "waiting" ? "asks" : r.s?.state || "idle";
  return r.s && svClosable({ sid: r.sid, st }) ? closeBtn(r.sid) : "";
}
/** The open session beside the list: its head (its dot, name and what it's at; Pin, Later, Snooze; the next ask), then
 *  the brief in two columns (stacked when narrow). No hover text: a tooltip the reused parts carry is left out. */
function inboxOpened(sid) {
  const p = !sessionOf(sid) && inboxParkedOf(sid);
  if (p) return inboxParkedPage(p);
  const r = inboxRowFor(sid), s = r.s;
  inboxToEnd = inboxAtEnd(sid, r.kind);
  const name = `${deskSessMark(sid)}<span class="ib-hname">${esc(nameOf(sid, r.project))}</span>`;
  const head = `<div class="ib-head"><button class="ib-back" data-inbox-back aria-label="Back to the list">‹</button><span class="ib-dot">${inboxDot(r)}</span>${name}<span class="ib-status">${esc(inboxStatus(r))}</span><span class="grow"></span>`
    + `<span class="ib-hacts">${inboxAddDeskBtn(r)}<button class="ib-act" data-inbox-full="${esc(sid)}">Open chat</button>${inboxActs(sid, s, "head")}${inboxCloseBtn(r)}</span>${inboxStep(sid)}</div>`;
  const body = `${inboxForBar(r)}<div class="ib-brief" data-scroll-key="inbox-brief">${inboxMain(r)}${inboxAside(r)}</div>`;
  return `<div class="inbox-page" data-inbox-page="${esc(sid)}" data-kind="${r.kind}" style="${projStyle(r.project)}">${head}${body}</div>`.replace(/\stitle="[^"]*"/g, "");
}
/** The Board's parked session `sid` (not running again yet); null when it isn't one. */
const inboxParkedOf = (sid) => (state.parked || []).find((p) => p.session_id === sid) || null;
/** A parked session beside the list, as a mail thread: its earlier messages, what it said last (whole), then a box
 *  whose Send resumes it first. Resume, Unpark and Open chat in its head. */
function inboxParkedPage(p) {
  const sid = p.session_id, w = waking.has(sid), name = p.name || baseName(p.cwd) || p.project || "session";
  const r = { sid, s: { thread: p.thread || [] }, it: null, project: p.project, kind: "done" }, thread = inboxThread(r, "");
  const said = thread.said, words = said ? said.text : "";
  const whole = inboxWhole.has(sid), short = inboxSaid(words, []), all = inboxSaid(words, [], true), shown = whole ? all : short;
  const more = short !== all ? `<button class="ib-link" data-inbox-whole="${esc(sid)}">${whole ? "Show less" : "Show the whole reply"}</button>` : "";
  const saidHtml = shown ? `<div class="ib-said">${inboxHead("It said", said.at_ms || 0)}<div class="ib-md msg">${md(shown, { pathsOf: sid })}</div>${thumbs(said.images)}${localImages(shown)}${more}</div>` : "";
  const empty = !thread.msgs.length && !saidHtml ? `<div class="ib-dim">Cue kept no messages from it.</div>` : "";
  const foot = inboxBox(`s:${sid}`, "Write to it… sending resumes it first", w ? "Resuming…" : "Resume & send", `data-wake-send="${esc(sid)}" ${w ? "disabled" : ""}`, `<button class="btn" data-inbox-full="${esc(sid)}">Open chat</button>`)
    .replace(/<button class="icon-btn" data-attach=[^>]*>[\s\S]*?<\/button>/, "");   // resuming sends words only
  const status = w ? "resuming…" : p.parked_ms ? `parked · ${ago(p.parked_ms)}` : "parked";
  const head = `<div class="ib-head"><button class="ib-back" data-inbox-back aria-label="Back to the list">‹</button><span class="ib-dot">${P_SIGN}</span><span class="ib-hname">${esc(name)}</span><span class="ib-status">${esc(status)}</span><span class="grow"></span>`
    + `<span class="ib-hacts"><button class="ib-act" data-unpark="${esc(sid)}" ${w ? "disabled" : ""}>Unpark</button><button class="ib-act" data-wake="${esc(sid)}" ${w ? "disabled" : ""}>↺ Resume</button></span></div>`;
  const body = `<div class="ib-brief one" data-scroll-key="inbox-brief"><div class="ib-main"><div class="ib-page-body ib-thread" data-scroll-key="inbox-main">${inboxEarlier(r, thread.msgs)}${saidHtml}${empty}</div><div class="ib-foot">${foot}</div></div></div>`;
  return `<div class="inbox-page parked" data-inbox-page="${esc(sid)}" data-kind="parked" style="${projStyle(p.project)}">${head}${body}</div>`;
}
/** A session on the calm page: its name, what you asked and what it's on (a working one's bar along its foot), or
 *  what it said and changed. Opens it. */
function inboxCalmCard(r) {
  const [subj, line] = r.kind === "run" ? inboxRunLines(r, false, true) : inboxDoneLines(r).map(esc);
  return `<div class="ib-calm-card ${r.kind}" role="button" tabindex="0" data-inbox-open="${esc(r.sid)}" style="${projStyle(r.project)}"><div class="ib-calm-t"><span class="ib-dot">${inboxDot(r)}</span>${nameSpan(r.sid, r.project, "ib-name")}<span class="grow"></span><span class="ib-time">${ago(r.at)}</span></div>`
    + `<div class="ib-calm-s">${subj}</div>${line ? `<div class="ib-calm-l">${line}</div>` : ""}${r.kind === "run" && r.s ? inboxBar(r.s) : ""}</div>`;
}
/** Beside the list with nothing open: what needs you, or a preview of running sessions. */
function inboxIdle() {
  const n = inboxCount();
  if (n) return `<div class="ib-idle"><b>${n} need${n === 1 ? "s" : ""} you</b><span>Pick one on the left.</span></div>`;
  const rows = inboxRows().filter((r) => inboxTabOf(r.s) === "inbox" && !inboxLater(r.s)).sort((a, b) => b.at - a.at);
  const run = rows.filter((r) => r.kind === "run");
  const group = (title, xs) => (xs.length ? `<div class="ib-k">${title} · ${xs.length}</div><div class="ib-calm-grid">${xs.map(inboxCalmCard).join("")}</div>` : "");
  const words = rows.some((r) => r.kind === "done") ? "Open Finished replies to catch up." : run.length ? "Running sessions come up here when they ask or finish." : "Nothing is working either.";
  return `<div class="ib-idle ib-calm"><div class="ib-calm-head"><b>Nothing needs you</b><span>${words}</span></div><div class="ib-calm-body ib-page-body" data-scroll-key="inbox-calm">${group("Running", run)}</div></div>`;
}
