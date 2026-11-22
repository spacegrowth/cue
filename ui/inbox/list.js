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
// what's done lately stand in. With a project picked in the header's Show (app/head_view.js), only its sessions.

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
    if (unresolvedCard(it) && inboxIn(it.project) && (!had || (had.kind === "waiting" && actionableRequest(it)))) by.set(it.session_id, it);
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
    if (items.has(s.session_id) || !inboxIn(s.project) || cutOff(s)) continue;
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
/** How many sessions need you in the projects the Inbox shows (the header's "Inbox 3" counts every project: inboxCountAll). */
const inboxCount = () => inboxNeedRows().length + inboxVerifications().length;

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
 *  quiet, or stuck). `html`: as markup, Cue's message and what waits to go in the message family's plain line
 *  (.mg-plain, #501); otherwise as text. */
function inboxRunLines(r, card = false, html = false) {
  const s = r.s, seat = inboxSeats.get(r.sid), sent = outbox.findLast((o) => o.sid === r.sid)?.text, to = nameOf(r.sid, s.project);
  const cue = !sent && cueMsgPlain(s.prompt, to), said = cue || plain(sent || s.prompt);
  const answered = r.it && answering.has(r.it.id) && !sent ? `You answered: ${r.it.kind === "waiting" ? inboxEndQuestion(finishedText(r.it)) || "its reply" : inboxAskSubject(r.it)}` : "";
  const you = !!(said && (sent || seat?.from === "need"));
  const subj = answered || (said ? (you ? `You: “${said}”` : said) : "Working");
  const stuck = s.stuck_ms && !s.compacting_ms ? `No new output for ${ago(s.stuck_ms)}` : "";
  const busy = s.state === "working" || s.state === "agent" || s.compacting_ms;
  const pending = card ? "" : inboxPendingLine(s);
  const line = s.compacting_ms ? "Compacting…" : preStart(s) ? `Not started: ${preStart(s)}` : stuck || pending || (!busy ? (!card && inboxTaken(s)) || "Sent" : s.state === "agent" ? idleNote(s) : s.doing || "Thinking");
  if (!html) return [subj, line];
  const cueHtml = !answered && !you && cue ? `<span class="mg-plain">${cueMsgPlainHtml(s.prompt, to)}</span>` : "";
  return [cueHtml || esc(subj), !s.compacting_ms && !preStart(s) && !stuck && pending ? inboxPendingLine(s, true) : esc(line)];
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
/** A row's two lines: [subject, the useful line under it]; `html`: as markup (escaped, a message's words in .mg-plain). */
function inboxLines(r, html = false) {
  if (html) return r.kind === "run" ? inboxRunLines(r, false, true) : inboxLines(r).map(esc);
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
  const { sid, s, kind } = r, seat = inboxSeats.get(sid), [subj, line] = inboxLines(r, true);
  // Its sink plays once, from when it came in: a redraw carries on from there rather than starting it over.
  const sinking = kind === "run" && seat?.from === "need" && now() - seat.at < INBOX_SINK_MS;
  const under = inboxSnoozed(s) ? `Back ${esc(inboxWhen(s.snooze_ms))}${line ? ` · ${line}` : ""}` : line;
  const side = `<span class="ib-time">${ago(r.at)}</span>${inboxPinned(s) ? `<span class="ib-pin">${PIN_ICON}</span>` : ""}${inboxFull(s) ? inboxMeter(s) : ""}`;
  return `<div class="ib-row ${kind} ${inboxLater(s) ? "later" : ""} ${sinking ? "sinking" : ""} ${(inboxSid || inboxLast) === sid ? "on" : ""}" role="button" tabindex="0" data-inbox-open="${esc(sid)}" style="${projStyle(r.project)}${sinking ? `;animation-delay:-${now() - seat.at}ms` : ""}">`
    + `<span class="ib-dot">${inboxDot(r)}</span><span class="ib-l1">${nameSpan(r.sid, r.project, "ib-name")}${inboxBadges(r)}<span class="ib-subj">${subj}</span></span>`
    + `<span class="ib-l2">${under}</span>${inboxForHtml(r)}<span class="ib-side">${side}</span>`
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
const inboxParkedRows = () => (state.parked || []).filter((p) => inboxTab === "inbox" && inboxIn(p.project)).sort((a, b) => (b.parked_ms || 0) - (a.parked_ms || 0));
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
  const verifications = inboxTab === "inbox" ? inboxVerifications() : [];
  const need = inboxNeedRows().length + verifications.length, run = of("run").length;
  const counts = inboxTab === "inbox" ? [[need, "need you"], [run, "running"]].filter(([c]) => c).map(([c, w]) => `${c} ${w}`).join(" · ") : "";
  // With Desks above, what's on none sits under its own heading.
  const loose = secs.some(([k]) => k === "desk") && of("need").length ? `<div class="ib-sect ib-loose">Not on a Desk</div>` : "";
  const body = secs.map((sec) => (sec[0] === "need" ? loose : "") + inboxSection(sec)
    + (sec[0] === "need" && verifications.length ? `<div class="ib-sect">Verify in Cue</div>${inboxVerificationRows(verifications)}` : "")).join("");
  return `<div class="inbox"><div class="ib-top"><b>Inbox</b><span class="ib-counts">${esc(counts)}</span>${inboxTabs()}</div>
    <div class="ib-scroll" data-scroll-key="inbox">${body || `<div class="ib-empty">${inboxEmpty()}</div>`}</div></div>`;
}
