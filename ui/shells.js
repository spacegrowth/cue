// ---------- Cue's own shells: the terminal peek and the Terminal view ----------
/** A terminal of Cue's own, tied to no session: plain shells (your login shell, at home; shells.rs), one tab
 *  each, named "mac: ~" (the host, and the folder it's in now) until you rename one (double-click its tab).
 *  Two places show the same shells and tabs: the peek, dropping down under the terminal button in the header
 *  (point at the button for a look; click it, or ⌘J, to type in it; Esc, ⌘J again, ⇥ or a click anywhere else
 *  puts it away; Esc stays the shell's while vim, ssh or the like is in front), and the Terminal view in the header's switch, filling the window (syncShellView). While it's
 *  away the button (and the switch's Terminal) shows a live dot while a shell runs something, and ✓ once that
 *  ends. + or ⌘T opens a new shell tab. With tmux, each tab's shell runs in a tmux session of its own:
 *  quitting or restarting Cue lets go of it, and the next Cue gives each its tab back (restoreShells), in
 *  order, with its name, the one on show the same. Its element lives outside the redraws (each puts it back
 *  under the button). */
const shells = [];             // { id, where, mac, name, cwd, busy, what, kept, gen, term, fit, host, queue, frame }
let shellCur = null;           // the tab on show
let shellMac = "";             // this Mac's short host name (from the first shell_open)
let shellRenaming = null;      // the tab whose name is being typed
let shellSeq = 0;
let pipEl = null;
let peek = null;               // the peek: null (away), "glance" (pointed at: a look, with the keyboard) or "open" (clicked, ⌘J or typed in: it stays)
let pipDone = false;           // a command ended while it was away (until it's back out)
let shellViewOn = false;       // the Terminal view is on show (syncShellView)
const pipOut = () => !!pipEl && !pipEl.hidden;
/** Esc is the shell's, not the peek's: in the Terminal view, or with something in front you sit inside (vim,
 *  less, ssh…: named but no job; shells.rs interactive), which needs Esc itself. At a prompt or while a job
 *  runs, Esc puts the peek away. */
const escToShell = (sh) => shellViewOn || peek !== "open" || (!!sh.what && !sh.busy);
/** The peek's hint, as Esc would act now. */
function peekHint() {
  const el = pipEl?.querySelector(".pip-hint"), sh = shellOf(shellCur);
  if (el) el.textContent = shellViewOn ? "" : peek === "glance" ? "Type, or click to keep it" : sh && escToShell(sh) ? "⌘J to put away" : "Esc to put away";
}
/** You have it in front of you: the peek open, or the Terminal view. A look (pointing at the button) isn't:
 *  the button keeps its dot and its look while you point at it, so nothing on it moves. */
const pipSeen = () => shellViewOn || peek === "open";
const shellOf = (id) => shells.find((s) => s.id === id);

/** While it's away: "run" (a shell runs something), "done" (something ended), or "". */
const shellsLive = () => pipSeen() ? "" : shells.some((s) => s.busy) ? "run" : pipDone ? "done" : "";
/** In the header, right after the usage meter: pressed while the peek is open; away, a live dot or ✓. */
function shellsBtn() {
  const live = shellsLive();
  return `<button class="top-btn icon shells-btn" data-act="shells" aria-pressed="${pipSeen()}" aria-label="Terminal (⌘J)">${TERM_ICON.replace('width="14" height="14"', 'width="16" height="16"')}<span class="sh-live ${live}">${live === "done" ? "✓" : ""}</span></button>`;
}
/** The button and the tucked edge, updated in place (no redraw). */
function markShells() {
  const live = shellsLive();
  for (const el of document.querySelectorAll(".shells-btn .sh-live, .sv-term .sh-live")) {
    el.className = `sh-live ${live}`;
    el.textContent = live === "done" ? "✓" : "";
  }
  document.querySelector(".shells-btn")?.setAttribute("aria-pressed", String(pipSeen()));
}

/** ⌘T: a new shell tab (at home), the terminal out, as in a terminal app. */
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

/** The button: a first shell (at home); else the peek, open to type in, or away again. ⌘J (`key`) first takes
 *  you into it when it's open but you're elsewhere in Cue; pressed again from inside, it puts it away. */
function toggleShells(key = false) {
  if (!shells.length) return openShell("");
  if (shellViewOn) return shellOf(shellCur)?.term?.focus();   // the Terminal view: it's out already
  if (peek === "open" && key && !pipEl.contains(document.activeElement)) return shellOf(shellCur)?.term?.focus();
  setPeek(peek === "open" ? null : "open");
}

function ensurePip() {
  if (pipEl) return;
  pipEl = document.createElement("section");
  pipEl.className = "pip";
  pipEl.hidden = true;
  pipEl.setAttribute("aria-label", "Terminal");
  pipEl.innerHTML = `<div class="pip-bar"><div class="pip-tabs" role="tablist"></div><button class="pip-add" aria-label="New shell (⌘T)">+</button><span class="pip-hint"></span><button class="pip-view" aria-label="Open in the Terminal view">${GROW_ICON}</button><button class="pip-hide" aria-label="Put it away (⌘J)">⇥</button></div>
    <div class="pip-slot"></div>`;
  document.body.appendChild(pipEl);
  pipEl.addEventListener("click", (e) => {
    const x = e.target.closest("[data-shell-x]");
    if (x) return closeShell(x.dataset.shellX);
    const t = e.target.closest("[data-shell]");
    if (t && !e.target.closest(".pip-rename")) return showShell(t.dataset.shell);
    if (e.target.closest(".pip-add")) return openShell("");
    if (e.target.closest(".pip-view")) { setPeek(null); view = "terminal"; return renderMain(); }
    if (e.target.closest(".pip-hide")) return setPeek(null);
  });
  // A look becomes the real thing as soon as you press or type in it (it stays when the pointer leaves).
  pipEl.addEventListener("pointerdown", () => { if (peek === "glance") setPeek("open"); });
  pipEl.addEventListener("keydown", () => { if (peek === "glance") setPeek("open"); }, true);
  // A double-click on a tab names it.
  pipEl.querySelector(".pip-tabs").addEventListener("dblclick", (e) => { const t = e.target.closest("[data-shell]"); if (t) renameShell(t.dataset.shell); });
  // More tabs than fit: the row scrolls sideways (a mouse wheel too, not only a trackpad), and fades at
  // an end with more tabs past it, instead of cutting them off unseen.
  const strip = pipEl.querySelector(".pip-tabs");
  strip.addEventListener("wheel", (e) => {
    if (strip.scrollWidth <= strip.clientWidth || Math.abs(e.deltaX) >= Math.abs(e.deltaY)) return;
    strip.scrollLeft += e.deltaY;
    e.preventDefault();
  }, { passive: false });
  strip.addEventListener("scroll", () => markTabEnds(strip), { passive: true });
  window.addEventListener("resize", placePip);
}

/** A new shell in `cwd` (home when empty), its tab on show and the terminal out. `action` ({folder, machine, id}):
 *  instead of a shell, that action's own output, attached where it runs (its tab again if it's open); closing the
 *  tab leaves it running. `name`: its tab's. */
function openShell(cwd, { name = "", action = null } = {}) {
  ensurePip();
  const open = action && shells.find((x) => x.action && x.action.id === action.id && x.action.folder === action.folder && x.action.machine === action.machine);
  const sh = open || addShell({ id: `shell:${action ? "act-" : ""}${Date.now().toString(36)}${++shellSeq}`, cwd, name });
  sh.action = action;
  // Out before its terminal is made: it's sized from where it sits (the peek, open, or the Terminal view).
  if (!shellViewOn) peek = "open";
  pipEl.hidden = false;
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
  // Esc puts the peek away, unless what's in front needs it (escToShell).
  term.attachCustomKeyEventHandler((e) => {
    if (e.type !== "keydown" || e.key !== "Escape" || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || escToShell(sh)) return true;
    setPeek(null);
    return false;
  });
  term.onResize(({ cols, rows }) => invoke("term_resize", { sessionId: id, cols, rows }).catch(() => {}));
  // Its title, as the shell sets it ("me@mac: ~/code/x"): the host and the folder it's in now, for its tab.
  term.onTitleChange((t) => {
    const [who, where] = String(t).includes(": ") ? String(t).split(/: (.*)/) : ["", String(t)];
    if (where) sh.where = baseName(where) || where;
    if (who.includes("@")) sh.mac = who.split("@").pop().split(".")[0];
    drawShellTabs();
  });
  try {
    const a = sh.action;
    const r = a ? { gen: await invoke("actions_term_open", { folder: a.folder, machine: a.machine, id: a.id, sid: id, cols: term.cols, rows: term.rows }) }
      : await invoke("shell_open", { id, cwd: sh.cwd, cols: term.cols, rows: term.rows });
    shellMac ||= r.host || "";
    if (shellOf(id)) { sh.gen = r.gen; sh.kept = !!r.kept; sh.mac ||= shellMac; drawShellTabs(); }
  } catch (e) {
    toast(`Couldn't open a shell: ${e}`);
    closeShell(id, true);
  }
}

/** At launch: the shells still running in tmux, each back in its tab, as they were (the tab on show the
 *  same), the peek away. Only the one on show is attached, once it's out. */
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
  showShell(shellOf(cur) ? cur : shells.at(-1).id, false);
  placePip();   // in the Terminal view already: it comes out there
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

/** What a shell runs ("shell" events): its tab names whatever is in front; a job (`busy`: not something you sit
 *  in, like ssh or vim; shells.rs interactive) gets the live dot, and one that ends while it's away leaves a ✓.
 *  SHORTCUT: ✓ only, never ✗: Cue sees the foreground process come and go, not its exit code. A ✗ needs
 *  the shell to say it (an OSC 133 mark from your prompt, as some terminals' shell integration has). */
function onShellEvent(p) {
  const sh = shellOf(p.id);
  if (!sh) return;
  if (sh.busy && !p.busy && !pipSeen()) pipDone = true;
  sh.busy = !!p.busy;
  sh.what = p.what || "";
  drawShellTabs();
  peekHint();
  markShells();
}

/** Tab `id` on show (`focus`: the keyboard into it, when the terminal is out). */
function showShell(id, focus = true) {
  if (shellCur === id && shellRenaming !== id) return shellOf(id)?.term?.focus();
  shellCur = id;
  try { localStorage.setItem("cue.shellCur", id); } catch {}
  for (const s of shells) s.host.hidden = s.id !== id;
  drawShellTabs();
  peekHint();
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
  if (!shells.length) { shellCur = null; peek = null; pipEl.hidden = true; pipDone = false; drawShellTabs(); markShells(); return homeFocus(); }
  if (shellCur === id) showShell((shells[i] || shells[i - 1]).id);
  else drawShellTabs();
  markShells();
}

/** A tab's name: yours, else its host and folder ("mac: ~"). */
const shellName = (s) => s.name || (s.mac ? `${s.mac}: ${s.where}` : s.where);
/** Which ends of the tab row have tabs past them (each fades there). */
function markTabEnds(strip) {
  const more = strip.scrollWidth - strip.clientWidth;
  strip.classList.toggle("more-left", more > 1 && strip.scrollLeft > 1);
  strip.classList.toggle("more-right", more > 1 && strip.scrollLeft < more - 1);
}
function drawShellTabs() {
  const el = pipEl?.querySelector(".pip-tabs");
  if (!el) return;
  // (Not while a name is typed: the field would go, and what was typed with it.)
  if (shellRenaming && el.querySelector(".pip-rename")) return;
  el.innerHTML = shells.map((s, i) => {
    const name = s.id === shellRenaming ? `<input class="pip-rename" value="${esc(shellName(s))}" aria-label="Name this tab" spellcheck="false" />`
      : `<span class="pip-name ${s.what || s.name ? "" : "mono"}">${esc(s.what || (s.busy ? "running" : shellName(s)))}</span>`;
    // (The one being named isn't a button: a field inside a button doesn't take clicks or keys in WebKit.)
    const tag = s.id === shellRenaming ? "span" : "button";
    return `<${tag} class="pip-tab" role="tab" data-shell="${esc(s.id)}" aria-selected="${s.id === shellCur}">${s.busy ? `<span class="sh-dot"></span>` : ""}${name}${shells.length > 1 && i < 9 ? `<span class="pip-key">⌘${i + 1}</span>` : ""}<span class="pip-x" data-shell-x="${esc(s.id)}" role="button" aria-label="Close this shell">×</span></${tag}>`;
  }).join("");
  // The tab on show is always in view (switched to with ⌘1–9, or a new one at the end).
  const on = el.querySelector('[aria-selected="true"]');
  if (on && (on.offsetLeft < el.scrollLeft || on.offsetLeft + on.offsetWidth > el.scrollLeft + el.clientWidth)) {
    el.scrollLeft = on.offsetLeft - Math.max(0, (el.clientWidth - on.offsetWidth) / 2);
  }
  markTabEnds(el);
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

// ---------- where it shows ----------
const PEEK = { w: 720, h: 420 };
/** Its place: the window under the header in the Terminal view; else, as the peek, dropping down under the
 *  header's terminal button (its point under the button), as wide as fits up to PEEK.w, PEEK.h tall at most. */
function placePip() {
  syncShellView();
  if (!pipEl || pipEl.hidden) return;
  pipEl.classList.toggle("in-view", shellViewOn);
  pipEl.classList.toggle("glance", peek === "glance");
  peekHint();
  const a = pipWhole();
  if (shellViewOn) {
    Object.assign(pipEl.style, { left: `${a.left}px`, top: `${a.top}px`, width: `${a.right - a.left}px`, height: `${a.bottom - a.top}px` });
    return fitShell();
  }
  const b = document.querySelector(".shells-btn")?.getBoundingClientRect();
  const w = Math.min(PEEK.w, a.right - a.left), h = Math.min(PEEK.h, a.bottom - a.top);
  const mid = b ? b.left + b.width / 2 : a.right;
  const left = Math.max(a.left, Math.min(a.right - w, mid - w * 0.7));
  Object.assign(pipEl.style, { left: `${left}px`, top: `${b ? b.bottom + 9 : a.top}px`, width: `${w}px`, height: `${h}px` });
  pipEl.style.setProperty("--point", `${Math.round(mid - left)}px`);
  fitShell();
}
/** The window under the header, inside the app's padding. */
function pipWhole() {
  const a = document.querySelector(".app"), cs = a ? getComputedStyle(a) : null, px = (k, d) => (cs && parseFloat(cs[k])) || d;
  return { left: px("paddingLeft", 20), right: innerWidth - px("paddingRight", 20), top: (document.querySelector(".top")?.getBoundingClientRect().bottom ?? 60) + 14, bottom: innerHeight - px("paddingBottom", 16) };
}
const GROW_ICON = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 2h4v4M6 14H2v-4M14 2 9.5 6.5M2 14l4.5-4.5"/></svg>`;
/** The Terminal view (the header's switch, core.js `view`): the same shells, filling the window under the header,
 *  the peek put away. Coming on, it brings the terminal out (a first shell if there's none); going, it puts it
 *  away. Its place under the shells (shellView) says how to open one when the last is closed. */
function syncShellView() {
  const on = view === "terminal";
  if (on === shellViewOn) return;
  shellViewOn = on;
  peek = null;
  if (on) {
    if (!shells.length) return openShell("");
    pipEl.hidden = false;
    pipDone = false;
    showShell(shellCur || shells[0].id);
  } else if (pipEl) pipEl.hidden = true;
  markShells();
}
const shellView = () => `<div class="shell-view"><div class="quiet-big">No shell open</div><button class="btn" data-act="shell-new">New shell <kbd>⌘T</kbd></button></div>`;
/** The peek: away (null), a look ("glance": it has the keyboard, so you can just type) or open ("open": it stays
 *  until put away). Away again, the keyboard goes back to the message box if it was in the terminal. */
function setPeek(m) {
  if (shellViewOn || !pipEl || !shells.length) return;
  const was = pipEl.contains(document.activeElement);
  peek = m;
  pipEl.hidden = !m;
  if (m === "open") pipDone = false;
  placePip();
  if (m) shellOf(shellCur)?.term?.focus();
  else if (was) { document.activeElement?.blur(); focusComposer(); }
  markShells();
}
// Pointing at the button for a moment brings a look; leaving both it and the look puts it away. A press
// anywhere outside an open peek (and its button, which has its own say) puts it away.
let glanceT = 0;
document.addEventListener("mouseover", (e) => {
  if (shellViewOn || !shells.length || peek === "open") return;
  const on = !!e.target.closest?.(".shells-btn") || !!pipEl?.contains(e.target);
  clearTimeout(glanceT);
  if (on && !peek) glanceT = setTimeout(() => setPeek("glance"), 250);
  else if (!on && peek === "glance") glanceT = setTimeout(() => setPeek(null), 200);
});
document.addEventListener("pointerdown", (e) => {
  if (peek === "open" && !pipEl.contains(e.target) && !e.target.closest?.(".shells-btn")) setPeek(null);
}, true);
