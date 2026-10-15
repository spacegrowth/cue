// ---------- Cue's own shells: the floating terminal ----------
/** The terminal button by the usage meter (or ⌘J) brings out a terminal of Cue's own, tied to no
 *  session: plain shells (your login shell, at home; shells.rs), one tab each, named "mac: ~" (the host,
 *  and the folder it's in now) until you rename one (double-click its tab). It floats
 *  over the board's last two columns, at their foot (or head, dragged up; let go, it springs back to one
 *  or the other, as picture in picture does), 45% of their height to start, and resizes from its free
 *  edges. With tmux, each tab's shell runs in a tmux session of its own: quitting or restarting Cue lets
 *  go of it, and the next Cue gives each its tab back (restoreShells), in order, with its name, the
 *  one on show the same. Tucked away it shrinks to a small tab on the right edge, at its corner, and comes back
 *  from there; the button and that tab show a live dot while a shell runs something, and ✓ once that
 *  ends. + or ⌘⇧T opens a new shell tab. Full screen (its ⤢, or a double-click on its bar) it fills the window under the header. Its
 *  element lives outside the redraws (each puts it back over the columns). Where it is, top or bottom,
 *  full or not and tucked or not, is kept in this browser. */
const shells = [];             // { id, where, mac, name, cwd, busy, what, kept, gen, term, fit, host, queue, frame }
let shellCur = null;           // the tab on show
let shellMac = "";             // this Mac's short host name (from the first shell_open)
let shellRenaming = null;      // the tab whose name is being typed
let shellSeq = 0;
let pipEl = null;
let pipAt = { corner: "br", tucked: false, full: false, share: 0.45, wide: 1 };
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

/** ⌘⇧T: a new shell tab (at home), the terminal out, as ⌘T is in a terminal app. */
const newShellTab = () => openShell("");
/** ⌘K inside it: the tab on show cleared, its prompt line the first. */
const clearShell = () => {
  const sh = shellOf(shellCur);
  if (!sh?.term) return;
  // In tmux, tmux clears it (and redraws the tab); else the tab's own terminal.
  if (sh.kept) invoke("shell_clear", { id: sh.id }).catch(() => sh.term.clear());
  else sh.term.clear();
};
/** ⌘W (the menu's Close, lib.rs app_menu): inside it, the tab you're in closes (the last one puts it
 *  away); anywhere else the window hides, as it always did. */
function closeKey() {
  if (shells.length && pipOut() && pipEl.contains(document.activeElement)) return closeShell(shellCur);
  invoke("hide_window").catch(() => {});
}
/** ⌘1–9 inside it: that tab (⌘9 the last, as in a terminal app). */
const shellNumber = (n) => { const s = n === 9 ? shells.at(-1) : shells[n - 1]; if (s) showShell(s.id); };

/** The button: a first shell (at home); else out, or tucked away. ⌘J (`key`) first takes you into it when
 *  it's out but you're elsewhere in Cue; pressed again from inside, it tucks it away. */
function toggleShells(key = false) {
  if (!shells.length) return openShell("");
  if (pipEl.hidden) { pipEl.hidden = false; return tuckPip(false); }
  if (key && pipOut() && !pipEl.contains(document.activeElement)) return shellOf(shellCur)?.term?.focus();
  tuckPip(!pipAt.tucked);
}

function ensurePip() {
  if (pipEl) return;
  pipEl = document.createElement("section");
  pipEl.className = "pip";
  pipEl.hidden = true;
  pipEl.setAttribute("aria-label", "Terminal");
  pipEl.innerHTML = `<div class="pip-bar"><div class="pip-tabs" role="tablist"></div><button class="pip-add" aria-label="New shell (⌘⇧T)">+</button><button class="pip-full" aria-label="Full screen" aria-pressed="false"></button><button class="pip-hide" aria-label="Tuck it away (⌘J)">⇥</button></div>
    <div class="pip-slot"></div><div class="pip-grip edge-x" data-grip="x"></div><div class="pip-grip edge-y" data-grip="y"></div><div class="pip-grip corner" data-grip="xy"></div>
    <button class="pip-peek" aria-label="Bring the terminal back"><span aria-hidden="true">‹</span><span class="sh-live"></span></button>`;
  document.body.appendChild(pipEl);
  pipEl.addEventListener("click", (e) => {
    const x = e.target.closest("[data-shell-x]");
    if (x) return closeShell(x.dataset.shellX);
    const t = e.target.closest("[data-shell]");
    if (t && !e.target.closest(".pip-rename")) return showShell(t.dataset.shell);
    if (e.target.closest(".pip-add")) return openShell("");
    if (e.target.closest(".pip-full")) return fullPip(!pipAt.full);
    if (e.target.closest(".pip-hide")) return tuckPip(true);
    if (e.target.closest(".pip-peek")) return tuckPip(false);
  });
  pipEl.querySelector(".pip-bar").addEventListener("pointerdown", dragPip);
  // A double-click on a tab names it.
  pipEl.querySelector(".pip-tabs").addEventListener("dblclick", (e) => { const t = e.target.closest("[data-shell]"); if (t) renameShell(t.dataset.shell); });
  // A double-click on the bar itself (not a tab or a button) goes full screen and back, as a window's title bar zooms.
  pipEl.querySelector(".pip-bar").addEventListener("dblclick", (e) => { if (!e.target.closest("button, .pip-tab")) fullPip(!pipAt.full); });
  pipEl.querySelectorAll("[data-grip]").forEach((g) => g.addEventListener("pointerdown", sizePip));
  pipEl.addEventListener("transitionend", (e) => { if (e.propertyName === "left" || e.propertyName === "top") fitShell(); });
  window.addEventListener("resize", placePip);
}

/** A new shell in `cwd` (home when empty), its tab on show and the terminal out. */
function openShell(cwd) {
  ensurePip();
  const sh = addShell({ id: `shell:${Date.now().toString(36)}${++shellSeq}`, cwd });
  // Out before its terminal is made: it's sized from where it sits.
  pipEl.hidden = false;
  pipAt.tucked = false;
  placePip();
  showShell(sh.id);
}

/** A tab for shell `id`. Its terminal is made, and the shell started or attached again, when the tab is
 *  first on show with the terminal out (attachShell): made tucked away, it would be sized to the tab. */
function addShell({ id, cwd = "", name = "", kept = false }) {
  const host = document.createElement("div");
  host.className = "pip-host";
  host.hidden = true;
  pipEl.querySelector(".pip-slot").appendChild(host);
  const sh = { id, cwd, where: baseName(cwd) || "~", mac: shellMac, name, kept, busy: false, what: "", gen: 0, term: null, fit: null, host, queue: [], frame: 0 };
  shells.push(sh);
  return sh;
}

/** Its terminal, and its shell opened in it (in tmux, a running one is attached again, as it was). */
async function attachShell(sh) {
  const { term, fit } = xterm(sh.host);
  sh.term = term;
  sh.fit = fit;
  const id = sh.id;
  term.onData((d) => invoke("term_write", { sessionId: id, data: d }).catch(() => {}));
  term.onResize(({ cols, rows }) => invoke("term_resize", { sessionId: id, cols, rows }).catch(() => {}));
  // Its title, as the shell sets it ("me@mac: ~/code/x"): the host and the folder it's in now, for its tab.
  term.onTitleChange((t) => {
    const [who, where] = String(t).includes(": ") ? String(t).split(/: (.*)/) : ["", String(t)];
    if (where) sh.where = baseName(where) || where;
    if (who.includes("@")) sh.mac = who.split("@").pop().split(".")[0];
    drawShellTabs();
  });
  try {
    const r = await invoke("shell_open", { id, cwd: sh.cwd, cols: term.cols, rows: term.rows });
    shellMac ||= r.host || "";
    if (shellOf(id)) { sh.gen = r.gen; sh.kept = !!r.kept; sh.mac ||= shellMac; drawShellTabs(); }
  } catch (e) {
    toast(`Couldn't open a shell: ${e}`);
    closeShell(id, true);
  }
}

/** At launch: the shells still running in tmux, each back in its tab, as they were (the terminal out
 *  or tucked away as it was, the tab on show the same). Only the one on show is attached, once it's out. */
async function restoreShells() {
  let r;
  try { r = await invoke("shell_list"); } catch { return; }
  const list = r?.shells || [];
  if (!list.length || shells.length) return;
  shellMac ||= r.host || "";
  ensurePip();
  for (const x of list) addShell({ id: x.id, cwd: x.cwd || "", name: x.name || "", kept: true });
  let cur = null;
  try { cur = localStorage.getItem("cue.shellCur"); } catch {}
  pipEl.hidden = false;
  placePip();
  showShell(shellOf(cur) ? cur : shells.at(-1).id, false);
  markShells();
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

/** Tab `id` on show (`focus`: the keyboard into it, when the terminal is out). */
function showShell(id, focus = true) {
  if (shellCur === id && shellRenaming !== id) return shellOf(id)?.term?.focus();
  shellCur = id;
  try { localStorage.setItem("cue.shellCur", id); } catch {}
  for (const s of shells) s.host.hidden = s.id !== id;
  drawShellTabs();
  fitShell();
  if (pipOut() && focus) shellOf(id)?.term?.focus();
}

/** Close a shell (`ended`: it already exited). The last one closed puts the terminal away. */
function closeShell(id, ended = false) {
  const i = shells.findIndex((s) => s.id === id);
  if (i < 0) return;
  const [sh] = shells.splice(i, 1);
  if (!ended) invoke("term_close", { sessionId: id }).catch(() => {});
  cancelAnimationFrame(sh.frame);
  sh.term?.dispose();
  sh.host.remove();
  // The last one closed: the terminal goes away, and the cursor to the message box (homeFocus).
  if (!shells.length) { shellCur = null; pipEl.hidden = true; pipDone = false; drawShellTabs(); markShells(); return homeFocus(); }
  if (shellCur === id) showShell((shells[i] || shells[i - 1]).id);
  else drawShellTabs();
  markShells();
}

/** A tab's name: yours, else its host and folder ("mac: ~"). */
const shellName = (s) => s.name || (s.mac ? `${s.mac}: ${s.where}` : s.where);
function drawShellTabs() {
  const el = pipEl?.querySelector(".pip-tabs");
  if (!el) return;
  // (Not while a name is typed: the field would go, and what was typed with it.)
  if (shellRenaming && el.querySelector(".pip-rename")) return;
  el.innerHTML = shells.map((s, i) => {
    const name = s.id === shellRenaming ? `<input class="pip-rename" value="${esc(shellName(s))}" aria-label="Name this tab" spellcheck="false" />`
      : `<span class="pip-name ${s.busy || s.name ? "" : "mono"}">${esc(s.busy ? s.what || "running" : shellName(s))}</span>`;
    // (The one being named isn't a button: a field inside a button doesn't take clicks or keys in WebKit.)
    const tag = s.id === shellRenaming ? "span" : "button";
    return `<${tag} class="pip-tab" role="tab" data-shell="${esc(s.id)}" aria-selected="${s.id === shellCur}">${s.busy ? `<span class="sh-dot"></span>` : ""}${name}${shells.length > 1 && i < 9 ? `<span class="pip-key">⌘${i + 1}</span>` : ""}<span class="pip-x" data-shell-x="${esc(s.id)}" role="button" aria-label="Close this shell">×</span></${tag}>`;
  }).join("");
  const field = el.querySelector(".pip-rename");
  if (!field) return;
  field.focus();
  field.select();
  // Enter keeps it (empty: back to its host and folder), Esc or clicking away leaves it as it was.
  // (A shell in tmux keeps its name with it, for the next Cue.)
  const done = (keep) => {
    if (shellRenaming === null) return;
    const sh = shellOf(shellRenaming);
    shellRenaming = null;
    if (keep && sh) { sh.name = field.value.trim(); invoke("shell_name", { id: sh.id, name: sh.name }).catch(() => {}); }
    drawShellTabs();
    sh?.term?.focus();
  };
  field.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === "Escape") { e.preventDefault(); e.stopPropagation(); done(e.key === "Enter"); } });
  field.addEventListener("blur", () => done(false));
}
/** Name a tab: its name becomes a field in place. */
function renameShell(id) {
  if (!shellOf(id)) return;
  shellRenaming = id;
  drawShellTabs();
}

/** The tab on show fitted to the terminal, while it's out; its terminal made first, the first time. */
function fitShell() {
  if (!pipOut()) return;
  const sh = shellOf(shellCur);
  if (!sh) return;
  if (!sh.term) return attachShell(sh);
  try { sh.fit.fit(); } catch {}
}

// ---------- where it floats ----------
/** Where it sits when out: over the board's last two columns (Waiting and Sessions), at their foot (or
 *  their head, dragged up), 45% of their height to start. In a view without those columns, the window's
 *  right 45% under the header. Resized, it keeps its size as a share of them (`share` of their height,
 *  `wide` times their width), so it follows them as the window or the board changes. */
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
/** Tucked away: a small tab, this wide and tall, on the right edge at its corner. Its left 6px are
 *  hidden until it's pointed at (style.css): it grows out of the edge, its right side never leaving it. */
const PIP_TAB = { w: 36, h: 76 };
function placePip() {
  if (!pipEl || pipEl.hidden) return;
  const r = pipRoom();
  pipEl.dataset.at = pipAt.corner;
  pipEl.classList.toggle("tucked", pipAt.tucked);
  pipEl.classList.toggle("full", pipAt.full && !pipAt.tucked);
  const fb = pipEl.querySelector(".pip-full");
  fb.innerHTML = pipAt.full ? SHRINK_ICON : GROW_ICON;
  fb.setAttribute("aria-pressed", String(pipAt.full));
  fb.setAttribute("aria-label", pipAt.full ? "Leave full screen" : "Full screen");
  if (pipAt.full && !pipAt.tucked) {
    const w = pipWhole();
    Object.assign(pipEl.style, { left: `${w.left}px`, top: `${w.top}px`, width: `${w.right - w.left}px`, height: `${w.bottom - w.top}px` });
    return fitShell();
  }
  // (Its shells keep their size tucked away: only one that's out is fitted to it.)
  const full = pipFull(r);
  const w = pipAt.tucked ? PIP_TAB.w : Math.round(Math.max(360, Math.min(full.w, (r.right - r.left) * pipAt.wide)));
  const h = pipAt.tucked ? PIP_TAB.h : Math.round(Math.max(160, Math.min(full.h, (r.bottom - r.top) * pipAt.share)));
  Object.assign(pipEl.style, {
    width: `${w}px`, height: `${h}px`,
    left: `${pipAt.tucked ? innerWidth - w : r.right - w}px`,
    top: `${pipAt.corner === "tr" ? r.top : r.bottom - h}px`,
  });
  fitShell();
}
/** Full screen: the window under the header, inside the app's padding. */
function pipWhole() {
  const a = document.querySelector(".app"), cs = a ? getComputedStyle(a) : null, px = (k, d) => (cs && parseFloat(cs[k])) || d;
  return { left: px("paddingLeft", 20), right: innerWidth - px("paddingRight", 20), top: (document.querySelector(".top")?.getBoundingClientRect().bottom ?? 60) + 14, bottom: innerHeight - px("paddingBottom", 16) };
}
const GROW_ICON = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 2h4v4M6 14H2v-4M14 2 9.5 6.5M2 14l4.5-4.5"/></svg>`;
const SHRINK_ICON = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M13.5 6.5h-4v-4M2.5 9.5h4v4M9.5 6.5 14 2M6.5 9.5 2 14"/></svg>`;
/** Full screen or back to its corner (its size there kept). */
function fullPip(f) {
  pipAt.full = f;
  savePip();
  // In one step: its place would glide while its size jumped.
  pipEl.classList.add("sizing");
  placePip();
  requestAnimationFrame(() => requestAnimationFrame(() => pipEl.classList.remove("sizing")));
  shellOf(shellCur)?.term?.focus();
}
/** The most it can be: as wide as from the window's left padding to the columns' right, as tall as they are. */
const pipFull = (r) => ({ w: r.right - (parseFloat(getComputedStyle(document.querySelector(".app") || document.body).paddingLeft) || 20), h: r.bottom - r.top });
/** Resized by a grip: its left edge (x), the edge away from its corner (y), or the corner between (xy).
 *  It stays at its corner, so it grows left and away from it; kept as a share of the columns. */
function sizePip(e) {
  const grip = e.currentTarget, kind = grip.dataset.grip;
  grip.setPointerCapture(e.pointerId);
  pipEl.classList.add("sizing");
  document.body.classList.add("pip-moving");
  const x0 = e.clientX, y0 = e.clientY, b0 = pipEl.getBoundingClientRect();
  const move = (m) => {
    const r = pipRoom(), full = pipFull(r);
    if (kind.includes("x")) pipAt.wide = Math.max(360, Math.min(full.w, b0.width + x0 - m.clientX)) / (r.right - r.left);
    if (kind.includes("y")) pipAt.share = Math.max(160, Math.min(full.h, b0.height + (pipAt.corner === "br" ? y0 - m.clientY : m.clientY - y0))) / (r.bottom - r.top);
    placePip();
  };
  grip.addEventListener("pointermove", move);
  grip.addEventListener("pointerup", () => { grip.removeEventListener("pointermove", move); pipEl.classList.remove("sizing"); document.body.classList.remove("pip-moving"); savePip(); }, { once: true });
}
/** Tucked into the right edge, or back out at its corner (where what ended while it was away is seen). */
function tuckPip(t) {
  pipAt.tucked = t;
  if (!t) pipDone = false;
  savePip();
  placePip();
  if (t) { document.activeElement?.blur(); focusComposer(); } else shellOf(shellCur)?.term?.focus();
  markShells();
}
/** Dragged by its bar (a press that doesn't move stays a click on the tab, +, ⇥ under it). Let go past
 *  the right edge (a third of it beyond) and it tucks away; else it springs back over the columns, to
 *  their top or bottom, whichever is nearer. */
function dragPip(e) {
  if (e.button !== 0 || e.target.closest(".pip-x, .pip-rename") || pipAt.full) return;
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
