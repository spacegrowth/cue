// Cue UI desk — what Cue itself sends a session or a Desk lead (#478), as one short plain line wherever the message
// shows: a session's chat, an item's page (its holder's chat), and the lead's chat. "Cue · Asked cue-383 ·
// #461 not done: “Are we done here completely?” · 3m", Cue's full text folded under it (a click on the line opens it).
// The session still gets the full text; only how the chat draws it changes. Every kind is listed in
// docs/cue-messages.md, with where it shows. Loaded after desk/lead.js (index.html, mini.html).

/** Each kind of message Cue sends, by how it starts (the hub's own words, quoted in docs/cue-messages.md): [pattern,
 *  (match, text) => { verb, id, what, more }, tag]: `tag` is the card's word for its kind (NOT DONE, NOTE…, #501). `verb` and the session's name make "Asked cue-383"; `id` is the item it's
 *  about (its number opens it); `what` says what it asks or tells, plain (`more`: a count of what's folded besides).
 *  Its standing words for the session (Say DONE…, Its fixes stay in…) are never shown, not even opened (#625): what
 *  opens is cueFoldHtml's rows, from `by` (whose words), `said` (those words, whole), `label` (their name when they
 *  aren't someone's: "Why", "Its call"…) and `next` (what happens next, `{who}` the session it went to). */
const cueQ = (t, max = 90) => `“${cueCut(String(t ?? "").replace(/\.$/, ""), max)}”`;
const cueLines = (t) => String(t).split("\n").map((l) => l.trim()).filter(Boolean);
const cueMore = (n, one, many = `${one}s`) => (n > 0 ? `${n} ${n === 1 ? one : many}` : "");
const CUE_ID = String.raw`#([\w.-]+)`;
const cueRe = (src, flags = "") => new RegExp(src.replaceAll("#ID", CUE_ID), flags);
const CUE_MSGS = [
  // To the session on an item (its holder, or a teammate through its lead).
  [cueRe(String.raw`^Desk item #ID \((.*)\) isn't done(?: \(you said it looks right, and now it's reopened\))?: ([^\n]*)`), (m, t) => { const log = /\nThe end of its log:/.test(t); return { verb: "Asked", id: m[1], what: `not done: ${cueQ(m[3])}`, more: log ? "its log" : "", by: log ? "" : "You", said: cueSentBack(t), next: "{who} fixes it and says it's done" }; }, "Not done"],
  [cueRe(String.raw`^Desk item #ID \((.*)\) is open again: ([^\n]*?)\. Put it back`), (m) => ({ verb: "Asked", id: m[1], what: `open again: ${cueQ(m[3])}`, said: m[3], next: "{who} puts its work back on main" }), "Open again"],
  [cueRe(String.raw`^Desk item #ID \((.*)\): you stopped before it was done`), (m) => ({ verb: "Asked", id: m[1], what: "pick up where it stopped", next: "{who} picks up where it stopped" }), "Pick up"],
  [cueRe(String.raw`^Note from the Desk lead on Desk item #ID \((.*)\): ([^]*)$`), (m) => ({ verb: "Told", id: m[1], what: `the lead's note: ${cueQ(m[3])}`, by: "The lead", said: m[3], next: "{who} reads it and carries on" }), "Lead's note"],
  [cueRe(String.raw`^Desk item #ID \((.*)\) has moved to (.+?): stop work on it`), (m) => ({ verb: "Told", id: m[1], what: `moved to ${m[3]}: stop work on it`, next: `{who} stops; ${m[3]} takes it over` }), "Moved"],
  [cueRe(String.raw`^Desk item #ID \((.*)\) is parked: it's off your list`), (m) => ({ verb: "Told", id: m[1], what: "parked: off its list", next: "{who} leaves it; Cue tells it if the item comes back" }), "Parked"],
  [cueRe(String.raw`^Desk item #ID is paused by the Desk lead(?: \((.*?)\))?\. Stop`), (m) => ({ verb: "Asked", id: m[1], what: `paused${m[2] ? ` (${m[2]})` : ""}: stop at a clean point`, said: m[2] || "", next: "{who} stops at a clean point and waits to be resumed" }), "Paused"],
  [cueRe(String.raw`^Desk item #ID is resumed(?: \((.*?)\))?: carry on`), (m) => ({ verb: "Told", id: m[1], what: `resumed${m[2] ? ` (${m[2]})` : ""}: carry on`, next: "{who} carries on from where it stopped" }), "Resumed"],
  [cueRe(String.raw`^Desk item #ID: the Desk lead took back its copy`), (m) => ({ verb: "Told", id: m[1], what: "copy taken back: stop work on it", next: "{who} stops until it's routed to it again" }), "Taken back"],
  [cueRe(String.raw`^Desk item #ID is closed: its work is already on (\S+)`), (m) => ({ verb: "Told", id: m[1], what: `closed: already on ${m[2]}`, next: `Nothing more: its work is already on ${m[2]}` }), "Closed"],
  [cueRe(String.raw`^Desk item #ID was dropped: stop work on it`), (m) => ({ verb: "Told", id: m[1], what: "dropped: stop work on it", next: "{who} stops work on it" }), "Dropped"],
  [cueRe(String.raw`^Desk item #ID \((.*)\): option (\S+) was picked, so it's done\. (?:#([\w.-]+) builds it|(Nothing follows it)|((?:#[\w.-]+, )+#[\w.-]+) follow it)`), (m) => ({ verb: "Told", id: m[1], what: `You picked ${m[3]}; ${m[4] ? `#${m[4]} builds it` : m[5] ? "nothing follows it" : `${m[6]} follow it`}`, next: m[4] ? `#${m[4]} builds option ${m[3]}` : m[5] ? "Nothing more: it's done" : `${m[6]} follow option ${m[3]}` }), "Picked"],
  [cueRe(String.raw`^Desk item #ID was folded into #([\w.-]+), which you have\. It said: ([^\n]*)`), (m) => ({ verb: "Told", id: m[1], what: `folded into #${m[2]}: ${cueQ(m[3])}`, said: m[3], label: "It asked", next: `{who} does it as part of #${m[2]}` }), "Folded"],
  [cueRe(String.raw`^Done\? Write DONE: #ID, or say what's left`), (m, t) => cueLines(t).every((l) => /^Done\? Write DONE: #/.test(l)) && { verb: "Asked", id: m[1], what: "done? or what's left", more: cueMore(cueLines(t).length - 1, "more item"), next: "{who} says it's done, or what's left" }, "Done?"],
  [/^The Desk is paused\. Stop at the next safe point/, () => ({ verb: "Asked", what: "the Desk is paused: stop at a safe point", next: "{who} stops at a safe point until the Desk is resumed" }), "Paused"],
  [/^The Desk is resumed: carry on with (.+?) from where you stopped/, (m) => ({ verb: "Told", what: `the Desk is resumed: carry on with ${m[1]}`, next: `{who} carries on with ${m[1]}` }), "Resumed"],
  [/^Your TO LEAD: line didn't reach the Desk's lead: ([^\n]*)/, (m) => ({ verb: "Told", what: `TO LEAD didn't reach the lead: ${cueQ(m[1])}`, said: m[1], next: "{who} can send it again once the lead is back" }), "To lead"],
  [/^TO LEAD: not sent: ([^\n]*)/, (m) => ({ verb: "Told", what: `TO LEAD not sent: ${cueQ(m[1])}`, said: m[1], next: "{who} fixes the line and sends it again" }), "To lead"],
  [cueRe(String.raw`^Not added as a new item: (.+?)\. Its fix or next step stays in #ID`), (m) => ({ verb: "Told", id: m[2], what: `not a new item: ${cueQ(m[1])} stays here`, said: m[1], label: "Its idea", next: `{who} does it in #${m[2]}, not as a new item` }), "Not new"],
  [/^Several Desks could receive this item/, () => ({ verb: "Told", what: "item not added: several Desks could take it", next: "{who} adds it through the Desk it's meant for" }), "Not added"],
  [/^.+? answered the steps held while they were away/, (m, t) => {
    const ok = (t.match(/ALLOWED: run it now/g) || []).length, no = (t.match(/NO: don't run it/g) || []).length;
    return { verb: "Told", what: `held steps answered: ${[ok && `${ok} allowed`, no && `${no} refused`].filter(Boolean).join(", ") || "none"}` };
  }, "Answered"],
  [/^Ask me before, changed by .+? just now: (?:ask .+? first, and end your turn, before (.+?)\. This replaces|(nothing is set))/, (m) => ({ verb: "Told", what: m[2] ? "Ask me before changed: nothing set" : `Ask me before changed: before ${m[1]}`, next: m[2] ? "{who} goes on without asking first" : `{who} asks first before ${m[1]}` }), "Ask me before"],
  [cueRe(String.raw`^Go back on your call(?: on #ID)?: you decided (.+?)(?: \(your reason: .*?\))?\. (?:The user's answer instead|.+?'s answer instead|Undo it)`), (m) => ({ verb: "Asked", id: m[1] || "", what: `go back on: ${cueQ(m[2])}`, said: m[2], label: "Its call", next: "{who} undoes it and takes the other way" }), "Go back"],
  [/^(Bring (.+?) back: it was set aside|Make a revert: (.+?): |Undo this: (.+?): )/, (m, t) => ({ verb: "Asked", what: m[2] ? `bring ${m[2]} back` : `revert: ${cueQ(t.split("\n")[0].replace(/^[^:]+: /, ""))}` }), "Undo"],
  [/^(?:\(Cue\) )?Your teammate (.+?) has (?:gone quiet|been quiet for (\d+) min)/, (m) => ({ verb: "Asked", what: `check on teammate ${m[1]}${m[2] ? ` (quiet ${m[2]} min)` : ""}`, next: `{who} asks ${m[1]} where it is` }), "Crew"],
  [/^Hand off: write a short note so a fresh session can carry on/, () => ({ verb: "Asked", what: "hand off: write a note for a fresh session", next: "{who} writes a hand-off note; a fresh session carries on from it" }), "Hand-off"],
  [/^Carry on from (an earlier session in this folder|the previous session's hand-off)/, (m) => ({ verb: "Asked", what: m[1].startsWith("an") ? "carry on from an earlier session" : "carry on from the hand-off note", next: "{who} reads what came before and carries on" }), "Carry on"],
  [/^You no longer run (.+?)'s Desk/, (m) => ({ verb: "Told", what: `no longer runs ${m[1]}'s Desk`, next: "{who} keeps on with the items it holds" }), "Not lead"],
  [/^The team from the session before you\./, (m, t) => ({ verb: "Told", what: "the team from the session before", more: cueMore(cueLines(t).filter((l) => l.startsWith("- ")).length, "teammate") }), "Crew"],
  // To a Desk lead.
  [cueRe(String.raw`^(.+?), on #ID, asks about clearing the way for its landing[^\n]*\n([^\n]*)`), (m, t) => ({ verb: "Asked", id: m[2], what: `${m[1]}'s landing question: ${cueQ(m[3])}`, more: cueMore(cueLines(t).filter((l) => /^\d+\. /.test(l)).length, "choice"), by: m[1], said: [m[3], ...cueLines(t).filter((l) => /^\d+\. /.test(l))].join("\n"), next: "The lead answers it; if it doesn't in 10 minutes, it comes to you" }), "Landing"],
  [/^Circle on the Desk: (.+?)\. None of them can move/, (m) => ({ verb: "Asked", what: `cut a wait circle: ${m[1]}`, next: "The lead cuts one of the waits so they can move" }), "Waits"],
  [cueRe(String.raw`^Cue couldn't start a new session for #ID \((.*)\): (.+?)\. It's still`), (m) => ({ verb: "Told", id: m[1], what: `no new session: ${cueQ(m[3])}`, said: m[3], next: "It stays new until the lead routes it again" }), "Couldn't start"],
  [/^The Desk is paused \(by (.+?), /, (m) => ({ verb: "Told", what: `the Desk is paused (by ${m[1]})`, next: "Its sessions wait at a safe point until it's resumed" }), "Paused"],
  [/^The Desk is resumed: its sessions carry on/, () => ({ verb: "Told", what: "the Desk is resumed", next: "Its sessions carry on" }), "Resumed"],
  [cueRe(String.raw`^Your ASK(?: about #ID)?: ([^\n]*)\nThe answer: ([^\n]*)`), (m) => ({ verb: "Told", id: m[1] || "", what: `its ASK answered: ${cueQ(m[3], 70)}`, by: "You", said: m[3], next: "The lead carries on with your answer" }), "Answered"],
  // Your own words on Cue's Yes / No card (desk/hub/core/desk.rs card_words, #651): the card stays open, the lead answers.
  [/^On Cue's question in your chat \("(.*)"\), .+? wrote: "([^\n]*)"\n/, (m) => ({ verb: "Asked", what: `your words on ${cueQ(m[1], 50)}: ${cueQ(m[2], 60)}`, by: "You", said: m[2], next: "The lead answers you here; the question stays open for your Yes or No" }), "Your words"],
  [cueRe(String.raw`^Cue asked .+? before routing #ID to (.+?), which isn't on the Desk\. The answer: (.*?)\. #`), (m) => ({ verb: "Told", id: m[1], what: `not routed to ${m[2]}: answer ${cueQ(m[3], 60)}` }), "Routed"],
  [cueRe(String.raw`^.+? said yes: (.+?) is on the Desk now, and Cue routed #ID`), (m) => ({ verb: "Told", id: m[2], what: `routed to ${m[1]} (a yes)` }), "Routed"],
  [cueRe(String.raw`^.+? said yes, but Cue couldn't route #ID to (.+?)\. ([^\n]*)`), (m) => ({ verb: "Told", id: m[1], what: `not routed to ${m[2]}: ${cueQ(m[3], 70)}` }), "Not routed"],
  [/^(?:Not asked: LANDS: .+|.+? said (?:no to changing how|yes: how|yes, but how) work lands on .+)/, (m, t) => ({ verb: "Told", what: cueCut(t.split("\n")[0], 90) }), "Landing"],
  [/^The owner is away: Cue didn't show them (this ASK|these ASKs)|^.+? is away: Cue didn't show them (this ASK|these ASKs)/, (m, t) => ({ verb: "Asked", what: "decide the open ASKs yourself (away)", more: cueMore(cueLines(t).filter((l) => l.startsWith("- ")).length, "ASK"), said: cueLines(t).filter((l) => l.startsWith("- ")).map((l) => l.slice(2)).join("\n"), label: "Its ASKs", next: "The lead decides them itself while you're away" }), "Away"],
  // A later pass before Away (#612): only what changed since the last one.
  [/^(?:The owner|.+?) is about to go away\. Since your last pass, only these changed:\n/, (m, t) => {
    const ch = cueLines(t).filter((l) => l.startsWith("- ")).map((l) => l.slice(2));
    return { verb: "Asked", what: `settle ${cueMore(ch.length, "change")} before Away`, said: ch.join("\n"), label: "What changed", next: "The lead settles only these; the rest stays as it left it" };
  }, "Away"],
  [/^(?:The owner|.+?) is about to go away\. Go through every open item/, () => ({ verb: "Asked", what: "settle what you can before Away", next: "The lead settles what it can; at most 5 questions wait for you" }), "Away"],
  // Back from Away (desk/hub/away/away.rs back_words, #612): how long and what moved, one line with the way to the Away tab.
  [/^Away mode is off: the owner is back(?: \(([^)\n]+)\))?/, (m) => {
    const [long, ...rest] = (m[1] || "").split(", "), did = rest.filter((r) => !r.startsWith("no "));
    return { verb: "Told", what: long ? `you're back after ${long}` : "you're back", more: long ? (did.length ? did.join(" · ") : "nothing moved") : "", next: "Cue shows you what happened in the Away tab; the lead doesn't recap it", go: ["away", "See what happened"] };
  }, "Back"],
  // The Desk's changes since the lead last heard (desk/hub/lead/feed.rs, #616): a paragraph each, numbered at the end.
  [/^([\s\S]+?)\n\n\(Desk changes [^)\n]*\)$/, (m) => cueChanges(m[1]), "Desk changes"],
  [cueRe(String.raw`^Queued: #ID is next for (.+?)(?:, after #([\w.-]+)| \(busy with #([\w.-]+)\))`), (m, t) => ({ verb: "Told", id: m[1], what: m[3] ? `next for ${m[2]}, after #${m[3]}` : `next for ${m[2]} (busy with #${m[4]})`, more: cueMore(cueLines(t).length - 1, "more queued", "more queued"), next: t.split("\n")[0].split("; ")[1] || "" }), "Queued"],
  [/^Cue asked .+?, for the leads of .+?: ([^\n]*)\nThe user's answer: ([^\n]*?)\./, (m) => ({ verb: "Told", what: `your answer: ${cueQ(m[2], 60)}`, by: "You", said: m[2], next: "The lead carries on with your answer" }), "Answered"],
  [/^Away mode:\n(?:Away is paused: ([^.\n]+)|(?:The owner|.+?) is away (until [^,\n]+))/, (m) => ({ verb: "Told", what: m[1] ? `Away paused: ${m[1]}` : `Away is on, ${m[2]}` }), "Away"],
  // An item's holder gone quiet (desk/hub/stall/stuck.rs silent_words, #816): who, how long, its last step.
  [cueRe(String.raw`^#ID \((.*)\) looks stuck: (.+?) has shown no new output for (\d+) minutes`), (m, t) => ({ verb: "Told", id: m[1], what: `looks stuck: ${m[3]} quiet for ${m[4]} min`, said: /Its last step: ([^\n]*)/.exec(t)?.[1] || "", label: "Its last step", next: "The lead looks at its chat, then nudges it, pauses it or routes it on" }), "Stuck"],
  // An item that stopped on its way to main (desk/hub/lead/stop_lead.rs), or landed without files left in the main folder (landing/own_work.rs).
  [cueRe(String.raw`^Desk item #ID \((.*)\) stopped while landing: ([^\n]*)`), (m) => ({ verb: "Told", id: m[1], what: `stopped while landing: ${cueQ(m[3])}`, said: m[3], label: "Why", next: "The lead sends it back or clears the way and says DONE again" }), "Landing"],
  [cueRe(String.raw`^#ID \((.*)\) lands from its copy without these files[^:]*: ([^\n]*?)\. They stay there`), (m) => ({ verb: "Told", id: m[1], what: `landed without ${m[3]}`, said: m[3], label: "Left in the main folder", next: "Those files stay uncommitted in the main folder" }), "Landing"],
  // The Desk's new lead's first message (desk/hub/lead/run.rs desk_handoff_words).
  [/^(.+?) now runs (.+?)'s Desk\./, (m) => ({ verb: "Told", what: `${m[1]} now runs ${m[2]}'s Desk`, next: "The new lead carries on with the Desk" }), "Hand-off"],
  // Replies to the lead's own command lines (Refused:, not cut, not split, …): its first line, the rest folded.
  [cueRe(String.raw`^(?:Refused: |Noted: |Told: |#ID (?:note )?(?:not|copy not) |#ID got no copy|Couldn't read this line as a route|No question from #ID|FEATURE #ID: left out|Cue couldn't (?:route|queue|start) |Cue didn't reopen )`), (m, t) => {
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
  ["picked", /^.+? picked option \S+ on #/],
];
/** A pick's row (desk/hub/turn/pick.rs: "<you> picked option B2 on #652 (title): "…". #652 is done; #666 builds
 *  it.") as its one line (#667): "You picked B2 on #652; #666 builds it". "" for any other row. */
function cuePickedLine(r) {
  const m = /picked option (\S+) on #([\w.-]+) [\s\S]*?#\2 is done; ([^.]*)\./.exec(r);
  return m ? `You picked ${m[1]} on #${m[2]}; ${m[3]}` : "";
}
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
  // A pick (#667): its one line first, the rows about what it filed folded into it ("#666 new" isn't said again).
  const picks = rows.map(cuePickedLine).filter(Boolean);
  if (picks.length) {
    const named = new Set(picks.join(" ").match(/#[\w.-]+/g));
    const rest = [...by].filter(([k]) => k !== "picked").map(([k, ids]) => [k, ids.filter((x) => x && !named.has(x))]).filter(([, ids]) => ids.length);
    const what = [...picks, ...rest.map(([k, ids]) => `${ids.join(", ")} ${k}`)].join(" · ");
    return { verb: "Told", ...(picks.length === 1 ? { id: /#([\w.-]+)/.exec(picks[0])[1] } : {}), what: cueCut(what, 100) };
  }
  const what = [...by].map(([k, ids]) => (one ? `${alone[k] || k}${said ? `: ${cueQ(said, 60)}` : ""}` : `${ids.filter(Boolean).join(", ")} ${k}`.trim())).join(" · ");
  // Opened (#625): your words, whole, and what happens next.
  const next = { "sent back": "Its holder fixes it and says it's done", asked: "Its holder answers you", restarted: "A session starts on it again", "moved up": "It goes before the other in line" };
  const fold = one && said ? { by: "You", said, next: next[[...by.keys()][0]] || "" } : {};
  return { verb: "Told", ...(one ? { id: [...seen][0] } : {}), what: cueCut(what, 100), ...fold };
}

/** The full text's first words up to the end of a sentence (or `max` characters, cut at a word). */
function cueCut(t, max = 110) {
  t = String(t ?? "").replace(/[`*]/g, "").replace(/\s+/g, " ").trim();
  if (t.length <= max) return t;
  return t.slice(0, max).replace(/\s+\S*$/, "") + "…";
}
/** The words an item was sent back with (act.rs send_back: "…isn't done[ (…)]: <words>\nIts fixes stay in…"), whole,
 *  without its log or its pictures line ("Pictures you attached: …", counted instead). */
function cueSentBack(t) {
  const m = /isn't done(?: \([^)]*\))?: ([\s\S]*?)(?:\nIts fixes stay in |$)/.exec(t);
  if (!m) return "";
  const ls = m[1].replace(/\nThe end of its log:[\s\S]*$/, "").split("\n");
  const pics = ls.filter((l) => l.startsWith("Pictures you attached: ")).flatMap((l) => l.match(/`[^`]+`/g) || []).length;
  return ls.filter((l) => !l.startsWith("Pictures you attached: ")).join("\n").trim().replace(/\.$/, "") + (pics ? `\n(+ ${cueMore(pics, "picture")})` : "");
}
/** Cue's standing words to the session (how to say DONE and CHECK, don't add items, don't change files…): never shown
 *  to you when a message is opened (#625); the session still gets them. A sentence that matches one is dropped. */
const CUE_RULES = /\b(?:DONE: #|CHECK: #|NOT DONE: #|ITEM: |IF PICKED: |PICKED: #|DECIDED|ANSWER: #|PASS: #|TO LEAD: <)|on a line of its own|don't add new items|Its fixes stay in|Fixes and next steps|Don't change any files|Nothing to do unless|No reply needed|This replaces any earlier|Answer or close any ASK|^Then stop\.?$/;
/** Text `t` without CUE_RULES' sentences, empty lines folded. */
const cueNoRules = (t) => String(t).split("\n").map((l) => l.split(/(?<=[.!?]) (?=[A-Z(])/).filter((x) => !CUE_RULES.test(x.trim())).join(" ")).join("\n").replace(/\n{3,}/g, "\n\n").trim();
/** What Cue's message `c` (cueMsgOf) shows once opened (#625): only what matters to you, in up to three rows: whose
 *  words ("You said “…”", the lead's note, a reason), which item, and what happens next, in one plain line. `t`: its
 *  full text (for a kind without its own rows: that text, Cue's standing words to the session dropped). `who`: who it
 *  went to; `title`: the item's title. Inside .mg-full. */
function cueFoldHtml(c, t, who = "", title = "") {
  // Waiting for a copy of its repository (trees.rs): that comes first.
  // A session's name stays as it is; any other first word is capitalized, or lowercased after "then".
  const tpl = String(c.next || "").replace(/\.$/, ""), waits = /It waits for a free copy\b/.test(t);
  const lead = (up) => (tpl.startsWith("{who}") ? (who || (up ? "It" : "it")) + tpl.slice(5) : tpl && (up ? tpl[0].toUpperCase() : tpl[0].toLowerCase()) + tpl.slice(1));
  const next = waits ? `Waits for a free copy${tpl ? `, then ${lead(false)}` : ""}` : lead(true);
  const said = String(c.said || "").trim().replace(/\.$/, "");
  if (!said && !next) return `<div class="mg-full">${esc(cueNoRules(t))}</div>`;
  const row = (k, v, cls = "") => `<div class="cue-fold-k">${esc(k)}</div><div class="cue-fold-v${cls}">${v}</div>`;
  const by = c.by ? `${c.by === "You" ? "You" : c.by} said` : c.label || "Why";
  const title2 = title || (c.id && new RegExp(`#${c.id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")} \\((.*?)\\)(?: is | isn't|: |$)`).exec(t)?.[1]) || "";
  return `<div class="mg-full cue-fold">${said ? row(by, esc(`“${said}”`), " said") : ""}`
    + `${c.id ? row("Item", `#${esc(c.id)}${title2 ? ` · ${esc(title2)}` : ""}`) : ""}${next ? row("Next", esc(next)) : ""}</div>`;
}
/** Your send-back of an item (desk/hub/turn/act.rs back_to_same: "[For your teammate X: ]Desk item #ID (title) isn't
 *  done[ (you said…)]: <words>\nIts fixes stay in…"), yours to see as your own bubble (#688): { id, it, title, words,
 *  mate }, `words` as you wrote them (cueSentBack). null for any other text, and for a check's own send-back (with
 *  "The end of its log:").
 *  SHORTCUT: a landing check's send-back without a log (landing/install.rs) reads as yours: its text alone can't tell
 *  them apart. Upgrade: the hub marks whose send-back it is. */
function cueBackOf(text) {
  const raw = deskSaid(String(text ?? "")).trim();
  if (!raw.includes(" isn't done")) return null;
  const [mate, t] = cueMate(raw);
  const m = /^Desk item #([\w.-]+) \((.*)\) isn't done(?: \(you said it looks right, and now it's reopened\))?: /.exec(t);
  if (!m || /\nThe end of its log:/.test(t)) return null;
  const it = (state.desk?.projects || []).flatMap((d) => d.items || []).find((i) => deskIdIs(i, m[1]));
  return { id: m[1], it, title: m[2], words: cueSentBack(t), mate };
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

/** That line in plain words, for a one-line place (a session's card, "Working on: …"): "Asked cue-383 · #461
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
  const foot = m ? msgLine({ ...m, by: "", acts: `${m.acts ? `${m.acts} ` : ""}${lk}` }) : `<div class="mg-f"><span class="mg-acts">${lk}</span></div>`;
  return `<div class="mg cue-msg${open ? " open" : ""}${m && m.st !== "delivered" ? ` ${m.st}` : ""}"><div class="mg-h">${head}</div><div class="mg-t">${words}</div>`
    + `${open ? cueFoldHtml(c, full, who, it?.title || "") : ""}${foot}</div>`;
}

/** A few of Cue's messages to a session in Cue's words for you, for its quiet line (#836): [pattern on its text, its
 *  Desk block and teammate prefix dropped, → { id, what }]. Any other one says its first sentence. */
const CUE_GISTS = [
  // A failed full run of main (landing/full_run.rs): which tests and the end of its log only in the folded text.
  [cueRe(String.raw`^Desk item #ID \([^\n]*\) is open again: the full run of main failed`), (m) => ({ id: m[1], what: `Reopened #${m[1]}: the full test run failed` })],
  [cueRe(String.raw`^Desk item #ID \([^\n]*\): the full run of main failed on its commit`), (m) => ({ id: m[1], what: `Told it the full test run failed on #${m[1]}'s commit: fix it there too` })],
  [cueRe(String.raw`^Your DONE for #ID is in, but its check line needs another go`), (m) => ({ id: m[1], what: "Asked it to redo its check line" })],
  [/^Your background agent “(.+?)” has gone quiet/, (m) => ({ what: `Asked it to wake its background agent ${m[1]}` })],
  [/^Stop your background agent “(.+?)”/, (m) => ({ what: `Asked it to stop its background agent ${m[1]}` })],
  [/^Carry on where you stopped: your usage limit reset/, () => ({ what: "Asked it to carry on: the usage limit reset" })],
  [/^\/handoff resume\b/, () => ({ what: "Asked it to carry on from the hand-off note" })],
];
/** What Cue's message `text` is about, in short: { mate, id, what }. A kind CUE_GISTS knows in its words; any other its
 *  first sentence (an item's "Desk item #id (title):" left out, its number kept for the chip), Cue's standing words to
 *  the session left out. */
function cueGist(text) {
  const [mate, t] = cueMate(deskSaid(String(text ?? "")).trim());
  for (const [re, f] of CUE_GISTS) {
    const m = re.exec(t);
    if (m) return { mate, id: "", ...f(m) };
  }
  const plain = cueNoRules(t) || t;
  const id = (/#([\w.-]+)/.exec(plain)?.[1] || "").replace(/\.+$/, "");
  const body = plain.replace(/^Desk item #[\w.-]+(?: \([^\n]*?\))?:\s*/, "");
  const what = (body.split(/(?<=[.!?])\s+|\n/).map((x) => x.trim()).find(Boolean) || body).replace(/:$/, "");
  return { mate, id, what: cueCut(what && what[0].toUpperCase() + what.slice(1), 110) };
}
/** One of Cue's own messages to a session, the hub's mark on it (#836, sessions/cue_said.rs), with no card of its own:
 *  one quiet line at the left, as Cue's lines in a lead's chat are (desk/lead.js deskCueLine): "+ ◔ Cue → cue-383 ·
 *  #832 · 3m", what it says in short under it (cueGist), and a click opens Cue's full text. `key`: what keeps it open
 *  (openAsks). `m`: its state when it hasn't gone (held: kept till its turn ends; queued). Never your bubble. */
function cueQuietHtml(e, s, key, m = null) {
  const g = cueGist(e.text);
  const who = g.mate || (s ? nameOf(s.session_id, s.project) : "");
  const it = g.id ? (state.desk?.projects || []).flatMap((d) => d.items || []).find((i) => deskIdIs(i, g.id)) : null;
  const chip = !g.id ? "" : typeof deskMgId === "function" ? ` ${deskMgId(g.id, it?.kind || "", deskNum(it || { id: g.id }))}` : ` <span class="mg-id">#${esc(g.id)}</span>`;
  // Nothing more than its line (#909: "Your whole Desk block, as you asked"): no fold that says it again.
  const full = deskSaid(String(e.text ?? "")).trim(), more = full !== String(g.what ?? "").trim(), open = more && openAsks.has(key);
  const st = m?.st === "held" ? " · goes when its turn ends" : m?.st === "queued" ? " · queued" : e.unsent ? " · not sent" : e.pending ? " · not read yet" : "";
  const to = who || st ? ` <span class="mg-v">${who ? `→ ${esc(who)}` : ""}${st}</span>` : "";
  return `<div class="dk-ln dk-cue cue-quiet${open ? " open" : ""}"><div class="mg-line"><button class="dk-cue-t" data-ask-open="${esc(key)}" aria-expanded="${open}">`
    + `${more ? `<span class="dk-cue-pm">${open ? "−" : "+"}</span>` : ""}${cueMark()}<b class="mg-who">Cue</b>${to}</button>${chip}`
    + `${e.at_ms ? `<span class="mg-age">${ago(e.at_ms)}</span>` : ""}</div><button class="dk-cue-w" data-ask-open="${esc(key)}" aria-expanded="${open}">${esc(g.what)}</button>`
    + `${open ? `<div class="mg-full">${esc(full)}</div>` : ""}</div>`;
}
/** A kept message's text (`text`) without Cue's `parts` of it (#836, the hub's cue_parts): your words alone. */
function cueWithout(text, parts) {
  let rest = String(text ?? "");
  for (const p of parts || []) {
    const t = String(p).trim();
    const i = t ? rest.indexOf(t) : -1;
    if (i >= 0) rest = rest.slice(0, i) + rest.slice(i + t.length);
  }
  return rest.split("\n\n").map((x) => x.trim()).filter(Boolean).join("\n\n");
}
