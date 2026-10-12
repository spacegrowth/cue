// Diff: what the open session changed, from git, as a tab beside its chat.
//   This turn      since its latest turn began (Cue's snapshot of the folder then)
//   Whole session  since its first turn Cue saw began
//   Uncommitted    everything not committed in its folder, whoever changed it
// For a session on another machine, git runs there, over Cue's own SSH.
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const SCOPES = [["turn", "This turn"], ["session", "Whole session"], ["uncommitted", "Uncommitted"]];
let cur = null, scope = null, seq = 0, lastKey = "";

const baseFor = (s, sc) => (sc === "turn" ? s.turn_from : sc === "session" ? s.session_from : null);
const usable = (s, sc) => sc === "uncommitted" || !!baseFor(s, sc);
const git = (s, args) => cue.run("git", ["--no-optional-locks", "-C", s.folder, ...args], { machine: s.machine || undefined });

function drawScope() {
  $("scope").innerHTML = SCOPES.map(([k, label]) => `<button data-k="${k}" class="${k === scope ? "on" : ""}" ${cur && usable(cur, k) ? "" : "disabled"}>${label}</button>`).join("");
}

async function load(why) {
  const s = cur, my = ++seq;
  if (!s) { $("sum").textContent = ""; $("body").innerHTML = `<div class="note">Open a session to see what it changed.</div>`; return; }
  if (!scope || !usable(s, scope)) scope = usable(s, "turn") ? "turn" : "uncommitted";
  drawScope();
  $("body").innerHTML = `<div class="note">Reading git…</div>`;
  try {
    const top = await git(s, ["rev-parse", "--show-toplevel"]);
    if (my !== seq) return;
    if (top.code !== 0) { $("sum").textContent = ""; $("body").innerHTML = `<div class="note">${esc(s.machine ? `Couldn't read git on ${s.machine}.` : "This folder isn't in a git repository.")}</div>`; cue.badge(s.id, ""); return; }
    const base = baseFor(s, scope);
    const against = base ? base.commit : "HEAD";
    const [diff, others] = await Promise.all([
      git(s, ["diff", against, "--no-color", "--no-ext-diff"]),
      git(s, ["ls-files", "--others", "--exclude-standard", "--full-name"]),
    ]);
    if (my !== seq) return;
    const before = new Set(base ? base.untracked : []);
    const fresh = others.stdout.split("\n").filter((p) => p && !before.has(p));
    // New files git isn't tracking: their content, as a diff from nothing (the first 20; the rest by name).
    const shown = await Promise.all(fresh.slice(0, 20).map((p) => git(s, ["diff", "--no-color", "--no-index", "--", "/dev/null", p]).then((r) => r.stdout, () => "")));
    if (my !== seq) return;
    render(s, diff.stdout + shown.join(""), fresh.slice(20), base);
  } catch (e) {
    if (my === seq) $("body").innerHTML = `<div class="note">${esc(e.message)}</div>`;
  }
}

function render(s, text, fresh, base) {
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
  const add = files.reduce((n, x) => n + x.add, 0), del = files.reduce((n, x) => n + x.del, 0), count = files.length + fresh.length;
  const since = scope === "turn" ? "since its latest turn began" : scope === "session" ? "since its first turn here" : "not committed";
  $("sum").innerHTML = count ? `<b>${count} file${count === 1 ? "" : "s"}</b> <span class="n"><span class="add">+${add}</span> <span class="del">−${del}</span></span> · ${since}` : since;
  cue.badge(s.id, count ? `${count} +${add} −${del}` : "");
  if (!count) { $("body").innerHTML = `<div class="note">No changes ${since}.</div>`; return; }
  const many = files.length > 12;
  $("body").innerHTML = files.map((x) => `
    <details class="file" ${many ? "" : "open"}>
      <summary><span class="path">${esc(x.path)}</span>${x.isNew ? `<span class="tag">new</span>` : ""}<span class="n add">+${x.add}</span><span class="n del">−${x.del}</span><span class="chev">›</span></summary>
      <div class="lines">${x.lines.map(([k, l]) => `<div class="${k}">${esc(l)}</div>`).join("")}</div>
    </details>`).join("") + fresh.map((p) => `
    <details class="file"><summary><span class="path">${esc(p)}</span><span class="tag">new</span><span class="chev">›</span></summary><div class="lines"><div class="h">A new file git isn't tracking yet.</div></div></details>`).join("");
}

$("scope").addEventListener("click", (e) => {
  const b = e.target.closest("button[data-k]");
  if (!b || b.disabled) return;
  scope = b.dataset.k;
  load("scope");
});
$("refresh").addEventListener("click", () => load("refresh"));

/** Cue says which session is open, and again whenever anything about it changes: reload when it's a
 *  different session, or a turn began or ended (its state or its snapshot moved). */
function follow(s) {
  const key = s ? JSON.stringify([s.id, s.state, s.turn_from?.commit, s.session_from?.commit]) : "";
  if (key === lastKey) return;
  const other = !cur || !s || cur.id !== s.id;
  lastKey = key;
  cur = s;
  if (other) scope = null;
  load("session");
}
cue.on("session", follow);
cue.ready.then(async () => follow(await cue.session()));
