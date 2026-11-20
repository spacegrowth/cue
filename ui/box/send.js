// Cue UI box — sending what's in a box, and putting words back in one. One path for every box: the guards ("It's
// restarting", "It's asking you something first"), pictures cut down to what goes (name, mime, data), the box emptied,
// and, when it doesn't go, the words back in the box ahead of what's typed there since. Nothing calls it yet: chat/send.js
// deliver, chat/compose.js sendTo, pullHeld and sendAtEdit, and desk_act.js's senders move onto it in later steps
// (docs/refactor-plan.md, Phase 3).

/** Why nothing can go to session `sid` now ("" when it can): it's restarting, or it's asking you something first. */
function boxHeld(sid) {
  if (!sid) return "";
  if (state.restarting?.[sid]) return "It's restarting. Send once it's back.";
  if (sessionOf(sid)?.state === "deciding") return "It's asking you something first. Answer that, then send.";
  return "";
}
/** A box's pictures as they're sent: name, mime and data only (not what the box keeps beside them). */
const boxImages = (images) => (images || []).map(({ name, mime, data }) => ({ name, mime, data }));
/** `text` put before what's in a box (`had`), a blank line between; just `text` when the box is empty. */
const boxMerge = (text, had) => (String(had ?? "").trim() ? `${text}\n\n${had}` : text);
/** `text` and `images` back in box `key`, ahead of what's typed there (that's kept, not written over). `o`:
 *  imagesLast puts the pictures after the box's own (pullHeld's order); save (default true) saves the drafts; draw
 *  (default true) redraws; focus: true puts the cursor at the end of the box, a function is called with `key` instead
 *  (focusComposer), false leaves it. The box's draft. */
function boxPutBack(key, text, images = [], o = {}) {
  const d = draft(key), back = (images || []).map((im) => ({ ...im }));
  d.text = boxMerge(text, d.text);
  d.images = o.imagesLast ? [...(d.images || []), ...back] : [...back, ...(d.images || [])];
  if (o.save !== false) saveDrafts();
  if (o.draw !== false) renderMain();
  if (typeof o.focus === "function") o.focus(key);
  else if (o.focus) {
    const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
    if (el) { el.focus(); el.selectionStart = el.selectionEnd = el.value.length; }
  }
  return d;
}
/** The pictures of a message Cue kept (their paths), read back as a box's pictures; one that can't be read is left out. */
async function boxHeldImages(paths) {
  const out = [];
  for (const path of paths || []) {
    const data = await invoke("image_data", { path }).catch(() => null);
    if (data) out.push({ name: path.split("/").pop(), mime: data.slice(5, data.indexOf(";")), data });
  }
  return out;
}
/** A send to session `sid` as a box's Send makes it (for boxSend's `call`): `about` goes before the words ("About
 *  #12 (…): "), `now` stops its turn first (Ctrl+Enter); the Insights count it. "(a picture)" stands in for no words
 *  when `about` is given (the Desk's holder retries). */
const boxToSession = (sid, o = {}) => (typed, images) => {
  const said = o.about ? `${o.about}${typed || "(a picture)"}` : typed;
  insSent(sid, said, !!o.now);
  return invoke("send_to_session", { sessionId: sid, text: said, images, now: !!o.now });
};
/**
 * Send what's in box `key` to `target` ({ sid } for a session, so the guards look at it; {} for the Desk's lead or an
 * item, which have none). Everything up to the call is synchronous: a quick phrase (compose.js sendPhrase) puts back
 * what you typed as soon as this returns its promise, so nothing may touch the box after that but the outcome.
 * `o`:
 *  - call(typed, images, entry): what sends it (a promise). Required; boxToSession(sid) for a session.
 *  - text, images: send these instead of the box's own (a retry); the box is then never emptied or written to.
 *  - guard: false skips the guards (a reply to a card). guardSid: the session they look at, when not target.sid.
 *  - before(text, draft): runs first, after dictation ends. Returns false when it dealt with the words itself (a
 *    "/btw", a "!" line), a string to send that instead (the typed form, " /…" keeping its space), else nothing.
 *  - empty: true sends even with no words and no pictures.
 *  - pending(text, images): what's shown while it goes (the outbox entry, the Desk's Sending line); passed on as `entry`.
 *  - clear: "now" (default) empties the box before it goes; "sent" only once it went, and not for a quick phrase
 *    (deskSendBack); "never".
 *  - save: true saves the drafts once it went (the Desk: a restart in between leaves your words in the box).
 *  - draw(entry): what draws before it goes (renderMain by default; drawAtChatEnd for a chat; deskSendBack draws only
 *    when it has an entry).
 *  - sent(result, entry): once it went. failed(error, entry): when it didn't; true when that's dealt with (red on the
 *    item's Thread with Retry), else the words go back in the box (not for a quick phrase: a toast says so) and a
 *    toast says why.
 *  - putBack: false leaves the box as it is when it didn't go.
 * Returns { ok, result } | { error } | { held } (a guard said no) | { empty } | { stopped } (`before` dealt with it).
 */
function boxSend(target, key, o) {
  const held = o.guard === false ? "" : boxHeld(o.guardSid || target?.sid);
  if (held) { toast(held); return Promise.resolve({ held }); }
  const own = o.text == null;
  // A quick phrase on its way (compose.js sendPhrase): the box holds what you typed, so it's left alone, whatever happens.
  const phrase = typeof phraseSending !== "undefined" && phraseSending;
  if (own && !phrase) endDictation(key);
  const d = own ? draft(key) : null;
  const text = own ? d.text.trim() : String(o.text).trim();
  const kept = own ? d.images || [] : o.images || [];
  let typed = text;
  if (o.before) {
    const r = o.before(text, d);
    if (r === false) return Promise.resolve({ stopped: true });
    if (typeof r === "string") typed = r;
  }
  const images = boxImages(kept);
  if (!text && !images.length && !o.empty) return Promise.resolve({ empty: true });
  const entry = o.pending ? o.pending(text, images) : null;
  const clear = own ? o.clear || "now" : "never";
  if (clear === "now") delete drafts[key];
  if (o.draw) o.draw(entry); else renderMain();
  // Called at once, as the senders do today (nothing waits a tick before it goes).
  let going;
  try { going = Promise.resolve(o.call(typed, images, entry)); } catch (e) { going = Promise.reject(e); }
  return going.then((result) => {
    if (clear === "sent" && !phrase) { delete drafts[key]; saveDrafts(); }
    else if (o.save) saveDrafts();
    o.sent?.(result, entry);
    return { ok: true, result };
  }, (e) => {
    const error = String(e);
    if (o.failed?.(error, entry)) return { error };
    if (phrase) toast(`Couldn't send “${text}”: ${e}`);
    else {
      if (own && o.putBack !== false) boxPutBack(key, text, kept, { save: false, draw: false });
      toast(`Couldn't send: ${e}`);
    }
    renderMain();
    return { error };
  });
}
