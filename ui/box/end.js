// Cue UI box — a chat's end: whether you're reading at it, and keeping it there. One test for every chat (the pane,
// the Desk's, the crew page's, the mini window's, the Inbox's thread), where each wrote its own "within 24px of the
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
