// Cue UI desk — acting: open, send, send back, Looks right and the other item actions, undo, and the Desk's clicks and keys.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (index.html, mini.html).
// ---------- acting ----------

/** Open `what` in the chat card (null: back to the lead's chat). */
function deskShow(what) {
  if (what?.kind === "item") deskSeeItem(what.id);
  deskOpen = what && { ...what, project: deskSelection() };
  if (what?.kind !== "chat") deskTurnAt = 0;
  deskDropArm = null;
  deskGiveFor = null;
  if (!what) deskChatEnd = true;
  renderMain();
}
/** A quick phrase picked on an item's page: it's in the box (sendPhrase added it), the cursor after it, nothing sent. */
function deskPhraseIn(key) {
  renderMain();
  const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  if (!el) return;
  el.focus();
  el.selectionStart = el.selectionEnd = el.value.length;
  el.dispatchEvent(new Event("input", { bubbles: true }));
}
/** Send what's in box `key`: to the lead (naming the open item), or to the lead (naming its crew member). Only what's
 *  sent is named: the box keeps what you typed if it doesn't go. */
async function deskSendBox(key) {
  const to = deskBoxes.get(key);
  const text = draft(key).text.trim();
  const images = draft(key).images.map(({ name, mime, data }) => ({ name, mime, data }));
  if (!to || (!text && !images.length)) return;
  if (to.sid) {
    // An answer from an item's page: the page says what you answered (deskChain).
    const it = to.item && deskItem(deskSelection(), to.item);
    if (it?.state === "needs_you" && sessionOf(to.sid)?.state !== "deciding" && !state.restarting?.[to.sid]) deskAnswers.set(it.id, { q: deskQuestion(it), a: text || "a picture", ask: it.ask?.item_id || "", at: now() });
    // From an item's page: it goes as about that item, so the session answers about it and its reply is that item's (#176).
    if (!to.mate && it && !/^[/!]/.test(text)) {
      if (state.restarting?.[to.sid]) return toast("It's restarting. Send once it's back.");
      if (sessionOf(to.sid)?.state === "deciding") return toast("It's asking you something first. Answer that, then send.");
      await deliver(to.sid, key, (typed, images) => {
        const said = typed ? `About #${it.id} (${it.title}): ${typed}` : `About #${it.id} (${it.title}): (a picture)`;
        insSent(to.sid, said, false);
        return invoke("send_to_session", { sessionId: to.sid, text: said, images, now: false });
      });
    }
    else if (!to.mate) await sendTo(to.sid, false, key);
    else if (state.restarting?.[to.sid]) return toast("It's restarting. Send once it's back.");
    else if (sessionOf(to.sid)?.state === "deciding") return toast("It's asking you something first. Answer that, then send.");
    else await deliver(to.sid, key, (typed, images) => {
      const said = it ? /* its item's number too, so the turn is that item's (#276) */ `About #${it.id} (${it.title}), for ${to.mate}: ${typed}` : `About ${to.mate}: ${typed}`;
      insSent(to.sid, said, false);
      return invoke("send_to_session", { sessionId: to.sid, text: said, images, now: false });
    });
    // Went (the box emptied): on the item's page, in its Steps.
    if (it && text && !draft(key).text.trim()) { deskPend(it.id, text, "holder"); deskWrote(it.id, "wrote_holder", text); renderMain(); }
    return;
  }
  const { project, item: id } = to;
  const it = id && deskItem(project, id);
  // About an item ready to check: it's sent back with your words, to its holder, as Send back does (off Waiting on you).
  if (it?.state === "to_check" && it.holder) {
    // Its page stays, as after anything you write about an item: the box empties and keeps the cursor.
    await deskSendBack(id, key, text, images);
    return renderMain();
  }
  const said = it ? `About #${it.id} (${it.title}): ${text}` : text;
  endDictation(key);
  delete drafts[key];
  saveDrafts();
  const list = deskSent.get(project) || [], mine = { text: said, images, at: now(), sent: 0 };
  list.push(mine);
  deskSent.set(project, list);
  // To the lead on its own: the chat shows it at once, Sending. From an item: its page stays, the box emptied and the
  // cursor still in it, so you can go on writing (it used to swap to the lead's chat once the message went, seconds
  // later, taking the box from under your typing); the lead's chat has it when you go back.
  if (!it) { deskTab = "lead"; deskChatEnd = true; }
  // About an item: on its Thread at once.
  const pend = it && text ? deskPend(it.id, text, "lead") : null;
  renderMain();
  try {
    await invoke("desk_send", { project, text: said, images });
    mine.sent = now();
    if (it) { toast(`Sent to ${deskLeadName(deskOf(project)) || "the lead"}`); if (text) deskWrote(it.id, "wrote_lead", text); }
  }
  catch (e) { if (pend) deskUnpend(it.id, pend); deskSent.set(project, (deskSent.get(project) || []).filter((p) => p !== mine)); Object.assign(draft(key), { text, images }); toast(`Not sent: ${e}`); renderMain(); }
}
/** What you wrote on item `id`'s page, sent (`act`: to its lead or its holder): kept in its Steps (hub/desk.rs act_on). */
const deskWrote = (id, act, text) => invoke("desk_item_act", { id, act, text }).catch((e) => console.error(e));
/** Item `id` sent back to its holder from box `key` with `text` and `images`: the box emptied once it went (kept when
 *  it didn't). Whether it went. */
async function deskSendBack(id, key, text, images) {
  endDictation(key);
  const pend = text.trim() ? deskPend(id, text.trim(), "back", (images || []).length) : null;
  if (pend) renderMain();
  try {
    const it = await invoke("desk_item_act", { id, act: "send_back", text, images });
    delete drafts[key];
    saveDrafts();
    toast(`Sent back${it?.holder?.name ? ` to ${it.holder.name}` : ""}`);
    return true;
  }
  catch (e) { if (pend) deskUnpend(id, pend); toast(`Not sent: ${e}`, 5000); return false; }
}
/** Item `id` of the project picked, open in the chat card (the Inbox's Open in Desk). */
function deskOpenItem(id) {
  deskSeeItem(id);
  deskOpen = { kind: "item", id, project: deskSelection(), fresh: now() };
  deskDropArm = deskGiveFor = null;
}
async function deskAct(id, act, el = null) {
  // Looks right commits the item's files at once; Undo on a done item only puts it back to check (desk_commit.js).
  if (act === "looks_right") return deskLooksRight(id);
  if (act === "looks_undo") return deskLooksUndo(id);
  if (act === "reopen") return deskReopen(id);
  if (act === "log_all") { if (!deskFinLogOpen.delete(id)) deskFinLogOpen.add(id); return renderMain(); }
  // Drop, and Remove on a kept copy, take two clicks: the first arms it.
  if ((act === "drop" || act === "remove_kept") && deskDropArm !== id) { deskDropArm = id; return renderMain(); }
  deskDropArm = null;
  // Send back opens the item's own box for why (deskBackHtml); its Send sends that, to the item's holder only.
  if (act === "send_back") { deskBackFor = deskBackFor === id ? null : id; renderMain(); return deskBackFocus(id); }
  // Send back to fix (#250): the same box, its why started with what stopped the landing.
  if (act === "fix_back") {
    const it = (state.desk?.projects || []).flatMap((p) => p.items || []).find((x) => x.id === id), d = draft(`back:${id}`);
    if (!d.text.trim() && it?.finish?.question) d.text = `The landing stopped: ${it.finish.question}. Fix it, then say DONE again.`;
    deskBackFor = id; renderMain(); return deskBackFocus(id);
  }
  if (act === "back_cancel") { endDictation(`back:${id}`); deskBackFor = null; return renderMain(); }
  if (act === "back_go") {
    const key = `back:${id}`, text = draft(key).text.trim();
    const images = draft(key).images.map(({ name, mime, data }) => ({ name, mime, data }));
    if (!text && !images.length) { toast("Say why it goes back first", 4000); return deskBackFocus(id); }
    if (await deskSendBack(id, key, text, images)) deskBackFor = null;
    return renderMain();
  }
  try {
    const it = await invoke("desk_item_act", { id, act, text: el?.dataset.on || el?.dataset.path || "" });
    if (act === "drop") deskOpen = null;
    // A plain retry that stopped once more: what stopped it this time, in git's own words when it said any.
    if (act === "rerun" && it?.finish?.question) toast(`Stopped again. ${it.finish.question}`, 8000);
  }
  catch (e) { toast(String(e), 5000); }
  renderMain();
}
/** Item `id`'s Send back, open: "Why send it back?" and Cancel over a box of its own (never the lead's), as the Desk's
 *  boxes are: words, the mic, a picture, Send. Enter sends, ⇧Enter a new line, Esc closes it. */
function deskBackHtml(id) {
  const key = `back:${id}`, mic = dictating?.key === key;
  return `<div class="dk-sb"><div class="dk-sb-h"><span class="dk-sb-l">Why send it back?</span><button class="dk-sb-x" data-desk-act="back_cancel" data-id="${esc(id)}">Cancel</button></div><div class="dk-box dk-sb-box" data-drop="${esc(key)}">${draftChips(key)}<div class="dk-box-row"><textarea rows="1" data-text="${esc(key)}" aria-label="Why send it back?" placeholder="What isn't right yet" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><button class="icon-btn mic ${mic ? "on" : ""}" data-mic="${esc(key)}" aria-label="${mic ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button><button class="icon-btn" data-attach="${esc(key)}" aria-label="Add an image">${IMAGE_ICON}</button><button class="btn primary small" data-desk-act="back_go" data-id="${esc(id)}">Send</button></div></div></div>`;
}
const deskBackFocus = (id) => document.querySelector(`textarea[data-text="${CSS.escape(`back:${id}`)}"]`)?.focus();
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

/** The Desk's clicks, by attribute: each with its element. */
const DESK_CLICKS = {
  deskPre: (el) => el.closest(".dk-box")?.querySelector("textarea")?.focus(),
  deskItab: (el) => { deskItemTabs.set(el.dataset.id, el.dataset.deskItab); renderMain(); },
  deskPanel: (el) => deskPanelToggle(el.dataset.deskPanel),
  deskItem: (el) => deskShow(deskOpen?.kind === "item" && deskOpen.id === el.dataset.deskItem ? null : { kind: "item", id: el.dataset.deskItem }),
  deskHolder: (el) => deskShow({ kind: "holder", sid: el.dataset.deskHolder }),
  deskAddon: (el) => deskShow({ kind: "addon", key: el.dataset.deskAddon }),
  deskCrew: (el) => deskShow({ kind: "crew", lead: el.dataset.lead, agent: el.dataset.deskCrew }),
  deskBack: (el) => deskShow(el.dataset.deskBack ? { kind: "holder", sid: el.dataset.deskBack } : null),
  deskTab: (el) => { deskTab = el.dataset.deskTab; deskChatEnd = true; renderMain(); },
  deskMovesClear: (el) => deskMovesClear(el.dataset.deskMovesClear),
  deskMovesKeep: () => { deskMovesClearing = null; renderMain(); },
  ...DESK_LEAD_CLICKS,
  deskSend: (el) => deskSendBox(el.dataset.deskSend),
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
  deskLogRow: (el) => { const id = el.dataset.deskLogRow; if (!deskLogOpen.delete(id)) deskLogOpen.add(id); renderMain(); },
  deskSession: (el) => openSession(el.dataset.deskSession),
  deskLeadChat: () => { deskPick = null; deskTab = "lead"; readerView = null; deskShow(null); },
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
  // Opened from an item's page or a holder's, it remembers that page so ‹ goes back to it (from the left list: the lead).
  // A reply's "This turn in …": that session, here in the middle as its name on the left opens it (#306; it was a
  // read-only pop-up, #276), at that reply; ‹ goes back to the item.
  deskTurn: (el) => {
    deskTurnAt = Number(el.dataset.at) || 0;
    deskChatEnd = !deskTurnAt;
    deskShow({ kind: "chat", sid: el.dataset.deskTurn, from: ["item", "holder"].includes(deskOpen?.kind) ? { ...deskOpen } : null });
  },
  deskChatOf: (el) => { deskChatEnd = true; deskShow({ kind: "chat", sid: el.dataset.deskChatOf, from: ["item", "holder"].includes(deskOpen?.kind) ? { ...deskOpen } : null }); },
  deskChatBack: () => deskShow(deskOpen?.from || null),
  deskAddSession: (el) => {
    const project = el.dataset.deskAddSession;
    deskAddingSession = deskAddingSession === project ? null : project;
    renderMain();
    document.querySelector("[data-desk-add-session]")?.focus();
  },
  deskOlder: (el) => {
    const key = el.dataset.deskOlder;
    if (!deskOlderOpen.delete(key)) deskOlderOpen.add(key);
    renderMain();
  },
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
  deskDiffFile: (el) => deskShowFile(el.dataset.deskDiffFile, +el.dataset.n),
  deskTurnFile: (el) => deskShowTurnFile(el.dataset.deskTurnFile, +el.dataset.n),
  deskMateFile: (el) => deskShowMateFile(el.dataset.deskMateFile, +el.dataset.n),
  deskSessDiff: (el) => { extTabOf.set(el.dataset.deskSessDiff, DIFF_TAB); openSession(el.dataset.deskSessDiff); },
  deskCrewOpen: (el) => { deskCrewMenu = ""; deskOpenCrew(el.dataset.lead, el.dataset.deskCrewOpen); },
  deskCrewMore: (el) => { deskCrewMenu = deskCrewMenu === el.dataset.deskCrewMore ? "" : el.dataset.deskCrewMore; renderMain(); },
  deskDelete: (el) => { deskDeleteAsk = el.dataset.deskDelete; deskCloseSelected = new Set(deskDeleteSessions(deskOf(deskDeleteAsk)).filter((s) => !s.shared && !s.busy).map((s) => s.sid)); renderMain(); },
  deskDeleteCancel: () => { deskDeleteAsk = null; renderMain(); },
  deskDeleteConfirm: async () => {
    if (deskDeleting || !deskDeleteAsk) return;
    const id = deskDeleteAsk; deskDeleting = true; renderMain();
    try {
      const closeSessions = deskDeleteSessions(deskOf(id)).filter((s) => deskCloseSelected.has(s.sid) && !s.shared && !s.busy).map((s) => s.sid);
      const result = await invoke("desk_delete", { id, closeSessions });
      if (result?.kept?.length) toast(`Desk deleted. Couldn't close: ${result.kept.map((x) => `${deskHolderName(x.sid)} (${x.reason})`).join(", ")}`, 10000);
      deskDeleteAsk = null;
      if (deskSelection() === id) { deskSelect(""); deskOpen = null; deskPick = null; }
    } catch (e) { toast(`Couldn't delete the Desk: ${e}`); }
    deskDeleting = false; renderMain();
  },
  deskSwitch: () => {
    if (deskSwitchOpen) { deskSwitchClose(); return renderMain(); }
    deskSwitchOpen = true;
    deskSwitchAt = Math.max(0, deskSwitchList().findIndex((d) => d.project === deskSelection()));
    renderMain();
    document.querySelector(".dk-sw-find")?.focus();
  },
  deskGo: (el) => { deskSwitchClose(); DESK_CLICKS.deskProject({ dataset: { deskProject: el.dataset.deskGo } }); },
  deskHome: () => { deskSwitchClose(); if (!deskDeleting) deskDeleteAsk = null; deskSelect(""); deskOpen = null; deskPick = null; renderMain(); },
  deskProject: (el) => { deskSelect(el.dataset.deskProject); deskOpen = null; deskTab = "lead"; deskChatEnd = true; renderMain(); },
};
/** A click rule's attribute: deskItem → data-desk-item. */
const deskAttr = (k) => `data-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
const DESK_SEL = Object.keys(DESK_CLICKS).map((k) => `[${deskAttr(k)}]`).join(", ");
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
  // The switcher closes on a click outside it, and on + New Desk (which opens its own sheet).
  if (deskSwitchOpen && (!e.target.closest?.(".dk-sw-anchor") || e.target.closest?.("[data-desk-new-desk]"))) {
    deskSwitchClose();
    queueMicrotask(renderMain);
    if (!e.target.closest?.("[data-desk-new-desk]")) return;
  }
  // A crew member's ⋯ menu closes on a click outside it.
  if (deskCrewMenu && !e.target.closest?.(".dk-cmore")) { deskCrewMenu = ""; queueMicrotask(renderMain); }
  // In a session's name box (✎ on its row): the click stays in the box, it doesn't open the row.
  if (e.target.closest?.(".dk-board .rename-in")) { e.stopPropagation(); return; }
  if (deskTake(e.target.closest?.(DESK_SEL))) e.stopPropagation();
}
/** Keys on the Desk: ⌘N goes to the quick-add line, Enter sends a box (⇧Enter a new line), the quick-add line's keys
 *  (desk_lead.js), Esc closes a Give to… menu, then the picker (when changing), then the open pane. */
function deskKey(e) {
  if (view !== "desk" || e.isComposing || e.keyCode === 229) return false;
  const key = e.target?.dataset?.text || "";
  // The switcher's keys: ↑ ↓ move, ↵ opens the lit Desk, Esc closes it and gives the keyboard back to its button.
  if (deskSwitchOpen && !e.metaKey && !e.ctrlKey && !e.altKey) {
    const n = deskSwitchList().length;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { if (n) deskSwitchAt = (Math.min(deskSwitchAt, n - 1) + (e.key === "ArrowDown" ? 1 : n - 1)) % n; renderMain(); return true; }
    if (e.key === "Enter") { const d = deskSwitchList()[Math.min(deskSwitchAt, n - 1)]; if (d) DESK_CLICKS.deskGo({ dataset: { deskGo: d.project } }); return true; }
    if (e.key === "Escape") { deskSwitchClose(); renderMain(); document.querySelector("[data-desk-switch]")?.focus(); return true; }
  }
  // A row (an item, a holder, a crew member, a project's Desk): Enter or Space opens it as a click does, and it keeps
  // the keyboard where it is.
  const row = e.target?.matches?.('.dk-board [role="button"]') && e.target.matches(DESK_SEL) ? e.target : null;
  if (row && (e.key === "Enter" || e.key === " ") && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    const at = Object.keys(DESK_CLICKS).find((k) => k in row.dataset), sel = `[${deskAttr(at)}="${CSS.escape(row.dataset[at])}"]`;
    deskTake(row);
    document.querySelector(`.dk-board ${sel}`)?.focus();
    return true;
  }
  if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key?.toLowerCase() === "n" && deskSelection() && !sheet && !(crewPageSid && document.querySelector(".crew-page"))) return deskQaFocus();
  if (deskQaKey(e, key)) return true;
  if (e.key === "Enter" && !e.shiftKey && deskBoxes.has(key)) { deskSendBox(key); return true; }
  if (e.key === "Enter" && !e.shiftKey && key.startsWith("back:")) { deskAct(key.slice(5), "back_go"); return true; }
  if (e.key === "Escape" && key.startsWith("back:")) { deskAct(key.slice(5), "back_cancel"); return true; }
  // Esc: only when nothing over the Desk wants it (the crew page, a sheet, search and the live list, the projects
  // list, a menu, a rename), as the Inbox's keys do.
  const over = sheet || lightbox || liveOpen || deleteAsk || renaming || pjRenaming || pjOpen || pjNewOpen || colorFor || newProjFor || btwFor || menuFor || redirectFor || (typeof moreFor !== "undefined" && moreFor) || (crewPageSid && document.querySelector(".crew-page"));
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
    if (deskOpen && !(e.target.matches?.("textarea, input") && e.target.value)) { deskShow(deskOpen.kind === "crew" ? { kind: "holder", sid: deskOpen.lead } : deskOpen.kind === "chat" ? deskOpen.from || null : null); return true; }
  }
  return false;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (deskKey(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
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
  document.addEventListener("scroll", (e) => { const c = e.target; if (c?.matches?.(".dk-chat")) deskChatEnd = c.scrollHeight - c.scrollTop - c.clientHeight < 24; }, true);
}
