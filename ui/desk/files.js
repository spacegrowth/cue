// The Desk panel's files (#313): what an item, a session or a crew member changed, listed as diff_overlay.js
// fileListHtml lists them: the first three with +/−, then "and N more files ›", a click on one opening
// its diff over the page.

const DESK_PFILES = 3;
/** Biggest change first. */
const deskBySize = (files) => [...files].sort((a, b) => (b.add || 0) + (b.del || 0) - ((a.add || 0) + (a.del || 0)));
/** A panel section: `head` and how many, then the list; "" with no files. */
const deskPfiles = (head, files, attrs, more) => (files.length ? deskPsec(`${head} · ${files.length}`, fileListHtml(files, { attrs, more, max: DESK_PFILES }), "dk-pfiles") : "");

/** Item `i`'s files, from the read its Files tab makes (deskFilesChanged, drawn first); till that's back, or while
 *  nothing in them has changed yet, the files it's on, "more" opening its Files tab. Always a section (#710), near the
 *  panel's top: with nothing changed, it says so. */
function deskItemFiles(i) {
  const checking = deskChecking(i), read = deskDiffs.get(deskDiffId(i))?.files, id = esc(i.id);
  if (read?.length) {
    const at = (n) => `data-desk-diff-file="${id}" data-n="${n}"`;
    return deskPfiles(checking ? "Files changed" : "Changed so far", read, at, at(DESK_PFILES));
  }
  const on = checking && read ? [] : deskFilesOn(i), open = (n) => deskOpenFileAttrs(i, on[n]);
  if (!on.length) return deskPsec("Files changed", `<p class="dk-note">${checking ? "None: it changed no files." : "None yet."}</p>`, "dk-pfiles");
  return deskPfiles(checking ? "Files changed" : "Files it's on", on.map((f) => ({ path: f.path })), open, `data-desk-itab="files" data-id="${id}"`);
}

/** The files item `i` is on, each with the folder its path is in (#677): a commit waiting on busy files says it per
 *  file ([folder, path]); the others are in the card's own working folder (deskItemFolder). */
const deskFilesOn = (i) => (i.commit_wait?.files || i.files || []).map((f) => (Array.isArray(f) ? { folder: f[0], path: f[1] } : { folder: "", path: f }));
/** The folder item `i` works in: its own copy, else its teammate's worktree, else its holder's folder; null if none
 *  is known (the viewer then looks from the path alone). */
function deskItemFolder(i) {
  if (i.tree?.path) return i.tree.path;
  const sid = deskHolderSid(i), lead = i.holder?.kind === "crew" ? i.holder.lead_sid : sid;
  const mate = i.holder?.kind === "crew" ? deskMate(lead, i.holder.sid) : i.crew ? deskMate(lead, i.crew) : null;
  return mate?.worktree || (sid && sessionOf(sid)?.cwd) || null;
}
/** A file row's click (desk/act.js deskOpenFile): item `i`'s file `f`, opened in Cue's file viewer. */
const deskOpenFileAttrs = (i, f) => `data-desk-open-file="${esc(f.path)}" data-id="${esc(i.id)}"${f.folder ? ` data-folder="${esc(f.folder)}"` : ""}`;
/** Item `id`'s file `path` in Cue's file viewer (or Read, for markdown), as a file named in a chat opens
 *  (chat/reader.js openPathOrDoc), found from the card's working folder. Works before anything in it has changed. */
function deskOpenFile(id, path, folder = "") {
  const i = deskItem(deskSelection(), id) || deskKeptItem(deskSelection(), id);
  if (!i) return;
  deskSeeItem(id);
  const sid = deskHolderSid(i) || null;
  // Another machine's files aren't on this Mac: openPathOrDoc says so as it does for a chat's.
  return openPathOrDoc(path, sid, folder || deskItemFolder(i));
}

/** Session id → { key, files, bulk, shown }: what its turn has changed so far (deskTurnDiff). */
const deskTurnDiffs = new Map();
/** What session `sid`'s turn has changed so far, read from git against where the turn began: Cue clears a session's
 *  changes as a turn starts and counts them as it ends, so a question mid-turn would show none. Read once per turn
 *  and state; files null while it's read; null when Cue has no start for its turn. */
function deskTurnDiff(sid) {
  const s = extSessionInfo(sid), key = s?.turn_from?.commit ? JSON.stringify([s.turn_from.commit, s.state]) : "";
  const had = deskTurnDiffs.get(sid);
  if (!key) return null;
  if (had?.key === key) return had;
  const rec = { key, files: had?.files || null, bulk: had?.bulk || [] };
  deskTurnDiffs.set(sid, rec);
  diffSince(s, s.turn_from).then((g) => { rec.files = g?.files || []; rec.bulk = g?.bulk || []; }, () => { rec.files = rec.files || []; })
    .then(() => { if (deskTurnDiffs.get(sid) === rec && view === "desk") renderMain(); });
  return rec;
}
/** Session `sid`'s files: what its turn changed (deskTurnDiff). Without a start to read its turn from, the files Cue
 *  counted as its last turn ended, a click opening its Diff tab. */
function deskSessFiles(sid) {
  const d = deskTurnDiff(sid), id = esc(sid);
  if (d && !d.files) return deskPsec("Changed this turn", `<div class="ib-dim">Reading git…</div>`, "dk-pfiles");
  const files = deskBySize(d ? d.files : sessionOf(sid)?.changes?.files || []);
  if (d) d.shown = files;
  const at = (n) => (d ? `data-desk-turn-file="${id}" data-n="${n}"` : `data-desk-sess-diff="${id}"`);
  return deskPfiles("Changed this turn", files, at, at(DESK_PFILES));
}

/** "lead|agent" → { key, files }: a crew member's diff so far, read once per change of its files or state and per
 *  page opened (team_action "diff", as its Diff button reads it); files null while it's read. */
const deskMateDiffs = new Map();
const deskMateShown = (files) => files.filter((f) => !MATE_SCRATCH.test(f.path) && !MATE_DEPS.test(f.path));
function deskCrewFiles(lead, t) {
  const agent = t.agent_id || t.call, k = `${lead}|${agent}`, names = mateShownFiles(t);
  if (!names.length && !t.worktree) return "";
  const key = JSON.stringify([t.state, names, t.stat || "", deskOpen]), had = deskMateDiffs.get(k);
  if (had?.key !== key) {
    const rec = { key, files: had?.files || null };
    deskMateDiffs.set(k, rec);
    const keep = (files) => { if (deskMateDiffs.get(k) !== rec) return; rec.files = files; if (view === "desk" && deskOpen?.kind === "crew") renderMain(); };
    invoke("team_action", { sessionId: lead, agent, action: "diff" }).then((text) => keep(deskMateShown(diffParse(text || "").files)), () => keep([]));
  }
  const files = deskMateDiffs.get(k).files;
  if (!files?.length) return deskPfiles("Changed so far", names.map((path) => ({ path })), () => "", "");
  const at = (n) => `data-desk-mate-file="${esc(k)}" data-n="${n}"`;
  return deskPfiles("Changed so far", deskBySize(files), at, at(DESK_PFILES));
}
/** Crew member "lead|agent"'s file `n`, over the page, its other files beside it. */
function deskShowMateFile(k, n) {
  const [lead, agent] = k.split("|"), t = deskMate(lead, agent), files = deskBySize(deskMateDiffs.get(k)?.files || []);
  diffOverlay(t?.name || "Teammate", "Changed so far", files, n);
}
/** Session `sid`'s turn file `n`, over the page: the turn's files on the left, file `n` open. */
function deskShowTurnFile(sid, n) {
  const d = deskTurnDiffs.get(sid), name = nameOf(sid, sessionOf(sid)?.project || "");
  diffOverlay(name ? `${name} · this turn` : "This turn", "Changed this turn", d?.shown || d?.files || [], n);
}
