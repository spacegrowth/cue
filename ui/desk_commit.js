// Cue UI desk_commit — Looks right on a Desk item commits its changed files at once (hub/desk_commit.rs: the word
// check first, its title the message, never pushed) and the item is done; one with no files is just done. When another
// session at work is editing some of them, the item is done at once and the commit waits ("Commit waiting: <file>
// (<session> is editing it)"), made on its own once they're free. "Committed <hash> · N files" shows with Undo, which
// takes the commit back (its changes stay) while it's still the last one.
// Each time a session says DONE for an item, the lead's chat shows a card for it (deskCheckRows): what to test, how
// many files it touched, whether the installed Cue has it, and Looks right / Send back while it waits on you. Kept by the hub
// (hub/desk_checks.rs), so they're there after a restart.
// Looks right waits a moment first: "Done · Undo" for DESK_LOOKS_WAIT on the item and in a toast, and Undo in that
// time leaves it to check with nothing committed (Cue hasn't told the hub yet). Then it goes as above.
// Commands: desk_looks_right, desk_commit_undo.

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
async function deskCommitUndo(id) {
  // Only the item goes back to check: a commit is taken back as a commit (its changes stay in place), or kept when
  // another commit came after it. No code is ever reverted.
  try {
    const was = (state.desk?.projects || []).flatMap((d) => d.items || []).find((x) => x.id === id);
    const it = await invoke("desk_commit_undo", { id });
    deskCommitted.delete(id);
    toast(it?.commit ? "Back to check · its commit stays (another came after it); no code changed"
      : was?.commit ? "Back to check · commit taken back, every change still in place" : "Back to check · no code changed", 4000);
  }
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
 *  commit "Looked right". */
function deskCommitLine(i) {
  const undo = `<button class="dk-undo-c" data-desk-act="undo_commit" data-id="${esc(i.id)}">${DESK_UNDO_WORDS}</button>`;
  if (!deskHasCommit(i)) return `<span class="dk-cmt">Looked right · ${undo}</span>`;
  const w = !i.commit && i.commit_wait;
  if (w?.error) return `<span class="dk-cmt">Commit not made: ${esc(w.error)} · ${undo}</span>`;
  if (w) return `<span class="dk-cmt">Commit waiting: ${esc(deskBusyWords(w.busy || []) || "its files")} · ${undo}</span>`;
  const r = deskCommitted.get(i.id), files = r?.commit === i.commit ? r.files : 0;
  return `<span class="dk-cmt">Committed <span class="dk-cm">${esc(i.commit)}</span>${files ? ` · ${files === 1 ? "1 file" : `${files} files`}` : ""} · ${undo}</span>`;
}

/** Project `d`'s cards for the lead's chat (chat/render.js chatHtml `extra`): one per DONE, at its time. Links and
 *  paths in them open as in the lead's own replies (its folder for relative paths). */
function deskCheckRows(d) {
  const lead = deskLead(d), ses = lead ? sessionOf(lead.sid) : null;
  const base = ses ? { cwd: ses.cwd, machine: ses.machine, sid: lead.sid } : null;
  return (d?.checks || []).map((c) => ({ at: c.at, html: keyed(deskCheckCard(c, base, d.project), `chk:${c.id}:${c.at}`) }));
}
/** A DONE as a card: "Bug #57 · Up/Down arrows" (a click opens the item), who did it (a click opens its chat) and how
 *  many files, what to test, whether it's installed, and Looks right / Send back while it's the one to check (else
 *  what became of it). Its words can be selected and copied; links and paths in them open. */
function deskCheckCard(c, base = null, project = "") {
  const by = c.by || "its session", ses = (state.sessions || []).find((s) => s.name === c.by && (!project || s.project === deskWorkspace(deskOf(project))));
  const who = ses ? `<button class="dk-chk-a" data-desk-chat-of="${esc(ses.session_id)}">${esc(by)}</button>` : `<b>${esc(by)}</b>`;
  const n = (c.files || []).length, files = n ? ` · ${n === 1 ? "1 file" : `${n} files`}` : "";
  // In it when Cue was installed after the work last changed (`edited`), not after the DONE: you install, then say DONE.
  const inst = state.desk?.installed_ms || 0;
  const waits = deskChkUninstalled(c), mocks = /^https?:\/\//.test(c.designs || "");
  const installed = !c.cue || mocks ? "" : waits ? "Not installed yet: install Cue to try it" : `In the installed Cue · installed ${esc(deskChkWhen(inst))}`;
  // Waiting for install: nothing to try yet, so no Looks right (Send back stays).
  const open = c.last && c.state === "to_check", early = open && waits;
  const acts = open && deskLooksSoon.has(c.id) ? `<div class="dk-acts">${deskLooksSoonHtml(c.id)}</div>` : open && deskBackFor === c.id ? deskBackHtml(c.id) : open ? `<div class="dk-acts">${early ? "" : `<button class="btn small primary" data-desk-act="looks_right" data-id="${esc(c.id)}">Looks right</button>`}<button class="btn small" data-desk-act="send_back" data-id="${esc(c.id)}">Send back</button></div>`
    : c.back || c.back_pics ? deskSentBack(c)
    : `<div class="dk-chk-after">${c.state === "done" ? (c.last ? deskCommitLine(c) : "Looked right") : "Sent back"}</div>`;
  return `<div class="dk-chk"><div class="dk-chk-k">${early ? "Waiting for install" : "Ready to check"}</div><div class="dk-chk-t">${deskKindIcon(c)}<button class="dk-chk-a" data-desk-chk-item="${esc(c.id)}">${esc(`${deskNum(c)} · ${c.title}`)}</button></div>
    <div class="dk-chk-w">Done by ${who} · ${ago(c.at)} ago${files}</div>
    <div class="dkc-l">What to test</div><div class="dkc-dw">${deskDoneWhenHtml(c.done_when, base) || `<span class="dim">No "done when" was given</span>`}</div>
    ${mocks ? `<div class="dkc-l">Mocks</div><div class="dk-chk-in"><a href="#" data-href="${esc(c.designs)}">Open the mocks ↗</a></div>` : installed ? `<div class="dkc-l">Installed</div><div class="dk-chk-in">${installed}</div>` : ""}${acts}</div>`;
}
/** A card sent back: "Sent back" and why, in your words (links and paths open), with how many pictures went too. */
function deskSentBack(c) {
  const pics = c.back_pics ? `<span class="dim"> · ${c.back_pics === 1 ? "1 picture" : `${c.back_pics} pictures`}</span>` : "";
  return `<div class="dkc-l">Sent back</div><div class="dkc-dw dk-chk-back">${c.back ? esc(c.back) : `<span class="dim">No words, just pictures</span>`}${pics}</div>`;
}
const deskChkWhen = (ms) => new Date(ms).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });
