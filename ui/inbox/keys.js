// Cue UI Inbox (ui/inbox/list.js says what it is): moving about, pin/later/snooze, keys and clicks.
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
  document.addEventListener("click", inboxClick, true);
  // Writing in the reply box, a click on a button in the Inbox (Allow, a choice, a phrase) leaves the cursor in the box.
  document.addEventListener("mousedown", (e) => {
    const a = document.activeElement;
    if (e.button === 0 && a?.matches?.(".inbox-page textarea[data-text]") && e.target.closest?.(".inbox-page button, .inbox-page [role=button]")) e.preventDefault();
  }, true);
}
