// Cue UI desk_commit — Looks right on a Desk item commits its changed files at once (hub/desk_commit.rs: the word
// check first, its title the message, never pushed) and the item is done; one with no files is just done. Files another
// session at work is editing are left out, and said. "Committed <hash> · N files" shows with Undo, which takes the
// commit back (its changes stay) while it's still the last one.
// Each time a session says DONE for an item, the lead's chat shows a card for it (deskCheckRows): what to test, the
// files it touched, whether the installed Cue has it, and Looks right / Send back while it waits on you. Kept by the hub
// (hub/desk_checks.rs), so they're there after a restart.
// Commands: desk_looks_right, desk_commit_undo.

/** What Looks right did, by item, since the window opened: { commit, files, skipped: [{ path, by }] }. */
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
/** "Committed abc123 · 2 files · left out ui/x.js (web-lead is editing it)", or "" for no commit. */
function deskCommitWords(r) {
  if (!r?.commit) return "";
  const n = r.files ? ` · ${r.files === 1 ? "1 file" : `${r.files} files`}` : "";
  const out = (r.skipped || []).map((s) => `${s.path} (${s.by} is editing it)`);
  return `Committed ${r.commit}${n}${out.length ? ` · left out ${out.join(", ")}` : ""}`;
}
/** A done item's commit as a line with Undo (`i`: the item, or a card: { id, commit }). */
function deskCommitLine(i) {
  const r = deskCommitted.get(i.id), files = r?.commit === i.commit ? r.files : 0;
  const out = r?.commit === i.commit ? (r.skipped || []) : [];
  return `<span class="dk-cmt">Committed <span class="dk-cm">${esc(i.commit)}</span>${files ? ` · ${files === 1 ? "1 file" : `${files} files`}` : ""} · <button class="dk-undo-c" data-desk-act="undo_commit" data-id="${esc(i.id)}">Undo</button></span>${out.length ? `<span class="dk-cmt-out">Left out: ${out.map((s) => `${esc(s.path)} (${esc(s.by)} is editing it)`).join(", ")}</span>` : ""}`;
}

/** Project `d`'s cards for the lead's chat (chat/render.js chatHtml `extra`): one per DONE, at its time. */
function deskCheckRows(d) {
  return (d?.checks || []).map((c) => ({ at: c.at, html: keyed(deskCheckCard(c), `chk:${c.id}:${c.at}`) }));
}
/** A DONE as a card: "Bug #57 · Up/Down arrows", who did it, what to test, its files, whether it's installed, and
 *  Looks right / Send back while it's the one to check (else what became of it). */
function deskCheckCard(c) {
  const files = c.files || [], many = files.length > 8;
  const list = files.length ? `<ul class="dk-chk-files">${files.slice(0, 8).map((f) => `<li>${esc(f)}</li>`).join("")}${many ? `<li class="dim">and ${files.length - 8} more</li>` : ""}</ul>` : `<div class="dim">None Cue can tell of</div>`;
  const inst = state.desk?.installed_ms || 0;
  const installed = !c.cue ? "" : inst > c.at ? `In the installed Cue · installed ${esc(deskChkWhen(inst))}` : "Not installed yet";
  const open = c.last && c.state === "to_check";
  const acts = open ? `<div class="dk-acts"><button class="btn small primary" data-desk-act="looks_right" data-id="${esc(c.id)}">Looks right</button><button class="btn small" data-desk-act="send_back" data-id="${esc(c.id)}">Send back</button></div>`
    : `<div class="dk-chk-after">${c.state === "done" ? (c.last && c.commit ? deskCommitLine(c) : "Looked right") : "Sent back"}</div>`;
  return `<div class="dk-chk"><div class="dk-chk-k">Ready to check</div><div class="dk-chk-t">${esc(`${deskNum(c)} · ${c.title}`)}</div>
    <div class="dk-chk-w">Done by <b>${esc(c.by || "its session")}</b> · ${ago(c.at)} ago</div>
    <div class="dkc-l">What to test</div><div class="dkc-dw">${c.done_when ? esc(c.done_when) : `<span class="dim">No "done when" was given</span>`}</div>
    <div class="dkc-l">Files</div>${list}${installed ? `<div class="dkc-l">Installed</div><div class="dk-chk-in">${installed}</div>` : ""}${acts}</div>`;
}
const deskChkWhen = (ms) => new Date(ms).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });
