// Cue UI chat — the chat's DOM kept between draws, focus, and the text box's height.
// ---------- the chat's DOM lives on between draws ----------
// renderMain rebuilds the whole window from a string; the chat (the Active pane's `.ap-chat`) is the one
// part that would show it: steps land every half second while a session works, and a rebuilt chat
// flickers, starts every fade-in over, drops your text selection and loses its scroll. So the chat's
// element is kept: while the same chat stays open, activePane() leaves a slot where it goes, settleChat
// puts the live element back into that slot, and patchChat brings it up to date by key: a row or step
// whose key is new goes in (in order), one whose HTML changed is updated in place, the rest are left
// alone. The whole chat is drawn afresh only when another chat opens (or the steps feed shrank: the
// transcript was replaced).
// Setting the window's HTML would take the chat out of the document, and an element put back after
// that starts its animations over and loses the selection in it. So liftChat moves it (and the sweep)
// aside first, with moveBefore where the engine has it: a move that keeps the element's state. Without
// it the element is kept but a fade-in mid-way would replay, so there the `reveal` of earlier lines is
// taken off at the next draw. A selection in the chat collapses on either move, so it's noted and put back.
const KEEPS_STATE = typeof Element.prototype.moveBefore === "function";
const moveTo = (parent, node, before = null) => (KEEPS_STATE ? parent.moveBefore(node, before) : parent.insertBefore(node, before));
let chatPending = null;   // what the slot stands for: the chat's HTML, until settleChat patches it in
/** Before the window's HTML is set: the chat, the sweep and a flick move aside, out of what's replaced. */
function liftChat() {
  const kept = ["[data-chat]", "[data-sweep]", "[data-flick]"].map((q) => document.querySelector(q)).filter(Boolean);
  const chat = kept.find((el) => el.matches("[data-chat]"));
  const top = chat?.scrollTop;
  const sel = getSelection();
  const picked = chat && sel.rangeCount && !sel.isCollapsed && chat.contains(sel.getRangeAt(0).commonAncestorContainer) ? [sel.anchorNode, sel.anchorOffset, sel.focusNode, sel.focusOffset] : null;
  for (const el of kept) moveTo(document.body, el);
  return { kept, top, picked };
}
/** The `.ap-chat` for a pane: the real thing on a full draw, else a slot for the element that's there. */
function chatBox(forKey, html) {
  const old = document.querySelector("[data-chat]");
  const turns = stepFeeds.get(forKey.slice(2))?.turns.length ?? 0;
  if (old && old.dataset.chatFor === forKey && turns >= +(old.dataset.turns || 0)) {
    chatPending = { forKey, html, turns };
    return `<div class="ap-chat" data-chat data-chat-slot></div>`;
  }
  chatPending = null;
  const f = stepFeeds.get(forKey.slice(2));
  if (f) f.fresh = false;   // whatever is in this draw was there when the chat was opened
  return `<div class="ap-chat" data-chat data-chat-for="${esc(forKey)}" data-turns="${turns}">${html}</div>`;
}
/** After renderMain set the window's HTML: the live chat (and the working sweep over it) go back where
 *  the slot is, then the chat is patched. `old`: the chat element from before the draw. Returns true when
 *  the chat was kept, false when this draw made a new one. */
function settleChat({ kept, top, picked }) {
  const slot = document.querySelector("[data-chat-slot]");
  const old = kept.find((el) => el.matches("[data-chat]"));
  if (!slot || !old || !chatPending) { chatPending = null; kept.forEach((el) => el.remove()); return false; }
  const { forKey, html, turns } = chatPending;
  chatPending = null;
  const pane = slot.closest(".active-pane");
  moveTo(slot.parentNode, old, slot);
  slot.remove();
  if (top != null && old.scrollTop !== top) old.scrollTop = top;
  // The sweep carries on where it was (its class and speed follow the new draw); so does a flick mid-pass.
  const sw = pane?.querySelector("[data-sweep]"), oldSw = kept.find((el) => el.matches("[data-sweep]"));
  if (sw && oldSw) { oldSw.className = sw.className; oldSw.style.cssText = sw.style.cssText; moveTo(sw.parentNode, oldSw, sw); sw.remove(); }
  else oldSw?.remove();
  const oldFlick = kept.find((el) => el.matches("[data-flick]"));
  if (sw && oldFlick) moveTo(pane, oldFlick, (pane.querySelector("[data-sweep]") || pane.firstChild).nextSibling);
  else oldFlick?.remove();
  if (!KEEPS_STATE) old.querySelectorAll(".reveal").forEach((el) => el.classList.remove("reveal"));
  const f = stepFeeds.get(forKey.slice(2));
  const fresh = !!f?.fresh;
  if (f) f.fresh = false;
  const tpl = document.createElement("template");
  tpl.innerHTML = html;
  const landed = patchKids(old, tpl.content, !fresh);
  old.dataset.turns = turns;
  if (landed && sw) flick(pane);
  // What you had selected, unless the patch replaced it.
  if (picked && picked[0].isConnected && picked[2].isConnected) try { getSelection().setBaseAndExtent(...picked); } catch {}
  return true;
}
/** A step just landed: one quick flick along the top edge, over the sweep. It outlives the draws that
 *  come while it plays (settleChat carries it over). */
function flick(pane) {
  pane.querySelector("[data-flick]")?.remove();
  const el = document.createElement("i");
  el.className = "sweep flick";
  el.dataset.flick = "1";
  el.addEventListener("animationend", () => el.remove());
  (pane.querySelector("[data-sweep]") || pane).after(el);
}
/** The chat's rows (or a turn's items) brought up to date, by key. A key not in the DOM goes in where it
 *  comes in order; one whose HTML differs from the last draw's is updated in place (morph); one that's
 *  gone is removed; the rest are left exactly as they are. A `.stx` block and a run of steps inside it
 *  are patched the same way, one level down, so a landing step only adds its own line. `reveal`: a new
 *  step or paragraph fades in. Returns how many step lines went in. */
const PATCH_INSIDE = ".stx, .stx-steps";
function patchKids(live, fresh, reveal) {
  const want = new Set(), old = new Map();
  for (const c of fresh.children) want.add(c.dataset.key);
  for (const c of [...live.children]) { const k = c.dataset.key; if (k == null || old.has(k) || !want.has(k)) c.remove(); else old.set(k, c); }
  let landed = 0, cursor = live.firstElementChild;
  const seen = new Set();
  for (const f of [...fresh.children]) {
    const k = f.dataset.key, h = f.outerHTML;
    const node = k != null && !seen.has(k) ? old.get(k) : null;
    if (!node) {
      live.insertBefore(f, cursor);
      f._html = h;
      const lines = [...f.querySelectorAll(".stx-line"), ...(f.matches(".stx-line") ? [f] : [])];
      landed += lines.length;
      if (reveal) for (const el of [...lines, ...f.querySelectorAll(".stx-say, .cv-result"), ...(f.matches(".stx-say, .cv-result") ? [f] : [])]) el.classList.add("reveal");
      continue;
    }
    seen.add(k);
    if (node === cursor) cursor = cursor.nextElementSibling;
    else live.insertBefore(node, cursor);   // out of order (rare: rows sort by time)
    if (node._html === h) continue;
    node._html = h;
    if (node.matches(PATCH_INSIDE) && f.matches(PATCH_INSIDE)) { morphAttrs(node, f); landed += patchKids(node, f, reveal); }
    else morph(node, f);
  }
  return landed;
}
/** `a` made to match `b` in place where their shapes agree (same tags, same number of children: the text
 *  and attributes are set), replaced where they don't. A line mid-fade-in keeps its `reveal`. */
function morph(a, b) {
  if (a.nodeType !== b.nodeType || a.nodeName !== b.nodeName) { a.replaceWith(b); return; }
  if (a.nodeType !== 1) { if (a.nodeValue !== b.nodeValue) a.nodeValue = b.nodeValue; return; }
  morphAttrs(a, b);
  if (a.childNodes.length !== b.childNodes.length) { a.replaceChildren(...b.childNodes); return; }
  const bc = [...b.childNodes];
  [...a.childNodes].forEach((c, i) => morph(c, bc[i]));
}
function morphAttrs(a, b) {
  if (a.classList.contains("reveal")) b.classList.add("reveal");
  for (const { name, value } of [...b.attributes]) if (a.getAttribute(name) !== value) a.setAttribute(name, value);
  for (const { name } of [...a.attributes]) if (!b.hasAttribute(name)) a.removeAttribute(name);
}
let escArmed = 0;           // when Esc was last pressed on a working session (twice within 1.5s stops it)
let focusedTarget = null;   // the Active target whose text box last got the cursor
let overWas = false;        // something covered the chat at the last draw (a sheet, the live list, an image…)
/** Put the cursor at the end of the Active pane's text box (opening something, Esc Esc). */
function focusComposer() {
  const el = document.querySelector(".active-pane textarea[data-text]");
  if (!el) return false;
  el.focus();
  el.setSelectionRange(el.value.length, el.value.length);
  return true;
}

/** Focus that has fallen to nothing goes home, to the open session's message box: what had it went away
 *  (a terminal tab you closed, a terminal put away) or let it go. Never over a choice of yours: not
 *  while something sits over the chat (a sheet, a menu, a dialog), not when Cue isn't the app you're in,
 *  and not after a click, where you picked where to be (and may be selecting text, which focusing the
 *  box would clear). A frame later, so whatever takes focus next has done so first. */
let pointedAt = 0;
document.addEventListener("pointerdown", () => { pointedAt = Date.now(); }, true);
// The tray's fold and an agent's "Put reply in the box": theirs alone, so they're handled here.
document.addEventListener("click", (e) => {
  const hr = e.target.closest("[data-helper-reply]");
  if (hr) { const [sid, id] = hr.dataset.helperReply.split("|"); helperReplyToBox(sid, id); }
});
function homeFocus() {
  requestAnimationFrame(() => {
    const a = document.activeElement;
    if ((a && a !== document.body) || !document.hasFocus() || Date.now() - pointedAt < 500) return;
    if (sheet || lightbox || liveOpen || menuFor || redirectFor || renaming || btwFor || deleteAsk || moving) return;
    focusComposer();
  });
}
// Focus let go to nothing (Tab past the last control, an element that blurs as it closes): home.
document.addEventListener("focusout", (e) => { if (!e.relatedTarget) homeFocus(); });

/** The chat jumps to its end when you open something else; a new row follows only if you were at the end.
 *  New target: jump to the end. Same chat, you were at the end (within 24 px, measured before the patch): stay
 *  there (an image added to the box, or a step landing, mustn't push the last lines out of view). Scrolled up to
 *  read: a row arriving leaves your place alone. Both windows place their chat with it (app/render.js, mini.js). */
function scrollChat(chat, was, lifted, kept) {
  const key = chat ? `${active?.id}|${active?.sid}` : null;
  // Back from a background agent's or script's view (chat/helpers.js closeHelperView): the end, whatever the view's scroll was.
  if (chat && chatToEnd) { pinChatEnd(chat); holdChat = null; }
  // A reply just opened in the reader (chat/reader.js readerPane): its start, wherever the chat was.
  else if (chat && !kept && chat.dataset.chatFor?.startsWith("r|")) { chat.scrollTop = 0; holdChat = null; }
  else if (chat && holdChat != null) { chat.scrollTop = chat.scrollHeight - holdChat; holdChat = null; }
  else if (chat && (key !== activeKey || was.chatPinned)) chat.scrollTop = chat.scrollHeight;
  // Drawn afresh for the same chat (its steps feed was replaced): where you were.
  else if (chat && !kept && was.oldChat?.dataset.chatFor === chat.dataset.chatFor && lifted.top != null) chat.scrollTop = lifted.top;
  activeKey = key;
}
/** Text boxes grow with their content, up to ~8 lines, then scroll. */
const BOX_MAX = 180;   // the text box's tallest (px); past it, it scrolls (styles/board.css says the same)
function grow(el) {
  // Already as tall as it gets and still overflowing: nothing to measure. (Measuring collapses it for a
  // moment, which moved what you were reading and jolted the chat above on every key in a long message.)
  if (el.clientHeight >= BOX_MAX - 1 && el.scrollHeight > el.clientHeight) {
    if (el.selectionEnd >= el.value.length) el.scrollTop = el.scrollHeight;   // typing at the end: the end in view
    return;
  }
  // Growing the box shrinks the chat above it. If you were reading the end of the chat, keep the
  // end in view (the conversation moves up with the box) instead of letting the box cover it.
  const chat = el.closest(".active-pane")?.querySelector("[data-chat]");
  const pinned = chat && chat.scrollHeight - chat.scrollTop - chat.clientHeight < 24;
  const top = el.scrollTop, atEnd = el.selectionEnd >= el.value.length;
  el.style.height = "auto";
  el.style.height = `${Math.min(el.scrollHeight, BOX_MAX)}px`;
  // The moment collapsed resets where it was scrolled (WebKit doesn't always put it back): with the cursor
  // at the end, the end stays in view; else where you were.
  el.scrollTop = atEnd ? el.scrollHeight : top;
  if (pinned) chat.scrollTop = chat.scrollHeight;
}
const picker = Object.assign(document.createElement("input"), { type: "file", accept: "image/*", multiple: true, hidden: true });
/** Finder's picker is open (the window lost focus to it, not to another app: the mini window stays open, mini.js). */
let picking = false;
/** 📎 on box `key`: Finder's picker, for that box. */
function pickImages(key) {
  picker.dataset.key = key;
  picker.value = "";
  picking = true;
  picker.click();
}
picker.addEventListener("change", () => { picking = false; attach(picker.dataset.key, [...picker.files]); });
// Closed Finder's picker without one: back to the box you were adding to.
picker.addEventListener("cancel", () => { picking = false; document.querySelector(`[data-text="${CSS.escape(picker.dataset.key)}"]`)?.focus(); });
// The window has its focus back: the panel closed, whether or not it said so (a dismissed panel may send no cancel).
addEventListener("focus", () => { picking = false; });
async function attach(key, files) {
  for (const f of files.slice(0, 6)) {
    if (f.size > 15 * 1024 * 1024) { toast(`${f.name} is over 15 MB`); continue; }
    const data = await new Promise((ok) => { const r = new FileReader(); r.onload = () => ok(r.result); r.readAsDataURL(f); });
    draft(key).images.push({ name: f.name || "pasted image", mime: f.type, data });
  }
  renderMain();
  document.querySelector(`[data-text="${CSS.escape(key)}"]`)?.focus();
}
