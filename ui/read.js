// Cue UI — Read: markdown files you opened from a chat (a .md path in a reply, or in the mini window), one open at a
// time with Listen, beside the ones you opened before and the open one's headings.
// ---------- Read: Recent · the file · Contents ----------
// Recent is the files you opened here, newest first by when you first opened each (opening one again doesn't move
// it), kept in this window's storage: a list of paths, nothing copied. The
// file is read from disk each time it's opened, and again when Cue comes forward (an edit made meanwhile shows), by
// read_doc (ipc/docs.rs). Opened from a session, its head has ‹ <session> chat back to it.
const READ_KEEP = 40;
let readDoc = null;       // { path, text, modified_ms, sid, from }: the file open in Read; from "new" when picked in New docs
let readTried = false;    // Read opened with nothing open: the newest of Recent asked for once
let readRecent = [];      // [{ path, sid, project, at }], newest first
try { readRecent = JSON.parse(localStorage.getItem("cue.readRecent") || "[]").filter((r) => r?.path); } catch {}
let readNew = [];         // [{ path, top, created_ms, modified_ms }]: the newest .md files in your sessions' repos
let readNewAt = 0;        // when they were last asked for
const readKeyOf = (path) => `file:${path}`;
const readName = (path) => String(path).split("/").pop();
/** A file's project, as Cue names it: the project of the session on this Mac whose folder holds it (the deepest), or
 *  one whose folder is in its repo (`top`); else the repo's folder name, else the file's own folder. */
function readProject(path, top = "") {
  return readPlace(path, top).name;
}
/** The project a file is in (readProject) and its folder within it ("docs/dev"; "" at the project's top); a file in
 *  no project's folder: its own folder, shortened, as `dir`. */
function readPlace(path, top = "") {
  const local = state.sessions.filter((s) => !s.machine && s.cwd && s.project);
  const into = local.filter((s) => path.startsWith(`${s.cwd.replace(/\/+$/, "")}/`)).sort((a, b) => b.cwd.length - a.cwd.length)[0]
    || (top && local.find((s) => s.cwd === top || s.cwd.startsWith(`${top}/`)));
  const dirOf = (root) => path.slice(root.replace(/\/+$/, "").length + 1).replace(/\/?[^/]*$/, "");
  if (into) return { name: projLabel(into.project), dir: dirOf(path.startsWith(`${into.cwd}/`) ? into.cwd : top) };
  if (top) return { name: readName(top), dir: dirOf(top) };
  const folder = path.replace(/\/[^/]*$/, "");
  return { name: readName(folder), dir: shortPath(folder.replace(/^\/Users\/[^/]+/, "~")) };
}

/** A file in Recent: one opened for the first time goes on top; one opened before stays where it is (the list never
 *  moves under your clicks), its session kept up to date. */
function readRemember(path, sid) {
  const was = readRecent.find((r) => r.path === path);
  if (was) { if (sid) Object.assign(was, { sid, project: sessionOf(sid)?.project || was.project }); }
  else readRecent = [{ path, sid: sid || null, project: (sid && sessionOf(sid)?.project) || "", at: now() }, ...readRecent].slice(0, READ_KEEP);
  try { localStorage.setItem("cue.readRecent", JSON.stringify(readRecent)); } catch {}
}
/** A file in Read: found from `cwd` when it's a session's (ipc/docs.rs read_doc: as written, from the folders up to
 *  its repo's top, or by name), remembered in Recent, its start in view. */
function readOpen(path, sid = null, cwd = null, from = "") {
  return invoke("read_doc", { path, cwd }).then((f) => {
    if (readDoc?.path !== f.path) readPicked = null, readAt = 0;
    readDoc = { ...f, sid, from };
    readRemember(f.path, sid);
    view = "read";
    sheet = null;
    renderMain();
    document.querySelector(".rdv-scroll")?.scrollTo(0, 0);
  }, (err) => toast(String(err)));
}
/** The open file read again when Cue comes forward. Not while Listen reads it: its sentences would move under it. */
function readRefresh() {
  const d = readDoc;
  if (!d || view !== "read" || listening?.key === readKeyOf(d.path)) return;
  invoke("read_doc", { path: d.path, cwd: null }).then((g) => {
    if (readDoc !== d || g.modified_ms === d.modified_ms) return;
    readDoc = { ...d, ...g };
    renderMain();
  }, () => {});
}
addEventListener("focus", readRefresh);
/** New docs: the markdown files made most recently in the repos of your sessions on this Mac (ipc/docs.rs
 *  recent_docs; another machine's folders are never looked at here). Asked for when Read shows, at most once a
 *  minute, and when Cue comes forward. */
function readLoadNew(force = false) {
  if (!force && now() - readNewAt < 60000) return;
  readNewAt = now();
  const folders = [...new Set(state.sessions.filter((s) => !s.machine && s.cwd).map((s) => s.cwd))];
  invoke("recent_docs", { folders, limit: 10 }).then((list) => { readNew = list || []; if (view === "read") renderMain(); }, () => {});
}
addEventListener("focus", () => { if (view === "read") readLoadNew(true); });
/** The chat ‹ goes back to: the one you came from, open behind Read. */
function readBackSid() {
  const sid = current()?.s?.session_id;
  return sid && sessionOf(sid) ? sid : null;
}
function readBack() {
  const sid = readBackSid();
  if (sid) setActive(null, sid);
  else { view = "board"; renderMain(); }
}
function readForget(path) {
  readRecent = readRecent.filter((r) => r.path !== path);
  try { localStorage.setItem("cue.readRecent", JSON.stringify(readRecent)); } catch {}
  if (readDoc?.path === path) readDoc = null;
  renderMain();
}

/** Contents: the headings of the file as drawn (md's doc headings carry their level), in order; `text` is md's
 *  own escaped HTML with the tags taken out, so it goes into the page as it is. */
function readHeadings(html) {
  return [...html.matchAll(/<h4 class="dh(\d)">([\s\S]*?)<\/h4>/g)].map((m, n) => ({ n, lv: +m[1], text: m[2].replace(/<[^>]+>/g, "") }));
}
/** The heading you're in: the last one whose top is above the top of the page, marked in Contents as you scroll (or
 *  Listen scrolls). Without a redraw. */
let readPicked = null;    // the heading picked in Contents: stays marked while it's in view (the end of a short file can't scroll it to the top)
let readAt = 0;           // the heading marked in Contents (kept across a redraw)
function readTrack() {
  const sc = document.querySelector(".rdv-scroll");
  if (!sc) return;
  const box = sc.getBoundingClientRect(), top = box.top + 48;
  const hs = [...sc.querySelectorAll(".msg h4")];
  let at = 0;
  hs.forEach((h, i) => { if (h.getBoundingClientRect().top <= top) at = i; });
  const p = readPicked != null && hs[readPicked]?.getBoundingClientRect();
  if (p && p.top >= box.top && p.bottom <= box.bottom) at = readPicked;
  else readPicked = null;
  readAt = at;
  document.querySelectorAll("[data-read-h]").forEach((b) => b.classList.toggle("on", +b.dataset.readH === at));
}
document.addEventListener("scroll", (e) => { if (e.target?.classList?.contains("rdv-scroll")) readTrack(); }, true);
/** Listen in Read starts at the section you're in (the one marked in Contents): its heading's sentence. */
function readListenFrom() {
  const root = document.querySelector(".rdv-scroll .msg"), on = document.querySelector(".rdv-toc .on");
  const h = root && on && root.querySelectorAll("h4")[+on.dataset.readH];
  if (!h) return 0;
  const after = (n) => n && (h.contains(n) || !!(h.compareDocumentPosition(n) & Node.DOCUMENT_POSITION_FOLLOWING));
  return Math.max(0, listenSents(root).findIndex((x) => after(x.range ? x.range.startContainer : x.el)));
}
function readJump(n) {
  const sc = document.querySelector(".rdv-scroll"), h = sc?.querySelectorAll(".msg h4")[n];
  if (!h) return;
  readPicked = n;
  // Its heading to the top of the page, by the page's own scroll (WebKit's scrollIntoView can move the window too).
  sc.scrollTop += h.getBoundingClientRect().top - sc.getBoundingClientRect().top - 16;
  readTrack();
}

function readView() {
  queueMicrotask(() => readLoadNew());
  if (!readDoc && !readTried && readRecent.length) { readTried = true; const r = readRecent[0]; queueMicrotask(() => readOpen(r.path, r.sid)); }
  const d = readDoc, key = d ? readKeyOf(d.path) : "";
  const list = readRecent.length
    ? readRecent.map((r) => `<div class="rdv-item ${d?.path === r.path && d.from !== "new" ? "on" : ""}"><button class="rdv-open" data-read-open="${esc(r.path)}"><span class="rdv-name">${esc(readName(r.path))}</span><span class="rdv-sub">${esc(`${readProject(r.path)} · ${ago(r.at)} ago`)}</span></button><button class="rdv-x" data-read-forget="${esc(r.path)}" aria-label="Take it off the list">×</button></div>`).join("")
    : `<div class="quiet-line">Files you open show here. Click a .md path in any reply.</div>`;
  // At the foot of the left: the newest docs in your sessions' repos, whether or not you opened them.
  const fresh = readNew.length ? `<div class="rdv-new"><div class="col-head">NEW DOCS</div>${readNew.map((r) => `<div class="rdv-item ${readDoc?.path === r.path && readDoc.from === "new" ? "on" : ""}"><button class="rdv-open" data-read-new="${esc(r.path)}"><span class="rdv-name">${esc(readName(r.path))}</span><span class="rdv-sub">${esc(`${readProject(r.path, r.top)} · ${ago(r.created_ms)} ago`)}</span></button></div>`).join("")}</div>` : "";
  if (!d) return `<div class="rdv"><div class="rdv-list"><div class="col-head">RECENT</div>${list}${fresh}</div><div class="rdv-doc rdv-none"><div class="quiet-line">${readRecent.length ? "Opening…" : "Nothing open yet."}</div></div></div>`;
  const html = md(d.text, { pathsOf: d.sid && sessionOf(d.sid) ? d.sid : null, doc: true });
  const heads = readHeadings(html).filter((h) => h.lv > 0);
  const back = readBackSid();
  const head = `<div class="bg-view-head rd-head rdv-head">${back ? `<button class="bg-back" data-read-back="1"><span class="bg-back-ch">‹</span>${esc(nameOf(back, sessionOf(back).project))} chat</button>` : ""}<span class="rdv-title"><span class="rd-file">${esc(readName(d.path))}</span><span class="rdv-where">${esc((({ name, dir }) => [name, dir].filter(Boolean).join(" · "))(readPlace(d.path)))}${d.modified_ms ? ` · changed ${ago(d.modified_ms)} ago` : ""}</span></span><span class="grow"></span>
    <span class="rd-seg"><button class="rd-a small" data-reader-size="-1" aria-label="Smaller text" ${readerSize ? "" : "disabled"}>A</button><button class="rd-a big" data-reader-size="1" aria-label="Larger text" ${readerSize < READER_SIZES.length - 1 ? "" : "disabled"}>A</button></span>
    ${listenPlainBox()}
    ${listening?.key === key ? "" : `<button class="btn ls-btn" data-listen="${esc(key)}" data-listen-reader="1">${SPEAKER_ICON}Listen</button>`}<button class="btn" data-reader-open="${esc(d.path)}">Open</button></div>`;
  const body = `<div class="rdv-scroll"><div class="rd" data-key="rd:${esc(key)}:${readerSize}" data-rdkey="${esc(key)}" style="--rd-size: ${READER_SIZES[readerSize]}px"><div class="msg">${html}</div></div></div>`;
  const toc = heads.length > 1 ? `<div class="rdv-toc"><div class="col-head">CONTENTS</div>${heads.map((h) => `<button class="rdv-h l${Math.min(h.lv, 4)} ${h.n === readAt ? "on" : ""}" data-read-h="${h.n}">${h.text}</button>`).join("")}</div>` : "";
  return `<div class="rdv ${toc ? "" : "no-toc"}"><div class="rdv-list"><div class="col-head">RECENT</div>${list}${fresh}</div><div class="rdv-doc">${head}${body}${listenPlayer(key)}</div>${toc}</div>`;
}

