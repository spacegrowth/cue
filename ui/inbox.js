// ---------- Inbox: where you see what a session asks, and answer it (the header's Inbox, after Board) ----------
// The list: a row per session, two lines each (its name and what it asks or is on; then why, or what it found), under
// a thin Running strip at the top (folded: "Running · 3" and a small moving bar for each), then Pinned, Needs you, Finished replies
// and, folded at the foot, Later and Parked (the Board's parked: put off, no notification). A finished turn that ends on
// a question needs you; one that doesn't is Done. Context maintenance and optional reviews have their own sections.
// Answer one and it sinks to Running, into the strip; opened, a Running row has a bar moving
// along its foot (slower when it's quiet, amber and still when it's stuck); it rises again when it asks again or
// finishes. A row keeps
// its place while its state stays the same. Hovering a row shows Pin, Later and Snooze (Snoozed is a tab of its own).
// Beside the list, a brief of the session open: never empty, as the top one that needs you opens by itself (its row
// lit, where it is). Its main column has its own words before the question (the last paragraph or two), the question
// with each choice and what it means, what it plans next when it says, and a box to answer in; its side column what
// you asked it, what it did this turn (repeats folded: "Read 6 files"), what this turn changed (a peek of the biggest
// change) and its context. After Send it stays open, as a sent email does; you move on. With nothing needing you, what's running and
// what's done lately stand in. With a project picked in the header, only its sessions.

/** The session open beside the list; null when none is (nothing needs you, or a narrow window shows the list). */
let inboxSid = null;
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
/** Briefly show idle sessions without a retained reply too. */
const INBOX_DONE_MS = 2 * 3600000;
/** Context this full (percent): its row shows a meter, and a session at rest comes up to compact or hand off. */
const INBOX_FULL = 50;
/** A working session with no new step for this long: its bar slows. */
const INBOX_QUIET_MS = 2 * 60000;
/** A row that came into Running this recently is still sinking: drawn faded. */
const INBOX_SINK_MS = 8000;
/** The Running strip shows this many small bars at most, then "+N". */
const INBOX_STRIP_BARS = 8;
/** "It says": the last paragraphs of its words before the question, at most this many, this long in all (characters). */
/** Its reply shows whole up to INBOX_SAID_LONG characters; a longer one shows its last paragraphs up to INBOX_SAID_MAX. */
/** An earlier message longer than this (characters, about two lines here) is cut: it says "more". */
const INBOX_MSG_CUT = 180;
const INBOX_SAID_LONG = 2500, INBOX_SAID_MAX = 1200;
/** "It did": this many lines at most (a folded run is one); the earlier ones are counted above them. */
const INBOX_STEPS_MAX = 6;
/** "Changed": this many files listed, and this many lines of the biggest change shown. */
const INBOX_FILES_MAX = 4, INBOX_PEEK_LINES = 6;
/** A sentence where it says what it does next: "Then I'll …", "Next I'd …", "After that, …". */
const INBOX_THEN = /^(?:then|next|after that|afterwards)\b|\bI'll (?:then|next)\b/i;

// ---------- what each session is, as a row ----------

/** Its marks, as Cue keeps them with the session. */
const inboxPinned = (s) => !!s?.pinned_ms;
/** Later: the Board's Later on a card (its "later" mark), put off with no notification. */
const inboxLater = (s) => !!s?.later_ms;
/** Snoozed until its time, unless it has asked something since it was snoozed (then it's back, as a new reply wakes
 *  a snoozed mail). */
const inboxSnoozed = (s) => (s?.snooze_ms || 0) > now() && !state.items.some((i) => i.status === "pending" && i.session_id === s.session_id && i.created_ms > (s.snoozed_at_ms || 0));
const inboxCtx = (s) => s?.context_pct || 0;
const inboxFull = (s) => inboxCtx(s) >= INBOX_FULL;
/** The tab a session is listed under. */
const inboxTabOf = (s) => (inboxSnoozed(s) ? "snoozed" : "inbox");

/** A reply's last line, plain ("Recommendation: send fixes"). */
const inboxLastLine = (text) => plain(String(text || "").split("\n").map((l) => l.trim()).filter(Boolean).at(-1) || "");
/** A lead's crew that waits on you, in a few words ("2 of its crew need you"). */
const inboxCrewSubject = (sid) => { const n = inboxCrew(sid); return `${n} of its crew need${n === 1 ? "s" : ""} you`; };
/** Who in its crew waits, and on what ("v5-loop: send fixes · at-picker: asks"). */
const inboxCrewLine = (sid) => inboxCrewActions(sid).map((t) => `${t.name}: ${mateAsk(sid, t) ? verb(mateAsk(sid, t)) : t.needs}`).join(" · ");
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
    if (unresolvedCard(it) && inProj(it.project) && (!had || (had.kind === "waiting" && actionableRequest(it)))) by.set(it.session_id, it);
  }
  return by;
}
/** You've written to it from here and Cue hasn't shown it in its thread yet (not one that failed to go). */
const inboxSending = (sid) => outbox.some((o) => o.sid === sid && !/^not sent/.test(o.via || ""));
const inboxCrewWaits = (sid) => typeof mateWaits === "function" ? mateWaits(sid) : [];
const inboxCrewActions = (sid) => inboxCrewWaits(sid).filter((t) => actionableRequest(mateAsk(sid, t)) || (!!t.needs && !mateOnLead(t)));
const inboxCrew = (sid) => inboxCrewActions(sid).length;
/** A session's row. Only an open ask, a turn requiring a reply or continuation, or a crew blocker needs you.
 *  Reviews and housekeeping have their own sections. Pinned, later and snoozed sessions can stay visible. */
function inboxRowOf(sid, s, it, keep = false) {
  const row = (kind, at) => ({ sid, s, it, project: s?.project || it?.project, at, kind });
  if (actionableRequest(it)) return row("ask", it.created_ms);
  if (it?.kind === "waiting" && !inboxSending(sid) && (it.interrupted || inboxEndQuestion(finishedText(it)))) return row("turn", it.created_ms);
  if (inboxCrew(sid)) return row("crew", s.since_ms || 0);
  if (s && state.items.some((i) => i.session_id === sid && i.status === "pending" && answering.has(i.id))) return row("run", now());
  if (s && (s.state === "working" || s.state === "agent" || s.compacting_ms || inboxSending(sid))) return row("run", s.since_ms || 0);
  if (s && inboxCrewWaits(sid).some((t) => !mateNeeds(sid, t))) return row("review", s.since_ms || 0);
  if (s && inboxFull(s) && !cutOff(s)) return row("chore", s.since_ms || 0);
  const recent = s && !hiddenIdle.has(idleKey(s)) && now() - (s.since_ms || 0) < INBOX_DONE_MS;
  const finished = it?.kind === "waiting" || (s?.state === "waiting" && lastSaid(s));
  return finished || recent || keep ? row("done", it?.created_ms || s?.since_ms || 0) : null;
}
/** Every row the Inbox has, any tab. Closed and ended sessions aren't in Cue's list, so never here. */
function inboxRows() {
  const items = inboxItems(), rows = [];
  for (const [sid, it] of items) {
    const s = sessionOf(sid);
    const r = inboxRowOf(sid, s, it, inboxPinned(s) || inboxLater(s) || inboxSnoozed(s));
    if (r) rows.push(r);
  }
  for (const s of state.sessions) {
    if (items.has(s.session_id) || !inProj(s.project) || cutOff(s)) continue;
    const r = inboxRowOf(s.session_id, s, null, inboxPinned(s) || inboxLater(s) || inboxTabOf(s) !== "inbox");
    if (r) rows.push(r);
  }
  return rows;
}
/** The open session's row (whatever its tab), or null once it's gone. */
const inboxRowFor = (sid) => inboxRowOf(sid, sessionOf(sid), inboxItems().get(sid) || null, true);
/** The section a row goes under: Parked whatever it's at; else by what it is. */
const inboxSectOf = (r, respectLater = true) => (respectLater && inboxLater(r.s) ? "later" : ["ask", "turn", "crew"].includes(r.kind) ? "need" : r.kind);
/** The rows needing you under the Inbox tab, a session once each (parked ones wait in Parked). */
const inboxNeedRows = () => inboxRows().filter((r) => inboxTabOf(r.s) === "inbox" && inboxSectOf(r) === "need");
/** How many sessions need you, for the header's "Inbox 3". */
const inboxCount = () => inboxNeedRows().length + inboxDeskVerifications().length;

/** `rows` (newest first) in the order they keep: one stays where it was while it stays in `sect`; one that came in
 *  goes on top. */
function inboxSeated(sect, rows) {
  const fresh = rows.filter((r) => inboxSeats.get(r.sid)?.sect !== sect);
  for (const r of fresh.reverse()) inboxSeats.set(r.sid, { sect, n: ++inboxSeq, at: now(), from: inboxSeats.get(r.sid)?.sect || "" });
  return rows.sort((a, b) => inboxSeats.get(b.sid).n - inboxSeats.get(a.sid).n);
}
/** The open tab's rows by section: Running, Pinned, each Desk's asks, Needs you, then non-actionable sections.
 *  (Pinned, Later and Parked on the Inbox tab only; the Snoozed tab lists its rows as they are). */
function inboxSections() {
  const rows = inboxRows().filter((r) => inboxTabOf(r.s) === inboxTab).sort((a, b) => b.at - a.at);
  const box = inboxTab === "inbox", parked = box ? rows.filter((r) => inboxLater(r.s)) : [];
  const pinned = box ? rows.filter((r) => inboxPinned(r.s) && !parked.includes(r)) : [];
  const rest = rows.filter((r) => !pinned.includes(r) && !parked.includes(r));
  const of = (k) => rest.filter((r) => inboxSectOf(r, box) === k);
  const [desks, need] = inboxByDesk(box ? of("need") : []);
  return [["run", "Running", inboxSeated("run", of("run"))], ["pin", "Pinned", inboxSeated("pin", pinned)], ...desks, ["need", "Needs you", inboxSeated("need", box ? need : of("need"))], ["review", "Reviews", inboxSeated("review", of("review"))], ["chore", "Housekeeping", inboxSeated("chore", of("chore"))], ["done", "Finished replies", of("done")], ["later", "Later", inboxSeated("later", parked)], ["parked", "Parked", inboxParkedRows()]];
}
/** Actionable session rows grouped by Desk, with the remaining rows under Needs you. */
function inboxByDesk(need) {
  const by = new Map();
  const looseNeed = need.filter((r) => {
    const d = deskForSession(r.sid);
    if (!d) return true;
    if (!by.has(d.project)) by.set(d.project, { d, need: [] });
    by.get(d.project).need.push(r);
    return false;
  });
  const groups = [...by.values()].sort((a, b) => b.need.length - a.need.length || deskName(a.d).localeCompare(deskName(b.d)));
  const secs = groups.map((g) => ["desk", deskName(g.d), inboxSeated(`desk:${g.d.project}:need`, g.need), g.need.length]);
  return [secs, looseNeed];
}

// ---------- a row's words ----------

/** What it did lately, in a line: the first line of its last reply. */
function inboxSoFar(r) {
  return plain(firstLine(r.it?.kind === "waiting" ? finishedText(r.it) : lastSaid(r.s || {})));
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
function inboxRunLines(r, card = false) {
  const s = r.s, seat = inboxSeats.get(r.sid), sent = outbox.findLast((o) => o.sid === r.sid)?.text, said = plain(sent || s.prompt);
  const answered = r.it && answering.has(r.it.id) && !sent ? `You answered: ${r.it.kind === "waiting" ? inboxEndQuestion(finishedText(r.it)) || "its reply" : inboxAskSubject(r.it)}` : "";
  const subj = answered || (said ? (sent || seat?.from === "need" ? `You: “${said}”` : said) : "Working");
  const stuck = s.stuck_ms && !s.compacting_ms ? `No new output for ${ago(s.stuck_ms)}` : "";
  const busy = s.state === "working" || s.state === "agent" || s.compacting_ms;
  const line = s.compacting_ms ? "Compacting…" : preStart(s) ? `Not started: ${preStart(s)}` : stuck || (card ? "" : inboxPendingLine(s)) || (!busy ? (!card && inboxTaken(s)) || "Sent" : s.state === "agent" ? idleNote(s) : s.doing || "Thinking");
  return [subj, line];
}
/** Your last message to session `s`, once it took it: "Delivered to <name> 14:12" (#422); "" before that. */
function inboxTaken(s) {
  const e = (s.thread || []).findLast((x) => x.role === "you");
  if (!e || e.pending || e.unsent || outbox.some((o) => o.sid === s.session_id && !landed(o))) return "";
  return `Delivered to ${nameOf(s.session_id, s.project)} ${clockAt(msgTaken(e))}`;
}
/** A finished row's lines: how it ended and what it said, then what it did. */
function inboxDoneLines(r) {
  const s = r.s, said = plain(firstLine(r.it ? finishedText(r.it) : lastSaid(s || {})));
  const ended = !r.it && s?.state === "stopped" ? "Stopped" : !r.it && s?.state === "limited" ? "Out of usage" : "";
  const subj = ended ? `${ended}${said ? `: ${said}` : ""}` : said || "Finished";
  return [subj, inboxChanged(s) || ""];
}
/** A row's two lines: [subject, the useful line under it]. */
function inboxLines(r) {
  if (r.kind === "ask") return [inboxAskSubject(r.it), inboxWhy(r.it) || inboxSoFar(r) || (r.it.kind === "question" ? inboxQuestionLine(r.it) : "")];
  if (r.kind === "turn") return [r.it.interrupted ? "Interrupted: it stopped mid-turn" : inboxEndQuestion(finishedText(r.it)) || inboxLastLine(finishedText(r.it)) || "Your turn", inboxSoFar(r)];
  if (r.kind === "crew") return [inboxCrewSubject(r.sid), inboxCrewLine(r.sid)];
  if (r.kind === "review") return ["Crew review available", inboxSoFar(r)];
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
/** Pin, Later and Snooze (shown on hover), with Snooze's menu when it's open here (`at`). */
function inboxActs(sid, s, at) {
  if (!s) return "";
  const pin = inboxPinned(s), park = inboxLater(s), snz = inboxSnoozed(s);
  return `<button class="ib-act ${pin ? "on" : ""}" data-inbox-pin="${esc(sid)}" aria-pressed="${pin}">${PIN_ICON}${pin ? "Unpin" : "Pin"}</button>`
    + `<button class="ib-act" data-inbox-later="${esc(sid)}">${park ? "Not later" : "Later"}</button>`
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
  // Its sink plays once, from when it came in: a redraw carries on from there rather than starting it over.
  const sinking = kind === "run" && seat?.from === "need" && now() - seat.at < INBOX_SINK_MS;
  const under = inboxSnoozed(s) ? `Back ${inboxWhen(s.snooze_ms)}${line ? ` · ${line}` : ""}` : line;
  const side = `<span class="ib-time">${ago(r.at)}</span>${inboxPinned(s) ? `<span class="ib-pin">${PIN_ICON}</span>` : ""}${inboxFull(s) ? inboxMeter(s) : ""}`;
  return `<div class="ib-row ${kind} ${inboxLater(s) ? "later" : ""} ${sinking ? "sinking" : ""} ${(inboxSid || inboxLast) === sid ? "on" : ""}" role="button" tabindex="0" data-inbox-open="${esc(sid)}" style="${projStyle(r.project)}${sinking ? `;animation-delay:-${now() - seat.at}ms` : ""}">`
    + `<span class="ib-dot">${inboxDot(r)}</span><span class="ib-l1">${nameSpan(r.sid, r.project, "ib-name")}${inboxBadges(r)}<span class="ib-subj">${esc(subj)}</span></span>`
    + `<span class="ib-l2">${esc(under)}</span>${inboxForHtml(r)}<span class="ib-side">${side}</span>`
    + `<span class="ib-acts ${inboxSnoozeAt === `row:${sid}` ? "open" : ""}">${inboxActs(sid, s, `row:${sid}`)}</span>${kind === "run" && s ? inboxBar(s) : ""}</div>`;
}
/** Whether a collapsible Inbox section is open (remembered). */
const inboxFoldOpen = (key) => { try { return localStorage.getItem(`cue.inboxFold.${key}`) === "1"; } catch { return false; } };
/** Open or fold an Inbox section. */
function inboxFold(key) {
  try { localStorage.setItem(`cue.inboxFold.${key}`, inboxFoldOpen(key) ? "0" : "1"); } catch {}
  renderMain();
}
/** The Board's Parked (stopped and kept, chat/parked.js), newest first, in the project picked. */
const inboxParkedRows = () => (state.parked || []).filter((p) => inboxTab === "inbox" && inProj(p.project)).sort((a, b) => (b.parked_ms || 0) - (a.parked_ms || 0));
/** A parked session: its name, what it said last, how long it's been parked, and Resume. A click opens it beside the
 *  list, as any row. */
function inboxParkedHtml(p) {
  const sid = p.session_id, w = waking.has(sid), last = [...(p.thread || [])].reverse().find(askLnSaid);
  return `<div class="ib-row parked ${(inboxSid || inboxLast) === sid ? "on" : ""}" role="button" tabindex="0" data-inbox-open="${esc(sid)}" style="${projStyle(p.project)}"><span class="ib-dot">${P_SIGN}</span>`
    + `<span class="ib-l1"><span class="ib-name">${esc(p.name || baseName(p.cwd) || p.project || "session")}</span><span class="ib-subj">${w ? "Resuming…" : "Parked"}</span></span>`
    + `<span class="ib-l2">${esc(last ? plain(firstLine(last.text)) : "")}</span><span class="ib-side"><span class="ib-time">${p.parked_ms ? ago(p.parked_ms) : ""}</span></span>`
    + `<span class="ib-acts"><button class="ib-act" data-wake="${esc(sid)}" ${w ? "disabled" : ""}>↺ Resume</button></span></div>`;
}
/** The Running strip's bars: one small moving bar a session, in its project's color; past INBOX_STRIP_BARS, "+N". */
function inboxStripBars(rows) {
  const bars = rows.filter((r) => r.s).slice(0, INBOX_STRIP_BARS).map((r) => `<span class="ib-strip-b" style="${projStyle(r.project)}">${inboxBar(r.s)}</span>`).join("");
  const more = rows.length - INBOX_STRIP_BARS;
  return `<span class="ib-strip-bars">${bars}${more > 0 ? `<span class="ib-strip-more">+${more}</span>` : ""}</span>`;
}
/** A heading and its rows; nothing when it has none. Running's is a thin strip at the top, "Running · 3" and its bars,
 *  that unfolds its rows; Later's and Parked's headings fold them, with their counts. */
function inboxSection([key, title, rows, asking]) {
  if (!rows.length) return "";
  if (key === "run") {
    const open = inboxFoldOpen(key);
    return `<button class="ib-sect fold ib-strip" data-inbox-fold="run" aria-expanded="${open}"><span class="ib-fold">${open ? "▾" : "▸"}</span>${title} · <span class="ib-sect-n">${rows.length}</span>${inboxStripBars(rows)}</button>${open ? rows.map(inboxRowHtml).join("") : ""}`;
  }
  if (key === "desk") return `<div class="ib-sect ib-dg">${deskKindIcon("tray")}<span class="ib-dg-name">${esc(title)}</span>${asking ? `<span class="ib-dg-need">· ${asking} ${asking === 1 ? "needs" : "need"} you</span>` : ""}</div>${rows.map(inboxRowHtml).join("")}`;
  if (key === "done") {
    const open = inboxFoldOpen(key);
    return `<button type="button" class="ib-sect fold" data-inbox-fold="done" aria-expanded="${open}"><span class="ib-fold" aria-hidden="true">${open ? "▾" : "▸"}</span>${title} · <span class="ib-sect-n">${rows.length}</span></button>${open ? rows.map(inboxRowHtml).join("") : ""}`;
  }
  if (key !== "later" && key !== "parked") return `<div class="ib-sect ${key}">${key === "pin" ? PIN_ICON : ""}${title}</div>${rows.map(inboxRowHtml).join("")}`;
  const open = inboxFoldOpen(key), draw = key === "parked" ? inboxParkedHtml : inboxRowHtml;
  return `<button class="ib-sect fold" data-inbox-fold="${key}" aria-expanded="${open}"><span class="ib-fold">${open ? "▾" : "▸"}</span>${title} <span class="ib-sect-n">${rows.length}</span></button>${open ? rows.map(draw).join("") : ""}`;
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
  const secs = inboxSections(), of = (k) => secs.find(([key]) => key === k)[2];
  const verifications = inboxTab === "inbox" ? inboxDeskVerifications() : [];
  const need = inboxNeedRows().length + verifications.length, run = of("run").length;
  const counts = inboxTab === "inbox" ? [[need, "need you"], [run, "running"]].filter(([c]) => c).map(([c, w]) => `${c} ${w}`).join(" · ") : "";
  // With Desks above, what's on none sits under its own heading.
  const loose = secs.some(([k]) => k === "desk") && of("need").length ? `<div class="ib-sect ib-loose">Not on a Desk</div>` : "";
  const body = secs.map((sec) => (sec[0] === "need" ? loose : "") + inboxSection(sec)
    + (sec[0] === "need" && verifications.length ? `<div class="ib-sect">Verify in Cue</div>${inboxVerificationRows(verifications)}` : "")).join("");
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
/** Its words before the question (markdown), `cut` (the question, the sentence on what's next) taken out: whole, unless
 *  it's very long (over INBOX_SAID_LONG; `all` keeps it whole anyway): then its last paragraphs up to INBOX_SAID_MAX
 *  characters; one long paragraph is cut from its start. */
function inboxSaid(text, cut = [], all = false) {
  let t = String(text || "");
  for (const c of cut) { const at = c ? t.lastIndexOf(c) : -1; if (at >= 0) t = t.slice(0, at) + t.slice(at + c.length); }
  const paras = inboxParas(t).filter((p) => plain(p)), out = [];
  if (all || plain(t).length <= INBOX_SAID_LONG) return paras.join("\n\n").trim();
  for (const p of paras.reverse()) {
    if (out.length && out.join("").length + p.length > INBOX_SAID_MAX) break;
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
const inboxLabel = (label) => choiceLabel(label);
/** A plain box to write in (pictures pasted or dropped on it go with it), and its button. Enter sends too (app/keys.js,
 *  by its draft key). With `sid`, Claude's suggestion for that session shows in the box while it's empty (inboxSuggest). */
const inboxBox = (key, placeholder, label, attrs, extra = "", sid = "", phr = "") => {
  const sug = sid ? suggestionFor(sid) : "";
  return `<div class="ib-box" ${inboxPics(key) ? `data-drop="${esc(key)}"` : ""}>${inboxPics(key) ? draftChips(key) : ""}<div class="ta-wrap"><textarea rows="2" data-text="${esc(key)}"${sug ? ` class="sug" data-sug-sid="${esc(sid)}" aria-label="${esc(`Suggested: ${sug}. Tab types it in.`)}"` : ""} placeholder="${esc(sug ? " " : placeholder)}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea>${inboxSuggest(sug, key, sid)}</div><div class="ib-box-acts"><button class="btn primary" ${attrs}>${label}</button>${inboxTalkBtns(key)}${extra}${phr}</div></div>`;
};
/** Talk and pictures, as the chat's box has them (chat/compose.js): the mic types what you say into the box; the
 *  picture button adds one (⌘V pastes one, or drop it on the box). */
const inboxTalkBtns = (key) => `<button class="icon-btn mic ${dictating?.key === key ? "on" : ""}" data-mic="${esc(key)}" aria-label="${dictating?.key === key ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button>${inboxPics(key) ? `<button class="icon-btn" data-attach="${esc(key)}" aria-label="Add an image">${IMAGE_ICON}</button>` : ""}`;
/** Whether pictures go with what's written in this box: a message to the session (its "s:" box) does; an answer to its
 *  question is words only. */
const inboxPics = (key) => key.startsWith("s:");
/** The reply, as in mail: you write right under the message (no frame or line round it), Claude's suggestion in the
 *  empty box, then one row: Send, talk, pictures and Open chat on the left, your quick phrases on the right (those that
 *  don't fit go in a menu that opens upward). */
function inboxReply(sid, key, ph, label, attrs) {
  return inboxBox(key, ph, label, attrs, `<button class="btn" data-inbox-full="${esc(sid)}">Open chat</button>`, sid, inboxPhrases(sid, key));
}
/** Claude's own suggested reply, in the box's first line while it's empty (the chat box's own line, with "or write your
 *  own"): Tab or a click puts it in the box, to send as it is or change; anything typed hides it (CSS). */
const inboxSuggest = (sug, key, sid) => sugLineHtml(sug, `<span class="sug-or">or write your own</span>`, `data-inbox-sug data-key="${esc(key)}" data-sid="${esc(sid)}"`);
/** The quick phrase menu open, by its box's draft key ("" when none is). */
let inboxPhrOpen = "";
/** Your quick phrases (Settings → Quick phrases; a click sends it, after anything in the box) at the right of the
 *  box's row; inboxFitPhrases hides those that don't fit and lists them in More's menu. */
function inboxPhrases(sid, key) {
  const ps = phrases();
  if (!ps.length) return "";
  const k = `data-key="${esc(key)}" data-sid="${esc(sid)}"`, open = inboxPhrOpen === key;
  const chip = (p, i) => `<button class="ib-sug" data-inbox-phrase="${esc(p)}" data-i="${i}" ${k}>${esc(p)}</button>`;
  return `<div class="ib-phr"><div class="ib-phr-row">${ps.map(chip).join("")}</div>`
    + `<span class="ib-phr-more" hidden><button class="ib-sug" data-inbox-phr-more="${esc(key)}" aria-expanded="${open}">More<span class="ib-caret">▴</span></button>`
    + `${open ? `<span class="ib-phr-menu" role="menu">${ps.map((p, i) => `<button role="menuitem" data-inbox-phrase="${esc(p)}" data-i="${i}" ${k}>${esc(p)}</button>`).join("")}</span>` : ""}</span></div>`;
}
/** After each draw: each phrase row shows the phrases that fit (in order), More shows when some don't, and its menu
 *  lists just those. */
function inboxFitPhrases() {
  for (const w of document.querySelectorAll(".inbox-page .ib-phr")) {
    const row = w.querySelector(".ib-phr-row"), more = w.querySelector(".ib-phr-more"), chips = [...row.children];
    chips.forEach((c) => { c.hidden = false; });
    more.hidden = true;
    if (row.scrollWidth <= row.clientWidth + 1) continue;
    more.hidden = false;
    const end = row.getBoundingClientRect().right + 1, fits = (c) => c.getBoundingClientRect().right <= end;
    let n = chips.findIndex((c) => !fits(c));
    if (n < 0) n = chips.length;
    chips.forEach((c, i) => { c.hidden = i >= n; });
    w.querySelectorAll(".ib-phr-menu [data-i]").forEach((b) => { b.hidden = +b.dataset.i < n; });
  }
}
/** A quick phrase clicked: added to what's in the box, and sent. */
function inboxSendPhrase(el) {
  const d = draft(el.dataset.key);
  d.text = withPhrase(d.text, el.dataset.inboxPhrase);
  saveDrafts();
  renderMain();
  document.querySelector(`.inbox-page .ib-box textarea[data-text="${CSS.escape(el.dataset.key)}"]`)?.closest(".ib-box")?.querySelector(".btn.primary")?.click();
}
/** Claude's suggestion clicked: into the box (as Tab does), the cursor at its end. */
function inboxTakeSuggestion(el) {
  const ta = el.closest(".ta-wrap")?.querySelector("textarea");
  if (!ta || !takeSuggestion(el.dataset.key, el.dataset.sid, ta)) return;
  ta.focus();
  ta.selectionStart = ta.selectionEnd = ta.value.length;
}
/** A question ask: [the whole question card (board.js questionHtml: every question, its suggestions picked, what
 *  will happen); a box for your own answer to the question you last picked in; Decline and one Send for all]. */
function inboxQuestions(it) {
  const n = questions(it).length, cur = stepOf(it);
  const card = `<div class="ib-card on ib-q">${questionHtml(it, "data-inbox-pick")}</div>`;
  const box = inboxReply(it.session_id, it.id, n > 1 ? `Your own answer to question ${cur + 1}…` : "Or say something else…", n > 1 ? "Use answer" : "Send", `data-act="send" data-id="${esc(it.id)}"`);
  return [card, box, `<button class="btn deny ib-decline" data-act="deny" data-id="${esc(it.id)}">Decline</button>${qSendBtn(it)}`];
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
/** Three dots that count 1, 2, 3 after a line still going ("Thinking"), in a fixed width so nothing moves. */
const INBOX_DOTS = `<span class="ib-dots" aria-hidden="true"><i>.</i><i>.</i><i>.</i></span>`;
/** A working session: what it's on now (one line), its latest steps, and its bar. The same size all along. */
function inboxRunCard(r) {
  // What waits to go shows in its own block (inboxPending), not here; sent and not at work yet, it's starting.
  const [, said] = inboxRunLines(r, true), line = said === "Sent" ? "Starting…" : said;
  const m = /^(.*?)(?:…|\.\.\.)?$/.exec(line), dots = /(…|\.\.\.)$/.test(line) || line === "Thinking";
  const { agents, scripts } = inboxLive(r), also = [agents.length && `${agents.length} agent${agents.length === 1 ? "" : "s"}`, scripts.length && `${scripts.length} script${scripts.length === 1 ? "" : "s"}`].filter(Boolean).join(" · ");
  return `<div class="ib-card run"><div class="ib-qn">Now</div><div class="ib-qt">${esc(dots ? m[1] : line)}${dots ? INBOX_DOTS : ""}${also ? `<span class="ib-also">+ ${also} at work</span>` : ""}</div>${inboxRunSteps(r)}${inboxBar(r.s)}</div>`;
}
/** The run card's step lines: the latest INBOX_RUN_STEPS of this turn, the ones before folded into one line above them,
 *  in a space of that many lines and the fold's from the start (empty until its first step), so it never grows. */
const INBOX_RUN_STEPS = 3;
function inboxRunSteps(r) {
  const steps = inboxTurnSteps(r), lines = inboxStepLines(steps), shown = lines.slice(-INBOX_RUN_STEPS), more = lines.length - shown.length;
  const fold = more ? `<div class="ib-step-more">${more} earlier</div>` : "";
  return `<div class="ib-steps ib-run-steps" style="--ib-run-n:${INBOX_RUN_STEPS + 1}">${fold}${shown.map(inboxStepHtml).join("")}</div>`;
}
/** Its steps in the turn it's on: none while the latest turn read is still the one before what you sent (its steps
 *  feed is read every few seconds). */
function inboxTurnSteps(r) {
  const turn = inboxTurn(r.sid), sent = Math.max(inboxGoing(r.sid).at(-1)?.at || 0, (r.s?.thread || []).filter((e) => e.role === "you").at(-1)?.at_ms || 0);
  return turn && (turn.at_ms || 0) >= sent - 5000 ? (turn.items || []).filter((x) => x.t === "step") : [];
}
/** Whether it takes a written reply here: a question, a finished turn, one at work or done (not a permission or the
 *  context row, which have their own buttons). */
const inboxCanReply = (r) => !!r && (["turn", "crew", "review", "run", "done"].includes(r.kind) || (r.kind === "ask" && r.it.kind === "question"));
/** The line under the box: the keys that work here (1–N for the question lit), then `acts` at
 *  its end. */
function inboxHint(r, acts = "") {
  const it = r.kind === "ask" && r.it.kind === "question" ? r.it : null, n = it ? Math.min(9, questions(it)[stepOf(it)]?.options?.length || 0) : 0;
  const keys = [n ? `<span><span class="ib-kbd">1</span>–<span class="ib-kbd">${n}</span> pick</span>` : ""].filter(Boolean);
  return keys.length || acts ? `<div class="ib-hint">${keys.join("")}<span class="grow"></span>${acts}</div>` : "";
}
/** A lead's crew that waits on you, a row each with its own buttons, as on its crew's page (team.js): a teammate's ask
 *  with Deny and Allow, a review ready with Commit or Send fixes. Empty for any other session. */
function inboxCrewHtml(sid) {
  const waits = inboxCrewWaits(sid).filter((t) => !mateNeeds(sid, t) || inboxCrewActions(sid).includes(t));
  if (!waits.length) return "";
  const need = inboxCrew(sid);
  return `<div class="ib-crew"><div class="ib-k">${need ? `Its crew · ${need} need${need === 1 ? "s" : ""} you` : "Crew reviews"}</div>${waits.map((t) => (mateNeeds(sid, t) ? mateNeedRow(sid, t) : mateReadyRow(sid, t))).join("")}${inboxMateMenu(sid, waits)}</div>`;
}
/** The sessions whose latest reply shows whole, not just its last paragraphs. */
const inboxWhole = new Set();
/** Earlier messages opened in full (`${sid}|${at_ms}|${role}`), and the sessions showing all their earlier ones. */
const inboxMsgOpen = new Set(), inboxAllEarlier = new Set();
/** How many earlier messages show (folded) before "Show N earlier". */
const INBOX_EARLIER = 6;
/** The conversation before its latest reply, as mail shows a thread: each message folded to a line (who, when, its
 *  first words); a click opens it in full. Its latest reply (shown below as It says) is left out. */
/** Its thread as mail's: { msgs: the earlier ones, said: its latest message (shown whole; null when the thread hasn't
 *  it), yours: what you wrote after that (at work: your reply, shown whole under it) }. */
/** `asks`: the asks it made after that message (an ask line the hub noted, with your answer: askLnPair), and decisions
 *  with no ask before them, shown under it. An ask is never its message, and an answer ("allowed") never your reply. */
function inboxThread(r, words) {
  const latest = plain(words || "").trim(), yours = [], asks = [];
  let msgs = askLnPair((r.s?.thread || []).filter((e) => e.text?.trim()));
  // An ask still open is the card below: not again as a row.
  if (r.kind === "ask" && msgs.at(-1)?.ask && !msgs.at(-1).answer) msgs = msgs.slice(0, -1);
  for (;;) {
    const e = msgs.at(-1);
    if (e?.ask || e?.decided) asks.unshift(e);
    else if (r.kind === "run" && e?.role === "you") yours.unshift(e);
    else break;
    msgs = msgs.slice(0, -1);
  }
  // What you just sent and its thread doesn't show yet goes where it will land, after the rest of yours: from the
  // moment you click Send the page is the one it will be at work, and the real message takes its place as it arrives.
  if (r.kind === "run") yours.push(...inboxGoing(r.sid).map((o) => ({ role: "you", text: o.text, at_ms: o.at, sent: o.images })));
  const last = msgs.at(-1);
  let said = null;
  if (last && last.role !== "you" && (r.kind === "run" ? yours.length || plain(last.text).trim() === latest : !latest || plain(last.text).trim() === latest || r.kind === "turn")) { said = last; msgs = msgs.slice(0, -1); }
  // Its message not the latest one (or none): the asks after it stay with the earlier ones, in time order.
  else { msgs = msgs.concat(asks); asks.length = 0; }
  return { msgs, said, yours, asks };
}
/** What you wrote after its latest message, whole, as your reply under it in a mail thread. */
/** A message's head as mail's: who on the left, when on the right ("2:14pm · 5m ago"; nothing when not known). */
const inboxHead = (who, ms) => `<div class="ib-mhead"><span class="ib-k">${who}</span>${ms ? `<span class="ib-when">${clockAt(ms)} · ${ago(ms)} ago</span>` : ""}</div>`;
function inboxYours(sid, yours) {
  // As its chat draws them (#422): the bubble, and its one line: Sending… while on its way, else delivered. Under
  // "Yours", with what still waits to go after them (inboxPending).
  const to = nameOf(sid, sessionOf(sid)?.project);
  return yours.map((e, n) => `<div class="ib-yours">${n ? "" : '<div class="ib-k">Yours</div>'}${msgRow(e.sent ? { st: "sending", to, sid } : { st: "delivered", to, at: msgTaken(e) }, `<div class="cv-you-text ib-yours-text">${esc(e.text).replace(/\n/g, "<br>")}</div>${e.sent ? inboxSentPics(e.sent) : thumbs(e.images)}`)}</div>`).join("");
}
/** The pictures of a message still on its way: what you picked, as they are (not files yet), as its chat shows them. */
const inboxSentPics = (images) => (images.length ? `<div class="thumbs">${images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : "");
/** What you sent it from Cue that its thread doesn't show yet and that goes now (not one kept till its turn ends, or one
 *  that failed: those wait to go, in their own block). */
const inboxGoing = (sid) => outbox.filter((o) => o.sid === sid && !landed(o) && !/^(kept in Cue|not sent)/.test(o.via || ""));
/** sid → { text, at }: the reply you last answered here, kept for its brief at work once Cue has moved it off Waiting
 *  (before its thread has it, it would otherwise go blank for a moment). */
const inboxAnswered = new Map();
function inboxEarlier(r, msgs) {
  if (!msgs.length) return "";
  const all = inboxAllEarlier.has(r.sid), hidden = all ? 0 : Math.max(0, msgs.length - INBOX_EARLIER);
  const more = hidden ? `<button class="ib-link ib-earlier-more" data-inbox-earlier="${esc(r.sid)}">Show ${hidden} earlier</button>` : "";
  return `<div class="ib-earlier"><div class="ib-k">Earlier</div>${more}${msgs.slice(hidden).map((e) => inboxMsgHtml(r.sid, e)).join("")}</div>`;
}
/** One earlier message: folded to a line, or open in full. */
function inboxMsgHtml(sid, e) {
  if (e.ask) return inboxAskMsgHtml(sid, e);
  // A decision with no ask before it (a terminal dialog's choice): one line, "You · Chose 1. Yes"; nothing to open.
  if (e.decided) return `<div class="ib-msg you decided"><span class="ib-msg-who">You</span><span class="ib-msg-at">${e.at_ms ? ago(e.at_ms) : ""}</span><span class="ib-msg-line ib-ask-ans ${e.decided.no ? "no" : ""}">${esc(e.decided.text)}</span></div>`;
  const k = `${sid}|${e.at_ms}|${e.role}`, open = inboxMsgOpen.has(k), you = e.role === "you";
  const who = `<span class="ib-msg-who">${you ? "You" : "It"}</span><span class="ib-msg-at">${e.at_ms ? ago(e.at_ms) : ""}</span>`;
  // Folded: two lines of it, and "more" when that cuts it (a click opens it whole; "less" folds it again).
  const line = plain(e.text).replace(/\s+/g, " ").trim(), cut = line.length > INBOX_MSG_CUT || /\n\s*\S/.test(String(e.text).trim());
  const tag = cut ? `<span class="ib-msg-more">${open ? "less" : "more"}</span>` : "";
  const body = open ? `<div class="ib-msg-body ${you ? "" : "ib-md msg"}">${you ? esc(e.text).replace(/\n/g, "<br>") : md(e.text, { pathsOf: sid })}${thumbs(e.images)}${you ? "" : localImages(e.text)}${tag}</div>` : `<span class="ib-msg-fold"><span class="ib-msg-line">${esc(line.slice(0, 600))}</span>${thumbs(e.images)}${tag}</span>`;
  return `<div class="ib-msg ${you ? "you" : "it"} ${open ? "open" : ""}" role="button" data-inbox-msg="${esc(k)}">${who}${body}</div>`;
}
/** An ask it made and your answer, as one row: "It · Asked to run `cmd…` · Allowed"; opened, the command whole, in code. */
function inboxAskMsgHtml(sid, e) {
  const { ask, answer } = e, k = `${sid}|${e.at_ms}|ask`, open = inboxMsgOpen.has(k);
  const line = ask.code || (ask.tool ? "" : ask.what), flat = line.replace(/\s+/g, " ").trim();
  const ans = answer ? ` · <span class="ib-ask-ans ${answer.no ? "no" : ""}">${esc(answer.text)}</span>` : "";
  const head = `${esc(ask.head)}${ask.tool ? "" : ":"}`;
  const tag = flat.length > 60 || line.includes("\n") ? `<span class="ib-msg-more">${open ? "less" : "more"}</span>` : "";
  const body = open
    ? `<div class="ib-msg-body"><div class="ib-ask-line">${head}${ans}</div>${ask.code ? `<pre class="ib-ask-code"><code>${ask.tool === "Bash" ? "$ " : ""}${esc(ask.code)}</code></pre>` : line ? `<div>${esc(line)}</div>` : ""}${tag}</div>`
    : `<span class="ib-msg-fold"><span class="ib-msg-line ib-ask-line">${head}${flat ? ` <code>${esc(flat.length > 60 ? `${flat.slice(0, 60)}…` : flat)}</code>` : ""}${ans}</span>${tag}</span>`;
  return `<div class="ib-msg it ask ${open ? "open" : ""}" role="button" data-inbox-msg="${esc(k)}"><span class="ib-msg-who">It</span><span class="ib-msg-at">${e.at_ms ? ago(e.at_ms) : ""}</span>${body}</div>`;
}
/** The asks it made after its message (and how you answered), and your decisions, under it: a row each, as in Earlier. */
const inboxAsksHtml = (sid, asks) => (asks.length ? `<div class="ib-asks">${asks.map((e) => inboxMsgHtml(sid, e)).join("")}</div>` : "");
/** What you've written to it that hasn't reached it yet, as its chat shows it (chat/render.js): on its way, queued in
 *  it till its next step (Send now), kept in Cue till its turn ends (Edit, Send now), or to go at a time (Edit,
 *  Cancel, Send now). Empty when nothing waits. */
function inboxPending(r, after = false) {
  const going = r.kind === "run" ? inboxGoing(r.sid) : [];   // at work, those show as yours in the thread
  const s = r.s, rows = outbox.filter((o) => o.sid === r.sid && !landed(o) && !going.includes(o)).map((o) => outboxRow(o, s, `ib-out:${o.at}`));
  if (s?.held) rows.push(heldRow(s));
  if (s?.send_at) rows.push(sendAtRow(s));
  if (s?.queued) rows.push(queuedRow(s));
  // Under the rest of yours as more of them, each its bubble and its one line (#422): "Yours" over them when nothing
  // of yours is above.
  return rows.length ? `<div class="ib-pending${after ? " after" : ""}">${after ? "" : '<div class="ib-k">Yours</div>'}${rows.join("")}</div>` : "";
}
/** What waits to go to it, in a line for its row, in its message's words (#422: "Queued for cue-418: …"); "" when
 *  nothing does. */
function inboxPendingLine(s) {
  const w = s?.queued ? [`Queued for ${nameOf(s.session_id, s.project)}`, s.queued.text] : s?.held ? ["Not sent yet", s.held.text] : s?.send_at ? ["Sends later", s.send_at.text] : null;
  return w ? `${w[0]}: “${plain(w[1] || "")}”` : "";
}
/** The brief's main column: its words before the question, the question with its choices, what it plans next, then
 *  the box to answer in (at the foot). */
function inboxMain(r) {
  const thread = inboxThread(r, inboxWords(r));
  // At work after your reply, its message is the one you answered (the thread's, or the ask's while it's still there,
  // or as it was when you answered it). That last is kept only until the thread has its message, or the session is
  // no longer at work, so it can't stand in for a later turn's.
  if (r.it?.kind === "waiting") inboxAnswered.set(r.sid, { text: finishedText(r.it), at: r.it.created_ms });
  else if (r.kind !== "run" || thread.said) inboxAnswered.delete(r.sid);
  const kept = inboxAnswered.get(r.sid);
  const words = r.kind === "run" && thread.yours.length ? thread.said?.text || (r.it?.kind === "waiting" ? finishedText(r.it) : kept?.text || "") : inboxWords(r), q = r.kind === "turn" && !r.it.interrupted ? inboxEndQuestion(words) : "";
  const then = r.kind === "run" ? "" : inboxThen(words), said = inboxSaid(words, [q, then]);
  const label = r.kind === "run" ? "It said last" : r.kind === "ask" || r.kind === "turn" ? "It says" : "It said";
  const whole = inboxWhole.has(r.sid), cut = said !== inboxSaid(words, [q, then], true);
  const shown = whole ? inboxSaid(words, [q, then], true) : said, more = cut ? `<button class="ib-link" data-inbox-whole="${esc(r.sid)}">${whole ? "Show less" : "Show the whole reply"}</button>` : "";
  const saidHtml = shown ? `<div class="ib-said">${inboxHead(label, thread.said?.at_ms || r.it?.created_ms || (r.kind === "run" && kept?.text === words ? kept.at : 0))}<div class="ib-md msg">${md(shown, { pathsOf: r.sid })}</div>${thumbs(thread.said?.images)}${localImages(shown)}${more}</div>` : "";
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
  // Oldest at the top, newest at the foot, as a mail thread: earlier, its message, your reply, what waits to go, then
  // what it's doing now, right over the box. (Housekeeping and a crew's asks lead with what they need of you.) The
  // thread scrolls; the box is pinned to the foot of the pane, so nothing drawn above it ever moves it.
  const earlier = inboxEarlier(r, thread.msgs), asksHtml = inboxAsksHtml(r.sid, thread.asks);
  const first = r.kind === "run" ? `${saidHtml}${asksHtml}${inboxYours(r.sid, thread.yours)}${inboxPending(r, thread.yours.length > 0)}${crew}${top}`
    : `${r.kind === "chore" || r.kind === "crew" ? `${crew}${top}${saidHtml}${asksHtml}` : `${saidHtml}${asksHtml}${top}${crew}`}${thenHtml}${inboxPending(r)}`;
  return `<div class="ib-main"><div class="ib-page-body ib-thread" data-scroll-key="inbox-main" data-sid="${esc(r.sid)}" data-kind="${r.kind}">${earlier}${first}</div><div class="ib-foot">${foot}${inboxHint(r, acts)}</div></div>`;
}
/** Whether the open session's thread is at its newest (its foot) as it's drawn again: it stays there, unless you've
 *  scrolled up. Read off the page before the draw replaces it; a session or shape new to the page (just opened, just
 *  answered) and a message just sent start there too. */
function inboxAtEnd(sid, kind) {
  const el = document.querySelector(".inbox-page .ib-thread");
  if (!el || el.dataset.sid !== sid || el.dataset.kind !== kind || (typeof chatToEnd !== "undefined" && chatToEnd)) return true;
  const { box, end } = inboxThreadEnd(el);
  return Math.abs(end - box.scrollTop) < 24;
}
/** What scrolls the open thread, and where it stands at its newest. With room, the thread scrolls itself and its newest
 *  is its foot. Narrow, the whole brief scrolls (the thread, then the side under it), and the newest is where the main
 *  column ends right at the pane's foot, its last message just over the box (not the brief's own foot, the side's). */
function inboxThreadEnd(el) {
  if (getComputedStyle(el).overflowY !== "visible") return { box: el, end: el.scrollHeight - el.clientHeight };
  const box = el.closest(".ib-brief"), main = el.closest(".ib-main");
  const end = main.getBoundingClientRect().bottom - box.getBoundingClientRect().top + box.scrollTop - box.clientHeight;
  return { box, end: Math.max(0, end) };
}
/** Set as the brief is drawn: the thread goes to its newest after the draw (inboxKeepEnd). */
let inboxToEnd = false;
/** After each draw: the open thread at its newest, when it was there (or is new). */
function inboxKeepEnd() {
  const el = inboxToEnd && document.querySelector(".inbox-page .ib-thread");
  if (!el) return;
  const { box, end } = inboxThreadEnd(el);
  box.scrollTop = end;
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
  const asked = you ? `<div class="ib-blk"><div class="ib-k">You asked${you.at ? ` · ${ago(you.at)} ago` : ""}</div><div class="ib-you">${esc(plain(you.text))}</div></div>` : "";
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
  const [subj, line] = r.kind === "run" ? inboxRunLines(r) : inboxDoneLines(r);
  return `<div class="ib-calm-card ${r.kind}" role="button" tabindex="0" data-inbox-open="${esc(r.sid)}" style="${projStyle(r.project)}"><div class="ib-calm-t"><span class="ib-dot">${inboxDot(r)}</span>${nameSpan(r.sid, r.project, "ib-name")}<span class="grow"></span><span class="ib-time">${ago(r.at)}</span></div>`
    + `<div class="ib-calm-s">${esc(subj)}</div>${line ? `<div class="ib-calm-l">${esc(line)}</div>` : ""}${r.kind === "run" && r.s ? inboxBar(r.s) : ""}</div>`;
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

// ---------- moving about ----------

/** A window wide enough for the list and the brief side by side: there, something is always open. */
const inboxWide = () => typeof matchMedia !== "function" || matchMedia("(min-width: 821px)").matches;
/** The Inbox view: the list on the left, the open session beside it. With none open (a wide window), the top one that
 *  needs you opens by itself. Answered, the open one stays open (you see what you sent, as in email); you move on. */
function inboxView() {
  const sid = inboxSid;
  inboxToEnd = false;
  if (sid && !sessionOf(sid) && !inboxItems().has(sid) && !inboxParkedOf(sid)) inboxAdvance(sid);
  else if (!sid && inboxWide()) inboxAdvance(null);
  return `<div class="board ib-board ${inboxSid ? "open" : ""}">${inboxList()}${inboxSid ? inboxOpened(inboxSid) : inboxIdle()}</div>`;
}
/** Past `sid` (answered, or gone; null: none open): the next session that needs you is the open one, else none. Drawn
 *  as it is. */
function inboxAdvance(sid) {
  const next = inboxTab === "inbox" ? inboxNeedOrder().find((x) => x !== sid) : null;
  if (!next && !sid) return;
  inboxSid = inboxLast = next || null;
  inboxSnoozeAt = "";
  if (next) { active = { id: inboxItems().get(next)?.id || null, sid: next, exact: false }; seenSession(next); }
  // Opened by itself (the Inbox shown, or what was open has gone): its box takes the cursor once it's drawn.
  if (next && typeof requestAnimationFrame === "function") requestAnimationFrame(inboxFocusReply);
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
  inboxSnoozeAt = "";
  renderMain();
  inboxFocusReply();
}
/** The cursor into the open session's reply box, to write at once; not on a question or permission (its keys pick),
 *  nor while you're writing somewhere else. */
function inboxFocusReply() {
  const a = document.activeElement;
  if (a?.matches?.("textarea, input, select") && !a.closest(".inbox-page")) return;
  const pg = inboxSid && document.querySelector(`.inbox-page[data-inbox-page="${CSS.escape(inboxSid)}"]`);
  if (!pg || pg.dataset.kind === "ask") return;
  const t = pg.querySelector(".ib-box textarea[data-text]");
  if (t && document.activeElement !== t) { t.focus(); t.setSelectionRange(t.value.length, t.value.length); }
}
/** Write to the open session: the cursor into its box (R does it too). */
function inboxReplyTo(sid) {
  renderMain();
  document.querySelector(`.inbox-page[data-inbox-page="${CSS.escape(sid)}"] .ib-box textarea[data-text]`)?.focus();
}
/** Back to the list (a narrow window shows one at a time). */
function inboxClose() {
  inboxSid = null;
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
  openSession(sid, inboxItems().get(sid)?.id || null);
}
/** What it changed, on the Board: its chat on its Diff tab. */
function inboxDiff(sid) {
  bgTabOn.delete(sid);
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
/** Put it off for later (the Board's Later: under Later at the foot, no notification), or bring it back. */
function inboxSetLater(sid, on) {
  const s = sessionOf(sid);
  if (s) s.later_ms = on ? now() : 0;
  invoke("set_later", { sessionId: sid, on }).catch((e) => toast(`Couldn't: ${e}`));
  if (on) toast(`${nameOf(sid, s?.project)} is put off for later`);
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
  if (gone && inboxSid === sid) { inboxSid = null; inboxAdvance(sid); }
  renderMain();
}
/** Snooze's button: its menu opens (or shuts) where it is; a snoozed one wakes. */
function inboxSnoozeClick(el) {
  const sid = el.dataset.inboxSnooze;
  if (inboxSnoozed(sessionOf(sid))) return inboxSetSnooze(sid, 0);
  inboxSnoozeAt = inboxSnoozeAt === el.dataset.at ? "" : el.dataset.at;
  renderMain();
}
/** One of a question's choices: picked, held until Send. */
function inboxPick(el) {
  const it = findItem(el.dataset.id), [qi, oi] = el.dataset.inboxPick.split(":").map(Number);
  if (it) pick(it, qi, oi);
}

/** Keys while the Inbox shows. The open session: Esc (a narrow window) back to the list, a Snooze menu first; 1–9 a
 *  choice of the question lit, R into the reply (Esc out of an empty one). The list: the Board's card keys (A, D, G, R,
 *  1–9) are kept from a card not on screen. Not while you type in a box with text in it, or something is open over it.
 *  True when it took the key. */
function inboxKey(e) {
  if (view !== "inbox" || e.metaKey || e.ctrlKey || e.altKey || e.isComposing) return false;
  if (sheet || lightbox || liveOpen || deleteAsk || renaming || btwFor || menuFor || redirectFor || (typeof moreFor !== "undefined" && moreFor)) return false;
  const f = document.activeElement, typing = f?.matches?.("textarea, input, select");
  if (e.key === "Escape" && inboxSnoozeAt) { inboxSnoozeAt = ""; renderMain(); return true; }
  if (!inboxSid && e.key === "Enter" && f?.matches?.(".ib-row, .ib-calm-card")) { inboxOpen(f.dataset.inboxOpen); return true; }
  if (!inboxSid) return !typing && /^[adgr1-9]$/.test(e.key);
  if (e.key === "Escape" && !e.shiftKey && typing && !f.value && f.closest?.(".inbox-page")) { f.blur(); return true; }
  if (e.key === "Escape" && !e.shiftKey) { if ((typing && f.value) || inboxWide()) return false; f?.blur?.(); inboxClose(); return true; }
  if (typing) return false;
  if (e.key === "r" && inboxCanReply(inboxRowFor(inboxSid))) { e.preventDefault(); inboxReplyTo(inboxSid); return true; }
  return inboxDigit(e.key);
}
/** 1–9 on an open question: that choice of the question lit, held with the rest when there are several. */
function inboxDigit(key) {
  const it = inboxItems().get(inboxSid);
  if (!/^[1-9]$/.test(key) || it?.kind !== "question") return false;
  pick(it, stepOf(it), +key - 1);
  return true;
}

/** The Inbox's own clicks, by attribute: each with its element. */
const INBOX_CLICKS = {
  inboxPin: (el) => inboxSetPin(el.dataset.inboxPin, !inboxPinned(sessionOf(el.dataset.inboxPin))),
  inboxLater: (el) => inboxSetLater(el.dataset.inboxLater, !inboxLater(sessionOf(el.dataset.inboxLater))),
  inboxSnooze: inboxSnoozeClick,
  inboxSnoozeTo: (el) => inboxSetSnooze(el.dataset.sid, +el.dataset.inboxSnoozeTo),
  inboxTab: (el) => { inboxTab = el.dataset.inboxTab; inboxSnoozeAt = ""; renderMain(); },
  inboxFold: (el) => inboxFold(el.dataset.inboxFold),
  inboxPick,
  inboxStep: (el) => { const [id, n] = el.dataset.inboxStep.split(/:(?=\d+$)/); qStep[id] = +n; renderMain(); },
  inboxSend: (el) => sendTo(el.dataset.inboxSend),
  inboxFull: (el) => inboxFullChat(el.dataset.inboxFull),
  inboxPhrase: (el) => { inboxPhrOpen = ""; inboxSendPhrase(el); },
  inboxPhrMore: (el) => { const k = el.dataset.inboxPhrMore; inboxPhrOpen = inboxPhrOpen === k ? "" : k; renderMain(); },
  inboxWhole: (el) => { const k = el.dataset.inboxWhole; if (!inboxWhole.delete(k)) inboxWhole.add(k); renderMain(); },
  inboxMsg: (el) => { if (getSelection()?.toString()) return; const k = el.dataset.inboxMsg; if (!inboxMsgOpen.delete(k)) inboxMsgOpen.add(k); renderMain(); },
  inboxEarlier: (el) => { inboxAllEarlier.add(el.dataset.inboxEarlier); renderMain(); },
  inboxSug: inboxTakeSuggestion,
  inboxDiff: (el) => inboxDiff(el.dataset.inboxDiff),
  inboxFile: (el) => inboxShowFile(el.dataset.inboxFile, +el.dataset.n),
  inboxDesk: (el) => openInDesk(el.dataset.inboxDesk, el.dataset.item),
  inboxAddDesk: (el) => inboxAddToDesk(el.dataset.inboxAddDesk, el.dataset.deskId),
  inboxBack: () => inboxClose(),
  inboxNext: () => inboxStepTo(1),
  inboxOpen: (el) => inboxOpen(el.dataset.inboxOpen),
  inboxHelper: (el) => { const [sid, id] = el.dataset.inboxHelper.split("|"); inboxFullChat(sid); helperView = { sid, id }; renderMain(); },
};
/** The attribute selector for each of INBOX_CLICKS ("inboxSnoozeTo" → "[data-inbox-snooze-to]"), innermost first. */
const INBOX_SEL = Object.keys(INBOX_CLICKS).map((k) => `[data-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}]`).join(", ");
/** A click in the Inbox: the innermost element with one of its attributes does its thing. */
function inboxClick(e) {
  // Inbox in the header while a session is open in it: the top one that needs you opens (the click goes on to switch
  // the view).
  if (e.target.closest?.('[data-view="inbox"]')) { if (view === "inbox") inboxSid = null; return; }
  // A file or web link in a card (a path in a question) opens as anywhere else (clicks_chat.js), not the card.
  if (e.target.closest?.("a[data-path], a[data-href]")) return;
  const el = e.target.closest?.(INBOX_SEL);
  if (!el) {
    if ((inboxSnoozeAt && !e.target.closest?.(".ib-snz")) || inboxPhrOpen) { inboxSnoozeAt = ""; inboxPhrOpen = ""; renderMain(); }
    return;
  }
  e.stopPropagation();
  const key = Object.keys(INBOX_CLICKS).find((k) => k in el.dataset);
  INBOX_CLICKS[key](el);
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (inboxKey(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", inboxClick, true);
  // Writing in the reply box, a click on a button in the Inbox (Allow, a choice, a phrase) leaves the cursor in the box.
  document.addEventListener("mousedown", (e) => {
    const a = document.activeElement;
    if (e.button === 0 && a?.matches?.(".inbox-page textarea[data-text]") && e.target.closest?.(".inbox-page button, .inbox-page [role=button]")) e.preventDefault();
  }, true);
}
/** The cursor was in an Inbox reply box that's gone (Allow moved it on to the next): the box on screen now takes it. */
const inboxRefocus = () => document.querySelector(".inbox-page .ib-box textarea[data-text]")?.focus();
