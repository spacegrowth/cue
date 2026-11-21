// Cue UI desk — what Cue itself sends a session or a Desk lead (#478), as one short plain line wherever the message
// shows: a session's chat, an item's page (its holder's chat), the lead's chat, and the Inbox. "Cue · Asked cue-383 ·
// #461 not done: “Are we done here completely?” · 3m", Cue's full text folded under it (a click on the line opens it).
// The session still gets the full text; only how the chat draws it changes. Every kind is listed in
// docs/cue-messages.md, with where it shows. Loaded after desk/lead.js (index.html, mini.html).

/** Each kind of message Cue sends, by how it starts (the hub's own words, quoted in docs/cue-messages.md): [pattern,
 *  (match, text) => { verb, id, what, more }, tag]: `tag` is the card's word for its kind (NOT DONE, NOTE…, #501). `verb` and the session's name make "Asked cue-383"; `id` is the item it's
 *  about (its number opens it); `what` says what it asks or tells, plain (`more`: a count of what's folded besides).
 *  Its standing words for the session (Say DONE…, Its fixes stay in…) are never in the line, only in the fold. */
const cueQ = (t, max = 90) => `“${cueCut(String(t ?? "").replace(/\.$/, ""), max)}”`;
const cueLines = (t) => String(t).split("\n").map((l) => l.trim()).filter(Boolean);
const cueMore = (n, one, many = `${one}s`) => (n > 0 ? `${n} ${n === 1 ? one : many}` : "");
const CUE_ID = String.raw`#([\w.-]+)`;
const cueRe = (src, flags = "") => new RegExp(src.replaceAll("#ID", CUE_ID), flags);
const CUE_MSGS = [
  // To the session on an item (its holder, or a teammate through its lead).
  [cueRe(String.raw`^Desk item #ID \((.*)\) isn't done: ([^\n]*)`), (m, t) => ({ verb: "Asked", id: m[1], what: `not done: ${cueQ(m[3])}`, more: /\nThe end of its log:/.test(t) ? "its log" : "" }), "Not done"],
  [cueRe(String.raw`^Desk item #ID \((.*)\) is open again: ([^\n]*?)\. Put it back`), (m) => ({ verb: "Asked", id: m[1], what: `open again: ${cueQ(m[3])}` }), "Open again"],
  [cueRe(String.raw`^Desk item #ID \((.*)\): you stopped before it was done`), (m) => ({ verb: "Asked", id: m[1], what: "pick up where it stopped" }), "Pick up"],
  [cueRe(String.raw`^Note from the Desk lead on Desk item #ID \((.*)\): ([^]*)$`), (m) => ({ verb: "Told", id: m[1], what: `the lead's note: ${cueQ(m[3])}` }), "Lead's note"],
  [cueRe(String.raw`^Desk item #ID \((.*)\) has moved to (.+?): stop work on it`), (m) => ({ verb: "Told", id: m[1], what: `moved to ${m[3]}: stop work on it` }), "Moved"],
  [cueRe(String.raw`^Desk item #ID \((.*)\) is parked: it's off your list`), (m) => ({ verb: "Told", id: m[1], what: "parked: off its list" }), "Parked"],
  [cueRe(String.raw`^Desk item #ID is paused by the Desk lead(?: \((.*?)\))?\. Stop`), (m) => ({ verb: "Asked", id: m[1], what: `paused${m[2] ? ` (${m[2]})` : ""}: stop at a clean point` }), "Paused"],
  [cueRe(String.raw`^Desk item #ID is resumed(?: \((.*?)\))?: carry on`), (m) => ({ verb: "Told", id: m[1], what: `resumed${m[2] ? ` (${m[2]})` : ""}: carry on` }), "Resumed"],
  [cueRe(String.raw`^Desk item #ID: the Desk lead took back its copy`), (m) => ({ verb: "Told", id: m[1], what: "copy taken back: stop work on it" }), "Taken back"],
  [cueRe(String.raw`^Desk item #ID is closed: its work is already on (\S+)`), (m) => ({ verb: "Told", id: m[1], what: `closed: already on ${m[2]}` }), "Closed"],
  [cueRe(String.raw`^Desk item #ID was dropped: stop work on it`), (m) => ({ verb: "Told", id: m[1], what: "dropped: stop work on it" }), "Dropped"],
  [cueRe(String.raw`^Desk item #ID \((.*)\): option (\S+) was picked, so it's done\. #([\w.-]+) builds it`), (m) => ({ verb: "Told", id: m[1], what: `option ${m[3]} picked: done, #${m[4]} builds it` }), "Picked"],
  [cueRe(String.raw`^Desk item #ID was folded into #([\w.-]+), which you have\. It said: ([^\n]*)`), (m) => ({ verb: "Told", id: m[1], what: `folded into #${m[2]}: ${cueQ(m[3])}` }), "Folded"],
  [cueRe(String.raw`^Done\? Write DONE: #ID, or say what's left`), (m, t) => cueLines(t).every((l) => /^Done\? Write DONE: #/.test(l)) && { verb: "Asked", id: m[1], what: "done? or what's left", more: cueMore(cueLines(t).length - 1, "more item") }, "Done?"],
  [/^The Desk is paused\. Stop at the next safe point/, () => ({ verb: "Asked", what: "the Desk is paused: stop at a safe point" }), "Paused"],
  [/^The Desk is resumed: carry on with (.+?) from where you stopped/, (m) => ({ verb: "Told", what: `the Desk is resumed: carry on with ${m[1]}` }), "Resumed"],
  [/^Your TO LEAD: line didn't reach the Desk's lead: ([^\n]*)/, (m) => ({ verb: "Told", what: `TO LEAD didn't reach the lead: ${cueQ(m[1])}` }), "To lead"],
  [/^TO LEAD: not sent: ([^\n]*)/, (m) => ({ verb: "Told", what: `TO LEAD not sent: ${cueQ(m[1])}` }), "To lead"],
  [cueRe(String.raw`^Not added as a new item: (.+?)\. Its fix or next step stays in #ID`), (m) => ({ verb: "Told", id: m[2], what: `not a new item: ${cueQ(m[1])} stays here` }), "Not new"],
  [/^Several Desks could receive this item/, () => ({ verb: "Told", what: "item not added: several Desks could take it" }), "Not added"],
  [/^.+? answered the steps held while they were away/, (m, t) => {
    const ok = (t.match(/ALLOWED: run it now/g) || []).length, no = (t.match(/NO: don't run it/g) || []).length;
    return { verb: "Told", what: `held steps answered: ${[ok && `${ok} allowed`, no && `${no} refused`].filter(Boolean).join(", ") || "none"}` };
  }, "Answered"],
  [/^Ask me before, changed by .+? just now: (?:ask .+? first, and end your turn, before (.+?)\. This replaces|(nothing is set))/, (m) => ({ verb: "Told", what: m[2] ? "Ask me before changed: nothing set" : `Ask me before changed: before ${m[1]}` }), "Ask me before"],
  [cueRe(String.raw`^Go back on your call(?: on #ID)?: you decided (.+?)(?: \(your reason: .*?\))?\. (?:The user's answer instead|.+?'s answer instead|Undo it)`), (m) => ({ verb: "Asked", id: m[1] || "", what: `go back on: ${cueQ(m[2])}` }), "Go back"],
  [/^(Bring (.+?) back: it was set aside|Make a revert: (.+?): |Undo this: (.+?): )/, (m, t) => ({ verb: "Asked", what: m[2] ? `bring ${m[2]} back` : `revert: ${cueQ(t.split("\n")[0].replace(/^[^:]+: /, ""))}` }), "Undo"],
  [/^(?:\(Cue\) )?Your teammate (.+?) has (?:gone quiet|been quiet for (\d+) min)/, (m) => ({ verb: "Asked", what: `check on teammate ${m[1]}${m[2] ? ` (quiet ${m[2]} min)` : ""}` }), "Crew"],
  [/^Hand off: write a short note so a fresh session can carry on/, () => ({ verb: "Asked", what: "hand off: write a note for a fresh session" }), "Hand-off"],
  [/^Carry on from (an earlier session in this folder|the previous session's hand-off)/, (m) => ({ verb: "Asked", what: m[1].startsWith("an") ? "carry on from an earlier session" : "carry on from the hand-off note" }), "Carry on"],
  [/^You no longer run (.+?)'s Desk/, (m) => ({ verb: "Told", what: `no longer runs ${m[1]}'s Desk` }), "Not lead"],
  [/^The team from the session before you\./, (m, t) => ({ verb: "Told", what: "the team from the session before", more: cueMore(cueLines(t).filter((l) => l.startsWith("- ")).length, "teammate") }), "Crew"],
  // To a Desk lead.
  [cueRe(String.raw`^(.+?), on #ID, asks about clearing the way for its landing[^\n]*\n([^\n]*)`), (m, t) => ({ verb: "Asked", id: m[2], what: `${m[1]}'s landing question: ${cueQ(m[3])}`, more: cueMore(cueLines(t).filter((l) => /^\d+\. /.test(l)).length, "choice") }), "Landing"],
  [/^Circle on the Desk: (.+?)\. None of them can move/, (m) => ({ verb: "Asked", what: `cut a wait circle: ${m[1]}` }), "Waits"],
  [cueRe(String.raw`^Cue couldn't start a new session for #ID \((.*)\): (.+?)\. It's still`), (m) => ({ verb: "Told", id: m[1], what: `no new session: ${cueQ(m[3])}` }), "Couldn't start"],
  [/^The Desk is paused \(by (.+?), /, (m) => ({ verb: "Told", what: `the Desk is paused (by ${m[1]})` }), "Paused"],
  [/^The Desk is resumed: its sessions carry on/, () => ({ verb: "Told", what: "the Desk is resumed" }), "Resumed"],
  [cueRe(String.raw`^Your ASK(?: about #ID)?: ([^\n]*)\nThe answer: ([^\n]*)`), (m) => ({ verb: "Told", id: m[1] || "", what: `its ASK answered: ${cueQ(m[3], 70)}` }), "Answered"],
  [cueRe(String.raw`^Cue asked .+? before routing #ID to (.+?), which isn't on the Desk\. The answer: (.*?)\. #`), (m) => ({ verb: "Told", id: m[1], what: `not routed to ${m[2]}: answer ${cueQ(m[3], 60)}` }), "Routed"],
  [cueRe(String.raw`^.+? said yes: (.+?) is on the Desk now, and Cue routed #ID`), (m) => ({ verb: "Told", id: m[2], what: `routed to ${m[1]} (a yes)` }), "Routed"],
  [cueRe(String.raw`^.+? said yes, but Cue couldn't route #ID to (.+?)\. ([^\n]*)`), (m) => ({ verb: "Told", id: m[1], what: `not routed to ${m[2]}: ${cueQ(m[3], 70)}` }), "Not routed"],
  [/^(?:Not asked: LANDS: .+|.+? said (?:no to changing how|yes: how|yes, but how) work lands on .+)/, (m, t) => ({ verb: "Told", what: cueCut(t.split("\n")[0], 90) }), "Landing"],
  [/^The owner is away: Cue didn't show them (this ASK|these ASKs)|^.+? is away: Cue didn't show them (this ASK|these ASKs)/, (m, t) => ({ verb: "Asked", what: "decide the open ASKs yourself (away)", more: cueMore(cueLines(t).filter((l) => l.startsWith("- ")).length, "ASK") }), "Away"],
  [/^(?:The owner|.+?) is about to go away\. Go through every open item/, () => ({ verb: "Asked", what: "settle what you can before Away" }), "Away"],
  [/^Away mode is off/, () => ({ verb: "Told", what: "Away is off" }), "Away"],
  // The Desk's changes since the lead last heard (desk/hub/lead/feed.rs, #616): a paragraph each, numbered at the end.
  [/^([\s\S]+?)\n\n\(Desk changes [^)\n]*\)$/, (m) => cueChanges(m[1]), "Desk changes"],
  [cueRe(String.raw`^Queued: #ID is next for (.+?), after #([\w.-]+)`), (m, t) => ({ verb: "Told", id: m[1], what: `next for ${m[2]}, after #${m[3]}`, more: cueMore(cueLines(t).length - 1, "more queued", "more queued") }), "Queued"],
  [/^Cue asked .+?, for the leads of .+?: ([^\n]*)\nThe user's answer: ([^\n]*?)\./, (m) => ({ verb: "Told", what: `your answer: ${cueQ(m[2], 60)}` }), "Answered"],
  [/^Away mode:\n(?:Away is paused: ([^.\n]+)|(?:The owner|.+?) is away (until [^,\n]+))/, (m) => ({ verb: "Told", what: m[1] ? `Away paused: ${m[1]}` : `Away is on, ${m[2]}` }), "Away"],
  // Replies to the lead's own command lines (Refused:, not cut, not split, …): its first line, the rest folded.
  [cueRe(String.raw`^(?:Refused: |Noted: |#ID (?:note )?(?:not|copy not) |Couldn't read this line as a route|No question from #ID|FEATURE #ID: left out|Cue couldn't (?:route|queue|start) |Cue didn't reopen )`), (m, t) => {
    const ls = cueLines(t), first = ls[0].split(/(?<=\.) /)[0].replace(/\.$/, "");
    // A reply about one item ("#d5 not cut: …") puts that item in the card's head, as every other line does.
    const [, id, rest] = /^#(\S+) (.+)$/.exec(first) || [];
    return { verb: "Told", ...(id ? { id } : {}), what: cueCut((rest || first).replace(/^Refused: /, "refused: "), 100), more: cueMore(ls.length - 1, "more line"), tag: /^Refused: /.test(first) ? "Refused" : /^Noted: /.test(first) ? "Noted" : "Reply" };
  }, "Reply"],
];

/** How each of the Desk's changes reads in its line (desk/hub/lead/feed.rs, its rows' words): [what it is, pattern on
 *  the row]. The first that fits names the row; one that fits none is "changed". */
const CUE_CHANGES = [
  ["sent back", / sent #[\w.-]+ \(.*\) back to its holder/], ["asked", /^.+? asked #[\w.-]+ \(.*\)'s holder/],
  ["restarted", / restarted #/], ["dropped", / dropped #/], ["moved up", / put #[\w.-]+ \(.*\) before #/],
  ["new", /^New item #/], ["finished", /^#[\w.-]+ \(.*\) finished: /], ["stuck finishing", /^#[\w.-]+ \(.*\) is stuck finishing/],
  ["clashed", /^#[\w.-]+ \(.*\) clashed with the main line/], ["stopped", /^#[\w.-]+ \(.*\) stopped/],
  ["asks you", /^#[\w.-]+ \(.*\)'s holder .*? asks /], ["holder gone", /^#[\w.-]+ \(.*\)'s holder /],
];
/** The Desk's changes `t` (their paragraphs, the numbers line dropped) as one line: "#528, #561 finished · #535 sent
 *  back". A row's follow-up about an item already in it (where your words went) isn't counted again. */
function cueChanges(t) {
  const rows = t.split(/\n\n+/).map((r) => r.trim()).filter(Boolean);
  const by = new Map(), seen = new Set();
  for (const r of rows) {
    const id = /#([\w.-]+)/.exec(r)?.[1];
    const kind = CUE_CHANGES.find(([, re]) => re.test(r))?.[0] || "changed";
    if (kind === "changed" && id && seen.has(id)) continue;
    if (id) seen.add(id);
    by.set(kind, [...(by.get(kind) || []), id ? `#${id}` : ""]);
  }
  const one = seen.size === 1 && by.size === 1;
  // One item: what happened to it in words, and what you said with it.
  const said = one && /: "([^\n]*)"\.?$/.exec(rows[0])?.[1];
  const alone = { "sent back": "you sent it back", asked: "you asked its holder", restarted: "you restarted it", dropped: "you dropped it", "moved up": "you moved it up", new: "a new item" };
  const what = [...by].map(([k, ids]) => (one ? `${alone[k] || k}${said ? `: ${cueQ(said, 60)}` : ""}` : `${ids.filter(Boolean).join(", ")} ${k}`.trim())).join(" · ");
  return { verb: "Told", ...(one ? { id: [...seen][0] } : {}), what: cueCut(what, 100) };
}

/** The full text's first words up to the end of a sentence (or `max` characters, cut at a word). */
function cueCut(t, max = 110) {
  t = String(t ?? "").replace(/[`*]/g, "").replace(/\s+/g, " ").trim();
  if (t.length <= max) return t;
  return t.slice(0, max).replace(/\s+\S*$/, "") + "…";
}
/** A teammate's message ("For your teammate X: …"): the teammate, and the text without it. */
const cueMate = (text) => { const m = /^For your teammate (.+?): /.exec(text); return m ? [m[1], text.slice(m[0].length)] : ["", text]; };

/** What Cue's message `text` (as the session got it, its `<desk>` block dropped) says, as { verb, id, what, more,
 *  mate, tag }; null when it isn't one of Cue's (your own words, the lead's, a hand-off card's). */
function cueMsgOf(text) {
  const raw = deskSaid(String(text ?? "")).trim();
  if (!raw) return null;
  const [mate, t] = cueMate(raw);
  for (const [re, f, tag] of CUE_MSGS) {
    const m = re.exec(t);
    if (!m) continue;
    const r = f(m, t);
    if (r) return { mate, tag, ...r };
  }
  return null;
}

/** That line in plain words, for a one-line place (the Inbox, a session's card, "Working on: …"): "Asked cue-383 · #461
 *  not done: “Are we done here completely?”". `to`: who it went to (its session's name). null for any other text. */
function cueMsgPlain(text, to = "") {
  const c = cueMsgOf(text);
  if (!c) return null;
  const who = c.mate || to;
  return `${c.verb}${who ? ` ${who}` : ""}${c.id ? ` · #${c.id}` : ""}${c.what ? `${c.id ? " " : " · "}${c.what}` : ""}`;
}
/** Cue's message `text` as its plain line in the message family's words (#501, .mg-plain): "Asked <who> · #id what",
 *  the session's name as .mg-who, the item as .mg-id, what it says as .mg-w; null for any other text. */
function cueMsgPlainHtml(text, to = "") {
  const c = cueMsgOf(text);
  if (!c) return null;
  const who = c.mate || to;
  return `${esc(c.verb)}${who ? ` <span class="mg-who">${esc(who)}</span>` : ""}${c.id ? ` · <span class="mg-id">#${esc(c.id)}</span>` : ""}${c.what ? `${c.id ? " " : " · "}<span class="mg-w">${esc(c.what)}</span>` : ""}`;
}

/** Cue's message in a chat (#501, messages.css): the family's card. Its head "◔ Cue → cue-383 · NOT DONE · #461 · 3m",
 *  its words "Asked: not done: “…”" in two lines, More opening Cue's full text under them (`key`: what keeps it open,
 *  openAsks, as + What Cue sent does). `m`: its message state when it hasn't gone (msgLine's: sending, held,
 *  delivered…), its foot's status as under any message (#422). null for any other text. */
function cueMsgHtml(e, s, key, m = null) {
  const c = cueMsgOf(e.text);
  if (!c) return null;
  const who = c.mate || (s ? nameOf(s.session_id, s.project) : "");
  const it = c.id ? (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, c.id)) : null;
  const d = it && deskOf(it.project);
  const num = c.id ? (d ? `<button class="mg-id" data-desk-hand="${esc(d.project)}" data-id="${esc(it.id)}">${deskKindIcon(it)}#${esc(c.id)}</button>` : `<span class="mg-id">#${esc(c.id)}</span>`) : "";
  const open = openAsks.has(key), full = deskSaid(String(e.text ?? "")).trim();
  const head = `${cueMark()}<span class="mg-who">Cue</span>${who ? `<span class="mg-to">→ ${esc(who)}</span>` : ""}<span class="mg-tag">${esc(c.tag)}</span>${num}${e.at_ms ? `<span class="mg-age">${ago(e.at_ms)}</span>` : ""}`;
  const what = c.what || c.verb;
  const words = `${esc(what[0].toUpperCase() + what.slice(1))}${c.more ? ` <span class="cue-more">· ${esc(c.more)}</span>` : ""}`;
  const lk = `<button class="mg-lk" data-ask-open="${esc(key)}" aria-expanded="${open}">${open ? "Less" : "More"}</button>`;
  const foot = m ? msgLine({ ...m, acts: `${m.acts ? `${m.acts} ` : ""}${lk}` }) : `<div class="mg-f"><span class="mg-acts">${lk}</span></div>`;
  return `<div class="mg cue-msg${open ? " open" : ""}${m && m.st !== "delivered" ? ` ${m.st}` : ""}"><div class="mg-h">${head}</div><div class="mg-t">${words}</div>`
    + `${open ? `<div class="mg-full">${esc(full)}</div>` : ""}${foot}</div>`;
}
