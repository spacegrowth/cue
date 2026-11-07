// Cue UI app — the window's listeners, put on once at boot: hover tips, scrolling, clicks (ui/app/clicks.js),
// typing (ui/app/inputs.js) and keys (ui/app/keys.js).

/** Hover text that shows at once, inside Cue's window: the browser's own (from `title`) waits about a
 *  second and doesn't show unless Cue is the app in front. Anything with a title gets it; with
 *  `data-cut` (the hover is just the text you see), only when that text is cut off. */
function bindTips() {
  const tip = document.createElement("div");
  tip.className = "tip";
  document.body.append(tip);
  let on = null;
  const hide = () => { tip.classList.remove("show"); on = null; };
  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest?.("[title], [data-tip]");
    if (!el) return hide();
    // Take the title, so the browser's own doesn't show as well.
    if (el.hasAttribute("title")) { el.dataset.tip = el.getAttribute("title"); el.removeAttribute("title"); }
    if (el === on || !el.dataset.tip) return;
    // Cut off: the text, or a part of it (a path's start, cut in the middle), doesn't fit.
    const cut = (x) => x.scrollWidth > x.clientWidth + 1 || x.scrollHeight > x.clientHeight + 1;
    if (el.hasAttribute("data-cut") && !cut(el) && ![...el.children].some(cut)) return hide();
    on = el;
    tip.textContent = el.dataset.tip;
    const r = el.getBoundingClientRect();
    tip.style.maxWidth = "320px";
    tip.classList.add("show");
    const w = tip.offsetWidth, h = tip.offsetHeight;
    const x = Math.min(Math.max(8, r.left + r.width / 2 - w / 2), innerWidth - w - 8);
    const below = r.bottom + 8 + h < innerHeight;
    tip.style.left = `${x}px`;
    tip.style.top = `${below ? r.bottom + 6 : r.top - h - 6}px`;
  });
  document.addEventListener("mousedown", hide);
  document.addEventListener("scroll", hide, true);
  addEventListener("blur", hide);
}

/** When you last moved the chat yourself: the wheel, a touch, a scroll key, or a press (its scrollbar). A
 *  scroll event without one is the chat moving on its own: it grew or shrank (a terminal opened above the
 *  box, a redraw), and the browser kept it in range. */
let chatMovedAt = 0;
const CHAT_MOVED_MS = 1000;
const SCROLL_KEYS = new Set(["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End", " "]);
function noteChatMove(e) {
  const moved = e.type === "keydown" ? SCROLL_KEYS.has(e.key) && !e.target?.closest?.("textarea, input, [contenteditable]") : e.target?.closest?.("[data-chat]");
  if (moved) chatMovedAt = Date.now();
}
/** Near the top of the chat: the next page of earlier messages comes in above. Only the chat itself (a
 *  background agent's view fills the same element: its steps' top isn't the chat's), and only when it's
 *  you scrolling: not the chat moving on its own (see chatMovedAt), nor the scroll events of a chat code
 *  just put at its end (chat/helpers.js pinChatEnd). Each page makes every draw after it longer. */
function onChatScroll(e) {
  if (!e.target?.matches?.("[data-chat]") || e.target.scrollTop > 60 || /^[hr]\|/.test(e.target.dataset.chatFor || "") || Date.now() - chatPinnedAt < 400) return;
  if (Date.now() - chatMovedAt > CHAT_MOVED_MS) return;
  const cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
  if (sid) loadOlder(sid);
}

function bindMain() {
  bindTips();
  document.body.append(picker);
  document.addEventListener("scroll", placeForward, true);   // the send-to menu follows its button
  document.addEventListener("scroll", onChatScroll, true);
  for (const type of ["wheel", "touchmove", "pointerdown", "keydown"]) document.addEventListener(type, noteChatMove, { capture: true, passive: true });
  // You scrolling the chat (the wheel or trackpad, or a press on its scroll bar, which lands on the chat itself):
  // a reply being read out stops scrolling it along (chat/listen.js listenScrolled).
  document.addEventListener("wheel", (e) => { const c = e.target.closest?.("[data-chat]"); if (c) listenScrolled(c); }, { passive: true, capture: true });
  document.addEventListener("pointerdown", (e) => { if (e.target.matches?.("[data-chat]")) listenScrolled(e.target); }, true);
  const app = document.getElementById("app");
  document.addEventListener("mousedown", (e) => { downInLive = !!e.target.closest?.(".lv-wrap"); }, true);
  // Into a session's box: start reading its "/" commands now (that takes a few seconds the first
  // time in a folder), so they're usually in by the time you type "/".
  app.addEventListener("focusin", (e) => { const sid = sidOfKey(e.target.dataset?.text || ""); if (sid) cmdList(sid); });
  delegate(app, "click", CLICKS);
  bindInputs(app);
  // Coming back to Cue: the cursor goes to the text box unless you were somewhere else in it.
  window.addEventListener("focus", () => { if (!sheet && !lightbox && document.activeElement === document.body) focusComposer(); });
  document.addEventListener("keydown", keyDeleteAsk, true);
  document.addEventListener("keydown", onKey);
}
