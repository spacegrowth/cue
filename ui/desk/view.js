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
/** An item page's box: a tag that stays above the input as you type, who gets it and which item ("To cue-131 · Task #131"), and
 *  the item's short title in the empty box ("Remove Actions…"). */
const deskToLine = (who, i) => ({ pre: `To ${who || "the lead"}${i ? ` · ${deskNum(i)}` : ""}`, ph: i ? `${deskShort(i.title)}…` : "" });
const deskPreHtml = (pre) => (pre ? `<span class="dk-pre" data-desk-pre="">${esc(pre)}</span>` : "");
/** The box at the foot: to the Desk's lead (about the open item, named, when one is), or to an open holder (about its
 *  crew member, when one is). None without a live lead; a holder whose session has ended has none either. `o` what's
 *  open (null: the lead's own box, as B2's chat card always shows it, desk/b2.js). */
function deskBox(d, o = deskOpen) {
  const lead = deskLeadName(d);
  // No lead, or one that has ended: no box to it (a holder's own box stays).
  if ((!deskLead(d) || deskLeadEnded(d)) && (!o || o.kind === "item" || o.kind === "addon")) return "";
  let to = { project: d.project }, ph = `Tell ${lead}: a bug, a feature, or a question…`, pre = "";
  if (o?.kind === "item") {
    to = { project: d.project, item: o.id };
    // Who gets it, as deskSendBox sends it: ready to check, it goes back to its holder; begun by a live session, straight
    // to it, sent as about the item, and the lead gets no copy (#276); else (no holder, not begun yet, or done) to the lead.
    const it = deskItem(d.project, o.id), hs = deskHolderSid(it);
    // A dropped item's kept copy (#608): to the session that held it, as its own chat's box (the item is off the Desk).
    if (it?.kept && hs && sessionOf(hs)) {
      to = { sid: hs };
      pre = `To ${deskHolderName(hs, it.holder.kind === "crew" ? "" : it.holder.name)}, who held it`;
      ph = "Write to it here…";
    }
    // No live holder: to the lead, in a box of its own (B2 shows the lead's box beside this page: never one draft for two).
    else if (it?.kept) { to = { project: d.project, kept: o.id }; ({ pre, ph } = deskToLine(lead, null)); }
    else if (it && hs && sessionOf(hs) && !["to_check", "done", "queued", "new", "dropped"].includes(it.state)) {
      to = it.holder.kind === "crew" ? { project: d.project, sid: hs, mate: it.holder.name, item: it.id } : { project: d.project, sid: hs, item: it.id };
      pre = it.holder.kind === "crew" ? `To ${it.holder.name}, through ${deskHolderName(hs)} · sent as #${it.id}` : `To ${it.holder.name}, who has this item · sent as #${it.id}`;
      ph = "Write to it here…";
    }
    else {
      ({ pre, ph } = deskToLine(it?.state === "to_check" && it.holder ? it.holder.name : lead, it));
      // Options to pick from: the chat is for what you think of them; a pick is its Pick button (#445, #543).
      if (deskPickOptions(it || {}).length) ph = "Comments… (Pick an option above to go ahead)";
    }
  }
  else if (o?.kind === "holder" || o?.kind === "crew" || o?.kind === "chat") {
    const sid = o.kind === "crew" ? o.lead : o.sid;
    if (!sessionOf(sid)) return `<div class="dk-ended">Its session has ended.</div>`;
    if (o.kind !== "crew") { to = { sid }; ph = `Talk to ${deskHolderName(sid, d.items.find((i) => i.holder?.kind !== "crew" && i.holder?.sid === sid)?.holder.name)}…`; }
    else { const t = deskMate(o.lead, o.agent); to = { sid, mate: t?.name || "" }; ph = `Tell ${deskHolderName(sid)} about ${t?.name || "it"}…`; }
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
  // talks to: the lead, or the open holder (not about an item, or a crew member: those words are Cue's, not its).
  const sugSid = to.item ? `item:${d.project}|${to.item}` : to.mate ? "" : to.sid || deskLead(d)?.sid || "";
  // An item's page shows its recipient and item above the input; quick phrases use its normal send flow.
  return boxHtml({ key, top: quickRow(`data-quick="${esc(key)}"`) + deskPreHtml(pre), wrap: true, sug: suggestionFor(sugSid), sugSid, placeholder: ph, send: { attrs: `data-desk-send="${esc(key)}"` } });
}
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
  return boxHtml({ key, top: deskPreHtml(line.pre), placeholder: line.ph, send: { attrs: `data-desk-send="${esc(key)}"` } });
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
  if (o?.kind === "sessions") return deskB2On() ? deskSessions(project) : ((deskOpen = null), null);
  let body;
  // Each page takes its foot, the box under its own column (#200); the add-on's pane has it under the whole card.
  const item = o?.kind === "item" ? deskItemsWithRequests(d, taskApprovals).find((i) => deskIdIs(i, o.id)) || deskItem(project, o.id) : null;
  const foot = () => (o?.kind === "item" ? deskItemFoot(item, d) : deskSessionRequestFoot(d, o));
  let tail = "";
  if (o?.kind === "item") {
    const i = item;
    if (i) { delete o.fresh; body = deskB2On() ? deskB2ItemFull(i, d, foot()) : deskItemPane(i, d, foot()); }
    // Just added: it opens once Cue's state has it (the chat shows till then); not there in a few seconds, it won't.
    else if (o.fresh && now() - o.fresh < 5000) return null;
  }
  else if (o?.kind === "holder") body = deskHolderPane(o.sid, d, foot());
  // A reply open in Read: read-only, as in a session's chat, so no box under it (Reply brings it back).
  else if (o?.kind === "chat") { const rd = deskReader(sessionOf(o.sid)); body = rd || deskSessChat(o.sid, o.from, d, foot()); }
  else if (o?.kind === "crew") body = deskCrewPane(o.lead, o.agent, d, foot());
  else if (o?.kind === "addon") {
    const h = deskGroups(project).extra.find((x) => (x.key || x.name) === o.key);
    let html = "";
    try { html = (h && addOn.deskPane?.(h)) || ""; } catch (e) { console.error(e); }
    body = h ? `<div class="dk-pane" data-scroll-key="addon:${esc(o.key)}">${deskBack(deskLeadName(d) || "Back")}${html || `<h1>${esc(h.name)}</h1><div class="dk-state">${esc(h.sub || "")}</div>`}</div>` : null;
    if (body) tail = foot();
  }
  if (o && !body) deskOpen = null;
  if (deskOpen) return `<div class="dk-card dk-right${deskB2On() ? " dk2b-open" : ""}">${body}${tail}</div>`;
  return null;
}
/** Item `i`'s page on B2 (#666), on the left in the lead's chat's place: ‹ Items N, its number and title; its summary
 *  (progress in six steps; Who, Doing now, Tests, Next; Open its chat and Changes); then what the page does: its panel
 *  (where it stands, Looks right, Close, Send back, Reopen, Starts on, what to try) and its chat in place (its holder's,
 *  else its Thread; your words, notes that wait, the live line), the build log and landing line under it, and its
 *  question's card or the box at the foot. */
function deskB2ItemFull(i, d, foot) {
  const at = deskB2At(i);
  const bar = DESK_B2_STEPS.map((n, k) => `<div class="dk2b-step ${k < at ? "done" : k === at ? "cur" : ""}"><i></i><span>${esc(n)}</span></div>`).join("");
  const sid = deskHolderSid(i), s = sid ? sessionOf(sid) : null;
  const since = s && i.state === "working" ? deskProgStamps(i)[0] || s.since_ms || 0 : 0;
  // The model its new session was started on, when not Claude Code's own pick (#620): the old row's tag.
  const model = i.state !== "held" ? deskModelTag(i) : "";
  const who = (i.holder ? `<b>${esc(deskHolderOf(i))}</b>${since ? ` · working for <span data-ago="${since}">${ago(since)}</span>` : ""}` : "Nobody has it yet") + model;
  const [now, mono] = deskB2Status(i);
  const tests = i.finish?.check_failed ? "Still fail" : at >= 3 ? "All pass" : at === 2 ? "Running" : "Not run yet";
  const next = i.state === "done" ? "Nothing: it's done." : i.state === "to_check" ? "Waits on you to check it." : i.state === "needs_you" ? "Waits on your answer."
    : i.state === "held" ? "Starts when you press Start." : at < 0 ? esc(deskQueueLine(i) || "Starts when a session is free.")
    : at < 3 ? "Lands when the tests pass, then waits on you to check." : "Waits on you to check once it has landed.";
  const kv = (k, v, cls = "") => `<span class="dk2b-ik">${k}</span><div class="dk2b-iv${cls}">${v}</div>`;
  // Its chat's tabs, as the old page's (deskItemTabbed): Chat, Record, Decisions (Pieces first for a feature). Changes
  // (the old Files tab) shows its files under these buttons instead, over its chat, pressed again to fold.
  const chat = deskItemChatOf(i), { tab, tabs, body } = deskItemTabbed(i, d, chat, false);
  const files = deskItemTabs.get(i.id) === "files", ft = deskItemFilesTab(i);
  const btns = `<div class="dk2b-iacts">${s ? `<button class="btn small" ${deskChatAttrs(sid, d)}>Open its chat</button>` : ""}<button class="btn small dk2b-chg" data-desk-itab="${files ? "thread" : "files"}" data-id="${esc(i.id)}" aria-pressed="${files}">Changes<span class="n">${ft.n}</span></button></div>`;
  // Its panel, as the old page had it beside its chat (deskItemSide; a dropped item's kept copy, deskKeptSide): under
  // the page that scrolls, over the box, so its buttons stay in reach however far the chat is scrolled.
  const panel = `<aside class="dk-panel dk2b-ipanel" data-side-for="${esc(i.id)}">${i.kept ? deskKeptSide(i, d) : deskItemSide(i, d)}${deskPageActivity()}</aside>`;
  const live = chat && tab === "thread";
  // Its head, as the old page's (deskPage): back on its own line with Hide panel, which folds the panel to its one line
  // (where it stands and its buttons), remembered for every item's page; then its kind's icon, kind and number, and its
  // name.
  const hid = deskPanelHidden("item");
  const tog = `<button class="btn small dk-ptog" data-desk-panel="item" aria-pressed="${!hid}">${DESK_PANEL_ICON}<span>${hid ? "Show panel" : "Hide panel"}</span></button>`;
  const kind = deskNum(i).slice(0, -`#${i.id}`.length);
  return `<div class="dk-pg dk2b-ipage${hid ? " hid" : ""}" data-desk-page="item"><div class="dk-ph dk2b-iph"><div class="dk-ph1">${deskBack("")}<span class="grow"></span>${tog}</div><h1 class="dk-ph2"><span class="dk-hk">${deskKindIcon(i)}${esc(kind)}<span class="dk2b-in">#${esc(i.id)}</span></span><span class="dk-hsep"> · </span><span class="dk-tname dk2b-ititle">${esc(i.title)}</span></h1></div>
    <div class="dk-pane dk-ipane dk2b-ipb${live ? " chat" : ""}" data-scroll-key="item:${esc(i.id)}"><div><div class="dk2b-sec">PROGRESS</div><div class="dk2b-steps">${bar}</div></div>
    <div class="dk2b-kv">${kv("Who", who)}${kv("Doing now", now, mono ? " mono" : "")}${kv("Tests", tests)}${kv("Next", next)}</div>
    ${btns}${files ? `<div class="dk2b-files">${ft.html}</div>` : ""}
    <div class="dk2b-ichat"><div class="dk2b-ichd"><span class="dk2b-sec">${chat ? "ITS CHAT" : "ITS THREAD"}</span><div class="dk-itabs" role="tablist">${tabs}</div></div><div class="dk2b-ichsub">${deskItemSub(i, d, chat)}</div><div class="dk-ibody dk-itab-${tab}">${body}</div></div></div>${panel}${foot}</div>`;
}
/** The chat card with nothing open: who runs the Desk (no lead yet, or Change), else the lead's chat (or the line
 *  saying it ended), or Decisions. */
function deskRightChat(d, taskApprovals) {
  if (!deskLead(d) || deskPick === d.project) return deskPicker(d);
  const tab = (t, label, n) => `<button class="dk-tab ${deskTab === t ? "on" : ""}" data-desk-tab="${t}" aria-pressed="${deskTab === t}">${label}${n ? `<span class="n">${n}</span>` : ""}</button>`;
  const ask = deskLeadEnded(d) ? null : deskLeadAsk(d);
  const rd = deskTab === "lead" && !deskLeadEnded(d) ? deskReader(deskLeadSession(d), ask) : null;
  // B2 (desk/b2.js): the chat card, its place taken by what's open (deskOpenCard).
  const card = deskB2On() ? "dk-card dk2b-chat dk-right" : "dk-card dk-right";
  if (rd) return `<div class="${card}">${rd}</div>`;
  const ended = deskLeadEnded(d);
  const main = deskTab === "decisions" ? deskDecisions(d) : deskTab === "moves" ? deskMoves(d) : deskTab === "away" ? deskAwayLog(d) : ended ? deskEndedLine(d) : deskChat(d);
  // One page (#200): the Desk's name for back (it's where you are), "Dispatcher · " and its lead's name; its chat and
  // Decisions at the left, the box (or the lead's ask) under them; in the panel its state, what waits on you and
  // the sessions at work for it.
  const open = d.items.filter((i) => i.state !== "done" && i.state !== "dropped").length;
  const working = deskWorkingFor(d);
  const meta = ["The Desk lead", `${open} ${open === 1 ? "item" : "items"}`, working.length ? (deskB2On() ? `${working.length} working` : `${working.length} ${working.length === 1 ? "session" : "sessions"} working for it`) : ""].filter(Boolean).join(" · ");
  // B2: what the list's WAITS ON YOU holds (questions, then what's ready to check), so the panel and the strip match it.
  const waits = deskB2On() ? (({ asks, check }) => [...asks, ...check])(deskGroups(d.project, taskApprovals)) : deskWaitingItems(d, taskApprovals);
  const why = (i) => (i.state === "needs_you" ? (i.ask?.kind === "permission" ? "Asks to use a tool" : "Asks you something") : "Ready to check");
  const waitRows = waits.map((i) => `<button class="dk-prow" data-desk-chk-item="${esc(i.id)}"><i class="dk-pdot ${i.state === "needs_you" ? "ask" : "check"}" aria-hidden="true"></i><span class="dk-prt">${deskKindIcon(i)}<b>${esc(deskNum(i))}</b></span><span class="dk-prs">${esc(why(i))} · ${ago(deskMs(i.updated))}</span></button>`).join("");
  const workRows = working.map(({ sid, name, on }) => `<button class="dk-prow" ${deskChatAttrs(sid, d)}><i class="dk-pdot work" aria-hidden="true"></i><span class="dk-prt"><b>${esc(name)}</b>${on ? ` <span class="dk-prs">on ${deskKindIcon(on)}#${esc(on.id)}</span>` : ""}</span></button>`).join("");
  // The lead's own ask waits on you too, at the top, until it's answered (its card is under its chat).
  const leadAsk = ask ? `<button class="dk-prow" data-desk-tab="lead"><i class="dk-pdot ask" aria-hidden="true"></i><span class="dk-prt"><b>${esc(deskLeadName(d))}</b></span><span class="dk-prs">${esc(summary(ask) || "Asks you something")} · ${ago(ask.created_ms || now())}</span></button>` : "";
  const panel = deskPsec(`Waits on you · ${waits.length + (ask ? 1 : 0)}`, leadAsk || waitRows ? `<div class="dk-prows">${leadAsk}${waitRows}</div>` : "")
    + deskPsec(`Working · ${working.length}`, workRows ? `<div class="dk-prows">${workRows}</div>` : "");
  const foot = ended || deskTab !== "lead" ? "" : ask ? `<div class="dk-ask">${askFoot(ask, ask.session_id, ask.harness || "claude", deskBoxKey({ project: d.project }))}</div>` : deskBox(d, deskB2On() ? null : deskOpen);
  // B2: with the panel hidden (Hide panel; it opens shown), Now and Waits on you are the strip over the chat (deskB2Strip).
  const strip = deskB2On() && deskPanelHidden("dispatcher");
  // Activity at the panel's foot (#688): its last 3 rows, the rest a click away.
  const activity = deskPanelActivity(deskActivity(d), d);
  return `<div class="${card}">${deskPage({ page: "dispatcher", kind: "desk", activity, back: `<span class="dk-back dk-here">${esc(deskName(d))}</span>`, acts: deskV2 ? deskV2ChatsMenu(d) : "", mark: "Dispatcher · ", title: esc(deskLeadName(d)), meta: esc(meta),
    now: strip ? deskB2Strip(d, waits, working, ask, taskApprovals) : deskLeadNow(d, waits, working, ask), panel, main: `<div class="dk-tabs dk-ptabs">${tab("lead", esc(deskLeadName(d)), 0)}${tab("decisions", "Decisions", deskOpenCalls(d).filter((c) => !c.minor).length)}${tab("moves", "Activity", 0)}${tab("away", "Away", 0)}</div>${main}`, foot })}</div>`;
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
  // An item's page at its end as this draw starts: it stays there as its chat grows (deskThreadFollow).
  const ip = document.querySelector(".dk-ipane");
  deskThreadPinned = !ip || boxAtEnd(ip);
  if (!deskSelection()) return deskAll();
  deskLastSet(deskSelection());
  queueMicrotask(deskSettle);
  const project = deskSelection();
  const taskApprovals = deskTaskApprovals(deskOf(project));
  // Progress (#409): over the Desk while open, the table of every item under way.
  const prog = deskProgShown ? deskProgress(project, taskApprovals) : "";
  // The new Desk page (#628, desk/v2.js): no sessions column, Desks in its head, Progress in the list.
  if (deskV2) return `<div class="board dk-board dk-v2">${deskV2Head(project)}${deskAwayBackIn(deskRight(project, taskApprovals))}${deskList(project, taskApprovals)}${deskV2Drawer(project)}</div>${deskAwayHtml()}`;
  // Today's page, B2 (#666, desk/b2.js): at the left the lead's chat, or in its place what's open (‹ Items N or Esc goes
  // back); at the right the item list, always.
  // The grip between the two trades their width (board/columns.js deskColDrag, styles/desk-cols.css).
  return `<div class="board dk-board dk-b2 dk-cols" style="${deskColStyle()}">${deskAwayBackIn(deskRight(project, taskApprovals))}${deskAwayBackIn(deskB2List(project, taskApprovals))}${deskColGrip()}${prog}</div>${deskAwayHtml()}`;
}
/** How many messages each item's Thread showed when last drawn, and for which opening of its page, by item. The page
 *  opens at its newest message, as a chat does (#145). After that it follows its chat only while you're at its end,
 *  as a session's chat does (#395): the reply being written stays in view as it grows, and a page you'd scrolled up
 *  stays where you were. */
const deskThreadSeen = new Map();
/** The open item page was at its end when this draw began (deskView). */
let deskThreadPinned = true;
/** You just wrote on an item's page (deskPend): it goes to its end to show what you sent. */
let deskThreadToEnd = false;
function deskThreadFollow() {
  const pane = document.querySelector(".dk-ipane"), th = pane?.querySelector("[data-thread-for]");
  if (!th) return;
  const rows = th.querySelectorAll(":scope > [data-key]"), key = pane.dataset.scrollKey, was = deskThreadSeen.get(key);
  deskThreadSeen.set(key, { n: rows.length, open: deskOpen });
  const toEnd = () => { pane.scrollTop = pane.scrollHeight; };
  // Just opened: at its end, the newest message and where it stands (what to check) in view.
  if (was?.open !== deskOpen || deskThreadToEnd) { deskThreadToEnd = false; return toEnd(); }
  if (!deskThreadPinned) return;
  // At its end, it stays there; but a whole new message taller than the page shows from its top, under the pinned
  // header (one written as you watched has grown in view already).
  let down = pane.scrollHeight - pane.clientHeight - pane.scrollTop;
  const last = rows[rows.length - 1];
  if (rows.length > was.n && last && !last.classList.contains("live") && last.dataset.key !== "where") {
    const head = pane.querySelector(".dk-ihead")?.getBoundingClientRect().bottom ?? pane.getBoundingClientRect().top;
    down = Math.min(down, last.getBoundingClientRect().top - head - 12);
  }
  if (down > 0) pane.scrollTop += down;
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
  // B2 (desk/b2.js): the lead's box stays on screen as something opens beside it; what opened takes the cursor still.
  const lead = deskB2Side() && a?.closest?.(".dk2b-chat");
  if (!k || sheet || (a?.matches?.("input, textarea, select, [contenteditable]") && !lead)) return;
  // Keys, not a click: a row, or the way back B2's item page gives the keyboard (desk/act.js deskKey).
  if (a?.closest?.(".dk-board") && a.matches('[role="button"], [data-desk-back]') && deskKeyAt > pointedAt) return;
  focusComposer(undefined, { home: "desk", scroll: false });
}
function deskSettle() {
  deskDoneWatch();
  deskMovesWatch();
  deskThreadFollow();
  deskFocusOpened();
  // The chat or item page of the card that shows what's open (B2: the right one while something is, desk/b2.js).
  const c = document.querySelector(".dk-right .dk-chat") || (!deskB2On() && document.querySelector(".dk-chat")), ip = !c && document.querySelector(".dk-ipane");
  // Earlier messages just went in above (chat/thread.js loadOlder): what you were reading stays put, in the chat
  // they went into (the lead's on the left or what's open on the right). An item's page too.
  const held = holdChat != null ? olderChat(holdChatSid, ".dk-chat, .dk-ipane") : null;
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
  else if (c && deskChatEnd) c.scrollTop = c.scrollHeight;
}
