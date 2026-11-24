// Cue UI desk — the boxes that send (deskBox), the right card, the lead's NOW, deskView, and what keeps the view settled after a draw.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (index.html, mini.html).
/** The draft key of the box that sends to `to` ({ project, item } or { sid, mate }), noted with where it sends. */
function deskBoxKey(to) {
  // An item's box keeps one key whoever it goes to (#437): a change of state mid-typing used to swap the box, and
  // what was in it was never sent.
  const key = to.item ? `desk:${JSON.stringify([to.project || deskSelection(), to.item])}` : to.sid ? `desk-to:${JSON.stringify([to.sid, to.mate || "", ...(to.item ? [to.item] : [])])}` : to.kept ? `desk-kept:${JSON.stringify([to.project, to.kept])}` : `desk:${JSON.stringify([to.project, to.item || ""])}`;
  deskBoxes.set(key, to);
  return key;
}
/** An item page's box: a tag at the left of the input, two lines as a crew member's (#803): who gets it, small, over
 *  the item, bold ("To cue-131" / "#131"; "Lead" for the lead's own), and the item's short title in the empty box. */
const deskToLine = (who, i) => ({ pre: [`To ${who || "the lead"}`, i ? `#${i.id}` : "Lead"], ph: i ? `${deskShort(i.title)}…` : "" });
// The old one-line tag (away.js): the two lines side by side.
const deskPreHtml = (pre) => (pre ? `<span class="dk-pre" data-desk-pre="">${esc([].concat(pre).join(" · "))}</span>` : "");
/** deskBox for an open item: who gets what you type (its holder, its lead or a kept copy's old session), the tag over
 *  the box, its placeholder, and whose suggested reply it shows. `ph`: the lead's placeholder, kept when nothing else fits. */
function deskBoxItem(d, o, lead, ph) {
  let to = { project: d.project, item: o.id }, pre = "", sugItem = "";
  // Who gets it, as deskSendBox sends it: ready to check, it goes back to its holder; begun by a live session, straight
  // to it, sent as about the item, and the lead gets no copy (#276); else (no holder, not begun yet, or done) to the lead.
  const it = deskItem(d.project, o.id), hs = deskHolderSid(it);
  // A dropped item's kept copy (#608): to the session that held it, as its own chat's box (the item is off the Desk).
  if (it?.kept && hs && sessionOf(hs)) {
    to = { sid: hs };
    pre = [`To ${deskHolderName(hs, it.holder.kind === "crew" ? "" : it.holder.name)}`, `Held #${o.id}`];
    ph = "Write to it here…";
  }
  // No live holder: to the lead, in a box of its own (B2 shows the lead's box beside this page: never one draft for two).
  else if (it?.kept) { to = { project: d.project, kept: o.id }; ({ pre, ph } = deskToLine(lead, null)); }
  else if (it && hs && sessionOf(hs) && !["to_check", "done", "queued", "new", "dropped"].includes(it.state)) {
    to = it.holder.kind === "crew" ? { project: d.project, sid: hs, mate: it.holder.name, item: it.id } : { project: d.project, sid: hs, item: it.id };
    // Its chat is its holder's (#744), and this goes to it: the suggested reply is that session's.
    sugItem = hs;
    // Two lines as a crew member's chip (#803): who gets it, over the item it's sent as.
    pre = [`To ${it.holder.name}`, `#${it.id}`];
    ph = "Write to it here…";
  }
  else {
    ({ pre, ph } = deskToLine(it?.state === "to_check" && it.holder ? it.holder.name : lead, it));
    // Ready to check, it goes back to its holder: that session's suggested reply.
    if (it?.state === "to_check" && hs && sessionOf(hs)) sugItem = hs;
    // Options to pick from: the chat is for what you think of them; a pick is its Pick button (#445, #543).
    if (deskPickOptions(it || {}).length) ph = "Comments… (Pick an option above to go ahead)";
  }
  return { to, pre, ph, sugItem };
}
/** The box at the foot: to the Desk's lead (about the open item, named, when one is), or to an open holder (about its
 *  crew member, when one is). None without a live lead; a holder whose session has ended has none either. `o` what's
 *  open (null: the lead's own box, as B2's chat card always shows it, desk/b2.js). */
function deskBox(d, o = deskOpen) {
  const lead = deskLeadName(d);
  // No lead, or one that has ended: no box to it (a holder's own box stays).
  if ((!deskLead(d) || deskLeadEnded(d)) && (!o || o.kind === "item" || o.kind === "addon")) return "";
  let to = { project: d.project }, ph = `Tell ${lead}: a bug, a feature, or a question…`, pre = "", sugItem = "";
  if (o?.kind === "item") ({ to, pre, ph, sugItem } = deskBoxItem(d, o, lead, ph));
  else if (o?.kind === "holder" || o?.kind === "crew" || o?.kind === "chat") {
    const sid = o.kind === "crew" ? o.lead : o.sid;
    if (!sessionOf(sid)) return `<div class="dk-ended">Its session has ended.</div>`;
    if (o.kind !== "crew") { to = { sid }; ph = `Talk to ${deskHolderName(sid, d.items.find((i) => i.holder?.kind !== "crew" && i.holder?.sid === sid)?.holder.name)}…`; }
    else {
      const t = deskMate(o.lead, o.agent);
      to = { sid, mate: t?.name || "" };
      ph = `Tell ${deskHolderName(sid)} about ${t?.name || "it"}…`;
      // A crew member has no box of its own: what you type goes to its lead, said over the box.
      // Two lines: whose box it is, small; "Lead" under it, larger.
      pre = [`To ${deskHolderName(sid)}`, "Lead"];
    }
  }
  const key = deskBoxKey(to);
  // The lead's own box is a session's chat box, as on the Board (#688, chat/compose.js box): two lines, Queue while the
  // lead is at work, the "/" menu, Send at, and Listen's line over it while a reply is read out. Its Send is the Desk's.
  if (deskLeadBoxTo(to)) {
    const s = deskLeadSession(d), busy = s?.state === "working";
    const after = s?.harness === "pi" ? "it reads this after its current step" : "Cue keeps it and sends it when this turn ends";
    return `${listenLine(s?.session_id)}<div class="foot">${box(key, busy ? `Message ${lead}… ${after}` : ph, busy ? "Queue" : "Send", `data-desk-send="${esc(key)}"`, true)}</div>`;
  }
  // Pictures pasted, dropped or added, and the mic, as in a session's chat box (app/inputs.js, chat/compose.js); its
  // quick phrases over the text, and Claude's suggested reply in the empty box (Tab types it in) from the session it
  // talks to: the lead, the open holder, or the holder of the open item (not a crew member: those words are Cue's).
  const sugSid = to.mate ? "" : to.item ? sugItem : to.sid || deskLead(d)?.sid || "";
  // The new layout: the same box as the lead's (a session's chat box), who it goes to at its left (deskCBox).
  return deskCBox(key, pre, ph, suggestionFor(sugSid), sugSid, !to.mate);
}
/** The new layout's box to anyone but the lead: the lead's own box (two lines, a session's chat box), with who it goes
 *  to ("To cue-744, its lead") in a tag at the left of where you type. */
// No quick phrases on a crew member's box (`quick` false): there's nothing quick to say about one.
const deskCBox = (key, pre, ph, sug = "", sugSid = "", quick = true) => `<div class="foot">${boxHtml({ key, look: "composer", rows: 2, drop: true, pre, placeholder: ph, sug, sugSid, top: quick ? quickRow(`data-quick="${esc(key)}"`) : "", send: { attrs: `data-desk-send="${esc(key)}"` } })}</div>`;
/** Box `to` is the lead's own: about no item, crew member, kept copy or other session. */
const deskLeadBoxTo = (to) => !!to && !to.item && !to.sid && !to.mate && !to.kept;
// The lead's box writes to its session (chat/compose.js sidOfKey): its "/" menu, Send at and suggestion are the lead's.
// It claims no box (act.js's entry sends it).
BOX_OWNERS.push({ owns: () => false, sid: (key) => { const to = deskBoxes.get(key); return deskLeadBoxTo(to) ? deskLead(deskOf(to.project))?.sid || null : null; } });
/** An item page's foot: while its holder asks you, the answer goes there, so you stay on its page. Its ask's card
 *  (choices, Decline, Type answer…) as under a chat; a turn that ended on it, a box to its holder. Else the box. */
function deskItemFoot(i, d) {
  if (i?.state !== "needs_you") return deskBox(d);
  const it = deskItemRequest(i);
  if (it) return `<div class="dk-ask">${askFoot(it, it.session_id, it.harness || "claude", deskBoxKey({ project: d.project, item: i.id }))}</div>`;
  if (!i.holder) return deskBox(d);
  const sid = i.ask?.sid || (i.holder.kind === "crew" ? i.holder.lead_sid : i.holder.sid);
  if (!sessionOf(sid)) return `<div class="dk-ended">Its session has ended.</div>`;
  const to = i.holder.kind === "crew" && !i.ask ? { project: d.project, sid, mate: i.holder.name, item: i.id } : { project: d.project, sid, item: i.id };
  const key = deskBoxKey(to), line = deskToLine(i.holder.name, i);
  // No quick phrases to a crew member, as on its own box.
  return deskCBox(key, line.pre, line.ph, "", "", !to.mate);
}
function deskRight(project, taskApprovals) {
  return deskOpenCard(project, taskApprovals) || deskRightChat(deskOf(project), taskApprovals);
}
/** What's open on the Desk (an item's page, a session's chat, a holder's, a crew member's, an add-on's pane) as its
 *  card, or null with nothing open (or an item just added that Cue's state doesn't have yet). Both Desk pages show it in
 *  the chat card's place, the list staying beside it. */
function deskOpenCard(project, taskApprovals) {
  const d = deskOf(project);
  const o = deskOpen;
  // B2's All N sessions: the Desk's sessions, as the old left column listed them (desk/sessions.js).
  if (o?.kind === "sessions") return deskSessions(project);
  let body;
  // Each page takes its foot, the box under its own column (#200); the add-on's pane has it under the whole card.
  const item = o?.kind === "item" ? deskItemsWithRequests(d, taskApprovals).find((i) => deskIdIs(i, o.id)) || deskItem(project, o.id) : null;
  const foot = () => (o?.kind === "item" ? deskItemFoot(item, d) : deskSessionRequestFoot(d, o));
  let tail = "";
  if (o?.kind === "item") {
    const i = item;
    // An item is its session's chat, its bar on top (#744); one no session has yet, its bar over what it's about (#748).
    if (i) { delete o.fresh; body = deskCItemChat(i, d, foot()) || deskCItemBare(i, d, foot()); }
    // Just added: it opens once Cue's state has it (the chat shows till then); not there in a few seconds, it won't.
    else if (o.fresh && now() - o.fresh < 5000) return null;
  }
  else if (o?.kind === "holder") body = deskHolderPane(o.sid, d, foot());
  // A reply open in Read: read-only, as in a session's chat, so no box under it (Reply brings it back).
  else if (o?.kind === "chat") { const rd = deskReader(sessionOf(o.sid)); body = rd || deskCSessChat(o.sid, o.from, d, foot()) || deskSessChat(o.sid, o.from, d, foot()); }
  else if (o?.kind === "crew") body = deskCrewPane(o.lead, o.agent, d, foot());
  else if (o?.kind === "addon") {
    const h = deskGroups(project).extra.find((x) => (x.key || x.name) === o.key);
    let html = "";
    try { html = (h && addOn.deskPane?.(h)) || ""; } catch (e) { console.error(e); }
    body = h ? `<div class="dk-pane" data-scroll-key="addon:${esc(o.key)}">${deskBack(deskLeadName(d) || "Back")}${html || `<h1>${esc(h.name)}</h1><div class="dk-state">${esc(h.sub || "")}</div>`}</div>` : null;
    if (body) tail = foot();
  }
  if (o && !body) deskOpen = null;
  if (deskOpen) return `<div class="dk-card dk-right dk2b-open">${body}${tail}</div>`;
  return null;
}
/** The chat card with nothing open: who runs the Desk (no lead yet, or Change), else the lead's chat (or the line
 *  saying it ended), or Decisions. */
function deskRightChat(d, taskApprovals) {
  if (!deskLead(d) || deskPick === d.project) return deskPicker(d);
  const tab = (t, label, n) => `<button class="dk-tab ${deskTab === t ? "on" : ""}" data-desk-tab="${t}" aria-pressed="${deskTab === t}">${label}${n ? `<span class="n">${n}</span>` : ""}</button>`;
  const ask = deskLeadEnded(d) ? null : deskLeadAsk(d);
  const rd = deskTab === "lead" && !deskLeadEnded(d) ? deskReader(deskLeadSession(d), ask) : null;
  // B2 (desk/b2.js): the chat card, its place taken by what's open (deskOpenCard).
  const card = "dk-card dk2b-chat dk-right";
  if (rd) return `<div class="${card}">${rd}</div>`;
  const ended = deskLeadEnded(d);
  const main = deskTab === "decisions" ? deskDecisions(d) : deskTab === "moves" ? deskMoves(d) : deskTab === "away" ? deskAwayLog(d) : ended ? deskEndedLine(d) : deskChat(d);
  // One page (#200): the Desk's name for back (it's where you are), "Dispatcher · " and its lead's name; its chat and
  // Decisions at the left, the box (or the lead's ask) under them; in the panel its state and Activity (the columns
  // either side show what waits and who works).
  const open = d.items.filter((i) => i.state !== "done" && i.state !== "dropped").length;
  const working = deskWorkingFor(d);
  const meta = ["The Desk lead", `${open} ${open === 1 ? "item" : "items"}`, working.length ? `${working.length} working` : ""].filter(Boolean).join(" · ");
  // What Items' WAITING ON YOU holds (questions, then what's ready to check), so the strip matches it.
  const waits = (({ asks, check }) => [...asks, ...check])(deskGroups(d.project, taskApprovals));
  const foot = ended || deskTab !== "lead" ? "" : ask ? `<div class="dk-ask">${askFoot(ask, ask.session_id, ask.harness || "claude", deskBoxKey({ project: d.project }))}</div>` : deskBox(d, null);
  // B2: with the panel hidden (Hide panel; it opens shown), Now and Waits on you are the strip over the chat (deskB2Strip).
  const strip = deskPanelHidden("dispatcher");
  // Activity at the panel's foot (#688): its last 3 rows, the rest a click away.
  const activity = deskPanelActivity(deskActivity(d), d);
  return `<div class="${card}">${deskPage({ page: "dispatcher", kind: "desk", activity, back: "", mark: "Dispatcher · ", title: esc(deskLeadName(d)), meta: esc(meta),
    now: strip ? deskB2Strip(d, waits, working, ask, taskApprovals) : deskLeadNow(d, waits, working, ask), panel: "", main: `<div class="dk-tabs dk-ptabs">${tab("lead", esc(deskLeadName(d)), 0)}${tab("decisions", "Decisions", deskOpenCalls(d).filter((c) => !c.minor).length)}${tab("moves", "Activity", 0)}${tab("away", "Away", 0)}</div>${main}`, foot })}</div>`;
}
/** The sessions at work for Desk `d` (not its lead): [{ sid, name, on }], `on` the item each is on. */
const deskWorkingFor = (d) => deskSessionsOf(d.project, d).filter((r) => deskHasSession(d, r.sid) && r.sid !== deskLead(d)?.sid && deskSessBusy(sessionOf(r.sid))).map((r) => ({ ...r, on: deskSessItem(d, r.sid) }));
/** The Dispatcher's NOW (Lead chats pick 1 + D, as a panel since #200): its state's dot and word; a sentence: what it
 *  routes ("Routing #125"), what waits on you, how many items are with sessions; Check #131 opening the first that
 *  waits on you. Folded: "· 4 items · 6 sessions working". */
function deskLeadNow(d, waits, working, ask = null) {
  const resume = d.items.filter((i) => i.state === "needs_resume").length, paused = d.items.filter((i) => i.state === "paused").length;
  const fold = `${waits.length + (ask ? 1 : 0)} waiting · ${working.length} ${working.length === 1 ? "session" : "sessions"} working${resume ? ` · ${resume} to resume` : ""}${paused ? ` · ${paused} paused` : ""}`;
  if (deskLeadEnded(d)) return deskPnow({ dot: "stop", word: "Ended", say: "Items stay here: resume it or pick another.", fold });
  const lead = deskLead(d), s = sessionOf(lead.sid);
  const [dot, word] = deskPstate(s);
  const fresh = d.items.filter((i) => i.state === "new" && !i.holder);
  const routing = word === "Working" && fresh.length === 1 ? fresh[0] : null;
  const held = d.items.filter((i) => i.holder && (deskMoving(i) || i.stopped || i.state === "queued" || i.state === "needs_resume" || i.state === "paused") && deskGroupKey(i.holder) !== lead.sid).length;
  const num = (i) => `${deskKindIcon(i)}<b class="dk-inum">#${esc(i.id)}</b>`;
  const say = [ask ? "It asks you something: its question is under its chat." : "", routing ? `Routing ${num(routing)} to a session.` : "", waits.length === 1 ? `${num(waits[0])} waits on you.` : waits.length ? `${waits.length} items wait on you.` : "",
    resume ? `${resume} ${resume === 1 ? "item needs" : "items need"} to resume.` : "",
    paused ? `${paused} ${paused === 1 ? "item is" : "items are"} paused.` : "",
    held ? `${held} ${held === 1 ? "item is" : "items are"} with sessions.` : ""].filter(Boolean).join(" ") || "Nothing to route right now.";
  const check = waits.length ? `<button class="btn small primary dk-pgo" data-desk-chk-item="${esc(waits[0].id)}">Check #${esc(waits[0].id)}</button>` : "";
  return deskPnow({ dot, word, say, acts: check, fold });
}
/** The lead's own open ask (an ASK line's question, a permission, a prompt), not a crew member's (those are in its
 *  crew's Needs you): while it's open the lead takes no message, so its card (choices, Decline, Type answer…) stands
 *  in the box's place, as under a session's chat. Null when none. */
function deskLeadAsk(d) {
  const sid = deskLead(d)?.sid;
  return (sid && deskSessionRequests(sid)[0]) || null;
}

// ---------- All projects: every Desk (desk/home.js) ----------

/** The Desk view: the project picked (its items, and its lead beside them), or with All projects every Desk. */
function deskView() {
  // What was open belongs to the project it was opened in.
  if (deskOpen && deskOpen.project !== deskSelection()) { deskOpen = null; deskChatEnd = true; }
  if (deskPick && deskPick !== deskSelection()) deskPick = null;
  const given = deskGiveFor && deskItem(deskSelection(), deskGiveFor.id);
  // Give to… goes once it has a holder; its ⋯ menu (#669) stays open with one.
  if (deskGiveFor && (!given || (given.holder && deskGiveFor.at !== "more"))) deskGiveFor = null;
  if (deskDropArm && !deskItem(deskSelection(), deskDropArm) && !(deskProjOf({ project: deskSelection() })?.kept || []).some((k) => k.id === deskDropArm)) deskDropArm = null;
  deskBoxes.clear();   // the boxes drawn now note theirs again
  if (!deskSelection()) return deskAll();
  deskLastSet(deskSelection());
  queueMicrotask(deskSettle);
  const project = deskSelection();
  const taskApprovals = deskTaskApprovals(deskOf(project));
  // Progress (#409): over the Desk while open, the table of every item under way.
  const prog = deskProgShown ? deskProgress(project, taskApprovals) : "";
  // Three columns (#738, desk/c.js): the Desk's sessions at the left, the lead's chat (or what's open in its place) in
  // the middle, Items as one white card with Up next and Done today under it on the green.
  // The grip between the chat and Items trades their width (board/columns.js deskColDrag, styles/desk-cols.css).
  return `<div class="board dk-board dk-b2 dk-cols dk-c" style="${deskColStyle()}">${deskCSessions(project)}${deskAwayBackIn(deskRight(project, taskApprovals))}${deskAwayBackIn(deskCList(project, taskApprovals))}${deskColGrip()}${prog}</div>${deskAwayHtml()}`;
}
/** After the draw: the lead's chat at its end, as a chat opens (unless you'd scrolled up in it); an item's Thread at
 *  a message that just came. */
/** What's open on the Desk when its box last had the chance to take the cursor ("desk|{open}"), null off the Desk. */
let deskBoxShown = null;
/** When a key was last pressed: a row opened with Enter after it keeps the keys (pointedAt: the last click, chat/patch.js). */
let deskKeyAt = 0;
/** Every key the views' steps leave on (app/keys.js KEY_FIRST) notes when it was pressed. */
function deskKeyNote() { deskKeyAt = Date.now(); return false; }
/** A chat opened on the Desk (the Desk itself, the lead's chat, an item's page, a session's or a crew member's): its
 *  text box takes the cursor, so you can just type (#154), as a chat on the Board does. Not over your choice: not
 *  while you're writing in another box, a sheet is open, or you're moving through the list with the keys. */
function deskFocusOpened() {
  const k = view === "desk" ? `${deskSelection()}|${JSON.stringify(deskOpen)}` : null;
  if (k === deskBoxShown) return;
  deskBoxShown = k;
  const a = document.activeElement;
  if (!k || sheet || a?.matches?.("input, textarea, select, [contenteditable]")) return;
  // Keys, not a click: a row, or the way back an open chat gives the keyboard (desk/act.js deskKey).
  if (a?.closest?.(".dk-board") && a.matches('[role="button"], [data-desk-back]') && deskKeyAt > pointedAt) return;
  focusComposer(undefined, { home: "desk", scroll: false });
}
function deskSettle() {
  deskDoneWatch();
  deskMovesWatch();
  deskFocusOpened();
  // The chat of the card that shows what's open.
  const c = document.querySelector(".dk-right .dk-chat");
  // Earlier messages just went in above (chat/thread.js loadOlder): what you were reading stays put, in the chat
  // they went into.
  const held = holdChat != null ? olderChat(holdChatSid, ".dk-chat") : null;
  if (held) held.scrollTop = held.scrollHeight - holdChat;
  if (holdChat != null) { holdChat = null; holdChatSid = null; }
  if (c && c === held) return;
  // A turn in the lead's chat (B2: at the left while a page is open on the right) is found in that chat.
  const tc = deskTurnLead ? document.querySelector(".dk2b-chat .dk-chat") : c;
  if (tc && deskTurnAt && tc.querySelector(".cv-agent[data-at]")) {
    // The reply "This turn in …" was on: its session's last reply at or before that time (the reply's own time can
    // come a moment after its message); not in what's loaded, the end. Its messages not drawn yet: on the next draw.
    const at = deskTurnAt, msgs = [...tc.querySelectorAll(".cv-agent[data-at]")];
    const hit = msgs.filter((m) => Number(m.dataset.at) <= at + 5_000).at(-1);
    deskTurnAt = 0;
    deskTurnLead = false;
    if (hit) { tc.scrollTop += hit.getBoundingClientRect().top - tc.getBoundingClientRect().top - 12; if (tc === c) deskChatEnd = false; } else tc.scrollTop = tc.scrollHeight;
  }
  // Just after you sent in the lead's chat, again a frame and a moment later (ui/box/end.js): what comes in late (a
  // picture, the Working line) mustn't leave your message just under the edge (#704).
  else {
    // You scrolled it (a wheel, touch, key or click of yours since Cue last put it at its end) and it's not at its end:
    // you're reading, even when this draw comes before the scroll event that would have said so (#782).
    if (c && deskChatEnd && !deskChatSent && (c._youAt || 0) > (c._pinAt || 0) && !boxAtEnd(c)) deskChatEnd = false;
    if (c && deskChatEnd) { boxStickToEnd(c, deskChatSent); deskChatSent = false; }
  }
}
