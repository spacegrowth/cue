// Diff: what the open session changed, from git, as a tab beside its chat. Git runs through Cue's
// read-only git (cue.git), in the session's folder, on this Mac or on the session's machine.
//   This session's edits  the files the session edited (from its log), wherever they are: their changes
//                         not committed yet
//   This turn             the session's folder since its latest turn began (Cue's snapshot of it then)
//   Whole session         the session's folder since its first turn Cue saw began
//   Uncommitted           everything not committed in the session's folder, whoever changed it
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const SCOPES = [["edits", "This session's edits"], ["turn", "This turn"], ["session", "Whole session"], ["uncommitted", "Uncommitted"]];
let cur = null, scope = null, seq = 0, lastKey = "", picked = false, noTools = false;

const baseFor = (s, sc) => (sc === "turn" ? s.turn_from : sc === "session" ? s.session_from : null);
const usable = (s, sc) => sc === "edits" || sc === "uncommitted" || !!baseFor(s, sc);
const git = (s, op, params) => cue.git(s.id, op, params);
const home = (p) => p.replace(/^\/(Users|home)\/[^/]+/, "~");

function drawScope() {
  $("scope").innerHTML = SCOPES.map(([k, label]) => `<button data-k="${k}" class="${k === scope ? "on" : ""}" ${cur && usable(cur, k) ? "" : "disabled"}>${label}</button>`).join("");
}

/** A unified diff (several files' worth) as files: path, new or not, lines, and +/- counts. */
function parseDiff(text) {
  const files = [];
  let f = null;
  for (const line of text.split("\n")) {
    if (line.startsWith("diff --git ")) { const m = line.match(/^diff --git a\/(.*) b\/(.*)$/); f = { path: m ? (m[1] === "dev/null" ? m[2] : m[1]) : line.slice(11), lines: [], add: 0, del: 0, isNew: false }; files.push(f); continue; }
    if (f && line.startsWith("new file")) f.isNew = true;
    if (!f || /^(index |--- |\+\+\+ |new file|deleted file|similarity|rename |old mode|new mode|Binary)/.test(line)) continue;
    if (line.startsWith("@@")) f.lines.push(["h", line]);
    else if (line.startsWith("+")) { f.add++; f.lines.push(["a", line]); }
    else if (line.startsWith("-")) { f.del++; f.lines.push(["d", line]); }
    else if (line) f.lines.push(["", line]);
  }
  return files;
}

/** New files, apart from folders full of them (build output git isn't told to ignore, say): a folder,
 *  two levels deep at most, with more than BULK new files in it is one row, not one row per file. */
const BULK = 20;
const dirOf = (p) => { const parts = p.split("/"); return parts.length < 2 ? "" : parts.slice(0, Math.min(2, parts.length - 1)).join("/") + "/"; };
function bulkSplit(paths) {
  const groups = new Map();
  for (const p of paths) { const d = dirOf(p); if (d) groups.set(d, (groups.get(d) || 0) + 1); }
  const bulk = [...groups].filter(([, n]) => n > BULK).map(([dir, n]) => ({ dir, n }));
  const inBulk = new Set(bulk.map((b) => b.dir));
  return { single: paths.filter((p) => !inBulk.has(dirOf(p))), bulk };
}

async function load() {
  const s = cur, my = ++seq;
  if (!s) { $("sum").textContent = ""; $("body").innerHTML = `<div class="note">Open a session to see what it changed.</div>`; return; }
  if (!scope || !usable(s, scope)) { scope = "edits"; picked = false; }
  drawScope();
  $("body").innerHTML = `<div class="note">Reading git…</div>`;
  try {
    if (scope === "edits") {
      const r = await git(s, "edited");
      if (my !== seq) return;
      // No edits with its edit tools (an agent that edits through shell commands, say: the log can't say
      // which files those changed): unless you picked this view, show the folder since this turn began.
      if (!(r.files || []).length && !picked) { scope = usable(s, "turn") ? "turn" : "uncommitted"; noTools = true; return load(); }
      noTools = false;
      renderEdits(s, r.files || []);
      return;
    }
    const top = await git(s, "toplevel");
    if (my !== seq) return;
    if (top.code !== 0) { show(s, [], [], `<div class="note">${esc(s.machine ? `Couldn't read git on ${s.machine}.` : "This folder isn't in a git repository.")}</div>`); return; }
    const base = baseFor(s, scope);
    const [diff, others] = await Promise.all([git(s, "diff", { against: base ? base.commit : "HEAD" }), git(s, "untracked")]);
    if (my !== seq) return;
    const before = new Set(base ? base.untracked : []);
    const { single, bulk } = bulkSplit(others.stdout.split("\n").filter((p) => p && !before.has(p)));
    // New files git isn't tracking: their content (the first 20; the rest by name).
    const shown = await Promise.all(single.slice(0, 20).map((p) => git(s, "new_file", { path: p }).then((r) => r.stdout, () => "")));
    if (my !== seq) return;
    const files = parseDiff(diff.stdout + shown.join(""));
    for (const p of single.slice(20)) files.push({ path: p, isNew: true, lines: [], add: 0, del: 0, unread: true });
    show(s, files, bulk);
  } catch (e) {
    if (my === seq) $("body").innerHTML = `<div class="note">${esc(e.message)}</div>`;
  }
}

/** This session's edits: each file it edited, with its changes now; ones with nothing left uncommitted
 *  listed after. */
function renderEdits(s, results) {
  const files = [], quiet = [];
  const short = (p) => (s.folder && p.startsWith(s.folder.replace(/\/$/, "") + "/") ? p.slice(s.folder.replace(/\/$/, "").length + 1) : home(p));
  for (const r of results) {
    const f = r.diff && parseDiff(r.diff)[0];
    if (f) files.push({ ...f, path: short(r.path), isNew: !!r.new }); else quiet.push({ path: short(r.path), note: r.note || "" });
  }
  show(s, files, [], "", quiet, results.length);
}

/** How many files open by themselves; the rest open when you click them. */
const OPEN_FIRST = 3;
let shownFiles = [];
const linesOf = (x) => (x.unread ? `<div class="h">A new file git isn't tracking yet.</div>` : x.lines.map(([k, l]) => `<div class="${k}">${esc(l)}</div>`).join(""));

const SINCE = { edits: "not committed yet", turn: "since its latest turn began", session: "since its first turn here", uncommitted: "not committed" };
function show(s, files, bulk, note = "", quiet = [], edited = 0) {
  const add = files.reduce((n, x) => n + x.add, 0), del = files.reduce((n, x) => n + x.del, 0), count = files.length;
  const since = SINCE[scope];
  const nBulk = bulk.reduce((n, b) => n + b.n, 0);
  const also = bulk.length ? ` · and ${nBulk.toLocaleString()} new files in ${bulk.length === 1 ? esc(bulk[0].dir) : `${bulk.length} folders`}` : "";
  const what = scope === "edits" ? (edited ? `of the ${edited} file${edited === 1 ? "" : "s"} it edited, ` : "") : "";
  const why = noTools && scope !== "edits" ? `<div class="why">It hasn't edited files with its edit tools (shell commands don't say which files they change), so this shows its folder.</div>` : "";
  $("sum").innerHTML = (count ? `<b>${count} file${count === 1 ? "" : "s"}</b> <span class="n"><span class="add">+${add}</span> <span class="del">−${del}</span></span> · ${what}${since}` : scope === "edits" && !edited ? "It hasn't edited any files" : `No changes ${since}`) + also;
  if (scope === "edits" || scope === "turn") cue.badge(s.id, count ? `${count} +${add} −${del}` : "");
  if (note) { $("body").innerHTML = why + note; return; }
  if (!count && !bulk.length && !quiet.length) { $("body").innerHTML = why + `<div class="note">${scope === "edits" ? "It hasn't edited any files yet." : `No changes ${since}.`}</div>`; return; }
  // Only the first few files open, and a file's lines go in the page only once it's open: a change to
  // hundreds of files (or one huge one) stays quick to draw.
  shownFiles = files;
  let opened = 0;
  $("body").innerHTML = why + files.map((x, i) => {
    const open = !x.unread && opened < OPEN_FIRST && ++opened;
    return `
    <details class="file" data-i="${i}" ${open ? "open" : ""}>
      <summary><span class="path">${esc(x.path)}</span>${x.isNew ? `<span class="tag">new</span>` : ""}${x.unread ? "" : `<span class="n add">+${x.add}</span><span class="n del">−${x.del}</span>`}<span class="chev">›</span></summary>
      <div class="lines">${open ? linesOf(x) : ""}</div>
    </details>`;
  }).join("") + bulk.map((b) => `
    <details class="file"><summary><span class="path">${esc(b.dir)}</span><span class="tag">${b.n.toLocaleString()} new files</span><span class="chev">›</span></summary><div class="lines"><div class="h">Files git isn't tracking and isn't told to ignore. If it's build output, add ${esc(b.dir)} to .gitignore.</div></div></details>`).join("")
    + (quiet.length ? `<div class="quiet"><div class="quiet-h">Also edited, nothing uncommitted now</div>${quiet.map((q) => `<div class="quiet-r"><code>${esc(q.path)}</code>${q.note && q.note !== "no uncommitted changes" ? `<span>${esc(q.note)}</span>` : ""}</div>`).join("")}</div>` : "");
}

$("scope").addEventListener("click", (e) => {
  const b = e.target.closest("button[data-k]");
  if (!b || b.disabled) return;
  scope = b.dataset.k;
  picked = true;
  load();
});
$("refresh").addEventListener("click", () => load());
// A file's lines go in the first time it's opened.
$("body").addEventListener("toggle", (e) => {
  const d = e.target;
  if (!d.matches?.("details.file[data-i]") || !d.open) return;
  const box = d.querySelector(".lines");
  if (!box.childElementCount) box.innerHTML = linesOf(shownFiles[+d.dataset.i]);
}, true);

/** Cue says which session is open, and again whenever anything about it changes: reload when it's a
 *  different session, or a turn began or ended (its state or its snapshot moved). */
function follow(s) {
  const key = s ? JSON.stringify([s.id, s.state, s.turn_from?.commit, s.session_from?.commit]) : "";
  if (key === lastKey) return;
  const other = !cur || !s || cur.id !== s.id;
  lastKey = key;
  cur = s;
  if (other) { scope = null; noTools = false; }
  load();
}
cue.on("session", follow);
cue.ready.then(async () => follow(await cue.session()));
