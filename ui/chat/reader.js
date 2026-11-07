// Cue UI chat — the reader: one reply set for reading.
// ---------- Reader: one reply, set for reading ----------
// Read on a long reply: it fills the chat area in one column, larger type, ‹ › to its other replies, three
// text sizes (remembered). Read-only on purpose: no box, so permissions, questions, the queue and the rest
// stay in the chat alone. Reply goes to the chat's box; ‹ Chat (or Esc) back to the reply in the chat.
let readerView = null;   // { sid, key }: the reply open in the reader; key as chatHtml's ("<sid>:<at_ms>")
const READER_SIZES = [15, 17, 20];
let readerSize = 1;      // index into READER_SIZES
try { const n = localStorage.getItem("cue.readerSize"); if (n != null && READER_SIZES[+n]) readerSize = +n; } catch {}
/** The session's replies the reader steps through, oldest first, keyed as in the chat. */
function readerReplies(it, s) {
  const id = s?.session_id || it?.id;
  return conversation(it, s).filter((e) => e.role === "agent" && e.text.trim()).map((e) => ({ key: `${id}:${e.at_ms}`, text: e.text, at_ms: e.at_ms }));
}
/** The reply open in this session's reader, with where it sits among the others; null when none (or it's gone). */
function readerViewed(sid, it, s) {
  if (!readerView) return null;
  if (readerView.sid !== sid) { readerView = null; return null; }
  const all = readerReplies(it, s), at = all.findIndex((r) => r.key === readerView.key);
  return at < 0 ? null : { all, at };
}
/** The reader is on screen (not just remembered behind another view). */
const readerShown = () => !!readerView && !!document.querySelector(".rd-head");
function openReader(sid, key) {
  readerView = { sid, key };
  renderMain();
}
/** ‹ › (and ← →): the previous or next reply, if there is one. */
function stepReader(dir) {
  const cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
  const rv = sid && readerViewed(sid, cur.it, cur.s);
  const next = rv && rv.all[rv.at + dir];
  if (!next) return;
  readerView = { sid, key: next.key };
  renderMain();
}
function sizeReader(dir) {
  const n = Math.max(0, Math.min(READER_SIZES.length - 1, readerSize + dir));
  if (n === readerSize) return;
  readerSize = n;
  try { localStorage.setItem("cue.readerSize", String(n)); } catch {}
  renderMain();
}
/** Back to the chat, at the reply you were reading (its start in view). */
function closeReader() {
  const key = readerView?.key;
  readerView = null;
  renderMain();
  const btn = key && document.querySelector(`[data-read="${CSS.escape(key)}"], [data-fwd="${CSS.escape(key)}"]`);
  btn?.closest(".cv-agent")?.scrollIntoView({ block: "start" });
}
/** Reply: to the chat's own box, at the chat's end, the cursor in it. */
function replyFromReader() {
  readerView = null;
  drawAtChatEnd();
  focusComposer();
}
function copyReader() {
  const cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
  const rv = sid && readerViewed(sid, cur.it, cur.s);
  if (rv) invoke("copy_text", { text: rv.all[rv.at].text }).then(() => toast("Copied"), () => toast("Couldn't copy"));
}
/** The reader, in place of the chat: a bar (‹ Chat, ‹ n of m ›, text size, Copy, Reply), the reply in one
 *  column, and a foot only when the session is asking you something (then Back to chat answers it there). */
function readerPane(sid, it, s, harness, rv) {
  const r = rv.all[rv.at], n = rv.all.length;
  // The reader has the pane to itself (activePane leaves out the session's head, bar, tabs and terminal strip):
  // its one bar says which session it is, and that it's waiting in its terminal when it is (an amber Terminal, as
  // the strip goes amber; a click: the chat, with the terminal open).
  const who = `<span class="badge h-${esc(harness || "other")}">${agentLetter(harness)}</span><span class="proj rd-name">${esc(nameOf(sid, it?.project || s?.project))}</span>`;
  const term = inTmux(sid) && waitsInTerminal(sid) ? `<button class="rd-term" data-reader-term="${esc(sid)}" aria-label="Waiting in its terminal: open it"><span class="tlive"></span>Terminal</button>` : "";
  const bar = `<div class="bg-view-head rd-head" data-key="head"><button class="bg-back" data-reader-back="1"><span class="bg-back-ch">‹</span>Chat</button>${who}<span class="grow"></span>${term}
    <span class="rd-seg"><button data-reader-step="-1" aria-label="Previous reply" ${rv.at ? "" : "disabled"}>‹</button><span class="rd-of">${rv.at + 1} of ${n}</span><button data-reader-step="1" aria-label="Next reply" ${rv.at < n - 1 ? "" : "disabled"}>›</button></span>
    <span class="rd-seg"><button class="rd-a small" data-reader-size="-1" aria-label="Smaller text" ${readerSize ? "" : "disabled"}>A</button><button class="rd-a big" data-reader-size="1" aria-label="Larger text" ${readerSize < READER_SIZES.length - 1 ? "" : "disabled"}>A</button></span>
    ${listening?.key === r.key ? "" : `<button class="btn ls-btn" data-listen="${esc(r.key)}" data-listen-reader="1">${SPEAKER_ICON}Listen</button>`}<button class="btn" data-reader-copy="1">Copy</button>${s ? `<button class="btn primary" data-reader-reply="1">Reply</button>` : ""}</div>`;
  const body = `<div class="rd" data-key="rd:${esc(r.key)}:${readerSize}" data-rdkey="${esc(r.key)}" style="--rd-size: ${READER_SIZES[readerSize]}px"><div class="rd-by">${esc(agentName(harness))} · ${ago(r.at_ms)} ago · reply ${rv.at + 1} of ${n}</div><div class="msg">${md(stripStatus(r.text))}</div>${localImages(r.text)}</div>`;
  const asks = isPending(it) && it.kind !== "waiting";
  const foot = asks ? `<div class="foot rd-ask"><span class="rd-ask-dot"></span><span class="rd-ask-t"><b>${esc(nameOf(sid, it.project))} ${it.kind === "question" ? "is asking you" : "needs a decision"}</b>${it.kind === "question" ? "" : `<span class="dim"> · ${esc(verb(it))}</span>`}</span><span class="grow"></span><button class="btn primary" data-reader-back="1">Back to chat</button></div>` : "";
  return { bar, body, foot: listenPlayer(r.key) + foot };
}
