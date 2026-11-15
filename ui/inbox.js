// ---------- Inbox: where you see what a session asks, and answer it (the header's Inbox, after Board) ----------
// The list: a row per session, two lines each (its name and what it asks or is on; then why, or what it found), under
// Pinned, Needs you, Running, Done and, folded at the foot, Parked (the Board's parked: put off, no notification). A
// finished turn that ends on a question needs you; one that doesn't is Done. A session whose context is half full or
// more comes up under Needs you to compact or hand off. Answer one and it sinks to Running, a bar moving along its foot
// (slower when it's quiet, amber and still when it's stuck); it rises again when it asks again or finishes. A row keeps
// its place while its state stays the same. Hovering a row shows Pin, Park and Snooze (Snoozed is a tab of its own).
// Beside the list, a brief of the session open: never empty, as the top one that needs you opens by itself (its row
// lit, where it is). Its main column has its own words before the question (the last paragraph or two), the question
// with each choice and what it means, what it plans next when it says, and a box to answer in; its side column what
// you asked it, what it did this turn (repeats folded: "Read 6 files"), what this turn changed (a peek of the biggest
// change) and its context. Send opens the next that needs you at once. With nothing needing you, what's running and
// what's done lately stand in. With a project picked in the header, only its sessions.

/** The session open beside the list; null when none is (nothing needs you, or a narrow window shows the list). */
let inboxSid = null;
/** When the open session was opened (Date.now()): an answer or a message to it after that moves on to the next. */
let inboxOpenedAt = 0;
/** The open session needed you while it was open: once it's at work again, the next that needs you opens. */
let inboxAsked = false;
/** The session last opened: its row stays lit, so you see where you were. */
let inboxLast = null;
/** The list's tab: "inbox" or "snoozed". */
let inboxTab = "inbox";
/** Where a Snooze menu is open: "row:<sid>", "head" or "card" (the open session's); "" when none is. */
let inboxSnoozeAt = "";
/** sid → { sect, n, at, from }: the section a row was last drawn in, its place there (higher on top), since when, and
 *  the section it came from. A row keeps its place while it stays in its section. */
const inboxSeats = new Map();
let inboxSeq = 0;
/** sid → { key, files, bulk }: what its turn has changed so far, as git says (files null while it's read). */
const inboxTurnDiffs = new Map();
/** How long a finished session with nothing waiting stays under Done, and how many of those show at most. */
const INBOX_DONE_MS = 2 * 3600000, INBOX_DONE_MAX = 6;
/** Context this full (percent): its row shows a meter, and a session at rest comes up to compact or hand off. */
const INBOX_FULL = 50;
/** A working session with no new step for this long: its bar slows. */
const INBOX_QUIET_MS = 2 * 60000;
/** A row that came into Running this recently is still sinking: drawn faded. */
const INBOX_SINK_MS = 8000;
/** "It says": the last paragraphs of its words before the question, at most this many, this long in all (characters). */
const INBOX_SAID_PARAS = 3, INBOX_SAID_MAX = 600;
/** "It did": this many lines at most (a folded run is one); the earlier ones are counted above them. */
const INBOX_STEPS_MAX = 6;
/** "Changed": this many files listed, and this many lines of the biggest change shown. */
const INBOX_FILES_MAX = 4, INBOX_PEEK_LINES = 6;
/** A sentence where it says what it does next: "Then I'll …", "Next I'd …", "After that, …". */
const INBOX_THEN = /^(?:then|next|after that|afterwards)\b|\bI'll (?:then|next)\b/i;

// ---------- what each session is, as a row ----------

/** Its marks, as Cue keeps them with the session. */
const inboxPinned = (s) => !!s?.pinned_ms;
/** Parked: the Board's parked (its "later" mark), put off with no notification. */
const inboxParked = (s) => !!s?.later_ms;
/** Snoozed until its time, unless it has asked something since it was snoozed (then it's back, as a new reply wakes
 *  a snoozed mail). */
const inboxSnoozed = (s) => (s?.snooze_ms || 0) > now() && !state.items.some((i) => i.session_id === s.session_id && i.created_ms > (s.snoozed_at_ms || 0));
const inboxCtx = (s) => s?.context_pct || 0;
const inboxFull = (s) => inboxCtx(s) >= INBOX_FULL;
/** The tab a session is listed under. */
const inboxTabOf = (s) => (inboxSnoozed(s) ? "snoozed" : "inbox");

/** A reply's last line, plain ("Recommendation: send fixes"). */
const inboxLastLine = (text) => plain(String(text || "").split("\n").map((l) => l.trim()).filter(Boolean).at(-1) || "");
/** A lead's crew that waits on you, in a few words ("2 of its crew need you"). */
const inboxCrewSubject = (sid) => { const n = inboxCrew(sid); return `${n} of its crew need${n === 1 ? "s" : ""} you`; };
/** Who in its crew waits, and on what ("v5-loop: send fixes · at-picker: asks"). */
const inboxCrewLine = (sid) => (typeof mateWaits === "function" ? mateWaits(sid).map((t) => `${t.name}: ${mateNeeds(sid, t) ? (mateAsk(sid, t) ? verb(mateAsk(sid, t)) : t.needs) : mateReadyAct(t) === "fixes" ? "send fixes" : "commit"}`).join(" · ") : "");
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
/** You've written to it from here and Cue hasn't shown it in its thread yet (not one that failed to go). */
const inboxSending = (sid) => outbox.some((o) => o.sid === sid && !/^not sent/.test(o.via || ""));
/** How many of a lead's crew wait on you (a teammate asking, or its review ready to commit or send back); 0 for
 *  any other session. */
const inboxCrew = (sid) => (typeof mateWaits === "function" ? mateWaits(sid).length : 0);
/** One session's row: { sid, s, it, project, at, kind } where kind is "ask", "turn" (a finished turn: your turn, as
 *  on the Board's Waiting, or interrupted), "crew" (a lead whose crew waits on you), "chore" (its context is getting
 *  full), "run" or "done" (finished, nothing waiting). One you just answered is at
 *  work at once, before Cue says so. Null when there's nothing to say of it, unless `keep` (pinned, parked or
 *  snoozed: always listed). */
function inboxRowOf(sid, s, it, keep = false) {
  const row = (kind, at) => ({ sid, s, it, project: s?.project || it?.project, at, kind });
  if (it && answering.has(it.id) && s) return row("run", now());
  if (it && it.kind !== "waiting") return row("ask", it.created_ms);
  if (it) return row("turn", it.created_ms);
  if (inboxCrew(sid)) return row("crew", s.since_ms || 0);
  if (s && (s.state === "working" || s.state === "agent" || s.compacting_ms || inboxSending(sid))) return row("run", s.since_ms || 0);
  if (s && inboxFull(s) && !cutOff(s)) return row("chore", s.since_ms || 0);
  const recent = s && !hiddenIdle.has(idleKey(s)) && now() - (s.since_ms || 0) < INBOX_DONE_MS;
  return recent || keep ? row("done", s?.since_ms || 0) : null;
}
/** Every row the Inbox has, any tab. Closed and ended sessions aren't in Cue's list, so never here. */
function inboxRows() {
  const items = inboxItems(), rows = [];
  for (const [sid, it] of items) rows.push(inboxRowOf(sid, sessionOf(sid), it, true));
  for (const s of state.sessions) {
    if (items.has(s.session_id) || !inProj(s.project) || cutOff(s)) continue;
    const r = inboxRowOf(s.session_id, s, null, inboxPinned(s) || inboxParked(s) || inboxTabOf(s) !== "inbox");
    if (r) rows.push(r);
  }
  return rows;
}
/** The open session's row (whatever its tab), or null once it's gone. */
const inboxRowFor = (sid) => inboxRowOf(sid, sessionOf(sid), inboxItems().get(sid) || null, true);
/** The section a row goes under: Parked whatever it's at; else by what it is. */
const inboxSectOf = (r) => (inboxParked(r.s) ? "park" : r.kind === "run" ? "run" : r.kind === "done" ? "done" : "need");
/** The rows needing you under the Inbox tab, a session once each (parked ones wait in Parked). */
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
/** The open tab's rows by section, in order: Pinned, Needs you, Running, Done, Parked (all but Needs you on the Inbox
 *  tab only; the Snoozed tab lists its rows as they are). */
function inboxSections() {
  const rows = inboxRows().filter((r) => inboxTabOf(r.s) === inboxTab).sort((a, b) => b.at - a.at);
  const box = inboxTab === "inbox", parked = box ? rows.filter((r) => inboxParked(r.s)) : [];
  const pinned = box ? rows.filter((r) => inboxPinned(r.s) && !parked.includes(r)) : [];
  const rest = rows.filter((r) => !pinned.includes(r) && !parked.includes(r));
  const of = (k) => rest.filter((r) => (inboxParked(r.s) ? "need" : inboxSectOf(r)) === k);
  let quiet = 0;
  const done = of("done").filter((r) => r.it || inboxPinned(r.s) || !box || ++quiet <= INBOX_DONE_MAX);
  return [["pin", "Pinned", inboxSeated("pin", pinned)], ["need", "Needs you", inboxSeated("need", of("need"))], ["run", "Running", inboxSeated("run", of("run"))], ["done", "Done", inboxSeated("done", done)], ["park", "Parked", inboxSeated("park", parked)]];
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
  return rest.length ? `Then: ${rest.map((x) => x.header || x.question).join(" · ")}` : (q?.options || []).map((o) => inboxLabel(o.label)[0]).join(" · ");
}
/** Why an ask asks: the last thing it said before asking, in a line. */
function inboxWhy(it) {
  const c = (it.context || []).filter((x) => x.role !== "user").at(-1);
  return plain(firstLine(c?.text || ""));
}
/** A working row's lines: what you asked it (what you just sent, else its prompt), and what it's on now (or that it's
 *  quiet, or stuck). */
function inboxRunLines(r) {
  const s = r.s, seat = inboxSeats.get(r.sid), sent = outbox.findLast((o) => o.sid === r.sid)?.text, said = plain(sent || s.prompt);
  const answered = r.it && answering.has(r.it.id) && !sent ? `You answered: ${r.it.kind === "waiting" ? inboxEndQuestion(finishedText(r.it)) || "its reply" : inboxAskSubject(r.it)}` : "";
  const subj = answered || (said ? (sent || seat?.from === "need" ? `You: “${said}”` : said) : plain(aboutTracked(r.sid)?.about) || "Working");
  const stuck = s.stuck_ms && !s.compacting_ms ? `No new output for ${ago(s.stuck_ms)}` : "";
  const busy = s.state === "working" || s.state === "agent" || s.compacting_ms;
  const line = s.compacting_ms ? "Compacting…" : preStart(s) ? `Not started: ${preStart(s)}` : stuck || (!busy ? "Sent" : s.state === "agent" ? idleNote(s) : s.doing || "Thinking");
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
  if (r.kind === "turn") return [r.it.interrupted ? "Interrupted: it stopped mid-turn" : inboxEndQuestion(finishedText(r.it)) || inboxLastLine(finishedText(r.it)) || "Your turn", inboxSoFar(r)];
  if (r.kind === "crew") return [inboxCrewSubject(r.sid), inboxCrewLine(r.sid)];
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
  if (r.kind === "turn" || r.kind === "crew" || r.kind === "chore" || (r.kind === "run" && stuck)) return `<span class="ib-amber"></span>`;
  return r.kind === "run" ? projDot(r.project, true) : `<span class="ib-still">${projDot(r.project)}</span>`;
}
/** Its context as a pill ("context 71%"), when it's half full or more. */
const inboxCtxPill = (s) => (inboxFull(s) ? `<span class="ib-chore">context ${inboxCtx(s)}%</span>` : "");
/** A small meter of how full its context is: amber from half full. */
const inboxMeter = (s, cls = "") => `<span class="ib-meter ${inboxFull(s) ? "" : "calm"} ${cls}"><i style="width:${Math.min(100, inboxCtx(s))}%"></i></span>`;
/** The badges after its name: how many questions, how full its context is. */
function inboxBadges(r) {
  const n = r.kind === "ask" && r.it.kind === "question" ? questions(r.it).length : 0;
  const crew = r.kind !== "crew" ? inboxCrew(r.sid) : 0;
  return `${n > 1 ? `<span class="ib-cnt">${n} questions</span>` : ""}${crew ? `<span class="ib-cnt">crew ${crew}</span>` : ""}${inboxCtxPill(r.s)}`;
}
/** The bar along a working row's foot: moving; slower and fainter when it's been quiet a while; amber and still when
 *  stuck. Each draw starts it where the clock says it is, so a redraw doesn't jump it back. */
function inboxBar(s) {
  const stuck = (s.stuck_ms || preStart(s)) && !s.compacting_ms;
  const quiet = !stuck && s.state === "working" && now() - Math.max(s.doing_ms || 0, s.since_ms || 0) > INBOX_QUIET_MS;
  return `<span class="ib-bar ${stuck ? "stuck" : quiet ? "slow" : ""}" style="--ib-d:-${Date.now() % (quiet ? 3200 : 1600)}ms" aria-hidden="true"></span>`;
}
/** Pin, Park and Snooze (shown on hover), with Snooze's menu when it's open here (`at`). */
function inboxActs(sid, s, at) {
  if (!s) return "";
  const pin = inboxPinned(s), park = inboxParked(s), snz = inboxSnoozed(s);
  return `<button class="ib-act ${pin ? "on" : ""}" data-inbox-pin="${esc(sid)}" aria-pressed="${pin}">${PIN_ICON}${pin ? "Unpin" : "Pin"}</button>`
    + `<button class="ib-act" data-inbox-park="${esc(sid)}">${park ? "Unpark" : "Park"}</button>`
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

/** One row: dot, name, badges and subject; the useful line; the time (a pin, a meter) at the right; Pin, Park and
 *  Snooze on hover; a working one's bar along its foot. */
function inboxRowHtml(r) {
  const { sid, s, kind } = r, seat = inboxSeats.get(sid), [subj, line] = inboxLines(r);
  const sinking = kind === "run" && seat?.from === "need" && now() - seat.at < INBOX_SINK_MS;
  const under = inboxSnoozed(s) ? `Back ${inboxWhen(s.snooze_ms)}${line ? ` · ${line}` : ""}` : line;
  const side = `<span class="ib-time">${ago(r.at)}</span>${inboxPinned(s) ? `<span class="ib-pin">${PIN_ICON}</span>` : ""}${inboxFull(s) ? inboxMeter(s) : ""}`;
  return `<div class="ib-row ${kind} ${inboxParked(s) ? "parked" : ""} ${sinking ? "sinking" : ""} ${(inboxSid || inboxLast) === sid ? "on" : ""}" role="button" tabindex="0" data-inbox-open="${esc(sid)}" style="${projStyle(r.project)}">`
    + `<span class="ib-dot">${inboxDot(r)}</span><span class="ib-l1">${nameSpan(sid, r.project, "ib-name")}${inboxBadges(r)}<span class="ib-subj">${esc(subj)}</span></span>`
    + `<span class="ib-l2">${esc(under)}</span><span class="ib-side">${side}</span>`
    + `<span class="ib-acts ${inboxSnoozeAt === `row:${sid}` ? "open" : ""}">${inboxActs(sid, s, `row:${sid}`)}</span>${kind === "run" && s ? inboxBar(s) : ""}</div>`;
}
/** Whether Parked is open at the foot of the list (folded unless you opened it; remembered per window). */
const inboxParkOpen = () => { try { return localStorage.getItem("cue.inboxParkedOpen") === "1"; } catch { return false; } };
/** Open or fold Parked. */
function inboxFoldPark() {
  try { localStorage.setItem("cue.inboxParkedOpen", inboxParkOpen() ? "0" : "1"); } catch {}
  renderMain();
}
/** A heading and its rows; nothing when it has none. Parked's heading folds it, with its count. */
function inboxSection([key, title, rows]) {
  if (!rows.length) return "";
  if (key !== "park") return `<div class="ib-sect ${key}">${key === "pin" ? PIN_ICON : ""}${title}</div>${rows.map(inboxRowHtml).join("")}`;
  const open = inboxParkOpen();
  return `<button class="ib-sect park" data-inbox-fold aria-expanded="${open}"><span class="ib-fold">${open ? "▾" : "▸"}</span>${title} <span class="ib-sect-n">${rows.length}</span></button>${open ? rows.map(inboxRowHtml).join("") : ""}`;
}
/** The head's tabs: Inbox, Snoozed N. */
function inboxTabs() {
  const n = inboxRows().filter((r) => inboxTabOf(r.s) === "snoozed").length;
  const tab = (t, label, c) => `<button class="ib-tab ${inboxTab === t ? "on" : ""}" data-inbox-tab="${t}" aria-pressed="${inboxTab === t}">${label}${c ? ` <span>${c}</span>` : ""}</button>`;
  return `<span class="ib-tabs">${tab("inbox", "Inbox", 0)}${tab("snoozed", "Snoozed", n)}</span>`;
}
/** What an empty tab says. */
const inboxEmpty = () => ({ inbox: "Nothing needs you, and nothing is working.", snoozed: "Nothing snoozed. Snooze on a row hides it until the time you pick." })[inboxTab];
/** The list: its head (how many need you and run, the tabs), then the sections. */
function inboxList() {
  const secs = inboxSections(), need = secs[0][2].filter((r) => inboxSectOf(r) === "need").length + secs[1][2].length, run = secs[2][2].length;
  const counts = inboxTab === "inbox" ? [[need, "need you"], [run, "running"]].filter(([c]) => c).map(([c, w]) => `${c} ${w}`).join(" · ") : "";
  const body = secs.map(inboxSection).join("");
  return `<div class="inbox"><div class="ib-top"><b>Inbox</b><span class="ib-counts">${esc(counts)}</span>${inboxTabs()}</div>
    <div class="ib-scroll" data-scroll-key="inbox">${body || `<div class="ib-empty">${inboxEmpty()}</div>`}</div></div>`;
}

// ---------- the brief: its words, the question, the box ----------

/** Text in paragraphs, split at blank lines (a code block stays whole). */
function inboxParas(text) {
  const out = [];
  let cur = [], fence = false;
  for (const l of String(text || "").split("\n")) {
    if (/^\s*```/.test(l)) fence = !fence;
    if (!fence && !l.trim()) { if (cur.length) out.push(cur.join("\n")); cur = []; } else cur.push(l);
  }
  if (cur.length) out.push(cur.join("\n"));
  return out;
}
/** The sentence where it says what it does next ("Next I'd tune the stop distance."), as written; "" when none. */
function inboxThen(text) {
  const all = String(text || "").split("\n").flatMap((l) => l.split(/(?<=[.!?])\s+/));
  return all.filter((x) => { const p = plain(x); return p && !/\?["'”)]*$/.test(p) && INBOX_THEN.test(p); }).at(-1) || "";
}
/** Its words before the question, trimmed (markdown): `cut` (the question, the sentence on what's next) taken out,
 *  then its last paragraph or few, up to INBOX_SAID_MAX characters; one long paragraph is cut from its start. */
function inboxSaid(text, cut = []) {
  let t = String(text || "");
  for (const c of cut) { const at = c ? t.lastIndexOf(c) : -1; if (at >= 0) t = t.slice(0, at) + t.slice(at + c.length); }
  const paras = inboxParas(t).filter((p) => plain(p)), out = [];
  for (const p of paras.reverse()) {
    if (out.length && (out.length >= INBOX_SAID_PARAS || out.join("").length + p.length > INBOX_SAID_MAX)) break;
    out.unshift(p);
  }
  const one = out[0] || "";
  if (out.length === 1 && one.length > INBOX_SAID_MAX * 1.3 && !one.includes("```")) {
    const tail = one.slice(-INBOX_SAID_MAX), from = tail.search(/(?<=[.!?]\s)\S/);
    out[0] = `…${tail.slice(from > 0 ? from : 0)}`;
  }
  return out.join("\n\n").trim();
}
/** Its latest words in its steps this turn (what it said between steps), "" when there are none. */
const inboxFeedSaid = (sid) => (inboxTurn(sid)?.items || []).filter((x) => x.t === "say").at(-1)?.text || "";
/** Its own words for the brief: a finished turn's reply; an ask's last words before it (as Cue caught them with the
 *  ask, else from its steps); else what it said last. */
function inboxWords(r) {
  if (r.it?.kind === "waiting") return finishedText(r.it);
  if (r.it && r.kind === "ask") return (r.it.context || []).filter((x) => x.role !== "user").at(-1)?.text || inboxFeedSaid(r.sid);
  return inboxFeedSaid(r.sid) || lastSaid(r.s || {});
}
/** A choice's label without its "(Recommended)", and whether it had one: [label, recommended]. */
function inboxLabel(label) {
  const m = /^(.*?)\s*\(recommended\)\s*$/i.exec(label || "");
  return m ? [m[1], true] : [label || "", false];
}
/** A plain box to write in, and its button. Enter sends too (app/keys.js, by its draft key). */
const inboxBox = (key, placeholder, label, attrs, extra = "") => `<div class="ib-box"><textarea rows="2" data-text="${esc(key)}" placeholder="${esc(placeholder)}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><div class="ib-box-acts"><button class="btn primary" ${attrs}>${label}</button>${extra}</div></div>`;
/** The reply, as in mail: you write right under the message (no frame round it), Send under what you write, Open chat
 *  and the project's actions beside it; suggested replies over it. */
function inboxReply(sid, key, ph, label, attrs) {
  return `${inboxSuggest(sid, key)}${inboxBox(key, ph, label, attrs, `<button class="btn" data-inbox-full="${esc(sid)}">Open chat</button>${inboxActions(sid)}`)}`;
}
/** Suggested replies, as mail suggests them: Claude's own suggestion (it goes into the box, to send as it is or
 *  change), then your quick phrases (Settings → Quick phrases: a click sends it, after anything in the box). */
function inboxSuggest(sid, key) {
  const sug = suggestionFor(sid), ps = phrases();
  if (!sug && !ps.length) return "";
  const k = `data-key="${esc(key)}" data-sid="${esc(sid)}"`;
  return `<div class="ib-sugs">${sug ? `<button class="ib-sug ai" data-inbox-sug ${k}><span class="ib-sug-k">Suggested</span>${esc(sug)}</button>` : ""}${ps.map((p) => `<button class="ib-sug" data-inbox-phrase="${esc(p)}" ${k}>${esc(p)}</button>`).join("")}</div>`;
}
/** Its project's actions (actions.js), as under its latest reply in the chat: run one yourself. */
function inboxActions(sid) {
  const s = sessionOf(sid);
  return s && typeof actReplyHtml === "function" ? actReplyHtml(s, s.since_ms || 0) : "";
}
/** A quick phrase clicked: added to what's in the box, and sent. */
function inboxSendPhrase(el) {
  const d = draft(el.dataset.key);
  d.text = withPhrase(d.text, el.dataset.inboxPhrase);
  saveDrafts();
  renderMain();
  document.querySelector(`.inbox-page .ib-box textarea[data-text="${CSS.escape(el.dataset.key)}"]`)?.closest(".ib-box")?.querySelector(".btn.primary")?.click();
}
/** Claude's suggestion clicked: into the box, the cursor at its end. */
function inboxTakeSuggestion(el) {
  draft(el.dataset.key).text = suggestionFor(el.dataset.sid);
  saveDrafts();
  inboxReplyTo(el.dataset.sid);
}
/** A question's choices, each with what it means and a "Recommended" tag; the one(s) picked lit; numbered on the
 *  question lit (its keys). A click picks (one question: that's the answer). */
function inboxOpts(it, q, qi, lit) {
  const d = draft(it.id);
  return `<div class="ib-opts">${(q.options || []).map((o, oi) => {
    const on = q.multiSelect ? d.choices[q.question]?.has?.(o.label) : d.choices[q.question] === o.label, [label, rec] = inboxLabel(o.label);
    return `<button class="ib-opt ${on ? "on" : ""}" data-inbox-pick="${qi}:${oi}" data-id="${esc(it.id)}">${lit && oi < 9 ? `<span class="ib-kbd">${oi + 1}</span>` : `<span></span>`}<b>${esc(label)}</b>${rec ? `<span class="ib-rec">Recommended</span>` : ""}${o.description ? `<span class="ib-od">${esc(o.description)}</span>` : ""}</button>`;
  }).join("")}</div>`;
}
/** What was picked for a question, in words. */
function inboxPicked(it, q) {
  const c = draft(it.id).choices[q.question];
  return inboxLabel(q.multiSelect ? [...(c || [])].join(", ") : c || "")[0];
}
/** One question as a card: the one to answer now lit, an answered one folded to its answer (a click opens it again),
 *  the ones after it waiting, their choices in a line. */
function inboxQuestionCard(it, q, qi, n, cur) {
  const done = isAnswered(it, q), on = qi === cur;
  const head = n > 1 ? `<div class="ib-qn">${qi + 1} of ${n}${done ? " · answered" : ""}${q.header ? ` · ${esc(q.header)}` : ""}</div>` : "";
  const text = `<div class="ib-qt">${esc(q.question)}${q.multiSelect ? `<span class="ib-dim"> · pick any</span>` : ""}</div>`;
  if (done && !on) return `<div class="ib-card ib-q done" role="button" data-inbox-step="${esc(it.id)}:${qi}">${head}${text}<div class="ib-qa">→ ${esc(inboxPicked(it, q))}</div></div>`;
  if (!on && qi > cur) return `<div class="ib-card ib-q wait" role="button" data-inbox-step="${esc(it.id)}:${qi}">${head}${text}<div class="ib-dim">${esc((q.options || []).map((o) => inboxLabel(o.label)[0]).join(" · "))}</div></div>`;
  return `<div class="ib-card ib-q ${on ? "on" : ""}" ${on ? "" : `data-inbox-step="${esc(it.id)}:${qi}"`}>${head}${text}${inboxOpts(it, q, qi, on)}</div>`;
}
/** A question ask: [its questions stacked as cards; a box for your own answer to the one lit; Decline and, with
 *  several, one Send for all of them (answersFor, as the Board's Send answers)]. */
function inboxQuestions(it) {
  const qs = questions(it), n = qs.length, cur = stepOf(it), ready = !!answersFor(it);
  const cards = qs.map((q, qi) => inboxQuestionCard(it, q, qi, n, cur)).join("");
  const send = n > 1 || qs.some((q) => q.multiSelect) ? `<button class="btn primary" data-act="submit-answers" data-id="${esc(it.id)}" ${ready ? "" : "disabled"}>${ready ? `Send ${n > 1 ? `${n} answers` : "answer"}` : `${qs.filter((q) => isAnswered(it, q)).length} of ${n} answered`}</button>` : "";
  const box = inboxReply(it.session_id, it.id, n > 1 ? `Your own answer to question ${cur + 1}…` : "Or say something else…", n > 1 ? "Answer" : "Send", `data-act="send" data-id="${esc(it.id)}"`);
  return [cards, box, `<button class="btn deny ib-decline" data-act="deny" data-id="${esc(it.id)}">Decline</button>${send}`];
}
/** Any other ask (a command to run, an edit, a plan, a form, its terminal's): the Board's own card body and buttons. */
const inboxAskCard = (it) => `<div class="ib-card on"><div class="ib-qn">${esc(verb(it).replace(/^./, (c) => c.toUpperCase()))}</div>${requestBody(it)}</div>${decisionButtons(it)}`;
/** A finished turn's question (or that it was interrupted) as a card. */
function inboxTurnCard(r, q) {
  const what = r.it.interrupted ? "Interrupted" : "It asks";
  return `<div class="ib-card on"><div class="ib-qn">${what}</div><div class="ib-qt">${esc(q || "It stopped mid-turn. Say “continue”, or what to do instead.")}</div></div>`;
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
/** A working session: what it's on now, and its bar. */
function inboxRunCard(r) {
  const [, line] = inboxRunLines(r);
  return `<div class="ib-card run"><div class="ib-qn">Now</div><div class="ib-qt">${esc(line)}</div>${inboxBar(r.s)}</div>`;
}
/** Whether it takes a written reply here: a question, a finished turn, one at work or done (not a permission or the
 *  context row, which have their own buttons). */
const inboxCanReply = (r) => !!r && (r.kind === "turn" || r.kind === "crew" || r.kind === "run" || r.kind === "done" || (r.kind === "ask" && r.it.kind === "question"));
/** The line under the box: the keys that work here (1–N for the question lit, N for the next ask), then `acts` at
 *  its end. */
function inboxHint(r, acts = "") {
  const it = r.kind === "ask" && r.it.kind === "question" ? r.it : null, n = it ? Math.min(9, questions(it)[stepOf(it)]?.options?.length || 0) : 0;
  const keys = [n ? `<span><span class="ib-kbd">1</span>–<span class="ib-kbd">${n}</span> pick</span>` : "", inboxNeedOrder().some((x) => x !== r.sid) ? `<span><span class="ib-kbd">N</span> next ask</span>` : ""].filter(Boolean);
  return keys.length || acts ? `<div class="ib-hint">${keys.join("")}<span class="grow"></span>${acts}</div>` : "";
}
/** A lead's crew that waits on you, a row each with its own buttons, as on its crew's page (team.js): a teammate's ask
 *  with Deny and Allow, a review ready with Commit or Send fixes. Empty for any other session. */
function inboxCrewHtml(sid) {
  const waits = typeof mateWaits === "function" ? mateWaits(sid) : [];
  if (!waits.length) return "";
  return `<div class="ib-crew"><div class="ib-k">Its crew · ${waits.length} need${waits.length === 1 ? "s" : ""} you</div>${waits.map((t) => (mateNeeds(sid, t) ? mateNeedRow(sid, t) : mateReadyRow(sid, t))).join("")}</div>`;
}
/** The brief's main column: its words before the question, the question with its choices, what it plans next, then
 *  the box to answer in (at the foot). */
function inboxMain(r) {
  const words = inboxWords(r), q = r.kind === "turn" && !r.it.interrupted ? inboxEndQuestion(words) : "";
  const then = r.kind === "run" ? "" : inboxThen(words), said = inboxSaid(words, [q, then]);
  const label = r.kind === "run" ? "It said last" : r.kind === "ask" || r.kind === "turn" ? "It says" : "It said";
  const saidHtml = said ? `<div class="ib-said"><div class="ib-k">${label}</div><div class="ib-md msg">${md(said, { pathsOf: r.sid })}</div></div>` : "";
  const thenHtml = then ? `<div class="ib-then">It plans next: <b>${esc(plain(then))}</b></div>` : "";
  const reply = (ph) => inboxReply(r.sid, `s:${r.sid}`, ph, "Send", r.it ? `data-act="send" data-id="${esc(r.it.id)}"` : `data-inbox-send="${esc(r.sid)}"`);
  let top = "", foot = "", acts = "";
  if (r.kind === "ask" && r.it.kind === "question") [top, foot, acts] = inboxQuestions(r.it);
  else if (r.kind === "ask") top = inboxAskCard(r.it);
  else if (r.kind === "turn") { top = q || r.it.interrupted ? inboxTurnCard(r, q) : ""; foot = reply("Reply…"); }
  else if (r.kind === "chore") top = inboxChoreCard(r);
  else if (r.kind === "run") { top = inboxRunCard(r); foot = reply("Tell it something (it reads it when it can)…"); }
  else foot = reply("Tell it what's next…");
  const crew = inboxCrewHtml(r.sid);
  const first = r.kind === "run" || r.kind === "chore" || r.kind === "crew" ? `${crew}${top}${saidHtml}` : `${saidHtml}${top}${crew}`;
  return `<div class="ib-page-body ib-main" data-scroll-key="inbox-main">${first}${thenHtml}<div class="ib-foot">${foot}${inboxHint(r, acts)}</div></div>`;
}

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
  const you = (r.s?.thread || []).filter((e) => e.role === "you").at(-1), turn = inboxTurn(r.sid);
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
  const line = (l) => `<div class="ib-step ${l.bad ? "bad" : ""} ${l.live ? "now" : ""}"><i></i><span class="ib-sv">${l.what}</span><span class="ib-sr">${l.live ? "" : esc(l.result || "")}</span></div>`;
  const earlier = more ? `<div class="ib-step-more">${more} earlier</div>` : "";
  return `<div class="ib-blk"><div class="ib-k">It did · ${steps.length} step${steps.length === 1 ? "" : "s"}</div><div class="ib-steps">${earlier}${shown.map(line).join("")}</div></div>`;
}
/** What its turn has changed so far, read from git against where the turn began: Cue clears a session's changes as a
 *  turn starts and counts them as it ends, so a question mid-turn would show none. Read once per turn and state
 *  (on open); null when Cue has no start for its turn. */
function inboxTurnDiff(sid) {
  const s = extSessionInfo(sid), key = s?.turn_from?.commit ? JSON.stringify([s.turn_from.commit, s.state]) : "";
  const had = inboxTurnDiffs.get(sid);
  if (!key) return null;
  if (had?.key === key) return had;
  const rec = { key, files: had?.files || null, bulk: had?.bulk || [] };
  inboxTurnDiffs.set(sid, rec);
  diffSince(s, s.turn_from).then((g) => { rec.files = g?.files || []; rec.bulk = g?.bulk || []; }, () => { rec.files = rec.files || []; })
    .then(() => { if (inboxTurnDiffs.get(sid) === rec && view === "inbox") renderMain(); });
  return rec;
}
/** The biggest change in a file: its largest hunk, from just before its first changed line, a few lines long. */
function inboxPeek(f) {
  const hunks = [];
  for (const l of f.lines || []) { if (l[0] === "h" || !hunks.length) hunks.push([]); hunks.at(-1).push(l); }
  const size = (h) => h.filter(([k]) => k === "a" || k === "d").length, big = hunks.sort((a, b) => size(b) - size(a))[0] || [];
  const at = Math.max(0, big.findIndex(([k]) => k === "a" || k === "d") - 1), head = big[0]?.[0] === "h" ? [big[0]] : [];
  const body = big.slice(Math.max(at, head.length), Math.max(at, head.length) + INBOX_PEEK_LINES);
  return [...head, ...body].map(([k, l]) => `<div class="${k}">${esc(l)}</div>`).join("");
}
/** "Changed this turn": its files with +/−, a peek of the biggest change, and the rest on its Diff tab. */
function inboxChangedBlock(r) {
  const d = inboxTurnDiff(r.sid), sid = esc(r.sid);
  const files = d?.files ? [...d.files].sort((a, b) => b.add + b.del - (a.add + a.del)) : (r.s?.changes?.files || []).map((f) => ({ ...f, lines: [] }));
  if (d && !d.files) return `<div class="ib-blk"><div class="ib-k">Changed this turn</div><div class="ib-dim">Reading git…</div></div>`;
  // Mid-turn with no start to read from, it can't be known: left out.
  const mid = r.kind === "ask" || r.kind === "run";
  if (!files.length) return d || !mid ? `<div class="ib-blk"><div class="ib-k">Changed this turn</div><div class="ib-dim">${mid ? "Nothing yet." : "No files changed."}</div></div>` : "";
  const add = files.reduce((n, f) => n + (f.add || 0), 0), del = files.reduce((n, f) => n + (f.del || 0), 0);
  const row = (f) => `<div class="ib-file"><code>${esc(f.path)}</code><span>${f.isNew ? `<span class="ib-dim">new</span> ` : ""}<span class="ib-add">+${f.add || 0}</span> <span class="ib-del">−${f.del || 0}</span></span></div>`;
  const peek = files[0].lines?.length ? `<div class="ib-peek"><div class="ib-peek-h"><code>${esc(files[0].path)}</code></div><div class="dv-lines">${inboxPeek(files[0])}</div></div>` : "";
  const rest = files.length > INBOX_FILES_MAX ? `<button class="ib-link" data-inbox-diff="${sid}">and ${files.length - INBOX_FILES_MAX} more files ›</button>` : "";
  return `<div class="ib-blk"><div class="ib-k">Changed this turn · ${files.length} file${files.length === 1 ? "" : "s"} <span class="ib-add">+${add}</span> <span class="ib-del">−${del}</span></div>
    <div class="ib-files">${files.slice(0, INBOX_FILES_MAX).map(row).join("")}</div>${peek}${rest}</div>`;
}
/** The brief's side column, in the order that says why it asks: You asked, It did, what changed, its context, and
 *  its full chat and diff as links. */
function inboxAside(r) {
  const you = inboxYouAsked(r), sid = esc(r.sid);
  const asked = you ? `<div class="ib-blk"><div class="ib-k">You asked${you.at ? ` · ${ago(you.at)} ago` : ""}</div><div class="ib-you">${esc(plain(you.text))}</div></div>` : "";
  const ctx = inboxCtx(r.s) ? `<div class="ib-blk"><div class="ib-k">Context</div><div class="ib-ctx">${inboxMeter(r.s, "wide")}<span>${inboxCtx(r.s)}%</span></div></div>` : "";
  const links = `<div class="ib-links"><button class="ib-link" data-inbox-diff="${sid}">Diff</button></div>`;
  return `<aside class="ib-page-body ib-aside" data-scroll-key="inbox-aside">${asked}${inboxDid(r)}${inboxChangedBlock(r)}${ctx}${links}</aside>`;
}

// ---------- the brief, put together ----------

/** What it's at, in a few words, for the brief's head ("asks you · 12s"). */
function inboxStatus(r) {
  const t = ago(r.at), n = r.kind === "ask" && r.it.kind === "question" ? questions(r.it).length : 0;
  if (r.kind === "ask") return `${n > 1 ? `${n} questions` : n ? "asks you" : verb(r.it)} · ${t}`;
  if (r.kind === "turn") return `${r.it.interrupted ? "interrupted" : "your turn"} · ${t}`;
  if (r.kind === "chore") return `context ${inboxCtx(r.s)}% full`;
  if (r.kind === "crew") return "its crew needs you";
  if (r.kind === "run") return `working · ${ago(r.s?.since_ms || r.at)}`;
  return `finished · ${t}`;
}
/** "2 of 6 ›" to the next that needs you ("next ask ›" from one that doesn't), when there's another. */
function inboxStep(sid) {
  const all = inboxNeedOrder(), at = all.indexOf(sid);
  if (!all.some((x) => x !== sid)) return "";
  return `<button class="ib-next" data-inbox-next>${at >= 0 ? `${at + 1} of ${all.length} ›` : "next ask ›"}</button>`;
}
/** The open session beside the list: its head (its dot, name and what it's at; Pin, Park, Snooze; the next ask), then
 *  the brief in two columns (stacked when narrow). No hover text: a tooltip the reused parts carry is left out. */
function inboxOpened(sid) {
  const r = inboxRowFor(sid), s = r.s;
  const head = `<div class="ib-head"><button class="ib-back" data-inbox-back aria-label="Back to the list">‹</button><span class="ib-dot">${inboxDot(r)}</span><span class="ib-hname">${esc(nameOf(sid, r.project))}</span><span class="ib-status">${esc(inboxStatus(r))}</span><span class="grow"></span>`
    + `<span class="ib-hacts"><button class="ib-act" data-inbox-full="${esc(sid)}">Open chat</button>${inboxActs(sid, s, "head")}</span>${inboxStep(sid)}</div>`;
  const body = `<div class="ib-brief" data-scroll-key="inbox-brief">${inboxMain(r)}${inboxAside(r)}</div>`;
  return `<div class="inbox-page" data-inbox-page="${esc(sid)}" data-kind="${r.kind}" style="${projStyle(r.project)}">${head}${body}</div>`.replace(/\stitle="[^"]*"/g, "");
}
/** A session on the calm page: its name, what you asked and what it's on (a working one's bar along its foot), or
 *  what it said and changed. Opens it. */
function inboxCalmCard(r) {
  const [subj, line] = r.kind === "run" ? inboxRunLines(r) : inboxDoneLines(r);
  return `<div class="ib-calm-card ${r.kind}" role="button" tabindex="0" data-inbox-open="${esc(r.sid)}" style="${projStyle(r.project)}"><div class="ib-calm-t"><span class="ib-dot">${inboxDot(r)}</span>${nameSpan(r.sid, r.project, "ib-name")}<span class="grow"></span><span class="ib-time">${ago(r.at)}</span></div>`
    + `<div class="ib-calm-s">${esc(subj)}</div>${line ? `<div class="ib-calm-l">${esc(line)}</div>` : ""}${r.kind === "run" && r.s ? inboxBar(r.s) : ""}</div>`;
}
/** Beside the list with nothing open. Nothing needs you: what's running and what's done lately, each a card that
 *  opens it. Something does (a narrow window, or the Snoozed tab): how many, and how to start. */
function inboxIdle() {
  const n = inboxCount();
  if (n) return `<div class="ib-idle"><b>${n} need${n === 1 ? "s" : ""} you</b><span>Pick one on the left, or press N for the first.</span></div>`;
  const rows = inboxRows().filter((r) => inboxTabOf(r.s) === "inbox" && !inboxParked(r.s)).sort((a, b) => b.at - a.at);
  const run = rows.filter((r) => r.kind === "run"), done = rows.filter((r) => r.kind === "done").slice(0, INBOX_DONE_MAX);
  const group = (title, xs) => (xs.length ? `<div class="ib-k">${title} · ${xs.length}</div><div class="ib-calm-grid">${xs.map(inboxCalmCard).join("")}</div>` : "");
  const words = run.length ? "Running sessions come up here when they ask or finish." : "Nothing is working either.";
  return `<div class="ib-idle ib-calm"><div class="ib-calm-head"><b>Nothing needs you</b><span>${words}</span></div><div class="ib-calm-body ib-page-body" data-scroll-key="inbox-calm">${group("Running", run)}${group("Done lately", done)}</div></div>`;
}

// ---------- moving about ----------

/** Whether you're done with the open session: you answered it or wrote to it here since it opened (from a row that
 *  needed you or was done; a working one keeps its brief), or it needed you while open and is at work again. Not
 *  while you type in its box (that would take the box from under you). */
function inboxAnswered(sid) {
  const r = inboxRowFor(sid), it = inboxItems().get(sid);
  if (r && r.kind !== "run") { inboxAsked = true; return false; }
  // Just sent from its box: the box still holds the words until this draw clears it.
  const wrote = (it && (answering.get(it.id) || 0) >= inboxOpenedAt) || outbox.some((o) => o.sid === sid && o.at >= inboxOpenedAt + clockSkew);
  if (wrote && inboxAsked) return true;
  const f = document.activeElement;
  return inboxAsked && !(f?.matches?.("textarea, input") && f.value);
}
/** A window wide enough for the list and the brief side by side: there, something is always open. */
const inboxWide = () => typeof matchMedia !== "function" || matchMedia("(min-width: 821px)").matches;
/** The Inbox view: the list on the left, the open session beside it. With none open (a wide window), the top one that
 *  needs you opens by itself; once the open one is answered, the next that needs you opens in its place. */
function inboxView() {
  const sid = inboxSid;
  if (sid && ((!sessionOf(sid) && !inboxItems().has(sid)) || inboxAnswered(sid))) inboxAdvance(sid);
  else if (!sid && inboxWide()) inboxAdvance(null);
  return `<div class="board ib-board ${inboxSid ? "open" : ""}">${inboxList()}${inboxSid ? inboxOpened(inboxSid) : inboxIdle()}</div>`;
}
/** Past `sid` (answered, or gone; null: none open): the next session that needs you is the open one, else none. Drawn
 *  as it is. */
function inboxAdvance(sid) {
  const next = inboxTab === "inbox" ? inboxNeedOrder().find((x) => x !== sid) : null;
  if (!next && !sid) return;
  inboxSid = inboxLast = next || null;
  inboxAsked = false;
  inboxOpenedAt = Date.now();
  inboxSnoozeAt = "";
  if (next) { active = { id: inboxItems().get(next)?.id || null, sid: next, exact: false }; seenSession(next); }
}
/** The sessions that need you, in the list's order (Pinned first). */
function inboxNeedOrder() {
  const seat = (sid) => inboxSeats.get(sid), rows = inboxNeedRows();
  const pin = rows.filter((r) => inboxPinned(r.s)), rest = rows.filter((r) => !inboxPinned(r.s));
  const order = (xs) => xs.sort((a, b) => (seat(b.sid)?.n || 0) - (seat(a.sid)?.n || 0) || b.at - a.at).map((r) => r.sid);
  return [...order(pin), ...order(rest)];
}
/** Open session `sid` beside the list, on what it asks (the Board's keys and buttons act on what's shown). */
function inboxOpen(sid) {
  const it = inboxItems().get(sid) || null;
  view = "inbox";
  active = { id: it?.id || null, sid, exact: false };
  seenSession(sid);
  inboxSid = inboxLast = sid;
  inboxAsked = false;
  inboxOpenedAt = Date.now();
  inboxSnoozeAt = "";
  renderMain();
}
/** Write to the open session: the cursor into its box (R does it too). */
function inboxReplyTo(sid) {
  renderMain();
  document.querySelector(`.inbox-page[data-inbox-page="${CSS.escape(sid)}"] .ib-box textarea[data-text]`)?.focus();
}
/** Back to the list (a narrow window shows one at a time). */
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

// ---------- pin, park, snooze ----------

/** Pin it to the top of the Inbox, or unpin it: shown now, kept with the session in Cue. */
function inboxSetPin(sid, on) {
  const s = sessionOf(sid);
  if (s) s.pinned_ms = on ? now() : 0;
  invoke("set_pinned", { sessionId: sid, on }).catch((e) => toast(`Couldn't: ${e}`));
  renderMain();
}
/** Park it (the Board's parked: under Parked at the foot, no notification), or unpark it. */
function inboxSetPark(sid, on) {
  const s = sessionOf(sid);
  if (s) s.later_ms = on ? now() : 0;
  invoke("set_later", { sessionId: sid, on }).catch((e) => toast(`Couldn't: ${e}`));
  if (on) toast(`${nameOf(sid, s?.project)} is parked`);
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
/** A session just parked or snoozed: if it was open, the next that needs you opens in its place. */
function inboxGone(sid, gone) {
  if (gone && inboxSid === sid) { inboxSid = null; inboxAsked = false; inboxAdvance(sid); }
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

/** Keys while the Inbox shows. The open session: Esc (a narrow window) back to the list, a Snooze menu first; N or →
 *  the next that needs you, ← the one before, 1–9 a choice of the question lit, R into the reply (Esc out of an empty one). None open: N opens the first that
 *  needs you; the list: the Board's card keys (A, D, G, R, 1–9) are kept from a card not on screen. Not while you type
 *  in a box with text in it, or something is open over it. True when it took the key. */
function inboxKey(e) {
  if (view !== "inbox" || e.metaKey || e.ctrlKey || e.altKey || e.isComposing) return false;
  if (sheet || lightbox || liveOpen || deleteAsk || renaming || btwFor || menuFor || redirectFor || (typeof moreFor !== "undefined" && moreFor)) return false;
  const f = document.activeElement, typing = f?.matches?.("textarea, input, select");
  if (e.key === "Escape" && inboxSnoozeAt) { inboxSnoozeAt = ""; renderMain(); return true; }
  if (!inboxSid && e.key === "Enter" && f?.matches?.(".ib-row, .ib-calm-card")) { inboxOpen(f.dataset.inboxOpen); return true; }
  if (!inboxSid && !typing && e.key === "n") { const first = inboxNeedOrder()[0]; if (first) inboxOpen(first); return true; }
  if (!inboxSid) return !typing && /^[adgr1-9]$/.test(e.key);
  if (e.key === "Escape" && !e.shiftKey && typing && !f.value && f.closest?.(".inbox-page")) { f.blur(); return true; }
  if (e.key === "Escape" && !e.shiftKey) { if ((typing && f.value) || inboxWide()) return false; f?.blur?.(); inboxClose(); return true; }
  if (typing) return false;
  if (e.key === "r" && inboxCanReply(inboxRowFor(inboxSid))) { e.preventDefault(); inboxReplyTo(inboxSid); return true; }
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
  inboxPark: (el) => inboxSetPark(el.dataset.inboxPark, !inboxParked(sessionOf(el.dataset.inboxPark))),
  inboxSnooze: inboxSnoozeClick,
  inboxSnoozeTo: (el) => inboxSetSnooze(el.dataset.sid, +el.dataset.inboxSnoozeTo),
  inboxTab: (el) => { inboxTab = el.dataset.inboxTab; inboxSnoozeAt = ""; renderMain(); },
  inboxFold: () => inboxFoldPark(),
  inboxPick,
  inboxStep: (el) => { const [id, n] = el.dataset.inboxStep.split(/:(?=\d+$)/); qStep[id] = +n; renderMain(); },
  inboxSend: (el) => sendTo(el.dataset.inboxSend),
  inboxFull: (el) => inboxFullChat(el.dataset.inboxFull),
  inboxPhrase: inboxSendPhrase,
  inboxSug: inboxTakeSuggestion,
  inboxDiff: (el) => inboxDiff(el.dataset.inboxDiff),
  inboxBack: () => inboxClose(),
  inboxNext: () => inboxStepTo(1),
  inboxOpen: (el) => inboxOpen(el.dataset.inboxOpen),
};
/** The attribute selector for each of INBOX_CLICKS ("inboxSnoozeTo" → "[data-inbox-snooze-to]"), innermost first. */
const INBOX_SEL = Object.keys(INBOX_CLICKS).map((k) => `[data-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}]`).join(", ");
/** A click in the Inbox: the innermost element with one of its attributes does its thing. */
function inboxClick(e) {
  // Inbox in the header while a session is open in it: the top one that needs you opens (the click goes on to switch
  // the view).
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
