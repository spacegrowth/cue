// ---------- The Diff tab: what the open session changed, from git ----------
// A tab beside Chat (after Agents and Scripts) for a session with changes, its count on it ("Diff 17 +880 −9"). Git
// runs through Cue's read-only git (extensions.rs diff_git), in the session's folder, on this Mac or its machine:
//   This session's edits  the files the session edited (from its log), wherever they are: their changes not committed yet
//   This turn             the session's folder since its latest turn began (Cue's snapshot of it then)
//   Whole session         the session's folder since its first turn Cue saw began
//   Uncommitted           everything not committed in the session's folder, whoever changed it
// A crew member's Diff (team.js) opens it on that member's files only, its diff as the hub reads it (team_action
// "diff"), a chip on top to go back to the whole diff.
// The page is one element kept across redraws (so a file you opened stays open and the list stays where you scrolled),
// moved into its slot after each draw (placeDiff). It follows the open session whether it shows or not, so the tab
// and its count show before it's opened.
const DIFF_TAB = ":diff";   // its place in extTabOf (extensions.js): no extension's name has a colon
const DIFF_SCOPES = [["edits", "This session's edits"], ["turn", "This turn"], ["session", "Whole session"], ["uncommitted", "Uncommitted"]];
const DIFF_SINCE = { edits: "not committed yet", turn: "since its latest turn began", session: "since its first turn here", uncommitted: "not committed" };
/** How many files open by themselves; the rest open when you click them. */
const DIFF_OPEN_FIRST = 3;
/** New files in a folder (two levels deep at most) with more than this many are one row, not one per file. */
const DIFF_BULK = 20;
const diffTabs = new Map();   // sid → { badge: "17 +880 −9", any: it has changes }
/** What the page shows: the session, its scope, and a crew member's files when it's filtered to them. */
const dv = { el: null, cur: null, scope: null, seq: 0, key: "", picked: false, noTools: false, files: [], top: 0, member: null };

const diffOn = (sid) => !!sid && extTabOf.get(sid) === DIFF_TAB;
const dvBase = (s, sc) => (sc === "turn" ? s.turn_from : sc === "session" ? s.session_from : null);
const dvUsable = (s, sc) => sc === "edits" || sc === "uncommitted" || !!dvBase(s, sc);
const dvHome = (p) => p.replace(/^\/(Users|home)\/[^/]+/, "~");
const dvPlural = (n, w) => `${n} ${w}${n === 1 ? "" : "s"}`;

/** Read-only git in session `s`'s folder (Cue's record of it, not the page's). */
function dvGit(s, op, params = {}) {
  const x = sessionOf(s.id) || (state.live || []).find((q) => q.session_id === s.id) || {};
  return invoke("diff_git", { op, folder: x.cwd || "", machine: x.machine || null, against: params.against || null, path: params.path || null, transcript: x.transcript_path || null });
}

/** A unified diff (several files' worth) as files: path, new or not, lines, and +/- counts. Lines that aren't part of
 *  a diff (a crew member's "notes/x.md: not in a git repository", say) are `notes`. */
function diffParse(text) {
  const files = [], notes = [];
  let f = null;
  for (const line of String(text || "").split("\n")) {
    if (line.startsWith("diff --git ")) { const m = line.match(/^diff --git a\/(.*) b\/(.*)$/); f = { path: m ? (m[1] === "dev/null" ? m[2] : m[1]) : line.slice(11), lines: [], add: 0, del: 0, isNew: false }; files.push(f); continue; }
    if (f && line.startsWith("new file")) f.isNew = true;
    if (/^(index |--- |\+\+\+ |new file|deleted file|similarity|rename |old mode|new mode|Binary)/.test(line)) { if (f) continue; }
    if (line.startsWith("@@") && f) f.lines.push(["h", line]);
    else if (line.startsWith("+") && f) { f.add++; f.lines.push(["a", line]); }
    else if (line.startsWith("-") && f) { f.del++; f.lines.push(["d", line]); }
    else if (/^[ \\]/.test(line) && f) f.lines.push(["", line]);
    else if (line.trim()) { notes.push(line.trim()); f = null; }
  }
  return { files, notes };
}

/** New files, apart from folders full of them (build output git isn't told to ignore, say). */
const dvDirOf = (p) => { const parts = p.split("/"); return parts.length < 2 ? "" : parts.slice(0, Math.min(2, parts.length - 1)).join("/") + "/"; };
function diffBulk(paths) {
  const groups = new Map();
  for (const p of paths) { const d = dvDirOf(p); if (d) groups.set(d, (groups.get(d) || 0) + 1); }
  const bulk = [...groups].filter(([, n]) => n > DIFF_BULK).map(([dir, n]) => ({ dir, n }));
  const inBulk = new Set(bulk.map((b) => b.dir));
  return { single: paths.filter((p) => !inBulk.has(dvDirOf(p))), bulk };
}

/** A crew member's files, each with its repo when they span repos. The hub's diff names a file in the member's own
 *  folder's repo from that repo's top, and one in another repo under that repo's label ("site/x.md"). `files` are its
 *  files as the stream gave them; `tops` the repo top of each absolute one's folder (and of its own folder: `home`).
 *  Each parsed file gets `repo` (shown only when there's more than one) and its path within it. */
function diffRepos(parsed, files, home, tops) {
  const bare = (p) => (p.startsWith("/private/") ? p.slice(8) : p);
  const homeTop = bare(home || "").replace(/\/$/, "");
  const name = (top) => top.split("/").filter(Boolean).pop() || top;
  // Every other repo's files as the hub writes them: its label, then the path from its top.
  const known = [];
  for (const f of files) {
    if (!f.startsWith("/")) continue;
    const top = bare(tops[f] || "").replace(/\/$/, "");
    if (!top || top === homeTop || !bare(f).startsWith(top + "/")) continue;
    known.push({ rest: bare(f).slice(top.length + 1), top });
  }
  for (const x of parsed) {
    const k = known.find((r) => x.path.endsWith("/" + r.rest) && name(r.top) === x.path.slice(0, -r.rest.length - 1).split("/").pop());
    if (k) { x.repo = x.path.slice(0, -k.rest.length - 1); x.path = k.rest; } else x.repo = homeTop ? name(homeTop) : "";
  }
  return new Set(parsed.map((x) => x.repo)).size > 1;
}

function dvElement() {
  if (dv.el) return dv.el;
  const el = document.createElement("div");
  el.className = "dv";
  el.innerHTML = `<header class="dv-head"><span class="dv-chip-at"></span><span class="dv-seg"></span><span class="dv-sum"></span><span class="grow"></span><button class="dv-refresh" data-dv-refresh>Refresh</button></header><div class="dv-body"></div>`;
  const body = el.querySelector(".dv-body");
  body.addEventListener("scroll", () => { if (body.isConnected) dv.top = body.scrollTop; });
  // A file's lines go in the first time it's opened.
  body.addEventListener("toggle", (e) => {
    const d = e.target;
    if (!d.matches?.("details.dv-file[data-i]") || !d.open) return;
    const box = d.querySelector(".dv-lines");
    if (!box.childElementCount) box.innerHTML = dvLines(dv.files[+d.dataset.i]);
  }, true);
  dv.el = el;
  return el;
}
const dvPart = (sel) => dvElement().querySelector(sel);
const dvNote = (html) => { dvPart(".dv-body").innerHTML = html; };

function dvDrawScope() {
  const s = dv.cur, member = dvMemberOn();
  dvPart(".dv-seg").hidden = !!member;
  dvPart(".dv-seg").innerHTML = DIFF_SCOPES.map(([k, label]) => `<button data-dv-scope="${k}" class="${k === dv.scope ? "on" : ""}" ${s && dvUsable(s, k) ? "" : "disabled"}>${label}</button>`).join("");
  dvPart(".dv-chip-at").innerHTML = member ? `<span class="dv-chip">${esc(member.name)}'s ${dvPlural(member.n ?? 0, "file")}<button data-dv-all aria-label="The whole diff">✕</button></span>` : "";
}

/** The tab's count and whether it shows: `badge` "" leaves the count off. Redraws when either changes. */
function dvTab(sid, badge, any) {
  const was = diffTabs.get(sid) || {}, now = { badge: badge ?? was.badge ?? "", any: any ?? was.any ?? false };
  diffTabs.set(sid, now);
  if (was.badge !== now.badge || !!was.any !== !!now.any) renderMain();
}

async function dvLoad() {
  const s = dv.cur, my = ++dv.seq;
  if (!s) { dvPart(".dv-sum").textContent = ""; dvNote(`<div class="dv-note">Open a session to see what it changed.</div>`); return; }
  if (dvMemberOn()) return dvLoadMember(my);
  if (!dv.scope || !dvUsable(s, dv.scope)) { dv.scope = "edits"; dv.picked = false; }
  dvDrawScope();
  dvNote(`<div class="dv-note">Reading git…</div>`);
  try {
    if (dv.scope === "edits") {
      const r = await dvGit(s, "edited");
      if (my !== dv.seq) return;
      // No edits with its edit tools (an agent that edits through shell commands, say: the log can't say which files
      // those changed): unless you picked this view, show the folder since this turn began.
      if (!(r.files || []).length && !dv.picked) { dv.scope = dvUsable(s, "turn") ? "turn" : "uncommitted"; dv.noTools = true; return dvLoad(); }
      dv.noTools = false;
      dvEdits(s, r.files || []);
      return;
    }
    const top = await dvGit(s, "toplevel");
    if (my !== dv.seq) return;
    if (top.code !== 0) { dvShow(s, [], [], `<div class="dv-note">${esc(s.machine ? `Couldn't read git on ${s.machine}.` : "This folder isn't in a git repository.")}</div>`); return; }
    const base = dvBase(s, dv.scope);
    const [diff, others] = await Promise.all([dvGit(s, "diff", { against: base ? base.commit : "HEAD" }), dvGit(s, "untracked")]);
    if (my !== dv.seq) return;
    const before = new Set(base ? base.untracked : []);
    const { single, bulk } = diffBulk(others.stdout.split("\n").filter((p) => p && !before.has(p)));
    // New files git isn't tracking: their content (the first 20; the rest by name).
    const shown = await Promise.all(single.slice(0, 20).map((p) => dvGit(s, "new_file", { path: p }).then((r) => r.stdout, () => "")));
    if (my !== dv.seq) return;
    const files = diffParse(diff.stdout + shown.join("")).files;
    for (const p of single.slice(20)) files.push({ path: p, isNew: true, lines: [], add: 0, del: 0, unread: true });
    dvShow(s, files, bulk);
  } catch (e) {
    if (my === dv.seq) dvNote(`<div class="dv-note">${esc(e?.message || e)}</div>`);
  }
}

/** This session's edits: each file it edited, with its changes now; ones with nothing left uncommitted listed after.
 *  The session's own files come first; what each of its background agents edited follows, under a heading per agent
 *  (`by`: "" for the session, else the agent's description). */
function dvEdits(s, results) {
  const files = [], quiet = [];
  const root = (s.folder || "").replace(/\/$/, "");
  const short = (p) => (root && p.startsWith(root + "/") ? p.slice(root.length + 1) : dvHome(p));
  for (const r of results) {
    const f = r.diff && diffParse(r.diff).files[0];
    if (f) files.push({ ...f, path: short(r.path), isNew: !!r.new, by: r.by || "" }); else quiet.push({ path: short(r.path), note: r.note || "" });
  }
  const own = files.filter((f) => !f.by), byAgent = new Map();
  for (const f of files) if (f.by) (byAgent.get(f.by) || byAgent.set(f.by, []).get(f.by)).push(f);
  for (const [by, fs] of byAgent) { fs[0].heading = `Edited by the ${by} agent`; own.push(...fs); }
  dvShow(s, own, [], "", quiet, results.length);
}

const dvLines = (x) => (x.unread ? `<div class="h">A new file git isn't tracking yet.</div>` : x.lines.map(([k, l]) => `<div class="${k}">${esc(l)}</div>`).join(""));
const dvCounts = (files) => ({ add: files.reduce((n, x) => n + x.add, 0), del: files.reduce((n, x) => n + x.del, 0), count: files.length });

/** The files, each a fold (the first few open), then folders full of new files, then files with nothing left. */
function dvFilesHtml(files, bulk = [], quiet = [], showRepo = false) {
  dv.files = files;
  let opened = 0;
  return files.map((x, i) => {
    const open = !x.unread && opened < DIFF_OPEN_FIRST && ++opened;
    return `${x.heading ? `<div class="dv-by">${esc(x.heading)}</div>` : ""}
    <details class="dv-file" data-i="${i}" ${open ? "open" : ""}>
      <summary>${showRepo && x.repo ? `<span class="dv-tag dv-repo">${esc(x.repo)}</span>` : ""}<span class="dv-path">${esc(x.path)}</span>${x.isNew ? `<span class="dv-tag">new</span>` : ""}${x.unread ? "" : `<span class="dv-n dv-add">+${x.add}</span><span class="dv-n dv-del">−${x.del}</span>`}<span class="dv-chev">›</span></summary>
      <div class="dv-lines">${open ? dvLines(x) : ""}</div>
    </details>`;
  }).join("") + bulk.map((b) => `
    <details class="dv-file"><summary><span class="dv-path">${esc(b.dir)}</span><span class="dv-tag">${b.n.toLocaleString()} new files</span><span class="dv-chev">›</span></summary><div class="dv-lines"><div class="h">Files git isn't tracking and isn't told to ignore. If it's build output, add ${esc(b.dir)} to .gitignore.</div></div></details>`).join("")
    + (quiet.length ? `<div class="dv-quiet"><div class="dv-quiet-h">${quiet.head || "Also edited, nothing uncommitted now"}</div>${quiet.map((q) => `<div class="dv-quiet-r"><code>${esc(q.path)}</code>${q.note && q.note !== "no uncommitted changes" ? `<span>${esc(q.note)}</span>` : ""}</div>`).join("")}</div>` : "");
}

function dvShow(s, files, bulk, note = "", quiet = [], edited = 0) {
  const { add, del, count } = dvCounts(files), scope = dv.scope, since = DIFF_SINCE[scope];
  const nBulk = bulk.reduce((n, b) => n + b.n, 0);
  const also = bulk.length ? ` · and ${nBulk.toLocaleString()} new files in ${bulk.length === 1 ? esc(bulk[0].dir) : `${bulk.length} folders`}` : "";
  const what = scope === "edits" ? (edited ? `of the ${dvPlural(edited, "file")} it edited, ` : "") : "";
  const why = dv.noTools && scope !== "edits" ? `<div class="dv-why">It hasn't edited files with its edit tools (shell commands don't say which files they change), so this shows its folder.</div>` : "";
  dvPart(".dv-sum").innerHTML = (count ? `<b>${dvPlural(count, "file")}</b> <span class="dv-n"><span class="dv-add">+${add}</span> <span class="dv-del">−${del}</span></span> · ${what}${since}` : scope === "edits" && !edited ? "It hasn't edited any files" : `No changes ${since}`) + also;
  const auto = scope === "edits" || scope === "turn";
  dvTab(s.id, auto ? (count ? `${count} +${add} −${del}` : "") : undefined, auto || !dv.picked ? count > 0 || bulk.length > 0 : undefined);
  if (note) return dvNote(why + note);
  if (!count && !bulk.length && !quiet.length) return dvNote(why + `<div class="dv-note">${scope === "edits" ? "It hasn't edited any files yet." : `No changes ${since}.`}</div>`);
  // Only the first few files open, and a file's lines go in the page only once it's open: a change to hundreds of files
  // (or one huge one) stays quick to draw.
  dvNote(why + dvFilesHtml(files, bulk, quiet));
}

// ---------- a crew member's files ----------
/** The crew member the page is filtered to, when it's for the session showing. */
const dvMemberOn = () => (dv.member && dv.cur && dv.member.sid === dv.cur.id ? dv.member : null);

/** A crew member's Diff (team.js): the session's Diff tab, on that member's files. */
function diffShowMember(sid, agent, name) {
  actTabOn.delete(sid);
  bgTabOn.delete(sid);
  aboutTabOn.delete(sid);
  extTabOf.set(sid, DIFF_TAB);
  dv.member = { sid, agent, name, n: null };
  if (dv.cur?.id === sid) dvLoad();
  renderMain();
}

async function dvLoadMember(my) {
  const m = dv.member, s = dv.cur;
  dvDrawScope();
  dvPart(".dv-sum").textContent = "";
  dvNote(`<div class="dv-note">Reading its diff…</div>`);
  let text;
  try { text = await invoke("team_action", { sessionId: m.sid, agent: m.agent, action: "diff" }); }
  catch (e) { if (my === dv.seq) dvNote(`<div class="dv-note">Couldn't read its diff: ${esc(e?.message || e)}</div>`); return; }
  if (my !== dv.seq) return;
  const { files, notes } = diffParse(text);
  // Its files outside its own folder: each one's repo, so a file can say which repo it's in.
  const t = (sessionOf(m.sid)?.team || []).find((x) => x.agent_id === m.agent || x.call === m.agent) || {};
  const given = t.files || [], folder = t.worktree || s.folder || "";
  const away = given.filter((f) => f.startsWith("/") && !(folder && f.startsWith(folder.replace(/\/$/, "") + "/")));
  const tops = {};
  let home = "";
  if (away.length) {
    const topOf = (dir) => invoke("diff_git", { op: "toplevel", folder: dir, machine: s.machine || null, against: null, path: null, transcript: null }).then((r) => (r.code === 0 ? r.stdout.trim() : ""), () => "");
    const dirs = [...new Set(away.map((f) => f.replace(/\/[^/]*$/, "") || "/"))];
    const [h, ...rest] = await Promise.all([folder ? topOf(folder) : "", ...dirs.map(topOf)]);
    if (my !== dv.seq) return;
    home = h;
    for (const f of away) tops[f] = rest[dirs.indexOf(f.replace(/\/[^/]*$/, "") || "/")];
  }
  const spans = diffRepos(files, given, home, tops);
  m.n = files.length;
  dvDrawScope();
  const { add, del, count } = dvCounts(files);
  dvPart(".dv-sum").innerHTML = count ? `<span class="dv-n"><span class="dv-add">+${add}</span> <span class="dv-del">−${del}</span></span> · not committed yet` : "";
  const quiet = notes.map((n) => { const i = n.indexOf(": "); return i > 0 ? { path: n.slice(0, i), note: n.slice(i + 2) } : { path: n, note: "" }; });
  quiet.head = "Not shown";
  if (!count && !quiet.length) return dvNote(`<div class="dv-note">No changes.</div>`);
  dvNote(dvFilesHtml(files, [], quiet, spans));
}

// ---------- the tab, its slot, and following the open session ----------
/** The tab's button, for a session with changes (or the tab open), with its count. */
function diffTabBtn(sid) {
  const t = diffTabs.get(sid), on = diffOn(sid);
  if (!on && !t?.any) return "";
  return `<button class="stab ${on ? "on" : ""}" data-stab="${DIFF_TAB}" data-sid="${esc(sid)}">Diff${t?.badge ? `<span class="stab-n">${esc(t.badge)}</span>` : ""}</button>`;
}
/** Where the page goes, in place of the chat (extensions.js placeExt hides the chat under it). */
const diffSlot = (sid) => (diffOn(sid) ? `<div class="dv-slot" data-diff-slot></div>` : "");

/** Cue's view of the open session changed (a different one, or a turn began or ended): read it again. */
function dvFollow(sid) {
  const s = sid ? extSessionInfo(sid) : null;
  const key = s ? JSON.stringify([s.id, s.state, s.turn_from?.commit, s.session_from?.commit]) : "";
  if (key === dv.key) { dv.cur = s || dv.cur; return; }
  const other = !dv.cur || !s || dv.cur.id !== s.id;
  dv.key = key;
  dv.cur = s;
  if (other) { dv.scope = null; dv.noTools = false; dv.top = 0; if (dv.member && dv.member.sid !== s?.id) dv.member = null; }
  if (s) dvLoad();
}

/** After each draw: follow the open session, and put the page in its slot (where it was scrolled). */
function placeDiff() {
  dvFollow(openSid());
  const slot = document.querySelector(".active-pane [data-diff-slot]");
  if (!slot) return;
  const el = dvElement(), top = dv.top;
  if (el.parentNode !== slot) { slot.append(el); el.querySelector(".dv-body").scrollTop = top; }
}

if (typeof document !== "undefined") document.addEventListener("click", (e) => {
  // The tab itself (extensions.js opens it): the whole diff, not a crew member's.
  if (e.target.closest?.(`[data-stab="${DIFF_TAB}"]`) && dvMemberOn()) { dv.member = null; dv.top = 0; dvLoad(); return; }
  const t = e.target.closest?.("[data-dv-scope], [data-dv-refresh], [data-dv-all]");
  if (!t) return;
  e.stopPropagation();
  if (t.dataset.dvAll != null) { dv.member = null; dv.top = 0; return dvLoad(); }
  if (t.dataset.dvRefresh != null) return dvLoad();
  if (t.disabled) return;
  dv.scope = t.dataset.dvScope;
  dv.picked = true;
  dvLoad();
}, true);
