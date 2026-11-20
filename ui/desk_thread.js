// Cue UI desk — an item's Thread and its page (deskItemPane), a holder's page, a crew member's page.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (index.html, mini.html).
/** What opens session `sid`'s chat here on the Desk, as its row in the sessions on the left does: the lead's chat
 *  for the Desk's lead, else that session's chat in the middle card. Never the board. */
const deskChatAttrs = (sid, d) => (sid === deskLead(d)?.sid ? `data-desk-lead-chat="${esc(sid)}"` : `data-desk-chat-of="${esc(sid)}"`);
/** Each item page's open tab ("thread" | "decisions" | "steps"), by item: Thread unless you picked another. */
const deskItemTabs = new Map();
/** What you wrote on an item's page that hasn't come back as one of its Steps yet, by item: [{ text, why, at }], so
 *  Thread has it the moment it's sent (deskSendBox, deskSendBack). Dropped once the step is in, or the send failed. */
const deskPending = new Map();
const deskPend = (id, text, why, pics = 0) => { const p = { text, why, pics, at: now() }; deskPending.set(id, [...(deskPending.get(id) || []), p]); deskThreadToEnd = true; return p; };
const deskUnpend = (id, p) => deskPending.set(id, (deskPending.get(id) || []).filter((x) => x !== p));
/** A step that hands the item to someone (hub/desk.rs routed_text, give): on its Thread as a Hand-off. */
const DESK_HANDOFF = /^(Routed|Gave|Started|Sent|Queued|Reopened) #/;
/** The lead unsticking an item itself (hub/desk_unstick.rs, #347): closed as on main, its copy taken back, moved up
 *  in the landing line, started now. On its Thread as a record line. */
const DESK_UNSTUCK = /^(Closed|Took back|Moved|Started) #\S+(:| now| up|'s copy)/;
/** Item `i`'s steps and calls, oldest first: today's (newest first in `decisions`) after the older ones. */
const deskItemLog = (i, d) => [...(d.older || []), ...[...(d.decisions || [])].reverse()].filter((c) => c.item_id === i.id);
/** Item `i`'s Thread, oldest first: the words it was added with, then what you wrote about it (Sent back in red), the
 *  hand-offs, and what its holder said as each turn ended (hub/desk.rs replied); last, its holder at work now. */
function deskThread(i, d) {
  const mine = deskItemLog(i, d).filter((c) => !c.call);
  const you = (i.from || "you") === "you";
  // Its Done when isn't in this first message: it's under the thread, by where it stands (deskItemPane).
  const pics = thumbs(i.images);
  const rows = [{ key: "first", at: deskMs(i.created), by: you ? "You" : deskFrom(i, d), you, html: `<div class="dk-md">${deskMd(i.words || i.title)}</div>${pics ? `<div class="dk-tpics">${pics}</div>` : ""}` }];
  const backPics = deskBackPics(i, d, mine);
  for (const c of mine) {
    if (c.by === "You" && /^(to |sent back)/.test(c.why || "")) {
      const back = c.why.startsWith("sent back");
      // Timed to the thousandth of a second when its message's delivery says (#395): a step is kept to the second, and
      // in the second a reply lands in, it would sort above it.
      rows.push({ key: `y:${c.id}`, at: c.delivery?.at_ms || deskMs(c.at), by: "You", you: true, back, tag: back ? deskBackLabel(c, i) : deskHeldTag(c, i), text: c.text, html: deskBackWords(c.text, backPics.get(c) || 0) });
    }
    // The lead's note to its holder (#338: NOTE:, at any stage): a message from the lead, with whether it was taken.
    else if (/^note to /.test(c.why || "")) {
      const at = deskMs(c.at);
      rows.push({ key: `n:${c.id}`, at, by: c.by, tag: `Note to ${c.why.slice(8)}`, text: c.text, html: `<div class="dk-md">${deskMd(c.text)}</div>${deskTakenHtml(i, { text: c.text, at })}` });
    }
    else if (c.by !== "You" && DESK_UNSTUCK.test(c.text)) rows.push({ key: `r:${c.id}`, at: deskMs(c.at), rec: "Unstuck", html: `${esc(c.by)}: ${esc(c.text)}${c.why ? `: ${esc(c.why)}` : ""}` });
    // A hand-off is a record line, not a message (#276): one quiet line in the time line.
    else if (DESK_HANDOFF.test(c.text)) rows.push({ key: `r:${c.id}`, at: deskMs(c.at), rec: "Hand-off", html: `${esc(c.by)}: ${esc(c.text)}${c.why ? `: ${esc(c.why)}` : ""}` });
  }
  // What you wrote to its holder from here: under it, whether the holder took it yet (#276).
  for (const r of rows) if (r.you && !r.back && r.text) r.html += deskTakenHtml(i, r);
  // A reply that's the one before it told again with more after it (a turn on a usage limit, saved twice before the
  // hub kept one): shown once, the longer.
  const said = (i.said || []).filter((r, n, all) => { const x = all[n + 1]; return !(x && x.by === r.by && x.text.startsWith(r.text.replace(/…$/, ""))); });
  // Each reply in full, with a link to that turn in its session; one that said DONE has a record line after it.
  for (const r0 of said) {
    const r = deskHookSplit(r0);
    rows.push({ key: `s:${r.at}`, at: r.at, by: r.by, tag: "Reply", text: r.text, html: `${r.text ? `<div class="dk-md">${deskMd(r.text)}</div>` : ""}${deskHookFold(i, r)}${deskTurnLink(r)}` });
    if (r.done) rows.push({ key: `d:${r.at}`, at: r.at + 1, rec: "Done", html: `${esc(r.by)} said ${esc(deskNum(i))} is done` });
  }
  // Sent from here, not back as a step yet.
  // A picture with no words is never one of its Steps: it's in once a reply came after it.
  const recorded = (p) => (p.text.trim() ? mine.some((c) => c.by === "You" && c.text.trim() === p.text.trim() && (c.delivery?.at_ms || deskMs(c.at) + 999) >= p.at) : (i.said || []).some((r) => r.at > p.at));
  for (const p of deskPending.get(i.id) || []) if (!recorded(p)) rows.push({ key: `p:${p.at}`, at: p.at, by: "You", you: true, back: p.why === "back", tag: p.why === "back" ? "Sent back (queued)" : p.why === "holder" || p.why === "lead" ? deskHeldTag({ text: p.text, why: `to ${p.why === "lead" ? deskLead(d)?.name : i.holder?.name}`, at: p.at / 1000 }, i) : "", html: deskBackWords(p.text, p.pics) + (p.why === "holder" ? deskTakenHtml(i, { text: p.text, at: p.at }) : "") });
  // At work on this one: its holder can hold several, and its work and latest words are only this one's while it's on
  // it (#176).
  const live = i.state === "working" && i.holder && i.now_on && !i.stopped;
  // The turn it just wrote here, till its reply is saved on the item (#395): the session is marked stopped a moment
  // before its reply is kept, and in that gap the page would show only its last line.
  const kept = live ? null : deskLiveKept.get(i.id);
  const keep = kept && Date.now() - kept.seen < DESK_KEEP_MS && !(i.said || []).some((r) => r.at >= kept.from);
  if (kept && !keep) deskLiveKept.delete(i.id);
  // Its latest words when no reply holds them yet (items from before replies were kept): after everything but what
  // you wrote since, so a message you send is still last (#160).
  const lat = (i.latest || "").trim(), lat1 = firstLine(lat);
  if (keep) rows.push({ key: "live", at: kept.seen, by: kept.by, tag: "Reply", text: kept.text, html: `${kept.text ? `<div class="dk-md">${deskMd(kept.text)}</div>` : ""}${deskHookFold(i, { ...kept, at: kept.from })}` });
  else if (!live && lat && !deskSaidHas(i, lat1)) {
    const fin = i.state === "to_check" || i.state === "done";
    const theirs = rows.filter((r) => !r.you).reduce((m, r) => Math.max(m, r.at), 0);
    const at = Math.max(deskMs(i.updated), theirs), r = deskHookSplit({ at, text: lat });
    rows.push({ key: "lat", at, by: i.holder?.name || i.by || "It", tag: fin ? "Done" : "Reply", on: fin, text: r.text, html: `${r.text ? `<div class="dk-md">${deskMd(r.text)}</div>` : ""}${deskHookFold(i, r)}` });
  }
  rows.sort((a, b) => a.at - b.at);
  // Its holder at work: last, under what you wrote, as the answer to it comes (#165): the turn it's writing, in full,
  // as it writes it, with a line for its steps (#276); its latest line till its steps are read.
  if (live) {
    const turn = deskLiveTurn(i), crew = deskCrewNow(i), crewHtml = crew ? `<div class="dk-stp dk-crewnow">Crew: ${esc(crew)}</div>` : "";
    if (turn?.words) deskLiveKept.set(i.id, { by: i.holder.name, text: turn.words, after: turn.after, from: turn.from, seen: Date.now() });
    rows.push({ key: "live", at: 0, by: i.holder.name, live: true, tag: turn?.words ? "Writing" : "", html: turn ? `${turn.steps ? `<div class="dk-stp">${esc(turn.steps)}</div>` : ""}${turn.words ? `<div class="dk-md">${deskMd(turn.words)}</div>` : ""}${crewHtml}<div class="dk-tbar"></div>`
      : `<div class="dk-md dk-quiet">${lat && !deskSaidHas(i, lat1) ? deskMd(lat1) : "Working…"}</div>${crewHtml}<div class="dk-tbar"></div>` });
  }
  return rows;
}
/** What item `i`'s holder's crew at work are doing, in a line ("loops-tests editing app.js; ui-fix running npm test"),
 *  while its holder is on this item; "" for a crew member's item or with none at work (#318). */
function deskCrewNow(i) {
  if (!i.holder || i.holder.kind === "crew" || !i.now_on || i.state !== "working") return "";
  const doing = (t) => (t.state === "reviewing" ? "reviewing its diff" : t.step ? t.step.charAt(0).toLowerCase() + t.step.slice(1) : "working");
  return deskCrew(i.holder.sid).filter(deskMateBusy).map((t) => `${t.name} ${doing(t)}`).join("; ");
}
/** The session item `i`'s holder works in itself (a crew member's lead for a crew member), or "". */
const deskHolderSid = (i) => (i?.holder ? (i.holder.kind === "crew" ? i.holder.lead_sid : i.holder.sid) : "");
/** The turn item `i`'s holder is writing now, from its steps (chat/thread.js loadSteps; the Desk follows it while the
 *  page is open, deskFollowSid): what it has said so far, whole, and its steps in a line ("read 3 files · 2 edits ·
 *  now: Ran npm test"). Null till its steps are read, or when its newest turn isn't about this item. */
function deskLiveTurn(i) {
  const sid = deskHolderSid(i), turn = sid && i.holder.kind !== "crew" ? stepFeeds.get(sid)?.turns?.at(-1) : null;
  if (!turn) return null;
  // The turn started on a message naming this item (the hub's now_on is by the same rule), else it's another one's;
  // one naming no item at all is this one's when the hub says it's on it (it holds only this one, #395).
  if (turn.prompt && !deskNames(turn.prompt, i.id) && !(i.now_on && !(deskOf(i.project)?.items || []).some((x) => deskNames(turn.prompt, x.id)))) return null;
  // A turn whose reply is saved on the item is over: at work again (on what you just sent), its new turn isn't read yet
  // (#407).
  if ((i.said || []).some((r) => r.at >= (turn.at_ms || 0))) return null;
  const steps = turn.items.filter((x) => x.t === "step"), going = steps.filter((x) => !x.done).at(-1);
  const done = turnSummary(steps.filter((x) => x.done));
  // Its words as the saved reply will have them (hub/desk_heard.rs replied): the Desk's own lines left out, what a
  // Stop hook made it write after its answer apart (#395).
  const say = (hooked) => deskSaidWords(turn.items.filter((x) => x.t === "say" && hookedPart(turn, x) === hooked).map((x) => x.text).join("\n\n"));
  let words = say(false), after = turn.hooked_ms ? say(true) : "";
  if (!words) [words, after] = [after, ""];
  return { words, after, from: turn.at_ms || 0, steps: [done, going ? `now: ${going.verb}${going.subject ? ` ${going.subject}` : ""}` : ""].filter(Boolean).join(" · ") };
}
/** Whether a reply saved on item `i` has `line` in it (its latest words, already shown as that reply). */
const deskSaidHas = (i, line) => (i.said || []).some((r) => r.text.includes(line) || r.after?.includes(line));
/** Whether `text` names item `id` as "#id" (not the start of a longer number or name), as the hub reads it. */
const deskNames = (text, id) => new RegExp(`#${String(id).replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\w-])`).test(text || "");
/** The lines that are the Desk's (DONE:, ITEM:, DECIDED: …), as the hub knows them (hub/desk_heard.rs desk_line_start). */
const DESK_LINE = /^(?:ITEM|ROUTE|ASK|DONE|CONTEXT|DESIGNS|TITLE|SEE|SPLIT|CUT|MERGE|NOTE|FEATURE|CLOSE|TAKE COPY|LAND FIRST|START|PARK|HOLD|PAUSE|RESUME|ANSWER|PASS|[Dd][Ee][Cc][Ii][Dd][Ee][Dd])[*`]*\s*:/;
/** A turn's words as its saved reply has them: without the Desk's own lines. */
const deskUnmarked = (l) => l.trim().replace(/^`+/, "").replace(/^[-*+ ]+/, "").replace(/^\d+[.)]\s+[-*+ ]*/, "").replace(/^`+/, "");
const deskSaidWords = (text) => String(text || "").split("\n").filter((l) => !DESK_LINE.test(deskUnmarked(l))).join("\n").trim();
/** The turn each item's page last showed being written ({ by, text, after, from: its start, seen: when }), by item:
 *  shown as its reply after the turn ends, till the saved one is in (deskThread). */
const deskLiveKept = new Map();
const DESK_KEEP_MS = 30_000;
/** Under what you wrote to item `i`'s holder (`r`: its words and when): "✓ cue-268 took it · 10:42" once its session
 *  took it, "Waiting: it reads this after its current step" while it works on something before it, else nothing. */
function deskTakenHtml(i, r) {
  const sid = deskHolderSid(i), s = sid && sessionOf(sid);
  if (!s) return "";
  const words = String(r.text || "").trim().slice(0, 80);
  const e = words && (s.thread || []).find((x) => x.role === "you" && x.at_ms >= r.at - 60000 && x.text.includes(`#${i.id}`) && x.text.includes(words));
  const took = e?.delivery?.find((x) => x.what.startsWith("taken"));
  if (took) return `<div class="dk-took"><span class="dk-ck" aria-hidden="true">✓</span><span><b>${esc(i.holder.name)}</b> took it · ${esc(clockAt(took.at_ms))}</span></div>`;
  return e && s.state === "working" ? `<div class="dk-took">Waiting: it reads this after its current step</div>` : "";
}
// Its first line, however it's written: "1. SHAPE:", "1. Shape:", "1. **Shape:**", "1 — Shape was…" (#353).
const DESK_HOOK_START = /^[ \t]*1[.)]?[ \t]*(?:[—–-][ \t]*)?\**[ \t]*shape\b/im;
/** Reply `r` with the stop hook's checklist ("1. SHAPE: …", "2. ASSUMPTIONS: …", the numbered lines after) as its
 *  `after`: the hub keeps it apart from replies since #310, but one it read in a single part, and every reply saved
 *  before, has it in its words (#325). */
function deskHookSplit(r) {
  if (r.after) return r;
  const at = String(r.text || "").search(DESK_HOOK_START);
  if (at < 0) return r;
  return { ...r, text: r.text.slice(0, at).trim(), after: r.text.slice(at).trim() };
}
/** Under a reply: what a Stop hook made its holder write after its answer (a checklist), folded as the chat folds it
 *  (chat/render.js hookFold), so it never reads as the reply (#310). "" when no hook sent it back. */
function deskHookFold(i, r) {
  if (!r.after) return "";
  const key = `said:${i.id}:${r.at}`, open = openFollow.has(key);
  return `<div class="follow"><button class="thread-toggle" data-follow="${esc(key)}">After a stop hook ${open ? "▴" : "▾"}</button>${open ? `<div class="follow-body dk-md">${deskMd(r.after)}</div>` : ""}</div>`;
}
/** Under a reply: a link to that turn in the session that said it, read where it is in its conversation (#276). */
const deskTurnLink = (r) => (r.sid ? `<div class="dk-tl"><button class="dk-tlink" data-desk-turn="${esc(r.sid)}" data-at="${r.at}">This turn in ${esc(r.by || "its session")} ›</button></div>` : "");
/** What you wrote on a Thread, in your words; for a send-back, how many pictures went with it, quiet under them. */
/** Item `i`'s send-backs (steps in `log`) and how many pictures went with each. The hub counts them on the DONE a
 *  send-back answers (hub/desk.rs send_back: `back_pics` on the last DONE then), so by time, not words: a DONE's
 *  count goes to the last send-back after it and before the next DONE (a later one on the same DONE overwrote it). */
function deskBackPics(i, d, log) {
  const out = new Map(), checks = (d.checks || []).filter((k) => k.id === i.id).sort((a, b) => a.at - b.at);
  const backs = log.filter((c) => c.by === "You" && (c.why || "").startsWith("sent back"));
  checks.forEach((k, n) => {
    const next = checks[n + 1]?.at ?? Infinity;
    const b = k.back_pics ? backs.filter((c) => deskMs(c.at) >= deskMs(k.at) && deskMs(c.at) < deskMs(next)).at(-1) : null;
    if (b) out.set(b, k.back_pics);
  });
  return out;
}
const deskBackWords = (text, pics = 0) => `<div class="dk-md">${deskMd(text)}</div>${pics ? `<div class="dk-tpn">${pics === 1 ? "1 picture" : `${pics} pictures`}</div>` : ""}`;
/** An item page's suggested reply (`key` "<project>|<id>"): the words its newest message asks you to reply with
 *  (Reply "merge"), while nothing's at work on it and you haven't written since (#186). */
function deskItemSuggestion(key) {
  const [project, id] = [key.slice(0, key.lastIndexOf("|")), key.slice(key.lastIndexOf("|") + 1)];
  const d = deskOf(project), i = d && deskItem(project, id);
  const last = i && deskThread(i, d).at(-1);
  return last && !last.you && !last.live ? replyAsked(last.text) : "";
}
/** A Thread as a time line: each message's age, a dot (yours and the live one in the accent colour, a send-back's
 *  red), who and what kind, then the words. */
function deskThreadHtml(rows, id = "") {
  const seen = new Map();
  const key = (r) => { const k = r.key || `${r.rec || r.by}:${r.at}`, n = seen.get(k) || 0; seen.set(k, n + 1); return esc(n ? `${k}~${n}` : k); };
  return `<div class="dk-thread"${id ? ` data-thread-for="${esc(id)}"` : ""}>${rows.map((r) => r.rec ? `<div class="dk-tm rec" data-key="${key(r)}"><span class="dk-age">${esc(ago(r.at))}</span><span class="dk-tdot"><i></i></span><div class="dk-tb"><b>${esc(r.rec)}</b> · ${r.html}</div></div>` : `<div class="dk-tm${r.live ? " live" : ""}" data-key="${key(r)}"><span class="dk-age">${r.live ? "now" : esc(ago(r.at))}</span><span class="dk-tdot${r.back ? " back" : r.you || r.live ? " on" : ""}"><i></i></span><div class="dk-tb"><div class="dk-tw"><b>${esc(r.by)}</b>${r.tag ? `<span class="dk-ttag${r.back ? " back" : r.on ? " on" : ""}">${esc(r.tag)}</span>` : ""}</div>${r.html}</div></div>`).join("")}</div>`;
}
function deskItemPane(i, d, foot = "") {
  const all = deskItemLog(i, d).reverse();
  const decs = all.filter((c) => c.call), steps = all.filter((c) => !c.call);
  // Its Files tab (#149): each file it changed with its counts, a click opening its diff; while it's worked on, what's
  // changed so far (the files it's on, till they've changed).
  const changed = deskFilesChanged(i), checking = i.state === "to_check" || i.state === "done";
  const read = deskDiffs.get(i.id)?.files, nFiles = (checking ? read?.length : read?.length || null) ?? (i.commit_wait?.files || i.files || []).length;
  const files = changed ? `<div class="dk-fcs">${changed}</div>` : "";
  // A feature nobody holds is its pieces (#282): its page opens on them.
  const feat = deskFeat(i), thread = deskThread(i, d), tab = deskItemTabs.get(i.id) || (feat && !i.holder ? "pieces" : "thread");
  const h = i.holder, sid = h ? (h.kind === "crew" ? h.lead_sid : h.sid) : null;
  const armed = deskDropArm === i.id;
  const tabBtn = (k, label, n) => `<button class="dk-itab${tab === k ? " on" : ""}" role="tab" aria-selected="${tab === k}" data-desk-itab="${k}" data-id="${esc(i.id)}">${label}<span class="n">${n}</span></button>`;
  // Under the thread, by the newest message and the box: writing why to send it back, what held it up or what it
  // asks, a build log, what it made. Its state, what you'll see, buttons and who has it are in the panel (#200).
  const backing = deskBackFor === i.id;
  const at = `${backing ? deskBackHtml(i.id) : ""}${deskSec("Couldn't give it out", i.route_failed ? `<p>${esc(i.route_failed)}</p>` : "")}${deskSec("Couldn't start", !i.holder && i.start_failed ? `<p>Cue couldn't start a new session for it: ${esc(i.start_failed)}</p>` : "")}
    ${i.state === "needs_you" ? deskChain(i) : deskSec(deskAnswerOf(i) ? "It asked you" : "What's holding it up", deskChain(i))}${deskSec("Build log", deskFinLog(i))}${deskSec("What it made", i.state === "to_check" || i.state === "done" ? deskMadeLinks(i) : "")}
    ${i.part_of ? `<p class="dk-split">Part of <a href="#" data-desk-item="${esc(i.part_of)}">${esc(deskItem(i.project, i.part_of)?.title || `#${i.part_of}`)}</a></p>` : ""}
    ${i.split_from ? `<p class="dk-split">Split from <a href="#" data-desk-item="${esc(i.split_from)}">#${esc(i.split_from)}</a></p>` : ""}`;
  // Its pieces, oldest first (#282): each its state, title, and kind, number, who and age; a click opens it.
  const pieceRow = (p) => `<div class="dk-pc${p.state === "done" ? " done" : ""}" role="button" tabindex="0" data-desk-item="${esc(p.id)}">${deskStateIcon(p)}<div class="dk-pc-t">${esc(p.title)}</div><div class="dk-pc-m">${deskKindIcon(p)}<span>${[esc(deskNum(p)), esc(deskHolderOf(p)), `${ago(deskMs(p.state === "done" ? p.done_at || p.updated : p.created))} ago`].filter(Boolean).join(" · ")}</span></div></div>`;
  const body = tab === "pieces" && feat ? `<div class="dk-pcl">${i.pieces.map(pieceRow).join("")}</div>`
    : tab === "decisions" ? deskCallsBox(decs, `item:${i.id}`, deskDecList, `<p class="dk-note">No decisions on it yet.</p>`)
    : tab === "steps" ? (steps.length ? deskLog(steps) : `<p class="dk-note">No steps yet.</p>`)
    : tab === "files" ? (files ? deskSec(checking ? "Files changed" : deskDiffs.get(i.id)?.files?.length ? "Changed so far" : "Files it's on", files) : `<p class="dk-note">No files on it yet.</p>`)
    : `${deskThreadHtml(thread, `${i.project}|${i.id}`)}<div class="dk-inow">${at}</div>`;
  const main = `<div class="dk-pane dk-ipane" data-scroll-key="item:${esc(i.id)}"><div class="dk-ihead"><div class="dk-itabs" role="tablist">${feat ? tabBtn("pieces", "Pieces", i.pieces.length) : ""}${tabBtn("thread", "Thread", thread.length)}${tabBtn("decisions", "Decisions", decs.filter((c) => !c.minor).length)}${tabBtn("steps", "Steps", steps.length)}${tabBtn("files", "Files", nFiles)}<span class="grow"></span>${i.state !== "done" ? `<button class="dk-drop${armed ? " armed" : ""}" data-desk-act="drop" data-id="${esc(i.id)}">${armed ? "Drop it" : "Drop this item"}</button>` : ""}</div></div>
    <div class="dk-ibody dk-itab-${tab}">${body}</div></div>`;
  // What you'll see as ticks (#382: its Done when is the session's check, not shown here): all ticked once it's to
  // check or done (its session said DONE), none before; "n of m".
  // SHORTCUT: the ticks come from the item's state, so they're all or none: Cue has no status per point. Upgrade: its
  // holder reports each point met, and each tick follows that.
  const pts = deskDoneWhenPoints(i.see), ticked = checking ? pts.length : 0;
  const tk = (y) => `<i class="dk-tk${y ? " y" : ""}" aria-hidden="true">${y ? `<svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>` : ""}</i>`;
  const dw = pts.length ? `<div class="dk-idw"><ul class="dk-ptk">${pts.map((p) => `<li${checking ? ` class="y"` : ""}>${tk(checking)}<span>${linkify(esc(p), sessBase(sid))}</span></li>`).join("")}</ul></div>` : "";
  const done = deskPsec(`You'll see${pts.length > 1 ? ` · ${ticked} of ${pts.length}` : ""}`, `${dw || (i.state === "done" ? "" : `<p class="dk-note">The lead hasn't said yet.</p>`)}${deskCheckInfo(i, d)}`, "dk-pdw");
  const who = h ? (h.kind === "crew" ? `<b>${esc(h.name)}</b>` : deskChatLink(h.sid, h.name, d)) : "";
  const leadOf = h?.kind === "crew" ? deskChatLink(h.lead_sid, deskHolderName(h.lead_sid), d) : "";
  const whoSec = deskPsec("Who", deskPwho("Held by", who ? `${who}${i.crew ? ` → <b>${esc(i.crew)}</b>` : ""}` : "nobody yet") + deskPwho("Lead", leadOf));
  return deskPage({ page: "item", kind: i.kind, back: deskBack(deskLeadName(d) || "Back"),
    acts: sid && sessionOf(sid) ? `<span class="dk-ilinks"><button class="btn small" ${deskChatAttrs(sid, d)}>Watch ${esc(h.name)}</button></span>` : "",
    mark: `${deskNum(i)} · `, title: esc(i.title), meta: `<span class="dk-state">${deskItemMeta(i, d)}</span>`,
    now: deskPnow({ ...deskItemNow(i), acts: `${backing ? deskBackActs(i) : deskLooksTop(i) + deskPickBtns(i) + deskItemActs(i)}${deskBuiltLink(i)}${deskDesignsLink(i)}`, fold: pts.length ? `${ticked} of ${pts.length} done` : "" }),
    panel: done + whoSec + deskItemFiles(i), main, foot });
}
/** The panel's buttons while you write why to send item `i` back (#394): the same ones in the same place, so nothing
 *  jumps: Looks right greyed, Send back pressed (pressing it again closes the box). */
function deskBackActs(i) {
  const looks = i.state === "to_check" && !deskInstallWait(i) ? `<button class="btn small primary" disabled>${deskUncommitted(i) ? "Commit to main" : "Looks right"}</button>` : "";
  return `${looks}<div class="dk-acts"><button class="btn small" data-desk-act="send_back" data-id="${esc(i.id)}" aria-pressed="true">Send back</button></div>`;
}
/** An item's quiet line under its title (#200): who has it, its commit, who added it and when; its state is the
 *  panel's, said once there. */
function deskItemMeta(i, d) {
  const who = i.holder ? `<b>${esc(i.holder.name)}</b>${i.crew ? ` → <b>${esc(i.crew)}</b>` : ""}` : "";
  const cm = i.state === "done" ? i.commit : i.state === "to_check" ? deskFinCommit(i) : "";
  // Landed with no check to run (its repository has none).
  const unchecked = cm && i.finish?.unchecked ? " · landed, not checked" : "";
  // Its project pushes main after each landing: pushed, or why not.
  const pushed = cm && i.finish?.pushed ? ` · ${esc(i.finish.pushed[0].toLowerCase() + i.finish.pushed.slice(1))}` : "";
  return [deskFeat(i) ? deskFeatBar(i, " big") : "", who ? `${i.state === "queued" ? "next for" : i.state === "done" ? "by" : "with"} ${who}` : "", cm ? `committed <span class="dk-cm">${esc(cm)}</span>${unchecked}${pushed}` : "",
    i.tree && i.state !== "done" ? `in its own copy <span class="dk-cm">${esc(i.tree.branch)}</span>` : "",
    `added by ${esc(deskFrom(i, d))}`, `${ago(deskMs(i.created))} ago`].filter(Boolean).join(" · ");
}
/** An item's NOW (#200): its state's dot and word, and one plain sentence: what it waits on you for (#182; a pick on
 *  its mocks, #178) and what that unblocks, else where it is. */
function deskItemNow(i) {
  const pick = deskVerbHtml(i), ub = pick ? deskUnblocksHtml(i).replace("dk-ub", "dk-ub dk-iub") : "";
  const who = esc(i.holder?.name || "Its session"), first = i.after && deskItem(i.project, i.after);
  // A feature nobody holds (#282): where its pieces are.
  const fs = deskFeat(i) && !i.holder ? deskFeatState(i) : null, left = fs && i.pieces.length - deskFeatDone(i);
  const [dot, word, say] = fs === "done" ? ["done", "Done", "All of its pieces are done."]
    : fs ? [fs === "work" ? "work" : "stop", fs === "work" ? "In progress" : "Next", `${left} of its ${i.pieces.length} pieces ${left === 1 ? "is" : "are"} left. Each is given out on its own.`]
    : i.state === "needs_you" ? ["ask", "Waiting on you", `${who} waits for your answer before it goes on. Its question is in its chat.`]
    : deskInstallWait(i) ? ["work", "Waiting for install", "Nothing to try till Cue is installed with it."]
    : deskUncommitted(i) ? ["ask", "Not on main yet", "Its files aren't committed. Commit to main puts them there and closes it."]
    : i.state === "to_check" ? ["check", "Ready to check", ""]
    : i.waiting && i.state === "working" ? ["work", "Waiting", `${who} waits: ${esc(firstLine(i.waiting))}`]
    : i.stopped ? ["stop", "Stopped", "The Desk lead is on it."]
    : i.state === "paused" ? ["stop", "Paused", `${sessionOf(i.holder?.sid)?.state === "working" ? `${who} is stopping at a clean point. ` : `${who} keeps its place. `}The Desk lead paused it${i.paused?.[1] ? `: ${esc(firstLine(i.paused[1]))}` : ""}.`]
    : i.state === "needs_resume" ? ["stop", "Needs resume", (i.restart ? `${esc(firstLine(i.restart))}. ` : "") + (deskCanRestart(i) ? "Resume wakes its session where it left off." : "Its session is closed.")]
    : i.state === "working" ? ["work", "Working", `${who} is on it.${i.now_on && i.latest ? ` ${esc(firstLine(i.latest))}` : ""}${deskCrewNow(i) ? `<span class="dk-crewnow">Crew: ${esc(deskCrewNow(i))}</span>` : ""}`]
    : deskFinishing(i) ? ["work", "Finishing", `<span class="dk-fstep">${deskFinStep(i)}</span>`]
    : deskCopyWaits(i) ? ["stop", "Next", `${esc(deskCopyWaitLine(i))}. It starts by itself when one is free, or now with Start now.`]
    : i.state === "queued" && i.tree_wait ? ["stop", "Next", `Starts when one of its repository's ${deskCopies(deskOf(i.project))} copies is free.`]
    : i.state === "queued" ? ["stop", "Next", first ? `Starts when ${esc(deskShort(first.title))} is done.` : i.holder ? `Next for ${who}.` : "Next up, not given to a session yet."]
    : i.state === "new" ? ["stop", "Not given out", "Not given to a session yet."]
    : i.state === "held" ? ["stop", "Parked", `${i.held?.[1] && i.held[1] !== "You" ? `The Desk lead parked it for you${i.held[2] ? `: ${esc(firstLine(i.held[2]))}` : ""}. ` : "You parked it. "}Nothing starts it until you press Start.`]
    : i.state === "done" ? ["done", "Done", ""] : ["stop", i.state, ""];
  return { dot, word, say: pick ? `<p class="dk-pickl">${pick}</p>${ub}` : say };
}

/** A session's latest words in a line. */
const deskLatestOf = (s) => plain(firstLine([...(s?.thread || [])].reverse().find((e) => e.role === "agent")?.text || ""));
/** What a session's page says of it now (#388): at work, the step it's on (its last reply is from before this turn);
 *  else the first line of its latest reply as its row says it (read from its log, deskSessSaid), and how long ago.
 *  { text, at, now } ({ text: "" } when there's nothing to say). */
function deskLatestNow(s) {
  if (!s) return { text: "" };
  if (deskPstate(s)[0] === "work") return { text: s.compacting_ms ? "Compacting…" : s.doing || "Working on its reply…", at: s.compacting_ms || s.doing_ms || 0, now: true };
  if (typeof deskInsAsk === "function") deskInsAsk(s.session_id);   // its log's numbers, asked for when no card brings them
  const said = deskSessSaid(s.session_id), last = [...(s.thread || [])].reverse().find((e) => e.role === "agent");
  const c = typeof insCache !== "undefined" ? insCache.get(s.session_id) : null;
  // Whichever is newer: its log's numbers are asked for once a minute, its thread moves as each turn ends.
  const fromLog = { text: said, at: c?.last_ms || 0 }, fromThread = { text: deskLatestOf(s), at: last?.at_ms || 0 };
  return said && (!fromThread.text || fromLog.at >= fromThread.at) ? fromLog : fromThread;
}
/** `deskLatestNow` as a line: its words, then how long it's been on that step ("· 3m") or when it replied ("· 2h ago"),
 *  kept current by data-ago. */
const deskLatestHtml = (l) => (l.text ? `${esc(l.text)}${l.at ? ` · <span data-ago="${l.at}">${ago(l.at)}</span>${l.now ? "" : " ago"}` : ""}` : "");
/** A crew member's dot: red when it asks you, filled at work, hollow otherwise. */
const deskMateDot = (lead, t) => (mateNeeds(lead, t) ? "ask" : t.state === "working" ? "" : "idle");
/** A lead (its crew, what's next, what it decided, its latest) or a session on its own. */
function deskHolderPane(sid, d, foot = "") {
  const s = sessionOf(sid);
  const mine = deskItemsWithRequests(d).filter((i) => deskGroupKey(i.holder) === sid && i.state !== "done");
  const name = deskHolderName(sid, mine.find((i) => i.holder?.kind !== "crew")?.holder?.name);
  const crew = deskCrew(sid), lead = deskIsLead(sid, mine);
  const waits = mine.filter(deskWaits).length;
  const head = [`${mine.length} item${mine.length === 1 ? "" : "s"}${waits ? `, ${waits} waiting on you` : ""}`, lead ? (crew.length ? `crew of ${crew.length}` : "on its own") : "", s?.since_ms ? `since ${clockAt(s.since_ms)}` : ""].filter(Boolean).join(" · ");
  const crewRows = crew.map((t) => {
    const it = deskCrewItem(d.project, sid, t.agent_id || t.call);
    const files = mateShownFiles(t).map((f) => f.split("/").pop()).join(", ");
    const line = [it ? it.title : t.what, mateNeeds(sid, t) ? "stopped to ask you" : t.step, files].filter(Boolean).join(" · ");
    return `<div class="dk-mb" role="button" tabindex="0" data-desk-crew="${esc(t.agent_id || t.call)}" data-lead="${esc(sid)}"><i class="${deskMateDot(sid, t)}"></i><div class="dk-lt">${esc(t.name)}</div><span class="dk-age">${esc(mateSpan(t))}</span><div class="dk-ls">${esc(line)}</div></div>`;
  }).join("");
  const next = mine.find((i) => i.state === "queued");
  const nextLine = next ? `<p class="dk-note">Next for this ${lead ? "lead" : "session"}: ${esc(next.title)}${next.after ? `, after ${esc(deskItem(d.project, next.after)?.title || "")}` : ""}.</p>` : "";
  const decs = d.decisions.filter((c) => c.call && (c.sid === sid || c.by === name));
  const decHtml = decs.length ? deskLog(decs, (c) => (c.item_id && deskItem(d.project, c.item_id) ? esc(deskItem(d.project, c.item_id).title) : "")) : "";
  const latest = deskLatestHtml(deskLatestNow(s));
  // One page (#200): its crew, what's next, its calls and latest at the left; in the panel, what of it waits on you
  // (Open #190), else its state, and its crew and items counted.
  const kind = deskSessKind(sid, d), wait = mine.find(deskWaits);
  const [dot, word] = deskPstate(s);
  const now = wait ? deskPnow({ dot: "ask", word: "Waiting on you", say: `${deskKindIcon(wait)}<b>${esc(deskNum(wait))}</b> ${deskVerbHtml(wait) || esc(wait.title)}`, acts: `<button class="btn small primary" data-desk-chk-item="${esc(wait.id)}">Open #${esc(wait.id)}</button>`, fold: esc(head) })
    : deskPnow({ dot, word, say: latest, fold: esc(head) });
  const by = (f) => crew.filter(f).length;
  const counts = [[by((t) => mateNeeds(sid, t)), "ask you"], [by((t) => !mateNeeds(sid, t) && t.state === "working"), "working"], [by((t) => !mateNeeds(sid, t) && t.state !== "working" && t.state !== "done"), "stopped"], [by((t) => t.state === "done"), "done"]].filter(([n]) => n).map(([n, w]) => `${n} ${w}`).join(" · ");
  const who = deskPsec("Who", deskPwho("Crew", lead ? counts || "none at work" : "") + deskPwho("Items", `${mine.length} open${waits ? ` · ${waits} ${waits === 1 ? "waits" : "wait"} on you` : ""}`));
  const main = `<div class="dk-pane" data-scroll-key="holder:${esc(sid)}">
    ${lead ? deskSec("Crew", crewRows ? `<div class="dk-crew">${crewRows}</div>` : `<p class="dk-note">No crew at work.</p>`) : ""}${nextLine}
    ${deskSec(`Decided by ${esc(name)} · ${decs.length}`, decHtml)}
    ${deskSec(`Latest from ${esc(name)}`, latest ? `<p class="dk-quiet">${latest}</p>` : "")}</div>`;
  return deskPage({ page: kind === "lead" ? "lead" : kind === "desk" ? "dispatcher" : "session", kind, back: deskBack(deskLeadName(d) || "Back"),
    acts: s ? `<button class="btn small" ${deskChatAttrs(sid, d)}>Open its chat</button>` : "", mark: `${DESK_MARKS[kind]} · `, title: esc(name), meta: esc(head), now, panel: who + deskSessFiles(sid), main, foot });
}
/** The crew member whose ⋯ menu is open on its page ("lead|agent"), so a redraw while it works keeps it open. */
let deskCrewMenu = "";
/** A crew member (#303, pick A): three heads in its own column, Doing (its step and the last few before it, from the
 *  lead's feed when it's read, team_mate.js crewMateSteps), Why (what the lead asked it, and the item it's for) and
 *  Result (its answer once back; till then what it changed so far, with its Diff). Stop it and ⋯ (Open its session)
 *  in the panel. No box of its own: the box goes to its lead. */
function deskCrewPane(lead, agent, d, foot = "") {
  const t = deskMate(lead, agent);
  const leadName = deskHolderName(lead), skey = `crew:${esc(lead)}:${esc(agent)}`;
  if (!t) return `<div class="dk-pane" data-scroll-key="${skey}">${deskBack(leadName, lead)}<div class="dk-empty">It isn't in ${esc(leadName)}'s crew any more.</div></div>${foot}`;
  const it = deskCrewItem(d.project, lead, agent), key = t.agent_id || t.call;
  const shown = mateShownFiles(t), working = t.state === "working";
  const stop = working ? `<button class="btn small" data-mt-act="stop" data-mt-sid="${esc(lead)}" data-mt-agent="${esc(key)}">Stop it</button>` : "";
  const asks = mateNeeds(lead, t);
  const [dot, word] = asks ? ["ask", "Waiting on you"] : working ? ["work", "Working"] : [t.state === "done" ? "done" : "stop", t.state ? t.state[0].toUpperCase() + t.state.slice(1) : "Stopped"];
  const say = asks ? "Stopped to ask you something." : t.step ? crewCode(crewStepCode(t.step)) : mateLine(t);
  const mk = `${lead}|${key}`, menuOpen = deskCrewMenu === mk;
  const more = `<span class="dk-cmore"><button class="btn small" data-desk-crew-more="${esc(mk)}" aria-expanded="${menuOpen}" aria-label="More">⋯</button>`
    + (menuOpen ? `<span class="dk-cmenu" role="menu"><button role="menuitem" data-desk-crew-open="${esc(key)}" data-lead="${esc(lead)}">Open its session</button></span>` : "") + `</span>`;
  const leadBtn = deskChatLink(lead, leadName, d);
  const who = deskPsec("Who", deskPwho("Lead", leadBtn) + deskPwho("On", it ? deskItemLink(it) : ""));
  // Doing: what it's on now in words and how long, then the last few steps (✓ done, ● now, ? its ask).
  const steps = crewMateSteps(lead, t).slice(-5);
  const head = asks ? "Stopped to ask you something" : working ? (t.step ? crewCode(crewStepCode(t.step)) : "At work") : esc(word);
  const age = working && t.step_ms ? ` <span class="dk-quiet">· ${esc(ago(t.step_ms))}</span>` : "";
  const list = steps.length > 1 || (steps.length && !working) ? `<ol class="dk-csteps">${steps.map((x) => `<li class="${x.ask ? "ask" : x.now ? "now" : ""}"><i aria-hidden="true">${x.ask ? "?" : x.now ? "●" : "✓"}</i><span>${x.html}</span></li>`).join("")}</ol>` : "";
  const doing = `<p class="dk-cnow">${head}${age}</p>${list}`;
  // Why: the lead's own words to it, quoted, and the item it's for.
  const asked = (t.asked || t.what || "").trim();
  const why = (asked ? `<blockquote class="dk-cwhy"><div class="dk-cwho">${esc(leadName)} asked it</div><div class="dk-md">${deskMd(asked)}</div></blockquote>` : "")
    + (it ? `<p class="dk-quiet">For ${deskItemLink(it)} ${esc(it.title || "")}</p>` : "");
  // Result: its answer once back; till then what it changed so far.
  const diff = mateBtn(lead, t, "diff", "Diff", "link");
  const size = t.stat || (shown.length ? plural(shown.length, "file") : ""), files = shown.length ? `<div class="dk-files">${shown.map(esc).join(" · ")}</div>` : "";
  const sofar = [size, diff].filter(Boolean).join(" · ");
  const result = t.summary ? `<div class="dk-md">${deskMd(t.summary)}</div>${sofar ? `<p class="dk-quiet">${sofar}</p>` : ""}${files}`
    : `<p class="dk-quiet">${working ? "Not back yet." : "Nothing came back."}${sofar ? ` So far: ${sofar}` : ""}</p>${files}`;
  const main = `<div class="dk-pane dk-crewpg" data-scroll-key="${skey}">
    ${deskSec(`Doing${t.started_ms ? ` <span class="dk-csp">${esc(mateSpan(t))}</span>` : ""}`, doing)}
    ${deskSec("Why", why)}
    ${deskSec("Result", result)}</div>`;
  return deskPage({ page: "crew", kind: "crew", back: deskBack(leadName, lead), mark: "Crew · ", title: esc(t.name),
    meta: `in <b>${esc(leadName)}</b>'s crew${it?.title || t.what ? ` · on <b>${esc(it?.title || t.what)}</b>` : ""} · ${esc(mateSpan(t))}`,
    now: deskPnow({ dot, word, say, acts: `${stop}${more}`, fold: `in ${esc(leadName)}'s crew` }),
    panel: who + deskCrewFiles(lead, t), main, foot });
}
