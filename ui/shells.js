// ---------- Cue's own shells: the floating terminal ----------
/** The terminal button by the usage meter (or ⌘J) brings out a terminal of Cue's own, tied to no
 *  session: plain shells (your login shell, in a folder you pick; shells.rs), one tab each. It floats
 *  over the board's last two columns, the bottom 35% of them (the top, dragged up; let go, it springs
 *  back to one or the other, as picture in picture does). Tucked away it shrinks to a small tab on the right edge, at its corner, and comes back
 *  from there; the button and that tab show a live dot while a shell runs something, and ✓ once that
 *  ends. Its element lives outside the redraws (each puts it back over the columns). Where it is, top or
 *  bottom and tucked or not, is kept in this browser. */
const shells = [];             // { id, title, cwd, busy, what, gen, term, fit, host, queue, frame }
let shellCur = null;           // the tab on show
let shellSeq = 0;
let pipEl = null;
let pipAt = { corner: "br", tucked: false };
try { Object.assign(pipAt, JSON.parse(localStorage.getItem("cue.pip") || "{}")); } catch {}
let pipDone = false;           // a command ended while it was away (until it's back out)
const pipOut = () => !!pipEl && !pipEl.hidden && !pipAt.tucked;
const savePip = () => { try { localStorage.setItem("cue.pip", JSON.stringify(pipAt)); } catch {} };
const shellOf = (id) => shells.find((s) => s.id === id);

/** While it's away: "run" (a shell runs something), "done" (something ended), or "". */
const shellsLive = () => pipOut() ? "" : shells.some((s) => s.busy) ? "run" : pipDone ? "done" : "";
/** In the header, right after the usage meter: on while it's out; away, a live dot or ✓. */
function shellsBtn() {
  const live = shellsLive();
  return `<button class="top-btn icon shells-btn" data-act="shells" aria-pressed="${pipOut()}" aria-label="Terminal (⌘J)">${TERM_ICON.replace('width="14" height="14"', 'width="16" height="16"')}<span class="sh-live ${live}">${live === "done" ? "✓" : ""}</span></button>`;
}
/** The button and the tucked edge, updated in place (no redraw). */
function markShells() {
  const live = shellsLive();
  for (const el of document.querySelectorAll(".shells-btn .sh-live, .pip-peek .sh-live")) {
    el.className = `sh-live ${live}`;
    el.textContent = live === "done" ? "✓" : "";
  }
  document.querySelector(".shells-btn")?.setAttribute("aria-pressed", String(pipOut()));
}

/** The button: a first shell (at home); else out, or tucked away. */
function toggleShells() {
  if (!shells.length) return openShell("");
  if (pipEl.hidden) { pipEl.hidden = false; return tuckPip(false); }
  tuckPip(!pipAt.tucked);
}

function ensurePip() {
  if (pipEl) return;
  pipEl = document.createElement("section");
  pipEl.className = "pip";
  pipEl.hidden = true;
  pipEl.setAttribute("aria-label", "Terminal");
  pipEl.innerHTML = `<div class="pip-bar"><div class="pip-tabs" role="tablist"></div><button class="pip-add" aria-label="New shell">+</button><button class="pip-hide" aria-label="Tuck it away (⌘J)">⇥</button></div>
    <div class="pip-slot"></div>
    <button class="pip-peek" aria-label="Bring the terminal back"><span aria-hidden="true">‹</span><span class="sh-live"></span></button>`;
  document.body.appendChild(pipEl);
  pipEl.addEventListener("click", (e) => {
    const x = e.target.closest("[data-shell-x]");
    if (x) return closeShell(x.dataset.shellX);
    const t = e.target.closest("[data-shell]");
    if (t) return showShell(t.dataset.shell);
    if (e.target.closest(".pip-add")) return folderMenu(e.target.closest(".pip-add"));
    if (e.target.closest(".pip-hide")) return tuckPip(true);
    if (e.target.closest(".pip-peek")) return tuckPip(false);
  });
  pipEl.querySelector(".pip-bar").addEventListener("pointerdown", dragPip);
  pipEl.addEventListener("transitionend", (e) => { if (e.propertyName === "left" || e.propertyName === "top") fitShell(); });
  window.addEventListener("resize", placePip);
}

/** A new shell in `cwd` (home when empty), its tab on show and the terminal out. */
async function openShell(cwd) {
  ensurePip();
  const id = `shell:${Date.now().toString(36)}${++shellSeq}`;
  const host = document.createElement("div");
  host.className = "pip-host";
  pipEl.querySelector(".pip-slot").appendChild(host);
  // Out, and its element on show, before its terminal is made: it's sized from where it sits.
  pipEl.hidden = false;
  pipAt.tucked = false;
  placePip();
  for (const s of shells) s.host.hidden = true;
  const { term, fit } = xterm(host);
  const sh = { id, cwd, title: baseName(cwd) || "~", busy: false, what: "", gen: 0, term, fit, host, queue: [], frame: 0 };
  shells.push(sh);
  showShell(id);
  term.onData((d) => invoke("term_write", { sessionId: id, data: d }).catch(() => {}));
  term.onResize(({ cols, rows }) => invoke("term_resize", { sessionId: id, cols, rows }).catch(() => {}));
  // Its title, as the shell sets it ("me@mac: ~/code/x"): the folder it's in now, for its tab.
  term.onTitleChange((t) => { const where = String(t).split(": ").pop(); if (where) { sh.title = baseName(where) || where; drawShellTabs(); } });
  term.focus();
  try {
    const gen = await invoke("shell_open", { id, cwd, cols: term.cols, rows: term.rows });
    if (shellOf(id)) sh.gen = gen;
  } catch (e) {
    toast(`Couldn't open a shell: ${e}`);
    closeShell(id, true);
  }
}

/** Its output ("term" events with its id), written once a frame; the end of it (the shell exited) closes its tab. */
function onShellOutput(p) {
  const sh = shellOf(p.sid);
  if (!sh || (sh.gen && p.gen !== sh.gen)) return;
  if (p.end) return closeShell(sh.id, true);
  sh.queue.push(p.data);
  if (!sh.frame) sh.frame = requestAnimationFrame(() => {
    sh.frame = 0;
    sh.term.write(b64Bytes(sh.queue));
    sh.queue = [];
  });
}

/** What a shell runs ("shell" events): its tab says it; one that ends while the terminal is away leaves a ✓.
 *  SHORTCUT: ✓ only, never ✗: Cue sees the foreground process come and go, not its exit code. A ✗ needs
 *  the shell to say it (an OSC 133 mark from your prompt, as some terminals' shell integration has). */
function onShellEvent(p) {
  const sh = shellOf(p.id);
  if (!sh) return;
  if (sh.busy && !p.busy && !pipOut()) pipDone = true;
  sh.busy = !!p.busy;
  sh.what = p.what || "";
  drawShellTabs();
  markShells();
}

function showShell(id) {
  shellCur = id;
  for (const s of shells) s.host.hidden = s.id !== id;
  drawShellTabs();
  fitShell();
  if (pipOut()) shellOf(id)?.term.focus();
}

/** Close a shell (`ended`: it already exited). The last one closed puts the terminal away. */
function closeShell(id, ended = false) {
  const i = shells.findIndex((s) => s.id === id);
  if (i < 0) return;
  const [sh] = shells.splice(i, 1);
  if (!ended) invoke("term_close", { sessionId: id }).catch(() => {});
  cancelAnimationFrame(sh.frame);
  sh.term.dispose();
  sh.host.remove();
  if (!shells.length) { shellCur = null; pipEl.hidden = true; pipDone = false; drawShellTabs(); return markShells(); }
  if (shellCur === id) showShell((shells[i] || shells[i - 1]).id);
  else drawShellTabs();
  markShells();
}

function drawShellTabs() {
  const el = pipEl?.querySelector(".pip-tabs");
  if (!el) return;
  el.innerHTML = shells.map((s) => `<button class="pip-tab" role="tab" data-shell="${esc(s.id)}" aria-selected="${s.id === shellCur}">${s.busy ? `<span class="sh-dot"></span>` : ""}<span class="pip-name ${s.busy ? "" : "mono"}">${esc(s.busy ? s.what || "running" : s.title)}</span><span class="pip-x" data-shell-x="${esc(s.id)}" role="button" aria-label="Close this shell">×</span></button>`).join("");
}

const fitShell = () => { if (pipOut()) try { shellOf(shellCur)?.fit.fit(); } catch {} };

/** Where a new shell can start: home, the open session's folder, then the folders your sessions here are in. */
function folderMenu(anchor) {
  document.querySelector(".pip-menu")?.remove();
  const open = current()?.s;
  const local = (x) => x?.cwd && !x.machine;
  const seen = new Set(), folders = [];
  for (const f of [local(open) ? open.cwd : null, ...state.sessions.filter(local).map((s) => s.cwd), ...(state.live || []).filter(local).map((q) => q.cwd)]) if (f && !seen.has(f)) { seen.add(f); folders.push(f); }
  const m = document.createElement("div");
  m.className = "pip-menu";
  m.setAttribute("role", "menu");
  m.innerHTML = `<div class="pip-menu-h">New shell in</div><button role="menuitem" data-cwd="">Home <span class="mono">~</span></button>`
    + folders.slice(0, 10).map((f) => `<button role="menuitem" data-cwd="${esc(f)}">${esc(baseName(f))} <span class="mono">${esc(homeless(f))}</span></button>`).join("");
  document.body.appendChild(m);
  const r = anchor.getBoundingClientRect();
  m.style.left = `${Math.max(8, Math.min(r.left, innerWidth - m.offsetWidth - 8))}px`;
  m.style.top = r.bottom + 6 + m.offsetHeight < innerHeight ? `${r.bottom + 6}px` : `${Math.max(8, r.top - m.offsetHeight - 6)}px`;
  const done = () => { m.remove(); document.removeEventListener("pointerdown", away, true); document.removeEventListener("keydown", key, true); };
  // (Opened by a click: its pointerdown has passed, so these see only what comes after.)
  const away = (e) => { if (!m.contains(e.target)) done(); };
  const key = (e) => { if (e.key === "Escape") { e.stopPropagation(); done(); } };
  document.addEventListener("pointerdown", away, true);
  document.addEventListener("keydown", key, true);
  m.addEventListener("click", (e) => { const b = e.target.closest("[data-cwd]"); if (b) { done(); openShell(b.dataset.cwd); } });
  m.querySelector("button")?.focus();
}

// ---------- where it floats ----------
/** Where it sits when out: over the board's last two columns (Waiting and Sessions), the bottom 35% of
 *  them, or the top 35% once dragged up. In a view without those columns, the window's right 45% under
 *  the header, the same height. Its size follows them: the window's, or the board's, changing moves it. */
const PIP_SHARE = 0.35;
function pipRoom() {
  const cols = [...document.querySelectorAll(".board > .col")];
  if (cols.length >= 3) {
    const b = document.querySelector(".board").getBoundingClientRect(), l = cols[cols.length - 2].getBoundingClientRect(), r = cols[cols.length - 1].getBoundingClientRect();
    return { left: l.left, right: r.right, top: b.top, bottom: b.bottom };
  }
  const top = (document.querySelector(".top")?.getBoundingClientRect().bottom ?? 60) + 14, a = document.querySelector(".app");
  const pad = a ? parseFloat(getComputedStyle(a).paddingRight) || 20 : 20, bottom = innerHeight - (a ? parseFloat(getComputedStyle(a).paddingBottom) || 16 : 16);
  return { left: innerWidth - pad - innerWidth * 0.45, right: innerWidth - pad, top, bottom };
}
/** Tucked away: a small tab, this wide and tall, on the right edge at its corner. */
const PIP_TAB = { w: 30, h: 76 };
function placePip() {
  if (!pipEl || pipEl.hidden) return;
  const r = pipRoom();
  pipEl.dataset.at = pipAt.corner;
  pipEl.classList.toggle("tucked", pipAt.tucked);
  // (Its shells keep their size tucked away: only one that's out is fitted to it.)
  const w = pipAt.tucked ? PIP_TAB.w : r.right - r.left, h = pipAt.tucked ? PIP_TAB.h : Math.max(160, Math.round((r.bottom - r.top) * PIP_SHARE));
  Object.assign(pipEl.style, {
    width: `${w}px`, height: `${h}px`,
    left: `${pipAt.tucked ? innerWidth - w : r.left}px`,
    top: `${pipAt.corner === "tr" ? r.top : r.bottom - h}px`,
  });
  fitShell();
}
/** Tucked into the right edge, or back out at its corner (where what ended while it was away is seen). */
function tuckPip(t) {
  pipAt.tucked = t;
  if (!t) pipDone = false;
  savePip();
  placePip();
  if (t) { document.activeElement?.blur(); focusComposer(); } else shellOf(shellCur)?.term.focus();
  markShells();
}
/** Dragged by its bar (a press that doesn't move stays a click on the tab, +, ⇥ under it). Let go past
 *  the right edge (a third of it beyond) and it tucks away; else it springs back over the columns, to
 *  their top or bottom, whichever is nearer. */
function dragPip(e) {
  if (e.button !== 0 || e.target.closest(".pip-x")) return;
  const x0 = e.clientX, y0 = e.clientY, r0 = pipEl.getBoundingClientRect();
  let moved = false;
  const move = (m) => {
    if (!moved && Math.hypot(m.clientX - x0, m.clientY - y0) < 5) return;
    if (!moved) { moved = true; pipEl.classList.add("drag"); document.body.classList.add("pip-moving"); }
    pipEl.style.left = `${r0.left + m.clientX - x0}px`;
    pipEl.style.top = `${r0.top + m.clientY - y0}px`;
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    if (!moved) return;
    // (the click that ends a drag picks nothing)
    window.addEventListener("click", (c) => { c.stopPropagation(); c.preventDefault(); }, { capture: true, once: true });
    pipEl.classList.remove("drag");
    document.body.classList.remove("pip-moving");
    const b = pipEl.getBoundingClientRect(), r = pipRoom();
    if (b.right > innerWidth + b.width / 3) return tuckPip(true);
    pipAt.corner = b.top + b.height / 2 < (r.top + r.bottom) / 2 ? "tr" : "br";
    pipAt.tucked = false;
    savePip();
    placePip();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up, { once: true });
}
