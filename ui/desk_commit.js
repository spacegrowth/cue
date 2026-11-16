// Cue UI desk_commit — Looks right on a Desk item commits its changed files at once (hub/desk_commit.rs: the word
// check first, its title the message, never pushed) and the item is done; one with no files is just done. When another
// session at work is editing some of them, the item is done at once and the commit waits ("Commit waiting: <file>
// (<session> is editing it)"), made on its own once they're free. "Committed <hash> · N files" shows with Undo, which
// takes the commit back (its changes stay) while it's still the last one.
// Each time a session says DONE for an item, the lead's chat shows a card for it (deskCheckRows): what to test, how
// many files it touched, whether the installed Cue has it, and Looks right / Send back while it waits on you. Kept by the hub
// (hub/desk_checks.rs), so they're there after a restart.
// Commands: desk_looks_right, desk_commit_undo.

/** What Looks right did, by item, since the window opened: { commit, files, waiting: [{ path, by }] }. */
const deskCommitted = new Map();

/** Looks right on item `id`: committed (or just done), said in a toast and on its card and item. */
async function deskLooksRight(id) {
  try {
    const r = await invoke("desk_looks_right", { id });
    deskCommitted.set(id, r);
    toast(deskCommitWords(r) || "Done");
  } catch (e) { toast(String(e), 5000); }
  renderMain();
}
async function deskCommitUndo(id) {
  try { await invoke("desk_commit_undo", { id }); deskCommitted.delete(id); toast("Commit taken back: its changes are still there"); }
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
/** A done item's commit as a line with Undo (`i`: the item, or a card: { id, commit, commit_wait }): "Committed <hash>",
 *  or while it waits "Commit waiting: <file> (<session> is editing it)", or why it wasn't made. */
function deskCommitLine(i) {
  const undo = `<button class="dk-undo-c" data-desk-act="undo_commit" data-id="${esc(i.id)}">Undo</button>`;
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
  const by = c.by || "its session", ses = (state.sessions || []).find((s) => s.name === c.by && (!project || s.project === project));
  const who = ses ? `<button class="dk-chk-a" data-desk-chat-of="${esc(ses.session_id)}">${esc(by)}</button>` : `<b>${esc(by)}</b>`;
  const n = (c.files || []).length, files = n ? ` · ${n === 1 ? "1 file" : `${n} files`}` : "";
  const inst = state.desk?.installed_ms || 0;
  const installed = !c.cue ? "" : inst > c.at ? `In the installed Cue · installed ${esc(deskChkWhen(inst))}` : "Not installed yet";
  const open = c.last && c.state === "to_check";
  const acts = open && deskBackFor === c.id ? deskBackHtml(c.id) : open ? `<div class="dk-acts"><button class="btn small primary" data-desk-act="looks_right" data-id="${esc(c.id)}">Looks right</button><button class="btn small" data-desk-act="send_back" data-id="${esc(c.id)}">Send back</button></div>`
    : c.back || c.back_pics ? deskSentBack(c)
    : `<div class="dk-chk-after">${c.state === "done" ? (c.last && deskHasCommit(c) ? deskCommitLine(c) : "Looked right") : "Sent back"}</div>`;
  return `<div class="dk-chk"><div class="dk-chk-k">Ready to check</div><div class="dk-chk-t">${deskKindIcon(c)}<button class="dk-chk-a" data-desk-chk-item="${esc(c.id)}">${esc(`${deskNum(c)} · ${c.title}`)}</button></div>
    <div class="dk-chk-w">Done by ${who} · ${ago(c.at)} ago${files}</div>
    <div class="dkc-l">What to test</div><div class="dkc-dw">${c.done_when ? linkify(esc(c.done_when), base) : `<span class="dim">No "done when" was given</span>`}</div>
    ${installed ? `<div class="dkc-l">Installed</div><div class="dk-chk-in">${installed}</div>` : ""}${acts}</div>`;
}
/** A card sent back: "Sent back" and why, in your words (links and paths open), with how many pictures went too. */
function deskSentBack(c) {
  const pics = c.back_pics ? `<span class="dim"> · ${c.back_pics === 1 ? "1 picture" : `${c.back_pics} pictures`}</span>` : "";
  return `<div class="dkc-l">Sent back</div><div class="dkc-dw dk-chk-back">${c.back ? esc(c.back) : `<span class="dim">No words, just pictures</span>`}${pics}</div>`;
}
const deskChkWhen = (ms) => new Date(ms).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });
