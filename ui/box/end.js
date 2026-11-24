// Cue UI box — a chat's end: whether you're reading at it, and keeping it there. One test for every chat (the pane,
// the Desk's, the crew page's, the mini window's), where each wrote its own "within 24px of the
// bottom" (docs/refactor-plan.md, Phase 3).

/** Whether `el` is scrolled to its end, give or take `px` (24 by default): you're reading the newest. False for none. */
const boxAtEnd = (el, px = 24) => !!el && el.scrollHeight - el.scrollTop - el.clientHeight < px;
/** `el` scrolled to its end by code, not you (`_pinTop`, `_pinAt` say where and when, so the scroll events of it can be
 *  told from yours). Unless `settle` is false, again on the next frame and after 250ms, while it's still there (what's
 *  below loading late, a picture or a table, would leave the end just out of view). `el`. */
function boxStickToEnd(el, settle = true) {
  if (!el) return el;
  const pin = () => { el.scrollTop = el.scrollHeight; el._pinTop = el.scrollTop; el._pinAt = Date.now(); };
  pin();
  if (settle) {
    const again = () => { if (el.isConnected && (el.scrollTop === el._pinTop || boxAtEnd(el))) pin(); };
    requestAnimationFrame(again);
    setTimeout(again, 250);
  }
  return el;
}

// ---------- what you're reading: Cue's notifications skip it, and take back what's about a chat you open (#799) ----------
let chatsOpenSent = null;
/** The chats on screen in Cue's window: [session, scrolled to its newest], one per session. The Active pane's and
 *  the crew page's ("m|<sid>"; a history or rewind view isn't the newest), and the Desk's: its lead chat (keyed by
 *  project: the Desk's lead) and an item's (keyed by the session holding it). */
function chatsOnScreen() {
  const at = new Map();
  for (const el of document.querySelectorAll("[data-chat-for], .dk-chat[data-desk-chat]")) {
    if (!el.isConnected || !el.getClientRects().length) continue;
    let sid = null;
    if (el.dataset.chatFor != null) sid = /^m\|(.+)$/.exec(el.dataset.chatFor)?.[1] || null;
    else {
      const key = el.dataset.deskChat, d = (state.desk?.projects || []).find((p) => p.project === key);
      sid = d ? d.lead?.sid || null : key;
    }
    if (sid) at.set(sid, at.get(sid) || boxAtEnd(el));
  }
  return [...at];
}
/** Tells Cue what's on screen when it changes: the open chats and whether Cue's window is in front of you. */
function chatsOpenTell() {
  const front = document.hasFocus() && document.visibilityState === "visible";
  const chats = chatsOnScreen(), key = JSON.stringify([chats, front]);
  if (key === chatsOpenSent) return;
  chatsOpenSent = key;
  invoke("chats_open", { chats, front }).catch(() => { chatsOpenSent = null; });
}
/** Cue's window only (the mini window says its own, mini_reading): on focus, scrolling and every second (a chat
 *  opened or redrawn). */
function chatsOpenStart() {
  let soon = 0;
  const tell = () => { clearTimeout(soon); soon = setTimeout(chatsOpenTell, 120); };
  addEventListener("focus", tell);
  addEventListener("blur", tell);
  document.addEventListener("visibilitychange", tell);
  document.addEventListener("scroll", tell, true);
  setInterval(chatsOpenTell, 1000);
  tell();
}
