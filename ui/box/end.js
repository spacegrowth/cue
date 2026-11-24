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
  const pin = () => { el.scrollTop = el.scrollHeight; el._pinTop = el.scrollTop; el._pinAt = Date.now(); el._wasEnd = true; };
  pin();
  if (settle) {
    const again = () => { if (el.isConnected && (el.scrollTop === el._pinTop || boxAtEnd(el))) pin(); };
    requestAnimationFrame(again);
    setTimeout(again, 250);
  }
  return el;
}

// ---------- a chat at its end follows what comes in after it's drawn (#795) ----------
// Each view places its chat as it draws it (app/render.js, desk/view.js deskSettle, mini.js, team_page.js). What lays
// out after that (a reply growing in place, a picture loading, a question card taking the box's place and shrinking the
// chat) moved nothing, so the newest lines slid under the edge. Here, for every chat in the window: whether it was at
// its end is noted as it scrolls and after each change of size, and when its size or its content's changes, measured
// once laid out, a chat that was at its end is put back there. Scrolled up to read, it's left where it is.
const BOX_CHATS = "[data-chat], .dk-chat";
/** The chats followed now. */
const boxFollowed = new Set();
const boxFollowSizes = typeof ResizeObserver === "function" ? new ResizeObserver((entries) => {
  const chats = new Set(entries.map((e) => (e.target._follows ? e.target : e.target.parentElement)).filter((c) => c?._follows && c.isConnected));
  for (const c of chats) {
    if (c._wasEnd && !boxAtEnd(c, 2)) { c.scrollTop = c.scrollHeight; c._pinTop = c.scrollTop; c._pinAt = Date.now(); }
    c._wasEnd = boxAtEnd(c);
  }
}) : null;
/** Chat `c` followed: its own size and each of its rows'. */
function boxFollow(c) {
  c._follows = true;
  c._wasEnd = boxAtEnd(c);
  boxFollowed.add(c);
  boxFollowSizes.observe(c, { box: "border-box" });
  for (const row of c.children) boxFollowSizes.observe(row, { box: "border-box" });
}
function boxUnfollow(c) {
  c._follows = false;
  boxFollowed.delete(c);
  boxFollowSizes.unobserve(c);
  for (const row of c.children) boxFollowSizes.unobserve(row);
}
if (boxFollowSizes) {
  // Chats drawn (each view's draw has placed them by now: this runs after it), rows coming and going in them, chats gone.
  new MutationObserver((records) => {
    for (const r of records) {
      if (!r.target._follows) continue;
      for (const n of r.addedNodes) if (n.nodeType === 1) boxFollowSizes.observe(n, { box: "border-box" });
      for (const n of r.removedNodes) if (n.nodeType === 1) boxFollowSizes.unobserve(n);
    }
    for (const c of boxFollowed) if (!c.isConnected) boxUnfollow(c);
    for (const c of document.querySelectorAll(BOX_CHATS)) if (!c._follows) boxFollow(c);
  }).observe(document.documentElement, { childList: true, subtree: true });
  // Scrolled (by you, or by a view placing it): where it is now.
  document.addEventListener("scroll", (e) => { if (e.target?._follows) e.target._wasEnd = boxEndAfterScroll(e.target); }, true);
  // A wheel, touch, key or click of yours in a chat (its scroll bar too): what scrolls it next is you.
  for (const type of ["wheel", "touchmove", "pointerdown", "keydown"]) document.addEventListener(type, (e) => { const c = e.target?.closest?.(BOX_CHATS); if (c) c._youAt = Date.now(); }, { capture: true, passive: true });
}
/** Whether chat `c`, just scrolled, counts as at its end, so what comes in keeps it there (#855). Where it is now, except
 *  for the scroll of Cue putting it at its end: its event comes a frame later, and by then the box under the chat may
 *  have grown a line or a reply come in, so it was measured short of the end and the chat stopped following, its last
 *  lines left under the box. Still where Cue put it, with no wheel, touch, key or click of yours since: at its end. */
function boxEndAfterScroll(c) {
  if (c._wasEnd && c.scrollTop === c._pinTop && Date.now() - (c._youAt || 0) > 1000) return true;
  return boxAtEnd(c);
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
