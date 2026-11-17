// Cue UI desk_commit — Looks right on a Desk item commits its changed files at once (hub/desk_commit.rs: the word
// check first, its title the message, never pushed) and the item is done; one with no files is just done. When another
// session at work is editing some of them, the item is done at once and the commit waits ("Commit waiting: <file>
// (<session> is editing it)"), made on its own once they're free. "Committed <hash> · N files" shows with Undo, which
// only puts it back to check: the commit stays, no code changes (a waiting commit isn't made).
// Each time a session says DONE for an item, the lead's chat shows one Ready to check line for it (deskCheckRows): its
// item and Check it ›, which opens the item's page, where what to test, Looks right and Send back are. Kept by the hub
// (hub/desk_checks.rs), so they're there after a restart.
// Looks right waits a moment first: "Done · Undo" for DESK_LOOKS_WAIT on the item and in a toast, and Undo in that
// time leaves it to check with nothing committed (Cue hasn't told the hub yet). Then it goes as above.
// Commands: desk_looks_right, desk_item_act (reopen).

/** What Looks right did, by item, since the window opened: { commit, files, waiting: [{ path, by }] }. */
const deskCommitted = new Map();

/** Items you said Looks right to in the last moment, not yet sent: id -> timer. */
const deskLooksSoon = new Map();
const DESK_LOOKS_WAIT = 3000;
/** Looks right on item `id`: "Done · Undo" for a moment (on the item and in a toast), then it goes (deskLooksGo). */
function deskLooksRight(id) {
  if (deskLooksSoon.has(id)) return;
  deskLooksSoon.set(id, setTimeout(() => { deskLooksSoon.delete(id); deskLooksGo(id); }, DESK_LOOKS_WAIT));
  toast("Done", DESK_LOOKS_WAIT);
  const t = document.querySelector(".toast"), u = document.createElement("button");
  u.className = "toast-undo"; u.textContent = DESK_UNDO_WORDS; u.onclick = () => deskLooksUndo(id);
  t?.append(" · ", u);
  t?.classList.add("acts");
  setTimeout(() => t?.classList.remove("acts"), DESK_LOOKS_WAIT);
  renderMain();
}
/** Undo in that moment: it stays to check, nothing sent, nothing committed. */
function deskLooksUndo(id) {
  if (!deskLooksSoon.has(id)) return;
  clearTimeout(deskLooksSoon.get(id));
  deskLooksSoon.delete(id);
  toast("Still to check");
  renderMain();
}
/** What every Undo after Looks right says: it only puts the item back to check, never touches the code. */
const DESK_UNDO_WORDS = "Undo · back to check";
/** In place of Looks right while it waits: "Done · Undo · back to check". */
const deskLooksSoonHtml = (id) => `<span class="dk-soon">Done · <button class="dk-undo-c" data-desk-act="looks_undo" data-id="${esc(id)}">${DESK_UNDO_WORDS}</button></span>`;
/** Items folding away after their moment: id -> { at, h } (when it began, the row's height then). The row draws
 *  with its fold already `now - at` in (a negative animation delay), so a redraw meanwhile doesn't start it again. */
const deskLeaving = new Map();
const DESK_LEAVE_MS = 280;
/** The style a leaving row draws with, "" for any other. */
function deskLeaveStyle(id) {
  const l = deskLeaving.get(id);
  return l ? ` dk-leave" style="--h:${l.h}px;animation-delay:-${Math.min(Date.now() - l.at, DESK_LEAVE_MS)}ms` : "";
}
/** Looks right sent: its row in the list folds away first, then it goes; committed (or just done), said in a toast
 *  and on its card and item. */
async function deskLooksGo(id) {
  const row = document.querySelector(`.dk-items .dk-it[data-desk-item="${CSS.escape(id)}"]`);
  if (row && !matchMedia("(prefers-reduced-motion: reduce)").matches) {
    deskLeaving.set(id, { at: Date.now(), h: row.offsetHeight });
    renderMain();
    await new Promise((r) => setTimeout(r, DESK_LEAVE_MS));
  }
  try {
    const r = await invoke("desk_looks_right", { id });
    deskCommitted.set(id, r);
    toast(deskCommitWords(r) || "Done");
  } catch (e) { toast(String(e), 5000); }
  deskLeaving.delete(id);
  renderMain();
}
/** Undo on a done item (its row, its page, its card): back to waiting on you, and nothing else. A commit made stays
 *  as it is and no code changes; a commit still waiting isn't made. */
async function deskReopen(id) {
  try { await invoke("desk_item_act", { id, act: "reopen" }); deskCommitted.delete(id); toast("Back to check · nothing in the work changed", 4000); }
  catch (e) { toast(String(e), 5000); }
  renderMain();
}
/** "Committed abc123 · 2 files", "Done · commit waiting: ui/x.js (web-lead is editing it)", or "" for no commit. */
function deskCommitWords(r) {
  if (r?.waiting?.length) return `Done · commit waiting: ${deskBusyWords(r.waiting)}`;
  if (!r?.commit) return "";
  return `Committed ${r.commit}${r.files ? ` · ${r.files === 1 ? "1 file" : `${r.files} files`}` : ""}`;
}
/** "ui/x.js (web-lead is editing it), ui/y.js (…)". */
const deskBusyWords = (busy) => busy.map((b) => `${b.path} (${b.by} is editing it)`).join(", ");
/** Whether done item `i` (or its card) has a commit to show: made, or waiting. */
const deskHasCommit = (i) => i.state === "done" && !!(i.commit || i.commit_wait);
/** A done item's line with Undo · back to check (`i`: the item, or a card: { id, commit, commit_wait }): "Committed
 *  <hash>", or while it waits "Commit waiting: <file> (<session> is editing it)", or why it wasn't made, or with no
 *  commit "Looked right" (`said`: it's said already, so Undo alone). */
function deskCommitLine(i, said = false) {
  const undo = `<button class="dk-undo-c" data-desk-act="reopen" data-id="${esc(i.id)}">${DESK_UNDO_WORDS}</button>`;
  if (!deskHasCommit(i)) return `<span class="dk-cmt">${said ? "" : "Looked right · "}${undo}</span>`;
  const w = !i.commit && i.commit_wait;
  if (w?.error) return `<span class="dk-cmt">Commit not made: ${esc(w.error)} · ${undo}</span>`;
  if (w) return `<span class="dk-cmt">Commit waiting: ${esc(deskBusyWords(w.busy || []) || "its files")} · ${undo}</span>`;
  const r = deskCommitted.get(i.id), files = r?.commit === i.commit ? r.files : 0;
  return `<span class="dk-cmt">Committed <span class="dk-cm">${esc(i.commit)}</span>${files ? ` · ${files === 1 ? "1 file" : `${files} files`}` : ""} · ${undo}</span>`;
}

/** Project `d`'s Ready to check lines for the lead's chat (chat/render.js chatHtml `extra`): one per DONE, at its time. */
function deskCheckRows(d) {
  return (d?.checks || []).map((c) => ({ at: c.at, html: keyed(deskCheckLine(c, d), `chk:${c.id}:${c.at}`) }));
}
/** A DONE as one line between thin rules while it's the one to check: "Ready to check: [task] Task #125 title… 12m
 *  Check it ›", the item and Check it › opening its page (what to test, Looks right and Send back are there). Once
 *  checked, a quiet centred line: "You · looked at [task] #124 and said it looks right" (or sent it back), the last
 *  one's commit and Undo · back to check after it. */
function deskCheckLine(c, d) {
  const num = `${deskKindIcon(c)}<b class="dk-inum">#${esc(c.id)}</b>`;
  if (!(c.last && c.state === "to_check")) {
    const said = c.state === "done" ? `looked at ${num} and said it looks right` : `sent ${num} back`;
    const cmt = c.last && c.state === "done" ? ` · ${deskCommitLine(c, true)}` : "";
    return `<div class="dk-chk-done dk-cl"><span class="dk-cl-t"><b>You</b> · ${said}${cmt}</span></div>`;
  }
  const open = `data-desk-chk-item="${esc(c.id)}"`;
  return `<div class="dk-rchk"><span class="dk-rchk-w">${deskChkUninstalled(c) && !deskFinCommit(d && deskItem(d.project, c.id)) ? "Waiting for install:" : "Ready to check:"}</span><button class="dk-rchk-it" ${open}>${deskKindIcon(c)}<b class="dk-inum">${esc(deskNum(c))}</b><span class="dk-rchk-t">${esc(c.title || "")}</span></button><span class="dk-cl-age">${ago(c.at)}</span><button class="dk-rchk-go" ${open}>Check it ›</button></div>`;
}
