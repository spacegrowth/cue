// Cue UI desk — acting: open, send, send back, Looks right and the other item actions, undo, and the Desk's clicks and keys.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (index.html, mini.html).
// ---------- acting ----------

/** Open `what` in the chat card (null: back to the lead's chat). */
function deskShow(what) {
  // Today's page (B2, desk/b2.js): the lead's chat is the chat card's own, so opening it goes back to it, rather than a
  // second copy of it.
  if (what?.kind === "chat" && deskB2On() && what.sid === deskLead(deskOf(deskSelection()))?.sid) {
    deskPick = null;
    deskOpen = null;
    deskTab = "lead";
    // "This turn in …" on the lead: its chat goes to that turn (desk/view.js deskSettle), not to its end.
    deskTurnLead = !!deskTurnAt;
    if (!deskTurnAt) deskChatEnd = true;
    return renderMain();
  }
  if (what?.kind === "item") deskSeeItem(what.id);
  deskOpen = what && { ...what, project: deskSelection() };
  if (what?.kind !== "chat") deskTurnAt = 0;
  deskTurnLead = false;
  deskDropArm = null;
  deskGiveFor = null;
  if (!what) deskChatEnd = true;
  renderMain();
}
/** Send what's in box `key`: to the lead (naming the open item), or to the lead (naming its crew member). Only what's
 *  sent is named: the box keeps what you typed if it doesn't go. `sendNow` (⌘↵, as in a session's chat, #630): to a
 *  session or lead at work, it goes at once instead of being kept in Cue until its turn ends. */
async function deskSendBox(key, sendNow = false) {
  const to = deskBoxes.get(key);
  const text = draft(key).text.trim();
  const images = boxImages(draft(key).images);
  if (!to || (!text && !images.length)) return;
  // Sent from Away's "Before you go": that one is answered there (#618).
  deskAwayNoteSent(to);
  // About an item, from its page or its row: where it goes is read now, from where the item stands, not from when the
  // box was drawn (#437).
  if (to.item && !/^[/!]/.test(text)) return deskSendItem(to, key, text);
  if (to.sid) {
    // To a crew member: through its lead, named (the guards, "It's restarting" and the rest, look at the lead).
    if (!to.mate) await sendTo(to.sid, sendNow, key);
    else await deliver(to.sid, key, (typed, images) => boxToSession(to.sid)(`About ${to.mate}: ${typed}`, images), true);
    return;
  }
  return deskSendLead(to, key, text, sendNow);
}
/** What you wrote about item `to.item` from box `key` (#437). Held by a live session: to it, at any stage, as a note
 *  the hub keeps on the item's Thread before it goes and marks sent, queued or not sent (with Retry); ready to check,
 *  it goes back with your words, unless they ask a question (to check or done: a note it answers). Held by no one,
 *  or done: to the lead. Your words stay in the box until the hub has
 *  them, so a restart or a change of state can't lose them. `text`: what's in the box, trimmed. */
async function deskSendItem(to, key, text) {
  const project = to.project || deskSelection(), it = deskItem(project, to.item);
  if (!it) return toast("That item isn't on this Desk any more.");
  // A question on an item to check or done (#475): its holder answers it, and the item stays where it is unless that
  // answer changes its code (the hub's desk/hub/turn/asked.rs).
  // A quick phrase on an item to check (#488, "Go ahead"): a note to its holder too, never a Send back.
  const phrase = phraseSending && it.state === "to_check";
  const asks = phrase || (["to_check", "done"].includes(it.state) && /\?\s*$/.test(text || ""));
  const hs = deskHolderSid(it), live = hs && (sessionOf(hs) || it.state === "to_check" || asks);
  if (!it.holder || !live || (!asks && ["done", "dropped"].includes(it.state))) return deskSendLead({ project, item: it.id }, key, text);
  if (it.state === "to_check" && !asks) {
    await deskSendBack(it.id, key);
    return renderMain();
  }
  // The guards look at its holder. The box empties at once and the note is on the Thread; the saved draft goes only
  // once the hub has the note.
  await boxSend({}, key, {
    guardSid: hs, save: true,
    // An answer: the page says what you answered (deskChain).
    before: (t) => { if (it.state === "needs_you") deskAnswers.set(it.id, { q: deskQuestion(it), a: t || "a picture", ask: it.ask?.item_id || "", at: now() }); },
    pending: (t, images) => deskPend(it.id, t || "(a picture)", "note", images.length, { key, images, act: phrase ? "phrase" : "note" }),
    call: (t, images, p) => invoke("desk_item_act", { id: it.id, act: p.act, text: p.text, images: p.images }),
    sent: () => renderMain(),
    // Didn't go: red on its Thread with why, Retry and Edit (#422). The saved draft keeps it till then, so a restart
    // brings it back to the box.
    failed: (e, p) => { p.failed = e; deskAnswers.delete(it.id); renderMain(); return true; },
  });
}
/** Pending note `p` on item `id` handed to the hub (act "note"): whether it took it; `p.failed` says why not. */
async function deskNote(id, p) {
  try { await invoke("desk_item_act", { id, act: p.act || "note", text: p.text, images: p.images || [] }); return true; }
  catch (e) { p.failed = String(e); return false; }
}
/** Words from box `key` (`text`: what's in it, trimmed) to the lead of `to.project`, about item `to.item` when there
 *  is one. */
async function deskSendLead(to, key, text, sendNow = false) {
  const { project, item: id } = to;
  const it = id && deskItem(project, id);
  // About an item ready to check: it's sent back with your words, to its holder, as Send back does (off Waiting on you).
  if (it?.state === "to_check" && it.holder) {
    // Its page stays, as after anything you write about an item: the box empties and keeps the cursor.
    await deskSendBack(id, key);
    return renderMain();
  }
  let pend = null;
  // Saved without it only once it went: a restart in between leaves your words in the box.
  await boxSend({}, key, {
    save: true,
    pending: (t, images) => {
      const mine = { text: it ? `About #${it.id} (${it.title}): ${t}` : t, images, at: now(), sent: 0, project };
      const list = deskSent.get(project) || [];
      list.push(mine);
      deskSent.set(project, list);
      // To the lead on its own: the chat shows it at once, Sending. From an item: its page stays, the box emptied and
      // the cursor still in it, so you can go on writing (it used to swap to the lead's chat once the message went,
      // seconds later, taking the box from under your typing); the lead's chat has it when you go back.
      if (!it) { deskTab = "lead"; deskChatEnd = true; }
      // About an item: on its Thread at once.
      pend = it && t ? deskPend(it.id, t, "lead", 0, { key, lead: mine }) : null;
      return mine;
    },
    call: (t, images, mine) => invoke("desk_send", { project, text: mine.text, images, ...(sendNow ? { now: true } : {}) }),
    sent: (r, mine) => {
      mine.sent = now();
      if (it) { toast(`Sent to ${deskLeadName(deskOf(project)) || "the lead"}`); if (text) deskWrote(it.id, "wrote_lead", text); }
    },
    // Didn't go: red where you wrote it, the lead's chat and the item's Thread, with why, Retry and Edit (#422). A
    // picture on its own from an item's page shows only in the lead's chat: it says so here too.
    failed: (e, mine) => { mine.failed = e; if (pend) pend.failed = e; else if (it) toast(`Not sent: ${e}`); renderMain(); return true; },
  });
}
/** What you wrote on item `id`'s page, sent (`act`: to its lead or its holder): kept in its Steps (desk/hub/core/desk.rs act_on). */
// Starts on, on an item's page (#597): where its session starts, yours to pick until it starts.
document.addEventListener("click", (e) => {
  const b = e.target.closest?.("[data-desk-start-on]");
  if (!b || b.classList.contains("on")) return;
  e.stopPropagation();
  invoke("desk_item_act", { id: b.dataset.id, act: "start_on", text: b.dataset.deskStartOn }).then(() => renderMain(), (err) => toast(`Couldn't change where it starts: ${err}`, 5000));
}, true);
const deskWrote = (id, act, text) => invoke("desk_item_act", { id, act, text }).catch((e) => console.error(e));
/** Whether item `id` is done (its Send back is a Reopen, #577). */
const deskDone = (id) => (state.desk?.projects || []).some((p) => (p.items || []).some((x) => x.id === id && x.state === "done"));
/** Item `id` sent back to its holder with what's in box `key`: the box emptied once it went (kept when it didn't).
 *  Whether it went. */
async function deskSendBack(id, key) {
  // A quick phrase (compose.js sendPhrase): the box holds what you typed, so it isn't emptied, whether it goes or not.
  const phrase = phraseSending, reopen = deskDone(id);
  let pend = null;
  const r = await boxSend({}, key, {
    clear: "sent",
    pending: (text, images) => (pend = text ? deskPend(id, text, "back", images.length, { key, images }) : null),
    draw: (p) => { if (p) renderMain(); },
    call: (text, images) => invoke("desk_item_act", { id, act: "send_back", text, images }),
    // Its session closed or busy on another item (#577): a fresh one takes it, with a hand-off.
    sent: (it) => toast(`${reopen ? "Reopened" : "Sent back"}${it?.holder?.name ? ` to ${it.holder.name}` : it && !it.holder ? " · a fresh session takes it" : ""}`),
    // Red on its Thread with why, Retry and Edit (#422), the box closed; a picture alone keeps its box and says why.
    failed: (e) => {
      if (!pend) { toast(`Not sent: ${e}`, 5000); return true; }
      pend.failed = e;
      if (!phrase) { delete drafts[key]; saveDrafts(); }
      if (deskBackFor === id) deskBackFor = null;
      return true;
    },
  });
  return !!r.ok;
}
/** Pending message `p` (deskPend, path "holder") sent to its holder session as about its item, from a box of its own
 *  so nothing in yours is touched: deliver's outcome ({ ok } | { error }; nothing when it never went). */
async function deskToHolder(p, k = p.key) {
  if (k !== p.key) Object.assign(draft(k), { text: p.text, images: (p.images || []).map((im) => ({ ...im })) });
  const res = await deliver(p.sid, k, boxToSession(p.sid, { about: p.about }));
  if (k !== p.key) { delete drafts[k]; saveDrafts(); }
  return res;
}
/** Item `id`'s message that couldn't go, at `at`. */
const deskFailedOf = (id, at) => (deskPending.get(id) || []).find((x) => String(x.at) === String(at) && x.failed != null);
/** The item Thread's copy of message `m` to the lead: [item id, its pending entry], or null. */
const deskPendOf = (m) => { for (const [id, list] of deskPending) { const p = list.find((x) => x.lead === m); if (p) return [id, p]; } return null; };
/** A message to the lead taken out of its chat's list. */
function deskSentOff(m) {
  for (const [project, list] of deskSent) if (list.includes(m)) deskSent.set(project, list.filter((x) => x !== m));
}
/** Retry on item `id`'s message that couldn't go: sent again from its own words, by the way it first went (#422).
 *  Red till it goes; the box isn't touched. */
async function deskMsgRetry(id, at) {
  const p = deskFailedOf(id, at);
  if (!p) return;
  if (p.why === "lead" && p.lead) return deskLeadRetry(p.lead);
  if (p.why === "note") {
    p.failed = null;
    renderMain();
    await deskNote(id, p);
    return renderMain();
  }
  if (p.why === "holder") {
    // The guards first: it stays red, and why, while its holder restarts or asks you something.
    const held = boxHeld(p.sid);
    if (held) return toast(held);
    p.failed = null;
    renderMain();
    const res = await deskToHolder(p, `retry:${id}:${p.at}`);
    if (res?.ok) deskWrote(id, "wrote_holder", p.text);
    else p.failed = res?.error || "";
    return renderMain();
  }
  // Sent back.
  p.failed = null;
  await boxSend({}, null, {
    text: p.text, images: p.images || [],
    call: () => invoke("desk_item_act", { id, act: "send_back", text: p.text, images: p.images || [] }),
    sent: (it) => { toast(`Sent back${it?.holder?.name ? ` to ${it.holder.name}` : ""}`); renderMain(); },
    failed: (e) => { p.failed = e; renderMain(); return true; },
  });
}
/** Retry on message `m` to the lead that couldn't go (its chat's, and its item's Thread's copy): sent again as it was. */
async function deskLeadRetry(m) {
  const pend = deskPendOf(m);
  m.failed = null;
  if (pend) pend[1].failed = null;
  // Its own words as they first went (the box isn't touched).
  await boxSend({}, null, {
    text: m.text, images: m.images || [],
    call: () => invoke("desk_send", { project: m.project, text: m.text, images: m.images || [] }),
    sent: () => { m.sent = now(); if (pend) deskWrote(pend[0], "wrote_lead", pend[1].text); renderMain(); },
    failed: (e) => { m.failed = e; if (pend) pend[1].failed = e; renderMain(); return true; },
  });
}
/** Edit on item `id`'s message that couldn't go: back into the box it went from (the item's box as drawn now when
 *  that one isn't), off its Thread and the lead's chat (#422). */
function deskMsgEdit(id, at) {
  const p = deskFailedOf(id, at);
  if (!p) return;
  deskUnpend(id, p);
  if (p.lead) deskSentOff(p.lead);
  if (p.why === "back") deskBackFor = id;
  const key = p.why === "back" ? `back:${id}` : document.querySelector(`textarea[data-text="${CSS.escape(p.key)}"]`) ? p.key : deskItemBox(id) || p.key;
  boxPutBack(key, p.text, p.images || [], { focus: true });
}
/** Retry and Edit on a message to the lead that couldn't go (its chat's, at `at`, in Desk `project`) (#422). Edit puts
 *  it in the lead's box as it was sent (from an item's page, with its "About #…"). */
function deskLeadFailed(project, at, edit) {
  const m = (deskSent.get(project) || []).find((x) => String(x.at) === String(at) && x.failed);
  if (!m) return;
  if (!edit) return deskLeadRetry(m);
  deskSentOff(m);
  const pend = deskPendOf(m);
  if (pend) deskUnpend(pend[0], pend[1]);
  boxPutBack(deskBoxKey({ project }), m.text, m.images || [], { focus: true });
}
/** The box on item `id`'s page, as drawn now ("" for none). */
const deskItemBox = (id) => [...deskBoxes].filter(([, to]) => to.item === id).map(([k]) => k).find((k) => document.querySelector(`textarea[data-text="${CSS.escape(k)}"]`)) || "";
/** Edit on a message Cue keeps for session `sid` (sent from item `id`'s page): taken back, as the session's chat's Edit
 *  does (chat/compose.js pullHeld), into this page's box, as your words alone (deskYourWords, #422, #605). */
async function deskHeldEdit(id, sid) {
  const key = deskItemBox(id);
  if (!key) return pullHeld(sid);
  forgetSent(sid);
  const h = await invoke("unhold_message", { sessionId: sid }).catch(() => null);
  if (!h) { toast("Too late: it had already gone"); return renderMain(); }
  boxPutBack(key, deskYourWords(h.text)?.words ?? h.text, await boxHeldImages(h.images), { focus: true });
}
/** Cancel on a message Cue keeps for session `sid`: taken back and dropped (#422). */
async function deskHeldCancel(sid) {
  forgetSent(sid);
  const h = await invoke("unhold_message", { sessionId: sid }).catch(() => null);
  toast(h ? "It won't be sent" : "Too late: it had already gone");
  renderMain();
}
/** Item `id` of the project picked, open in the chat card (the Inbox's Open in Desk). */
function deskOpenItem(id) {
  deskSeeItem(id);
  deskOpen = { kind: "item", id, project: deskSelection(), fresh: now() };
  deskDropArm = deskGiveFor = null;
}
async function deskAct(id, act, el = null) {
  // Looks right commits the item's files at once; Undo on a done item only puts it back to check (desk/commit.js).
  if (act === "looks_right") return deskLooksRight(id);
  if (act === "looks_undo") return deskLooksUndo(id);
  if (act === "reopen") return deskReopen(id);
  if (act === "msg_retry") return deskMsgRetry(id, el?.dataset.at);
  if (act === "msg_edit") return deskMsgEdit(id, el?.dataset.at);
  if (act === "held_edit") return deskHeldEdit(id, el?.dataset.sid);
  if (act === "held_cancel") return deskHeldCancel(el?.dataset.sid);
  if (act === "lead_retry" || act === "lead_edit") return deskLeadFailed(id, el?.dataset.at, act === "lead_edit");
  if (act === "log_all") { if (!deskFinLogOpen.delete(id)) deskFinLogOpen.add(id); return renderMain(); }
  // Drop, and Remove on a kept copy, take two clicks: the first arms it.
  if ((act === "drop" || act === "remove_kept") && deskDropArm !== id) { deskDropArm = id; return renderMain(); }
  deskDropArm = null;
  // Send back opens the item's own box for why (deskBackHtml); its Send sends that, to the item's holder only.
  if (act === "send_back") { deskBackFor = deskBackFor === id ? null : id; renderMain(); focusComposer(`back:${id}`, { end: false }); return; }
  // Send back to fix (#250): the same box, its why started with what stopped the landing.
  if (act === "fix_back") {
    const it = (state.desk?.projects || []).flatMap((p) => p.items || []).find((x) => x.id === id), d = draft(`back:${id}`);
    if (!d.text.trim() && it?.finish?.question) d.text = `The landing stopped: ${it.finish.question}. Fix it, then say DONE again.`;
    deskBackFor = id; renderMain(); focusComposer(`back:${id}`, { end: false }); return;
  }
  if (act === "back_cancel") { endDictation(`back:${id}`); deskBackFor = null; return renderMain(); }
  if (act === "back_go") {
    const key = `back:${id}`;
    if (!draft(key).text.trim() && !draft(key).images.length) { toast("Say why it goes back first", 4000); focusComposer(key, { end: false }); return; }
    if (await deskSendBack(id, key)) deskBackFor = null;
    return renderMain();
  }
  try {
    const it = await invoke("desk_item_act", { id, act, text: el?.dataset.on || el?.dataset.path || "" });
    if (act === "drop") deskOpen = null;
    // A plain retry that stopped once more: what stopped it this time, in git's own words when it said any.
    if (act === "rerun" && it?.finish?.question) toast(`Stopped again. ${it.finish.question}`, 8000);
    if (act === "clear_land" && it?.finish?.question) toast(`Cleared, and it stopped again. ${it.finish.question}`, 8000);
  }
  catch (e) { toast(String(e), 5000); }
  renderMain();
}
/** Item `id`'s Send back, open: "Why send it back?" and Cancel over a box of its own (never the lead's), as the Desk's
 *  boxes are: words, the mic, a picture, Send. Enter sends, ⇧Enter a new line, Esc closes it. */
function deskBackHtml(id) {
  const done = deskDone(id), key = `back:${id}`, label = done ? "Why reopen it?" : "Why send it back?";
  return boxHtml({ key, label, placeholder: done ? "What isn't right" : "What isn't right yet", head: { label, cancel: `data-desk-act="back_cancel" data-id="${esc(id)}"` }, send: { attrs: `data-desk-act="back_go" data-id="${esc(id)}"` } });
}
async function deskUndo(id) {
  try { toast((await invoke("desk_undo", { decisionId: id })) || "Asked to undo it"); }
  catch (e) { toast(String(e)); }
  renderMain();
}
/** A live session found with ⌘K on the Desk (#316): its chat in the middle, as its name on the left opens it (‹ back to
 *  the item or holder open). With All Desks shown, on the first Desk it's in. False when there's no Desk to show it on. */
function deskOpenFound(sid) {
  if (!sessionOf(sid)) return false;
  if (!deskSelection()) {
    const d = (state.desk?.projects || []).find((x) => deskHasSession(x, sid));
    if (!d) return false;
    deskSelect(d.project);
  }
  deskChatEnd = true;
  deskShow({ kind: "chat", sid, from: ["item", "holder"].includes(deskOpen?.kind) && deskOpen.project === deskSelection() ? { ...deskOpen } : null });
  return true;
}
/** What ⌘K finds on the Desk (#316): its items (a bug, a feature, a task: by title, number, kind, what it asks or who
 *  holds it) and, as `sids`, the sessions that work for it; the Desk picked, or every Desk with All Desks. Typed nothing:
 *  what's open now, the ones waiting on you first. */
function deskFindHere(q) {
  const ds = (state.desk?.projects || []).filter((d) => !deskSelection() || d.project === deskSelection());
  const sids = new Set(ds.flatMap((d) => [deskLead(d)?.sid, ...(d.members || [])]).filter(Boolean));
  const order = { you: 0, stop: 1, work: 2, fin: 3, q: 4, done: 5 };
  const all = ds.flatMap((d) => deskOf(d.project).items.map((i) => ({ i, d })));
  const hay = ({ i, d }) => [i.title, i.words, i.kind, deskNum(i), `#${i.id}`, deskHolderOf(i), deskStateOf(i) === "done" ? "done" : "", ds.length > 1 ? deskName(d) : ""].join(" ").toLowerCase();
  const hits = !q ? all.filter(({ i }) => i.state !== "done").sort((a, b) => order[deskStateOf(a.i)] - order[deskStateOf(b.i)] || deskMs(b.i.updated) - deskMs(a.i.updated)).slice(0, 8)
    : all.map((x) => ({ ...x, s: x.i.id === q.replace(/^#/, "") ? -1 : nameScore(x.i.title, q) ?? (q.split(/\s+/).every((w) => hay(x).includes(w)) ? 5 : null) }))
      .filter((x) => x.s !== null).sort((a, b) => a.s - b.s || (a.i.state === "done") - (b.i.state === "done") || deskMs(b.i.updated) - deskMs(a.i.updated)).slice(0, 20);
  const where = ds.length === 1 ? deskName(ds[0]) : "every Desk";
  return {
    title: `Items · ${where}`, sessions: `Sessions · ${where}`, sids, place: ds.length === 1 ? "on this Desk" : "on the Desks", ask: "Find an item, a session, or anything said on the Desk…",
    rows: hits.map(({ i, d, s: score }) => ({ weak: score === 5,
      html: `${deskStateIcon(i)}${deskKindIcon(i)}<span class="lv-name">${esc(i.title)}</span><span class="sx-branch">${esc(deskNum(i))}${ds.length > 1 ? ` · ${esc(deskName(d))}` : ""}</span>`,
      open: () => { if (deskSelection() !== d.project) deskSelect(d.project); deskShow({ kind: "item", id: i.id }); },
    })),
  };
}
/** A crew member's own session: its lead's crew page, on that member. */
function deskOpenCrew(lead, agent) {
  const t = deskMate(lead, agent);
  if (!t || !sessionOf(lead)) return;
  if (typeof addOn.openCrewPage !== "function") return openSession(lead);
  addOn.openCrewPage(lead, { from: "desk", back: "the Desk" });
  view = "desk";   // its lead becomes the active session, which picks the Board: the page shows on the Desk
  crewLeftSet(lead, { kind: "mate", key: mateKey(lead, t), tab: "", file: "" });
  renderMain();
}

/** The Desk's clicks, by attribute: each with its element (desk/clicks.js; the lead's are in desk/lead.js). Added in a few
 *  groups, in this order (the first rule an element has is the one taken). An item's page, its panels and boxes: */
deskClicks({
  deskPre: (el) => el.closest(".dk-box")?.querySelector("textarea")?.focus(),
  deskItab: (el) => { deskItemTabs.set(el.dataset.id, el.dataset.deskItab); renderMain(); },
  // An item's chat (#418): a turn about another item opened in place, or folded again; its record, all of it.
  deskFold: (el) => { const k = el.dataset.deskFold; if (!deskTurnOpen.delete(k)) deskTurnOpen.add(k); renderMain(); },
  deskPanel: (el) => deskPanelToggle(el.dataset.deskPanel),
  deskItem: (el) => deskShow(deskOpen?.kind === "item" && deskOpen.id === el.dataset.deskItem ? null : { kind: "item", id: el.dataset.deskItem }),
  deskHolder: (el) => deskShow({ kind: "holder", sid: el.dataset.deskHolder }),
  deskAwayFold: (el) => deskAwayFold(el.dataset.deskAwayFold),
  deskAwayLog: (el) => deskAwayLogToggle(el.dataset.deskAwayLog),
  deskHeld: (el) => deskHeldAnswer(el.dataset.deskHeld, el.dataset.allow === "1"),
  deskAddon: (el) => deskShow({ kind: "addon", key: el.dataset.deskAddon }),
  // Opened from All sessions (B2), its ‹ goes back there, its row lit, as a session's chat does.
  deskCrew: (el) => deskShow({ kind: "crew", lead: el.dataset.lead, agent: el.dataset.deskCrew, ...(deskOpen?.kind === "sessions" ? { from: { kind: "sessions", crew: [el.dataset.lead, el.dataset.deskCrew] } } : {}) }),
  deskBack: (el) => deskShow(el.dataset.deskBack ? { kind: "holder", sid: el.dataset.deskBack } : null),
  deskTab: (el) => { deskTab = el.dataset.deskTab; deskChatEnd = true; renderMain(); },
  deskMovesClear: (el) => deskMovesClear(el.dataset.deskMovesClear),
  deskMovesKeep: () => { deskMovesClearing = null; renderMain(); },
  deskSend: (el) => deskSendBox(el.dataset.deskSend),
  deskRetry: (el) => deskRetry(el.dataset.item, el.dataset.deskRetry),
  deskAct: (el) => { deskGiveFor = null; deskAct(el.dataset.id, el.dataset.deskAct, el); },
  deskUndo: (el) => deskUndo(el.dataset.deskUndo),
  deskTick: (el) => { const id = el.dataset.deskTick; if (!deskTicked.delete(id)) deskTicked.add(id); if (!deskTicked.size) deskGoBackOpen = null; renderMain(); },
  deskShowAll: (el) => { const k = el.dataset.deskShowAll; if (!deskShowAll.delete(k)) deskShowAll.add(k); renderMain(); },
  deskGoBack: (el) => {
    const v = el.dataset.deskGoBack;
    if (v === "send") return deskGoBackSend();
    if (v === "clear") { deskTicked.clear(); deskGoBackOpen = null; }
    else deskGoBackOpen = v === "cancel" ? null : v;
    renderMain();
    if (deskGoBackOpen) document.querySelector('textarea[data-text="goback"]')?.focus();
  },
  deskAway: (el) => deskAwayOpen(el.dataset.deskAway),
  deskPause: (el) => deskPauseToggle(el.dataset.deskPause),
});
/** The items' List or Progress, and the rows on the left. */
deskClicks({
  // Items as a List or as Progress (#409, desk/progress.js); a Progress row opens its item's page.
  deskProgToggle: () => deskProgToggle(),
  deskProgOpen: (el) => deskProgOpen(el.dataset.deskProgOpen),
  deskProgNote: (el) => { const id = el.dataset.deskProgNote; if (!deskProgNoteOpen.delete(id)) deskProgNoteOpen.add(id); renderMain(); },
  deskLogRow: (el) => { const id = el.dataset.deskLogRow; if (!deskLogOpen.delete(id)) deskLogOpen.add(id); renderMain(); },
  deskSession: (el) => openSession(el.dataset.deskSession),
  // B2: its chat is the left card already; from All sessions the list stays beside it, its card lit.
  deskLeadChat: () => { deskPick = null; deskTab = "lead"; readerView = null; deskShow(deskB2On() && deskOpen?.kind === "sessions" ? { kind: "sessions" } : null); },
  deskChkItem: (el) => deskShow({ kind: "item", id: el.dataset.deskChkItem }),
  deskDecision: (el) => {
    const it = state.items.find((i) => i.id === el.dataset.id && i.kind === "permission" && i.status === "pending");
    if (it && !answering.has(it.id)) return el.dataset.deskDecision === "allow" ? allow(it) : deny(it);
  },
  // A question's Send its suggestions on its row (#383): what's picked, sent.
  deskSendAnswers: (el) => {
    const it = state.items.find((i) => i.id === el.dataset.deskSendAnswers && i.kind === "question" && i.status === "pending");
    const a = it && !answering.has(it.id) && answersFor(it);
    if (a) return respond(it, { behavior: "allow", answers: a });
  },
  deskRen: (el) => renameSession({ dataset: { rename: el.dataset.deskRen } }),
});
/** Where ‹ goes back to from session `sid`'s chat: the item's page, holder's or All sessions it was opened from (null:
 *  the lead's). Back in All sessions, `sid`'s row is the lit one, with its › (B2, #666: its chat took the list's place). */
const deskChatFrom = (sid) => (["item", "holder", "sessions"].includes(deskOpen?.kind) ? { ...deskOpen, ...(deskOpen.kind === "sessions" ? { was: sid } : {}) } : null);
/** Chats, sessions and files opened from the Desk. */
deskClicks({
  // Opened from an item's page or a holder's, it remembers that page so ‹ goes back to it (from the left list: the lead).
  // A reply's "This turn in …": that session, here in the middle as its name on the left opens it (#306; it was a
  // read-only pop-up, #276), at that reply; ‹ goes back to the item.
  deskTurn: (el) => {
    deskTurnAt = Number(el.dataset.at) || 0;
    deskChatEnd = !deskTurnAt;
    deskShow({ kind: "chat", sid: el.dataset.deskTurn, from: deskChatFrom(el.dataset.deskTurn) });
  },
  deskChatOf: (el) => { deskChatEnd = true; deskShow({ kind: "chat", sid: el.dataset.deskChatOf, from: deskChatFrom(el.dataset.deskChatOf) }); },
  deskChatBack: () => deskShow(deskOpen?.from || null),
  deskAddSession: (el) => {
    const project = el.dataset.deskAddSession;
    deskAddingSession = deskAddingSession === project ? null : project;
    renderMain();
    document.querySelector("[data-desk-add-session]")?.focus();
  },
  // Older's offer (#485): close the sessions idle there over a day; nothing deleted, their chats kept to reopen.
  deskCloseIdle: async (el) => {
    const sids = el.dataset.deskCloseIdle.split(" ").filter(Boolean), failed = [];
    for (const sessionId of sids) { try { await invoke("close_session", { sessionId }); } catch (e) { failed.push(`${deskHolderName(sessionId)} (${e})`); } }
    const done = sids.length - failed.length;
    toast(failed.length ? `Closed ${done}. Couldn't close: ${failed.join(", ")}` : `Closed ${done} session${done === 1 ? "" : "s"} · their chats are kept`, failed.length ? 10000 : undefined);
    renderMain();
  },
  deskOlder: (el) => {
    const key = el.dataset.deskOlder;
    if (!deskOlderOpen.delete(key)) deskOlderOpen.add(key);
    renderMain();
  },
  deskGroup: (el) => deskGroupFold(el.dataset.deskGroup),
  deskDoneFold: (el) => {
    const sid = el.dataset.deskDoneFold;
    if (deskFoldOpen.has(sid)) deskFoldOpen.delete(sid); else deskFoldOpen.add(sid);
    try { localStorage.setItem("cue.deskFold", JSON.stringify([...deskFoldOpen])); } catch { /* kept for this window only */ }
    renderMain();
  },
  deskMade: (el) => {
    const id = el.closest("[data-desk-item]")?.dataset.deskItem || deskOpen?.id;
    deskSeeItem(id);
    openPathOrDoc(el.dataset.deskMade, deskHolderSid(deskItem(deskSelection(), id)) || null);
    renderMain();
  },
  deskOpenFile: (el) => deskOpenFile(el.dataset.id, el.dataset.deskOpenFile, el.dataset.folder || ""),
  deskDiffFile: (el) => deskShowFile(el.dataset.deskDiffFile, +el.dataset.n),
  deskTurnFile: (el) => deskShowTurnFile(el.dataset.deskTurnFile, +el.dataset.n),
  deskMateFile: (el) => deskShowMateFile(el.dataset.deskMateFile, +el.dataset.n),
  deskSessDiff: (el) => { extTabOf.set(el.dataset.deskSessDiff, DIFF_TAB); openSession(el.dataset.deskSessDiff); },
  deskCrewOpen: (el) => { deskCrewMenu = ""; deskOpenCrew(el.dataset.lead, el.dataset.deskCrewOpen); },
  deskCrewMore: (el) => { deskCrewMenu = deskCrewMenu === el.dataset.deskCrewMore ? "" : el.dataset.deskCrewMore; renderMain(); },
});
/** A Desk's own: Delete, the switcher, Home and the next Desk. */
deskClicks({
  deskDelete: (el) => { deskDeleteAsk = el.dataset.deskDelete; deskCloseSelected = new Set(deskDeleteSessions(deskOf(deskDeleteAsk)).filter((s) => !s.shared && !s.busy).map((s) => s.sid)); renderMain(); },
  deskDeleteCancel: () => { deskDeleteAsk = null; renderMain(); },
  deskDeleteConfirm: async () => {
    if (deskDeleting || !deskDeleteAsk) return;
    const id = deskDeleteAsk; deskDeleting = true; renderMain();
    try {
      const closeSessions = deskDeleteSessions(deskOf(id)).filter((s) => deskCloseSelected.has(s.sid) && !s.shared && !s.busy).map((s) => s.sid);
      const result = await invoke("desk_delete", { id, closeSessions });
      if (result?.kept?.length) toast(`Desk deleted. Couldn't close: ${result.kept.map((x) => `${deskHolderName(x.sid)} (${x.reason})`).join(", ")}`, 10000);
      const kept = new Set((result?.kept || []).map((x) => x.sid));
      deskGoneNow.desks.add(id);
      for (const sid of closeSessions) if (!kept.has(sid)) deskGoneNow.sessions.add(sid);
      deskGoneApply(state);
      deskDeleteAsk = null;
      if (deskSelection() === id) { deskSelect(""); deskOpen = null; deskPick = null; }
    } catch (e) { toast(`Couldn't delete the Desk: ${e}`); }
    deskDeleting = false; renderMain();
  },
  deskGo: (el) => { DESK_CLICKS.deskProject({ dataset: { deskProject: el.dataset.deskGo } }); },
  deskHome: () => { if (!deskDeleting) deskDeleteAsk = null; deskSelect(""); deskOpen = null; deskPick = null; renderMain(); },
  deskProject: (el) => { deskSelect(el.dataset.deskProject); deskOpen = null; deskTab = "lead"; deskChatEnd = true; renderMain(); },
});
/** A click rule's attribute: deskItem → data-desk-item. */
const deskAttr = (k) => `data-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
/** Every Desk click attribute as one selector, from the rules there are when it's asked. */
const deskSel = () => Object.keys(DESK_CLICKS).map((k) => `[${deskAttr(k)}]`).join(", ");
/** Element `el` (one with a Desk attribute, or null) clicked or pressed: an armed Drop disarms on anything else (as
 *  Remove does in the projects list); then `el` does its thing. True when it was the Desk's. */
function deskTake(el) {
  if (deskDropArm && !((el?.dataset.deskAct === "drop" || el?.dataset.deskAct === "remove_kept") && el.dataset.id === deskDropArm)) { deskDropArm = null; if (!el) renderMain(); }
  if (!el || !el.closest(".dk-board")) return false;
  DESK_CLICKS[Object.keys(DESK_CLICKS).find((k) => k in el.dataset)](el);
  return true;
}
/** A click in the Desk: the innermost element with one of its attributes. */
function deskClick(e) {
  if (view !== "desk" || e.button > 0) return;
  if (deskAddingSession && !e.target.closest?.(".dk-add-anchor")) {
    deskAddingSession = null;
    queueMicrotask(renderMain);
  }
  // The new Desk page's menus close on a click outside them (desk/v2.js).
  deskV2Outside(e);
  // Progress (#409) closes on a click outside it.
  if (deskProgShown && !e.target.closest?.(".dk-prog, [data-desk-prog-toggle]")) { deskProgShown = false; queueMicrotask(renderMain); }
  // A crew member's ⋯ menu closes on a click outside it.
  if (deskCrewMenu && !e.target.closest?.(".dk-cmore")) { deskCrewMenu = ""; queueMicrotask(renderMain); }
  // The header's Desks menu (app/head_view.js) too: a Desk click stops here, before the window's own clicks see it.
  if (hvMenu && !e.target.closest?.(".hv-wrap")) { hvMenu = ""; delete drafts["hv-desk-find"]; queueMicrotask(renderMain); }
  // In a session's name box (✎ on its row): the click stays in the box, it doesn't open the row.
  if (e.target.closest?.(".dk-board .rename-in")) { e.stopPropagation(); return; }
  // A link in a row (an item's canvas, #436): it opens, not the row.
  if (e.target.closest?.(".dk-board a[data-href]")) return;
  if (deskTake(e.target.closest?.(deskSel()))) e.stopPropagation();
}
/** The Desk's box on screen that sends to session `sid` itself (a session's or holder's page, or the lead's chat for its
 *  lead), where Edit on a message Cue kept for it brings it back (#498); "" for none. */
function deskKeptBox(sid) {
  const shown = (k) => document.querySelector(`textarea[data-text="${CSS.escape(k)}"]`);
  const lead = (to) => !to.sid && !to.item && deskLead(deskOf(to.project))?.sid === sid;
  return [...deskBoxes].filter(([, to]) => (to.sid === sid && !to.mate && !to.item) || lead(to)).map(([k]) => k).find(shown) || "";
}
/** The session Desk box `key` writes to itself: a session's or holder's page's, or the lead's chat's (its lead); null
 *  for one that goes about an item or a crew member. */
function deskBoxSid(key) {
  const to = deskBoxes.get(key);
  if (!to || to.item || to.mate) return null;
  return to.sid || deskLead(deskOf(to.project))?.sid || null;
}
// The Desk's boxes, on the window's shared box (box/send.js BOX_OWNERS): Enter and a quick phrase send them, ↑ ↓ bring
// back what went to their session, an item page's suggested reply, and the Desk's chat end for the reader.
BOX_OWNERS.push({
  owns: (key) => key.startsWith("back:") || deskBoxes.has(key),
  send: (key) => (key.startsWith("back:") ? deskAct(key.slice(5), "back_go") : deskSendBox(key)),
  // ↵ sends; ⌘↵ as in a session's box (app/keys.js enterInSessionBox, #630): what's typed goes now, and with nothing
  // typed, the message Cue kept for the session this box writes to goes now.
  enter(e, key) {
    if (key.startsWith("back:") || !(e.metaKey || e.ctrlKey)) return this.send(key);
    const typed = draft(key).text.trim() || draft(key).images.length, sid = deskBoxSid(key);
    if (!typed && sid && sendWaitingNow(sid)) return;
    return deskSendBox(key, true);
  },
  recall: (key) => deskRecall(key),
  suggestion: (sid) => (sid?.startsWith?.("item:") ? deskItemSuggestion(sid.slice(5)) : null),
  view: "desk",
  chatEnd: (atEnd) => { deskChatEnd = atEnd; },
  keptBox: (sid) => deskKeptBox(sid),
});
// Go back on ticked decisions (desk/page.js): Enter sends it, as its Send button does (#522).
BOX_OWNERS.push({ owns: (key) => key === "goback", send: () => deskGoBackSend() });
/** Esc's session on the Desk (#494), as the Board's open session: the lead's in its chat, the session on a session's or
 *  holder's page (a crew member's: its lead), the item's holder on an item's page (its page is its holder's chat, #418).
 *  Handed to the Board's Esc (app/keys.js escSession) with the box at the foot: a message Cue kept for it comes back
 *  there (on an item's page as its Edit does, deskHeldEdit), and Esc twice stops it. */
function deskEscSession(at = document.activeElement) {
  const d = deskOf(deskSelection()), o = deskOpen;
  if (!d) return false;
  // B2 with a page open on the right: the lead's chat keeps its card at the left, and Esc there is the lead's (its kept
  // message into its own box, Esc twice stops it), never the page's on the right.
  if (deskLeadCardHas(at)) {
    const lk = [...document.querySelectorAll(".dk2b-chat textarea[data-text]")].map((t) => t.dataset.text).find((k) => deskBoxes.has(k));
    return deskPick !== d.project && deskLead(d) && !deskLeadEnded(d) ? escSession(deskLead(d).sid, lk) : false;
  }
  // The open card's box (.dk-right): on B2 the lead's chat keeps its own box at the left while a page is open.
  const boxIn = (q) => [...document.querySelectorAll(q)].map((t) => t.dataset.text).find((k) => deskBoxes.has(k));
  const key = boxIn(".dk-right textarea[data-text]") || boxIn("textarea[data-text]");
  if (o?.kind === "item") return escSession(deskHolderSid(deskItem(d.project, o.id)), key, (sid) => deskHeldEdit(o.id, sid));
  const lead = !o && deskTab === "lead" && deskPick !== d.project && deskLead(d) && !deskLeadEnded(d) ? deskLead(d).sid : "";
  return escSession(o?.kind === "crew" ? o.lead : o?.kind === "chat" || o?.kind === "holder" ? o.sid : lead, key);
}
/** Whether `el` is in the lead's card while B2 shows a page beside it (the left card, not the one marked .dk-right). */
const deskLeadCardHas = (el) => !!(deskB2Side() && el?.closest?.(".dk2b-chat:not(.dk-right)"));
/** Keys on the Desk: ⌘N goes to the quick-add line (a box's Enter is the window's, app/keys.js enterInBox), the quick-add line's keys
 *  (desk/lead.js), Esc closes a Give to… menu, then the picker (when changing), then takes back a message Cue kept for
 *  the open session or stops it (Esc twice, deskEscSession), then the open pane. */
function deskKey(e) {
  if (view !== "desk" || e.isComposing || e.keyCode === 229) return false;
  const key = e.target?.dataset?.text || "";
  // A row (an item, a holder, a crew member, a project's Desk): Enter or Space opens it as a click does, and it keeps
  // the keyboard where it is.
  const row = e.target?.matches?.('.dk-board [role="button"]') && e.target.matches(deskSel()) ? e.target : null;
  if (row && (e.key === "Enter" || e.key === " ") && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    const at = Object.keys(DESK_CLICKS).find((k) => k in row.dataset), sel = `[${deskAttr(at)}="${CSS.escape(row.dataset[at])}"]`;
    deskTake(row);
    // Today's page (B2, desk/b2.js) opens an item in the lead chat's place and the list stays: the keyboard stays on
    // its row (else the page's way back, ‹ Items N), never in its box.
    (document.querySelector(`.dk-board ${sel}`) || document.querySelector(".dk-right .dk-ph [data-desk-back]"))?.focus();
    return true;
  }
  if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key?.toLowerCase() === "n" && deskSelection() && !sheet && !(crewPageSid && document.querySelector(".crew-page"))) return deskQaFocus();
  if (deskQaKey(e, key)) return true;
  if (e.key === "Escape" && deskV2Escape()) return true;
  if (e.key === "Escape" && deskProgShown) { deskProgToggle(false); document.querySelector("[data-desk-prog-toggle]")?.focus(); return true; }
  if (e.key === "Escape" && key.startsWith("back:")) { deskAct(key.slice(5), "back_cancel"); return true; }
  // Esc: only when nothing over the Desk wants it (the crew page, a sheet, search and the live list, the projects
  // list, a menu, a rename, a dialog: How work lands, Ask me before, quitting, trusting a folder), as the Inbox's keys do.
  const over = sheet || lightbox || liveOpen || deleteAsk || quitAsk || trustAsk || (typeof lands !== "undefined" && lands) || (typeof askBefore !== "undefined" && askBefore) || renaming || pjRenaming || pjOpen || pjNewOpen || colorFor || newProjFor || btwFor || menuFor || redirectFor || (typeof moreFor !== "undefined" && moreFor) || (crewPageSid && document.querySelector(".crew-page"));
  if (e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey && !over) {
    if (deskAddingSession && deskAddingSession === deskSelection()) {
      deskAddingSession = null;
      renderMain();
      document.querySelector("[data-desk-add-session]")?.focus();
      return true;
    }
    if (deskDropArm) { deskDropArm = null; renderMain(); return true; }
    if (deskGiveFor) { deskGiveFor = null; renderMain(); return true; }
    if (deskPick && deskLead(deskOf(deskSelection()))) { deskPick = null; renderMain(); return true; }
    // Then the session the Desk has open, as on the Board (#494): a message Cue kept comes back, Esc twice stops it.
    // Not while the reader or a background agent's view is in the chat's place (the Board's Esc closes those first).
    const shown = readerShown() || (helperView && helperViewed(helperView.sid)) || (key && cmdMatches(key).length);
    if (!shown && deskEscSession(e.target)) return true;
    // The page on the right closes from its own card or the list's, not from the lead's box at the left.
    if (deskOpen && !deskLeadCardHas(e.target) && !(e.target.matches?.("textarea, input") && e.target.value)) { deskShow(deskOpen.kind === "crew" ? deskOpen.from || { kind: "holder", sid: deskOpen.lead } : deskOpen.kind === "chat" ? deskOpen.from || null : null); return true; }
  }
  return false;
}

if (typeof document !== "undefined") {
  document.addEventListener("click", deskClick, true);
  document.addEventListener("change", (e) => {
    const input = e.target.closest?.(".dk-add-option [data-desk-member]");
    if (input) deskMember(input.dataset.project, input.dataset.deskMember, input.checked);
  });
  // Leaving a session's name box (✎ on the Desk) drops the rename (Enter has kept it already). Checked a moment later:
  // a redraw puts the cursor back in the new box, and that isn't leaving it.
  document.addEventListener("focusout", (e) => {
    if (!e.target?.matches?.(".dk-board .rename-in")) return;
    const key = e.target.dataset.text;
    setTimeout(() => {
      if (!renaming || `rename:${renaming}` !== key || document.activeElement?.dataset?.text === key) return;
      renaming = null;
      delete drafts[key];
      renderMain();
    }, 0);
  }, true);
  // The lead's chat scrolled: whether it's at its end (new messages keep it there).
  document.addEventListener("scroll", (e) => { const c = e.target; if (c?.matches?.(".dk-chat")) deskChatEnd = boxAtEnd(c); }, true);
}
