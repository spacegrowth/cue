// Cue UI desk/commit — Looks right on a Desk item commits its changed files at once (desk/hub/landing/commit.rs: the word
// check first, its title the message, never pushed) and the item is done; one with no files is just done. When another
// session at work is editing some of them, the item is done at once and the commit waits ("Commit waiting: <file>
// (<session> is editing it)"), made on its own once they're free. "Committed <hash> · N files" shows with Undo, which
// only puts it back to check: the commit stays, no code changes (a waiting commit isn't made).
// Each time a session says DONE for an item, the Desk's Activity shows one Ready to check line for it (deskCheckLine;
// not the lead's chat since #789): its item and Check it ›, which opens the item's page, where what to test, Looks
// right and Send back are. Kept by the hub (desk/hub/lead/checks.rs), so they're there after a restart.
// Looks right waits a moment first: "Done · Undo" for DESK_LOOKS_WAIT on the item and in a toast, and Undo in that
// time leaves it to check with nothing committed (Cue hasn't told the hub yet). Then it goes as above.
// Commands: desk_looks_right, desk_item_act (reopen).

/** What Looks right did, by item, since the window opened: { commit, files, waiting: [{ path, by }] }. */
const deskCommitted = new Map();

/** Items you said Looks right to, by id: a timer for the moment Undo can still take it back, then DESK_LOOKS_GOING
 *  while the hub takes it, then a DESK_LOOKS_HELD place (deskLooksPlace) once it's done. In all three its row stays
 *  where it was, dimmed with "Done · Undo", and an opened item's bar keeps its height (#791): nothing folds away or
 *  is pulled out while you look. A done one leaves Waiting on you (for Done today) only when you open something else
 *  or pick another Desk (deskLooksLetGo), so the list moves when you move, not under you. */
const deskLooksSoon = new Map();
const DESK_LOOKS_WAIT = 3000, DESK_LOOKS_GOING = "going", DESK_LOOKS_HELD = "held:";
/** What's open on the Desk, as a held row's place: the Desk, and the item, session or chat open on it. */
const deskLooksPlace = () => DESK_LOOKS_HELD + JSON.stringify([deskSelection(), deskOpen?.kind || "", deskOpen?.id || deskOpen?.sid || deskOpen?.key || ""]);
/** Looks right on item `id`: "Done · Undo" for a moment (on the item and in a toast), then it goes (deskLooksGo). */
function deskLooksRight(id) {
  if (deskLooksSoon.has(id)) return;
  deskLooksSoon.set(id, setTimeout(() => { deskLooksSoon.set(id, DESK_LOOKS_GOING); deskLooksGo(id); }, deskLooksWaitMs()));
  toast("Done", deskLooksWaitMs());
  const t = document.querySelector(".toast"), u = document.createElement("button");
  u.className = "toast-undo"; u.textContent = DESK_UNDO_WORDS; u.onclick = () => deskLooksUndo(id);
  t?.append(" · ", u);
  t?.classList.add("acts");
  setTimeout(() => t?.classList.remove("acts"), deskLooksWaitMs());
  renderMain();
}
/** Undo in that moment: it stays to check, nothing sent, nothing committed. Once sent, the done row's Undo reopens it. */
function deskLooksUndo(id) {
  const v = deskLooksSoon.get(id);
  if (v === undefined || typeof v === "string") return;
  clearTimeout(v);
  deskLooksSoon.delete(id);
  toast("Still to check");
  renderMain();
}
/** What every Undo after Looks right says: it only puts the item back to check, never touches the code. */
const DESK_UNDO_WORDS = "Undo · back to check";
/** In place of Looks right while it waits: "Done · Undo · back to check". */
const deskLooksSoonHtml = (id) => `<span class="dk-soon">Done · <button class="dk-undo-c" data-desk-act="looks_undo" data-id="${esc(id)}">${DESK_UNDO_WORDS}</button></span>`;
const deskLooksWaitMs = () => DESK_LOOKS_WAIT;
/** Looks right sent: committed (or just done), said in a toast and on its card and item; its row stays where it is,
 *  done, till you open something else (deskLooksLetGo). Couldn't: it's to check again, as it was. */
async function deskLooksGo(id) {
  try {
    const r = await invoke("desk_looks_right", { id });
    deskCommitted.set(id, r);
    toast(deskCommitWords(r) || "Done");
    if (deskLooksSoon.get(id) === DESK_LOOKS_GOING) deskLooksSoon.set(id, deskLooksPlace());
  } catch (e) { toast(String(e), 5000); deskLooksSoon.delete(id); }
  renderMain();
}
/** Done rows held in place (deskLooksSoon) go once what's open on the Desk isn't what was open when they were done, or
 *  they aren't done any more (reopened, sent back). Called as Items is drawn. */
function deskLooksLetGo(items) {
  const at = deskLooksPlace(), byId = new Map(items.map((i) => [i.id, i]));
  for (const [id, v] of deskLooksSoon) {
    const st = byId.get(id)?.state;
    if (st === "done") deskLooksSawDone.add(id);
    if (typeof v !== "string" || !v.startsWith(DESK_LOOKS_HELD)) continue;
    // To check before it's done is the hub not having said done yet; to check after it was done is back to check.
    if (v !== at || (st !== "done" && st !== "to_check") || (st === "to_check" && deskLooksSawDone.has(id))) { deskLooksSoon.delete(id); deskLooksSawDone.delete(id); }
  }
}
/** Held items seen done since Looks right (deskLooksLetGo). */
const deskLooksSawDone = new Set();
/** Whether item `i` is one you just said Looks right to that's done now, held where it was in Waiting on you: from
 *  the start, as the hub's done can come in before its answer to Looks right does. */
const deskLooksHeld = (i) => i.state === "done" && deskLooksSoon.has(i.id);
/** Undo on a done item (its row, its page, its card): back to waiting on you, and nothing else. A commit made stays
 *  as it is and no code changes; a commit still waiting isn't made. */
async function deskReopen(id) {
  deskLooksSoon.delete(id);
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

/** An item's title for a one-line mention: its first words, cut at a word near `max` characters. */
function deskCheckTitle(c, max = 60) {
  const t = String(c.title || "").trim(), cut = t.length > max ? t.slice(0, max).replace(/\s+\S*$/, "") + "…" : t;
  return cut ? `<span class="dk-chk-t">${esc(cut)}</span>` : "";
}
/** What you wrote as you sent it back (#410): your note in quotes, cut to a line, and the pictures counted; "no note"
 *  when you wrote none. */
function deskBackNote(c) {
  const note = plain(c.back || ""), pics = c.back_pics ? ` · ${c.back_pics === 1 ? "1 picture" : `${c.back_pics} pictures`}` : "";
  if (!note) return c.back_by === "You" ? ` · <span class="dk-chk-note">no note</span>${pics}` : pics;
  return `: <span class="dk-chk-note">“${esc(note.length > 140 ? note.slice(0, 140).replace(/\s+\S*$/, "") + "…" : note)}”</span>${pics}`;
}
/** A DONE as the family's card (#501) while it's the one to check: "Cue READY TO CHECK Task #125 12m", its title,
 *  Check it ›; the item and Check it › opening its page (what to test, Looks right and Send back are there). Once
 *  past it, a quiet centred line under whoever moved it: "You · looked at [task] #124 and said it looks right", the
 *  last one's commit and Undo · back to check after it; "You · sent #124 … back · no note" only when your Send back
 *  (or your words to its session) did it, else under who did (#779: Cue, the lead, its holder); "Cue · finishing
 *  #124 …" while Cue lands it, nobody having sent it back. */
function deskCheckLine(c, d) {
  const num = `${deskKindIcon(c)}<b class="dk-inum">#${esc(c.id)}</b>`;
  if (!(c.last && c.state === "to_check")) {
    const line = (who, said) => `<div class="dk-chk-done dk-cl"><span class="dk-cl-t">${who ? `<b>${esc(who)}</b> · ` : ""}${said}</span></div>`;
    if (c.last && c.state === "done") return line("You", `looked at ${num} and said it looks right · ${deskCommitLine(c, true)}`);
    if (c.last && c.state === "finishing" && !c.back_by && !c.back) return line("Cue", `finishing ${num} ${deskCheckTitle(c)}`);
    // Sent back, by whoever did it; saved before Cue kept who, it says only that it went back.
    if (c.back_by) return line(c.back_by, `sent ${num} ${deskCheckTitle(c)} back${deskBackNote(c)}`);
    return line("", `${num} ${deskCheckTitle(c)} went back${deskBackNote(c)}`);
  }
  const open = `data-desk-chk-item="${esc(c.id)}"`;
  const wait = deskChkUninstalled(c) && !deskFinCommit(d && deskItem(d.project, c.id));
  return deskMg({ kind: "check", cue: true, who: "Cue", tag: wait ? "WAITING FOR INSTALL" : "READY TO CHECK", id: deskMgId(c.id, c, deskNum(c)), age: c.at,
    body: `<div class="mg-t">${esc(c.title || "")}</div>`, foot: `<span class="mg-acts"><button class="mg-lk" ${open}>Check it ›</button></span>` });
}
