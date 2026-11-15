// ---------- Inbox: where you see what a session asks, and answer it (the header's Inbox, after Board) ----------
// The list: a row per session, two lines each (its name and what it asks or is on; then why, or what it found), under
// Pinned, Needs you, Running and Done. A finished turn that ends on a question needs you; one that doesn't is Done. A
// session whose context is half full or more comes up under Needs you to compact or hand off. Answer one and it sinks
// to Running, a bar moving along its foot (slower when it's quiet, amber and still when it's stuck); it rises again
// when it asks again or finishes. A row keeps its place while its state stays the same. Hovering a row shows Pin,
// Later and Snooze (Later and Snoozed are tabs of their own). Opening a row shows it in the list's place: what it's
// doing and has done so far, then its question(s) as cards with their choices and a box to answer in. Its full chat
// is a link at the foot. Esc or ‹ Inbox goes back. With a project picked in the header, only its sessions.

/** The session open in the Inbox's place; null while the list shows. */
let inboxSid = null;
/** The open session needed you while it was open: once it's at work again, the Inbox goes back to the list. */
let inboxAsked = false;
/** The session last opened from the list: its row is lit when the list comes back, so you see where you were. */
let inboxLast = null;
/** The list's tab: "inbox", "later" or "snoozed". */
let inboxTab = "inbox";
/** Where a Snooze menu is open: "row:<sid>", "head" or "card" (the open session's); "" when none is. */
let inboxSnoozeAt = "";
/** sid → { sect, n, at, from }: the section a row was last drawn in, its place there (higher on top), since when, and
 *  the section it came from. A row keeps its place while it stays in its section. */
const inboxSeats = new Map();
let inboxSeq = 0;
/** How long a finished session with nothing waiting stays under Done, and how many of those show at most. */
const INBOX_DONE_MS = 2 * 3600000, INBOX_DONE_MAX = 6;
/** Context this full (percent): its row shows a meter, and a session at rest comes up to compact or hand off. */
const INBOX_FULL = 50;
/** A working session with no new step for this long: its bar slows. */
const INBOX_QUIET_MS = 2 * 60000;
/** A row that came into Running this recently is still sinking: drawn faded. */
const INBOX_SINK_MS = 8000;

// ---------- what each session is, as a row ----------

/** Its marks, as Cue keeps them with the session. */
const inboxPinned = (s) => !!s?.pinned_ms;
/** Snoozed until its time, unless it has asked something since it was snoozed (then it's back, as a new reply wakes
 *  a snoozed mail). */
const inboxSnoozed = (s) => (s?.snooze_ms || 0) > now() && !state.items.some((i) => i.session_id === s.session_id && i.created_ms > (s.snoozed_at_ms || 0));
const inboxCtx = (s) => s?.context_pct || 0;
const inboxFull = (s) => inboxCtx(s) >= INBOX_FULL;
/** The tab a session is listed under. */
const inboxTabOf = (s) => (inboxSnoozed(s) ? "snoozed" : s?.later_ms ? "later" : "inbox");

/** The question a reply ends on (the last sentence with a "?" in its last line), plain; "" when it asks nothing there. */
function inboxEndQuestion(text) {
  const last = plain(String(text || "").split("\n").map((l) => l.trim()).filter(Boolean).at(-1) || "");
  return last.split(/(?<=[.!?])\s+/).filter((x) => /\?["'”)]*$/.test(x)).at(-1) || "";
}
/** Each session's item to show: what it asks before a finished turn, the newest of each. */
function inboxItems() {
  const by = new Map();
  for (const it of [...state.items].sort((a, b) => b.created_ms - a.created_ms)) {
    const had = by.get(it.session_id);
    if (inProj(it.project) && (!had || (had.kind === "waiting" && it.kind !== "waiting"))) by.set(it.session_id, it);
  }
  return by;
}
/** One session's row: { sid, s, it, project, at, kind } where kind is "ask", "turn" (a finished turn that asks you
 *  back, or was interrupted), "chore" (its context is getting full), "run" or "done". Null when there's nothing to
 *  say of it, unless `keep` (pinned, put off or snoozed: always listed). */
function inboxRowOf(sid, s, it, keep = false) {
  const row = (kind, at) => ({ sid, s, it, project: s?.project || it?.project, at, kind });
  if (it && it.kind !== "waiting") return row("ask", it.created_ms);
  if (it && (it.interrupted || inboxEndQuestion(finishedText(it)))) return row("turn", it.created_ms);
  if (s && (s.state === "working" || s.state === "agent" || s.compacting_ms)) return row("run", s.since_ms || 0);
  if (s && inboxFull(s) && !cutOff(s)) return row("chore", s.since_ms || 0);
  if (it) return row("done", it.created_ms);
  const recent = s && !hiddenIdle.has(idleKey(s)) && now() - (s.since_ms || 0) < INBOX_DONE_MS;
  return recent || keep ? row("done", s?.since_ms || 0) : null;
}
/** Every row the Inbox has, any tab. */
function inboxRows() {
  const items = inboxItems(), rows = [];
  for (const [sid, it] of items) rows.push(inboxRowOf(sid, sessionOf(sid), it, true));
  for (const s of state.sessions) {
    if (items.has(s.session_id) || !inProj(s.project) || cutOff(s)) continue;
    const r = inboxRowOf(s.session_id, s, null, inboxPinned(s) || inboxTabOf(s) !== "inbox");
    if (r) rows.push(r);
  }
  return rows;
}
/** The open session's row (whatever its tab), or null once it's gone. */
const inboxRowFor = (sid) => inboxRowOf(sid, sessionOf(sid), inboxItems().get(sid) || null, true);
/** The section a row goes under. */
const inboxSectOf = (r) => (r.kind === "run" ? "run" : r.kind === "done" ? "done" : "need");
/** The rows needing you under the Inbox tab, a session once each. */
const inboxNeedRows = () => inboxRows().filter((r) => inboxTabOf(r.s) === "inbox" && inboxSectOf(r) === "need");
/** How many sessions need you, for the header's "Inbox 3". */
const inboxCount = () => inboxNeedRows().length;

/** `rows` (newest first) in the order they keep: one stays where it was while it stays in `sect`; one that came in
 *  goes on top. */
function inboxSeated(sect, rows) {
  const fresh = rows.filter((r) => inboxSeats.get(r.sid)?.sect !== sect);
  for (const r of fresh.reverse()) inboxSeats.set(r.sid, { sect, n: ++inboxSeq, at: now(), from: inboxSeats.get(r.sid)?.sect || "" });
  return rows.sort((a, b) => inboxSeats.get(b.sid).n - inboxSeats.get(a.sid).n);
}
/** The open tab's rows by section, in order: Pinned (Inbox tab only), Needs you, Running, Done. */
function inboxSections() {
  const rows = inboxRows().filter((r) => inboxTabOf(r.s) === inboxTab).sort((a, b) => b.at - a.at);
  const pinned = inboxTab === "inbox" ? rows.filter((r) => inboxPinned(r.s)) : [];
  const rest = rows.filter((r) => !pinned.includes(r)), of = (k) => rest.filter((r) => inboxSectOf(r) === k);
  let quiet = 0;
  const done = of("done").filter((r) => r.it || inboxPinned(r.s) || inboxTab !== "inbox" || ++quiet <= INBOX_DONE_MAX);
  return [["pin", "Pinned", inboxSeated("pin", pinned)], ["need", "Needs you", inboxSeated("need", of("need"))], ["run", "Running", inboxSeated("run", of("run"))], ["done", "Done", inboxSeated("done", done)]];
}

// ---------- a row's words ----------

/** What it did lately, in a line: its About's newest turn line (Settings → Summaries), else where it stands there,
 *  else the first line of its last reply. */
function inboxSoFar(r) {
  const t = aboutTracked(r.sid), turn = t?.turns?.at(-1)?.line || t?.now;
  return plain(turn || firstLine(r.it?.kind === "waiting" ? finishedText(r.it) : lastSaid(r.s || {})));
}
/** What its last turn changed, in words ("3 files changed, +40 −2"); "" when nothing. */
function inboxChanged(s) {
  const c = s?.changes, n = c?.files?.length || 0;
  return n ? `${n} file${n === 1 ? "" : "s"} changed, +${c.add || 0} −${c.del || 0}` : "";
}
/** An ask's subject: the question, or what it wants ("Wants to run: npm test"). */
function inboxAskSubject(it) {
  if (it.kind === "question") return questions(it)[0]?.question || "A question";
  const v = verb(it).replace(/^./, (c) => c.toUpperCase()), what = plain(summary(it));
  // summary() names an edit by its tool ("edit …/Cart.tsx"): the verb says that already.
  return `${v}: ${it.tool_name && what.toLowerCase().startsWith(`${it.tool_name.toLowerCase()} `) ? what.slice(it.tool_name.length + 1) : what}`;
}
/** A question ask's second line when it said nothing before asking: the questions after the first, else the choices. */
function inboxQuestionLine(it) {
  const [q, ...rest] = questions(it);
  return rest.length ? `Then: ${rest.map((x) => x.header || x.question).join(" · ")}` : (q?.options || []).map((o) => o.label).join(" · ");
}
/** Why an ask asks: the last thing it said before asking, in a line. */
function inboxWhy(it) {
  const c = (it.context || []).filter((x) => x.role !== "user").at(-1);
  return plain(firstLine(c?.text || ""));
}
/** A working row's lines: what you asked it, and what it's on now (or that it's quiet, or stuck). */
function inboxRunLines(r) {
  const s = r.s, seat = inboxSeats.get(r.sid), said = plain(s.prompt);
  const subj = said ? (seat?.from === "need" ? `You: “${said}”` : said) : plain(aboutTracked(r.sid)?.about) || "Working";
  const stuck = s.stuck_ms && !s.compacting_ms ? `No new output for ${ago(s.stuck_ms)}` : "";
  const line = s.compacting_ms ? "Compacting…" : preStart(s) ? `Not started: ${preStart(s)}` : stuck || (s.state === "agent" ? idleNote(s) : s.doing || "Thinking");
  return [subj, line];
}
/** A finished row's lines: how it ended and what it said, then what it did. */
function inboxDoneLines(r) {
  const s = r.s, said = plain(firstLine(r.it ? finishedText(r.it) : lastSaid(s || {})));
  const ended = !r.it && s?.state === "stopped" ? "Stopped" : !r.it && s?.state === "limited" ? "Out of usage" : "";
  const subj = ended ? `${ended}${said ? `: ${said}` : ""}` : said || "Finished";
  const about = aboutTracked(r.sid)?.turns?.at(-1)?.line;
  return [subj, plain(about) || inboxChanged(s) || ""];
}
/** A row's two lines: [subject, the useful line under it]. */
function inboxLines(r) {
  if (r.kind === "ask") return [inboxAskSubject(r.it), inboxWhy(r.it) || inboxSoFar(r) || (r.it.kind === "question" ? inboxQuestionLine(r.it) : "")];
  if (r.kind === "turn") return [r.it.interrupted ? "Interrupted: it stopped mid-turn" : inboxEndQuestion(finishedText(r.it)), inboxSoFar(r)];
  if (r.kind === "chore") return ["Getting full: compact or hand off?", inboxSoFar(r)];
  if (r.kind === "run") return inboxRunLines(r);
  return inboxDoneLines(r);
}

// ---------- the list ----------

/** A row's dot: an ask's red, amber for your turn or a full context, pulsing while it works (amber when stuck),
 *  still when done. */
function inboxDot(r) {
  const s = r.s, stuck = s && (s.stuck_ms || preStart(s)) && !s.compacting_ms;
  if (r.kind === "ask") return `<span class="hdot ask"></span>`;
  if (r.kind === "turn" || r.kind === "chore" || (r.kind === "run" && stuck)) return `<span class="ib-amber"></span>`;
  return r.kind === "run" ? projDot(r.project, true) : `<span class="ib-still">${projDot(r.project)}</span>`;
}
/** Its context as a pill ("context 71%"), when it's half full or more. */
const inboxCtxPill = (s) => (inboxFull(s) ? `<span class="ib-chore">context ${inboxCtx(s)}%</span>` : "");
/** A small amber meter of how full its context is. */
const inboxMeter = (s, cls = "") => `<span class="ib-meter ${cls}"><i style="width:${Math.min(100, inboxCtx(s))}%"></i></span>`;
/** The badges after its name: how many questions, how full its context is. */
function inboxBadges(r) {
  const n = r.kind === "ask" && r.it.kind === "question" ? questions(r.it).length : 0;
  return `${n > 1 ? `<span class="ib-cnt">${n} questions</span>` : ""}${inboxCtxPill(r.s)}`;
}
/** The bar along a working row's foot: moving; slower and fainter when it's been quiet a while; amber and still when
 *  stuck. Each draw starts it where the clock says it is, so a redraw doesn't jump it back. */
function inboxBar(s) {
  const stuck = (s.stuck_ms || preStart(s)) && !s.compacting_ms;
  const quiet = !stuck && s.state === "working" && now() - Math.max(s.doing_ms || 0, s.since_ms || 0) > INBOX_QUIET_MS;
  return `<span class="ib-bar ${stuck ? "stuck" : quiet ? "slow" : ""}" style="--ib-d:-${Date.now() % (quiet ? 3200 : 1600)}ms" aria-hidden="true"></span>`;
}
/** Pin, Later and Snooze (shown on hover), with Snooze's menu when it's open here (`at`). */
function inboxActs(sid, s, at) {
  if (!s) return "";
  const pin = inboxPinned(s), later = !!s.later_ms, snz = inboxSnoozed(s);
  return `<button class="ib-act ${pin ? "on" : ""}" data-inbox-pin="${esc(sid)}" aria-pressed="${pin}">${PIN_ICON}${pin ? "Unpin" : "Pin"}</button>`
    + `<button class="ib-act" data-inbox-later="${esc(sid)}">${later ? "Not later" : "Later"}</button>`
    + `<span class="ib-snz-wrap"><button class="ib-act" data-inbox-snooze="${esc(sid)}" data-at="${at}" aria-expanded="${inboxSnoozeAt === at}">${snz ? "Wake" : "Snooze"}</button>${inboxSnoozeAt === at ? inboxSnoozeMenu(sid) : ""}</span>`;
}
/** The times Snooze offers: in an hour, tomorrow at 9, next Monday at 9. */
function inboxSnoozeTimes() {
  const at = (days, h) => { const d = new Date(now()); d.setDate(d.getDate() + days); d.setHours(h, 0, 0, 0); return d.getTime(); };
  const monday = (8 - new Date(now()).getDay()) % 7 || 7;
  return [["In 1 hour", now() + 3600000], ["Tomorrow, 9:00", at(1, 9)], ["Monday, 9:00", at(monday, 9)]];
}
/** Snooze's menu: a few times to bring it back at. */
const inboxSnoozeMenu = (sid) => `<span class="ib-snz" role="menu">${inboxSnoozeTimes().map(([label, ms]) => `<button role="menuitem" data-inbox-snooze-to="${ms}" data-sid="${esc(sid)}">${label}</button>`).join("")}</span>`;
/** When a snoozed session comes back, in words ("Tue 9:00"). */
const inboxWhen = (ms) => new Date(ms).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });

/** One row: dot, name, badges and subject; the useful line; the time (a pin, a meter) at the right; Pin, Later and
 *  Snooze on hover; a working one's bar along its foot. */
function inboxRowHtml(r) {
  const { sid, s, kind } = r, seat = inboxSeats.get(sid), [subj, line] = inboxLines(r);
  const sinking = kind === "run" && seat?.from === "need" && now() - seat.at < INBOX_SINK_MS;
  const under = inboxSnoozed(s) ? `Back ${inboxWhen(s.snooze_ms)}${line ? ` · ${line}` : ""}` : line;
  const side = `<span class="ib-time">${ago(r.at)}</span>${inboxPinned(s) ? `<span class="ib-pin">${PIN_ICON}</span>` : ""}${inboxFull(s) ? inboxMeter(s) : ""}`;
  return `<div class="ib-row ${kind} ${sinking ? "sinking" : ""} ${inboxLast === sid ? "on" : ""}" role="button" tabindex="0" data-inbox-open="${esc(sid)}" style="${projStyle(r.project)}">`
    + `<span class="ib-dot">${inboxDot(r)}</span><span class="ib-l1">${nameSpan(sid, r.project, "ib-name")}${inboxBadges(r)}<span class="ib-subj">${esc(subj)}</span></span>`
    + `<span class="ib-l2">${esc(under)}</span><span class="ib-side">${side}</span>`
    + `<span class="ib-acts ${inboxSnoozeAt === `row:${sid}` ? "open" : ""}">${inboxActs(sid, s, `row:${sid}`)}</span>${kind === "run" && s ? inboxBar(s) : ""}</div>`;
}
/** A heading and its rows; nothing when it has none. */
const inboxSection = ([key, title, rows]) => rows.length ? `<div class="ib-sect ${key}">${key === "pin" ? PIN_ICON : ""}${title}</div>${rows.map(inboxRowHtml).join("")}` : "";
/** The head's tabs: Inbox, Later N, Snoozed N. */
function inboxTabs() {
  const rows = inboxRows(), n = (t) => rows.filter((r) => inboxTabOf(r.s) === t).length;
  const tab = (t, label, c) => `<button class="ib-tab ${inboxTab === t ? "on" : ""}" data-inbox-tab="${t}" aria-pressed="${inboxTab === t}">${label}${c ? ` <span>${c}</span>` : ""}</button>`;
  return `<span class="ib-tabs">${tab("inbox", "Inbox", 0)}${tab("later", "Later", n("later"))}${tab("snoozed", "Snoozed", n("snoozed"))}</span>`;
}
/** What an empty tab says. */
const inboxEmpty = () => ({ inbox: "Nothing needs you, and nothing is working.", later: "Nothing put off. Later on a row puts it here.", snoozed: "Nothing snoozed. Snooze on a row hides it until the time you pick." })[inboxTab];
/** The list: its head (how many need you and run, the tabs), then the sections. */
function inboxList() {
  const secs = inboxSections(), need = secs[0][2].filter((r) => inboxSectOf(r) === "need").length + secs[1][2].length, run = secs[2][2].length;
  const counts = inboxTab === "inbox" ? [[need, "need you"], [run, "running"]].filter(([c]) => c).map(([c, w]) => `${c} ${w}`).join(" · ") : "";
  const body = secs.map(inboxSection).join("");
  return `<div class="inbox"><div class="ib-top"><b>Inbox</b><span class="ib-counts">${esc(counts)}</span>${inboxTabs()}</div>
    <div class="ib-scroll" data-scroll-key="inbox">${body || `<div class="ib-empty">${inboxEmpty()}</div>`}</div></div>`;
}

// ---------- a session opened ----------

/** What it's waiting on from you, in a few words. */
function inboxWaitingOn(r) {
  if (r.kind === "ask") return r.it.kind === "question" ? (questions(r.it).length > 1 ? `Your answers to ${questions(r.it).length} questions` : "Your answer") : r.it.kind === "input" ? "A form from you" : `Your OK${/^wants to /.test(verb(r.it)) ? ` to ${verb(r.it).slice(9)}` : ""}`;
  if (r.kind === "turn") return r.it.interrupted ? "You: say “continue”, or what to do instead" : "Your reply";
  if (r.kind === "chore") return "You: compact it, or hand off to a fresh session";
  return "";
}
/** The facts over its cards: what it's doing, what it's done so far (and its changes), what it waits on. */
function inboxFacts(r) {
  const s = r.s, t = aboutTracked(r.sid);
  const doing = s?.state === "working" && s.doing ? s.doing : plain(t?.about) || plain(s?.prompt) || plain(s?.crew_job);
  const changed = inboxChanged(s), soFar = inboxSoFar(r);
  const so = soFar || changed ? `${esc(soFar)}${changed ? `${soFar ? " · " : ""}<button class="ib-link" data-inbox-diff="${esc(r.sid)}">${esc(changed)}</button>` : ""}` : "";
  const rows = [["Doing", esc(doing)], ["So far", so], ["Waiting on", esc(inboxWaitingOn(r))]].filter(([, v]) => v);
  return rows.length ? `<div class="ib-facts">${rows.map(([k, v]) => `<span class="ib-k">${k}</span><span class="ib-v">${v}</span>`).join("")}</div>` : "";
}
/** A plain box to write in, and its button. Enter sends too (app/keys.js, by its draft key). */
const inboxBox = (key, placeholder, label, attrs) => `<div class="ib-box"><textarea rows="2" data-text="${esc(key)}" placeholder="${esc(placeholder)}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><button class="btn primary" ${attrs}>${label}</button></div>`;
/** A question's choices: the one(s) picked lit. A click picks (one question: that's the answer). */
function inboxOpts(it, q, qi) {
  const d = draft(it.id);
  return `<div class="ib-opts">${(q.options || []).map((o, oi) => {
    const on = q.multiSelect ? d.choices[q.question]?.has?.(o.label) : d.choices[q.question] === o.label;
    return `<button class="ib-opt ${on ? "on" : ""}" data-inbox-pick="${qi}:${oi}" data-id="${esc(it.id)}"><b>${esc(o.label)}</b>${o.description ? `<span>${esc(o.description)}</span>` : ""}</button>`;
  }).join("")}</div>`;
}
/** What was picked for a question, in words. */
function inboxPicked(it, q) {
  const c = draft(it.id).choices[q.question];
  return q.multiSelect ? [...(c || [])].join(", ") : c || "";
}
/** One question as a card: the one to answer now lit, an answered one dimmed with its answer (a click opens it again). */
function inboxQuestionCard(it, q, qi, n, cur) {
  const done = isAnswered(it, q), on = qi === cur;
  const head = `<div class="ib-qn">${n > 1 ? `${qi + 1} of ${n}${done ? " · answered" : ""}` : "Question"}${q.header ? ` · ${esc(q.header)}` : ""}</div>`;
  const text = `<div class="ib-qt">${esc(q.question)}${q.multiSelect ? `<span class="ib-dim"> · pick any</span>` : ""}</div>`;
  if (done && !on) return `<div class="ib-card ib-q done" role="button" data-inbox-step="${esc(it.id)}:${qi}">${head}${text}<div class="ib-qa">→ ${esc(inboxPicked(it, q))}</div></div>`;
  return `<div class="ib-card ib-q ${on ? "on" : ""}" ${on ? "" : `data-inbox-step="${esc(it.id)}:${qi}"`}>${head}${text}${inboxOpts(it, q, qi)}</div>`;
}
/** A question ask: its questions stacked as cards, a box for your own answer to the one lit, and with several, one
 *  Send for all of them (answersFor, as the Board's Send answers). */
function inboxQuestions(it) {
  const qs = questions(it), n = qs.length, cur = stepOf(it), ready = !!answersFor(it);
  const cards = qs.map((q, qi) => inboxQuestionCard(it, q, qi, n, cur)).join("");
  const send = n > 1 || qs.some((q) => q.multiSelect) ? `<button class="btn primary" data-act="submit-answers" data-id="${esc(it.id)}" ${ready ? "" : "disabled"}>${ready ? `Send ${n > 1 ? `${n} answers` : "answer"}` : `${qs.filter((q) => isAnswered(it, q)).length} of ${n} answered`}</button>` : "";
  const box = inboxBox(it.id, n > 1 ? `Your own answer to question ${cur + 1}…` : "Your own answer…", "Answer", `data-act="send" data-id="${esc(it.id)}"`);
  return `${cards}${box}<div class="ib-foot-acts"><button class="btn deny" data-act="deny" data-id="${esc(it.id)}">Decline</button><span class="grow"></span>${send}</div>`;
}
/** Any other ask (a command to run, an edit, a plan, a form, its terminal's): the Board's own card body and buttons. */
const inboxAskCard = (it) => `<div class="ib-card on"><div class="ib-qn">${esc(verb(it).replace(/^./, (c) => c.toUpperCase()))}</div>${requestBody(it)}${whyLine(it)}</div>${decisionButtons(it)}`;
/** A finished turn: the question it ends on (else its opening lines), and a box to reply in. */
function inboxTurnCard(r) {
  const text = r.it ? finishedText(r.it) : lastSaid(r.s || {}), q = r.it?.interrupted ? "" : inboxEndQuestion(text);
  const lines = String(text).split("\n").filter((l) => l.trim()).map(plain);
  const lead = q || lines.slice(0, 2).join(" ") || "Finished.";
  // Its opening line under the question, unless So far says it already.
  const ctx = q && lines[0] && !lines[0].includes(q) && lines[0] !== inboxSoFar(r) ? `<div class="ib-why">${esc(lines[0])}</div>` : "";
  const what = r.it?.interrupted ? "Interrupted" : q ? "It asks" : "It said";
  const attrs = r.it ? `data-act="send" data-id="${esc(r.it.id)}"` : `data-inbox-send="${esc(r.sid)}"`;
  return `<div class="ib-card ${r.kind === "turn" ? "on" : ""}"><div class="ib-qn">${what}</div><div class="ib-qt">${esc(lead)}</div>${ctx}</div>${inboxBox(`s:${r.sid}`, "Reply…", "Send", attrs)}`;
}
/** Getting full: how full, and Compact now, Hand off (its hand-off skill, else Cue's: as the hand-off insight does),
 *  or Remind me later (Snooze). */
function inboxChoreCard(r) {
  const { sid, s } = r, h = handoffSkills(sid, s)[0];
  const hand = h ? `data-act="ins-fill" data-ins-cmd="/${esc(h.name)}"` : `data-act="ins-cue"`;
  const acts = insBusy.has(sid) ? insBusyNote(sid) : `<button class="btn primary" data-act="ins-compact" data-sid="${esc(sid)}">Compact now</button><button class="btn" ${hand} data-sid="${esc(sid)}">Hand off to a fresh session</button>`
    + `<span class="ib-snz-wrap"><button class="btn" data-inbox-snooze="${esc(sid)}" data-at="card" aria-expanded="${inboxSnoozeAt === "card"}">Remind me later</button>${inboxSnoozeAt === "card" ? inboxSnoozeMenu(sid) : ""}</span>`;
  return `<div class="ib-card on chore"><div class="ib-full">${inboxMeter(s, "big")}<span>${inboxCtx(s)}% of its context used</span></div>
    <div class="ib-qt">Getting full: compact or hand off?</div>
    <div class="ib-why">Compact sums up its chat in place and keeps this session. A hand-off writes a note of where things stand and starts a fresh session from it; this one closes (Closed keeps it).</div>
    <div class="ib-foot-acts">${acts}</div></div>`;
}
/** A working session: what it's on, its bar, and a box to tell it something (it reads it when it can). */
function inboxRunCard(r) {
  const [, line] = inboxRunLines(r);
  return `<div class="ib-card run"><div class="ib-qn">Working</div><div class="ib-qt">${esc(line)}</div>${inboxBar(r.s)}</div>${inboxBox(`s:${r.sid}`, "Tell it something…", "Send", `data-inbox-send="${esc(r.sid)}"`)}`;
}
/** The open session's cards, by what it is. */
function inboxCards(r) {
  if (r.kind === "ask") return r.it.kind === "question" ? inboxQuestions(r.it) : inboxAskCard(r.it);
  if (r.kind === "chore") return inboxChoreCard(r);
  if (r.kind === "run") return inboxRunCard(r);
  return inboxTurnCard(r);
}
/** "next ask ›": to the next session that needs you, when there's another. */
function inboxStep(sid) {
  const others = inboxNeedOrder().filter((x) => x !== sid);
  return others.length ? `<button class="ib-next" data-inbox-next>next ask ›</button>` : "";
}
/** The open session: its head (‹ Inbox, its name, Pin, Later, Snooze, next ask), its facts and cards, and its full
 *  chat as a link at the foot. No hover text: a tooltip the reused parts carry is left out. */
function inboxOpened(sid) {
  const r = inboxRowFor(sid), s = r.s;
  const head = `<div class="ib-head"><button class="ib-back" data-inbox-back>‹ Inbox</button><span class="ib-hname">${esc(nameOf(sid, r.project))}</span>${inboxCtxPill(s)}<span class="grow"></span>`
    + `<span class="ib-hacts">${inboxActs(sid, s, "head")}</span>${inboxStep(sid)}</div>`;
  const body = `<div class="ib-page-body" data-scroll-key="inbox-page">${inboxFacts(r)}${inboxCards(r)}<button class="ib-fullchat" data-inbox-full="${esc(sid)}">Full chat ›</button></div>`;
  return `<div class="inbox-page" data-inbox-page="${esc(sid)}" data-kind="${r.kind}" style="${projStyle(r.project)}">${head}${body}</div>`.replace(/\stitle="[^"]*"/g, "");
}

// ---------- moving about ----------

/** Whether the open session is done with you: it was at rest while open (asking, or finished), and it's at work
 *  again. Not while you type in its box (that would take the box from under you). */
function inboxAnswered(sid) {
  const r = inboxRowFor(sid);
  if (r && r.kind !== "run") { inboxAsked = true; return false; }
  const f = document.activeElement;
  return inboxAsked && !(f?.matches?.("textarea, input") && f.value);
}
/** The Inbox view: the open session, or the list. The open one closes when it goes away, or it needed you, was
 *  answered and works again. */
function inboxView() {
  const sid = inboxSid;
  if (sid && ((!sessionOf(sid) && !inboxItems().has(sid)) || inboxAnswered(sid))) { inboxSid = null; inboxAsked = false; }
  return `<div class="board ib-board">${inboxSid ? inboxOpened(inboxSid) : inboxList()}</div>`;
}
/** The sessions that need you, in the list's order (Pinned first). */
function inboxNeedOrder() {
  const seat = (sid) => inboxSeats.get(sid), rows = inboxNeedRows();
  const pin = rows.filter((r) => inboxPinned(r.s)), rest = rows.filter((r) => !inboxPinned(r.s));
  const order = (xs) => xs.sort((a, b) => (seat(b.sid)?.n || 0) - (seat(a.sid)?.n || 0) || b.at - a.at).map((r) => r.sid);
  return [...order(pin), ...order(rest)];
}
/** Open session `sid` in the list's place, on what it asks (the Board's keys and buttons act on what's shown). */
function inboxOpen(sid) {
  const it = inboxItems().get(sid) || null;
  view = "inbox";
  active = { id: it?.id || null, sid, exact: false };
  seenSession(sid);
  inboxSid = inboxLast = sid;
  inboxAsked = false;
  inboxSnoozeAt = "";
  renderMain();
  const r = inboxRowFor(sid);
  if (r && r.kind !== "ask" && r.kind !== "chore") document.querySelector(".inbox-page textarea[data-text]")?.focus();
}
/** Back to the list. */
function inboxClose() {
  inboxSid = null;
  inboxAsked = false;
  inboxSnoozeAt = "";
  renderMain();
}
/** The next (`dir` 1) or previous (-1) session that needs you, after the open one; wraps. */
function inboxStepTo(dir = 1) {
  const all = inboxNeedOrder(), others = all.filter((x) => x !== inboxSid);
  if (!others.length) return;
  const at = all.indexOf(inboxSid);
  inboxOpen(at < 0 ? others[dir > 0 ? 0 : others.length - 1] : all[(at + dir + all.length) % all.length]);
}
/** Its full chat, on the Board. */
function inboxFullChat(sid) {
  inboxSid = null;
  inboxAsked = false;
  openSession(sid, inboxItems().get(sid)?.id || null);
}
/** What it changed, on the Board: its chat on its Diff tab. */
function inboxDiff(sid) {
  actTabOn.delete(sid);
  bgTabOn.delete(sid);
  aboutTabOn.delete(sid);
  extTabOf.set(sid, DIFF_TAB);
  inboxFullChat(sid);
}

// ---------- pin, later, snooze ----------

/** Pin it to the top of the Inbox, or unpin it: shown now, kept with the session in Cue. */
function inboxSetPin(sid, on) {
  const s = sessionOf(sid);
  if (s) s.pinned_ms = on ? now() : 0;
  invoke("set_pinned", { sessionId: sid, on }).catch((e) => toast(`Couldn't: ${e}`));
  renderMain();
}
/** Put it off for later (the session's own Later, as the Board's Need to decide), or back. */
function inboxSetLater(sid, on) {
  const s = sessionOf(sid);
  if (s) s.later_ms = on ? now() : 0;
  invoke("set_later", { sessionId: sid, on }).catch((e) => toast(`Couldn't: ${e}`));
  if (on) toast(`${nameOf(sid, s?.project)} is under Later`);
  inboxGone(sid, on);
}
/** Snooze it until `until` (0: back now): hidden from the Inbox till then. */
function inboxSetSnooze(sid, until) {
  const s = sessionOf(sid);
  if (s) { s.snooze_ms = until; s.snoozed_at_ms = until ? now() : 0; }
  inboxSnoozeAt = "";
  invoke("set_snooze", { sessionId: sid, untilMs: until }).catch((e) => toast(`Couldn't: ${e}`));
  if (until) toast(`${nameOf(sid, s?.project)} is snoozed until ${inboxWhen(until)}`);
  inboxGone(sid, !!until);
}
/** A session just put off or snoozed: if it was open, back to the list, where it's gone from the Inbox tab. */
function inboxGone(sid, gone) {
  if (gone && inboxSid === sid) { inboxSid = null; inboxAsked = false; }
  renderMain();
}
/** Snooze's button: its menu opens (or shuts) where it is; a snoozed one wakes. */
function inboxSnoozeClick(el) {
  const sid = el.dataset.inboxSnooze;
  if (inboxSnoozed(sessionOf(sid))) return inboxSetSnooze(sid, 0);
  inboxSnoozeAt = inboxSnoozeAt === el.dataset.at ? "" : el.dataset.at;
  renderMain();
}
/** One of a multi-question card's choices: picked and held until Send (a single question's click is its answer). */
function inboxPick(el) {
  const it = findItem(el.dataset.id), [qi, oi] = el.dataset.inboxPick.split(":").map(Number);
  if (it) pick(it, qi, oi, questions(it).length > 1);
}

/** Keys while the Inbox shows. The open session: Esc back to the list (a Snooze menu first), N or → the next that
 *  needs you, ← the one before, 1–9 a choice of the question lit. The list: the Board's card keys (A, D, G, R, 1–9)
 *  are kept from a card not on screen. Not while you type in a box with text in it, or something is open over it.
 *  True when it took the key. */
function inboxKey(e) {
  if (view !== "inbox" || e.metaKey || e.ctrlKey || e.altKey || e.isComposing) return false;
  if (sheet || lightbox || liveOpen || deleteAsk || renaming || btwFor || menuFor || redirectFor || (typeof moreFor !== "undefined" && moreFor)) return false;
  const f = document.activeElement, typing = f?.matches?.("textarea, input, select");
  if (e.key === "Escape" && inboxSnoozeAt) { inboxSnoozeAt = ""; renderMain(); return true; }
  if (!inboxSid && e.key === "Enter" && f?.matches?.(".ib-row")) { inboxOpen(f.dataset.inboxOpen); return true; }
  if (!inboxSid) return !typing && /^[adgr1-9]$/.test(e.key);
  if (e.key === "Escape" && !e.shiftKey) { if (typing && f.value) return false; f?.blur?.(); inboxClose(); return true; }
  if (typing) return false;
  if (e.key === "n" || e.key === "ArrowRight") { inboxStepTo(1); return true; }
  if (e.key === "ArrowLeft") { inboxStepTo(-1); return true; }
  return inboxDigit(e.key);
}
/** 1–9 on an open question: that choice of the question lit, held with the rest when there are several. */
function inboxDigit(key) {
  const it = inboxItems().get(inboxSid);
  if (!/^[1-9]$/.test(key) || it?.kind !== "question") return false;
  pick(it, stepOf(it), +key - 1, questions(it).length > 1);
  return true;
}

/** The Inbox's own clicks, by attribute: each with its element. */
const INBOX_CLICKS = {
  inboxPin: (el) => inboxSetPin(el.dataset.inboxPin, !inboxPinned(sessionOf(el.dataset.inboxPin))),
  inboxLater: (el) => inboxSetLater(el.dataset.inboxLater, !sessionOf(el.dataset.inboxLater)?.later_ms),
  inboxSnooze: inboxSnoozeClick,
  inboxSnoozeTo: (el) => inboxSetSnooze(el.dataset.sid, +el.dataset.inboxSnoozeTo),
  inboxTab: (el) => { inboxTab = el.dataset.inboxTab; inboxSnoozeAt = ""; renderMain(); },
  inboxPick,
  inboxStep: (el) => { const [id, n] = el.dataset.inboxStep.split(/:(?=\d+$)/); qStep[id] = +n; renderMain(); },
  inboxSend: (el) => sendTo(el.dataset.inboxSend),
  inboxFull: (el) => inboxFullChat(el.dataset.inboxFull),
  inboxDiff: (el) => inboxDiff(el.dataset.inboxDiff),
  inboxBack: () => inboxClose(),
  inboxNext: () => inboxStepTo(1),
  inboxOpen: (el) => inboxOpen(el.dataset.inboxOpen),
};
/** The attribute selector for each of INBOX_CLICKS ("inboxSnoozeTo" → "[data-inbox-snooze-to]"), innermost first. */
const INBOX_SEL = Object.keys(INBOX_CLICKS).map((k) => `[data-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}]`).join(", ");
/** A click in the Inbox: the innermost element with one of its attributes does its thing. */
function inboxClick(e) {
  // Inbox in the header while a session is open in it: back to the list (the click goes on to switch the view).
  if (e.target.closest?.('[data-view="inbox"]')) { if (view === "inbox") inboxSid = null; return; }
  const el = e.target.closest?.(INBOX_SEL);
  if (!el) { if (inboxSnoozeAt && !e.target.closest?.(".ib-snz")) { inboxSnoozeAt = ""; renderMain(); } return; }
  e.stopPropagation();
  const key = Object.keys(INBOX_CLICKS).find((k) => k in el.dataset);
  INBOX_CLICKS[key](el);
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (inboxKey(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", inboxClick, true);
}
