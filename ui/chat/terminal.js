// Cue UI chat — the built-in terminal docked above a session's reply box.
// ---------- the built-in terminal: a session's real screen, docked above its reply box (sessions in tmux) ----------
/** Closed: one strip showing its screen's last line, amber while it waits on something only the terminal
 *  can answer. Open: xterm.js draws the real terminal, streamed from a tmux client of Cue's own (term.rs),
 *  and what you type goes straight to it. Its element lives outside the redraws: each one puts it back. */
const inTmux = (sid) => !!sessionOf(sid)?.tmux_pane || !!execTerm(sid);
let termSid = null;            // whose terminal is open
let termGen = 0;               // which opening its output belongs to (0: not known yet)
let termObj = null, termFit = null, termHost = null, termWatch = null, termWires = [];
/** The xterm, its element and its fit, kept from one opening to the next: made once (a GPU-drawn terminal takes
 *  a few hundred ms to make, all of it on the window's main thread, and a busy window waited on that at every
 *  opening), then cleared and moved back in on each. */
let termKept = null;
let termQueue = [], termFrame = 0;   // output waiting for the next frame: one write per frame, however many chunks came
const termLast = new Map();    // sid -> its screen's last line, for the closed strip
const waitsInTerminal = (sid) => state.items.some((i) => i.session_id === sid && inTerminal(i));
const cssVar = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
/** The terminal app a session runs in, by name (from what its hooks reported). */
/** Where a session runs, as a small icon for the lists: Cue's mark (in Cue, tmux underneath) or a terminal. */
function placeIcon(sid) {
  const s = sessionOf(sid);
  if (!s) return "";
  return inTmux(sid) ? `<span class="lv-place" title="Runs in Cue">${CUE_MARK_SM}</span>` : `<span class="lv-place" title="Runs in ${esc(termAppName(s))}">${TERM_ICON}</span>`;
}
function termAppName(s) {
  const t = s?.term_program || "";
  return { "iTerm.app": "iTerm", Apple_Terminal: "Terminal", WezTerm: "WezTerm", ghostty: "Ghostty", vscode: "VS Code", kitty: "kitty" }[t] || (s?.kitty_window_id ? "kitty" : s?.wezterm_pane ? "WezTerm" : "its terminal");
}
/** Where a session runs, set into its reply box's top right corner: a terminal app's (a click goes to its
 *  tab), or Cue's own (a click opens its terminal above the box, as the strip does). */
function placeTab(sid) {
  const s = sid && sessionOf(sid);
  if (!s) return "";
  // In Cue: the same corner opens (or closes) its terminal above the box.
  if (inTmux(sid)) return `<button class="tplace-btn" data-act="term" data-sid="${esc(sid)}" aria-expanded="${termSid === sid}">${termSid === sid ? "Close" : "Open"} ${s.machine ? esc(machShort(s.machine)) : "Cue"} <span aria-hidden="true">${termSid === sid ? "▾" : "▴"}</span></button>`;
  // A terminal app's: its tab, and for Claude Code, Move to Cue (resumed in Cue's terminal).
  return `<span class="tplace">${moveBtn(sid, s.harness, s.state, "tplace-btn move")}<button class="tplace-btn" data-act="go-session" data-sid="${esc(sid)}">Open ${esc(termAppName(s))} <span aria-hidden="true">↗</span></button></span>`;
}
/** Move to Cue, for a Claude Code session in a terminal app's tab: ended there, resumed in Cue's terminal.
 *  Shown even when it can't run yet, greyed, and a click says why, so it never just isn't there. */
function moveBtn(sid, harness, st, cls) {
  if (harness !== "claude") return "";
  const why = !state.settings?.sessions?.tmux_installed ? "Needs tmux on this Mac (brew install tmux), then reopen Cue"
    : st === "working" ? "Available once it finishes what it's doing (or Esc it in its tab)"
    : st === "deciding" ? "Answer what it's asking first"
    : "";
  return `<button class="${cls}" data-act="move-cue" data-sid="${esc(sid)}" ${why ? `aria-disabled="true" data-why="${esc(why)}" title="${esc(why)}"` : ``}>Move to Cue</button>`;
}
/** "Open in iTerm ↗": where a terminal-app session's own tab is, for the places that have no reply box. */
const openInLabel = (s) => `Open in ${esc(s ? termAppName(s) : "its terminal")} <span aria-hidden="true">↗</span>`;
function termDock(sid) {
  if (!sid) return "";
  if (!inTmux(sid)) return "";   // where it runs instead: a tab on its reply box (placeTab)
  const open = termSid === sid, waiting = waitsInTerminal(sid);
  return `<div class="tdock ${open ? "open" : ""} ${waiting ? "waiting" : ""}">
    <button class="tstrip" data-act="term" data-sid="${esc(sid)}" aria-expanded="${open}" aria-label="${open ? "Close" : "Open"} its terminal"><span class="tlive"></span><span class="tlbl">${waiting ? "Waiting in its terminal" : "Terminal"}</span><span class="tlast">${esc(termLast.get(sid) || "")}</span><span class="tchev">▴</span></button>
    ${open ? `<div class="tslot" data-term-slot></div>` : ""}</div>`;
}
async function openTerm(sid) {
  if (!inTmux(sid)) return;
  // Already open, or opening (its client's on the way): that one, not a second.
  if (termSid === sid && termObj) return termObj.focus();
  if (termSid) closeTerm(true);
  termSid = sid;
  termGen = 0;
  renderMain();                     // the slot appears
  const slot = document.querySelector("[data-term-slot]");
  if (!slot || typeof Terminal === "undefined") { termSid = null; return renderMain(); }
  ({ host: termHost, term: termObj, fit: termFit } = keptTerm(slot));
  termWires = [termObj.onData(termTyping(termHost, sid)), termObj.onResize(({ cols, rows }) => { if (termSid) invoke("term_resize", { sessionId: termSid, cols, rows }).catch(() => {}); })];
  termWatch = new ResizeObserver(() => { try { termFit?.fit(); } catch {} });
  termWatch.observe(termHost);
  termObj.focus();
  try {
    const gen = await invoke("term_open", { sessionId: sid, cols: termObj.cols, rows: termObj.rows });
    if (termSid === sid) termGen = gen;
  } catch (e) { toast(`Couldn't open its terminal: ${e}`); if (termSid === sid) closeTerm(); }
}
/** The kept terminal (`termKept`) in `slot`, cleared of the last session's screen and of a stalled notice:
 *  made the first time (it must be in the window to measure itself). */
function keptTerm(slot) {
  if (!termKept) {
    const host = document.createElement("div");
    host.className = "term-host";
    slot.appendChild(host);
    termKept = { host, ...xterm(host) };
    return termKept;
  }
  slot.appendChild(termKept.host);
  termKept.host.querySelector(".term-stalled")?.remove();
  termKept.term.reset();
  try { termKept.fit.fit(); } catch {}
  return termKept;
}
/** A remote executor, from its row in its lead's tree: its chat when Cue holds it, else a pane of its
 *  own (execPane), and either way its terminal, attached to its own tmux session there. Once per
 *  executor: a second click on the same row keeps the terminal that's open. */
function openExec(sid, id = null) {
  if (sessionOf(sid)) setActive(id, sid);
  else if (quietOpen !== sid || view !== "board") {
    if (quietOpen !== sid) setActive(null, null);   // (not when it's this one: that would end its terminal)
    view = "board";
    quietOpen = sid;
    followProject(sid);
    renderMain();
  }
  return openTerm(sid);
}
/** A session Cue holds, opened from any row, chip or list that names it: its chat, and for a remote
 *  executor in a tmux session of its own (one a lead on a machine started, whose hooks reached Cue too)
 *  also that terminal, as from its row in its lead's tree (openExec). `id`: the card to open it on. */
function openSession(sid, id = null) {
  if (sessionOf(sid) && execTerm(sid)) return openExec(sid, id);
  return setActive(id, sid);
}
/** Close it (the session keeps running in tmux). `quiet`: no redraw, another opening follows. */
function closeTerm(quiet = false) {
  const sid = termSid, gen = termGen;
  termSid = null;
  termGen = 0;
  termWatch?.disconnect();
  cancelAnimationFrame(termFrame);
  termQueue = [];
  termFrame = 0;
  termWires.forEach((w) => w.dispose());
  termWires = [];
  termHost?.remove();   // the terminal itself is kept for the next opening (termKept)
  termWatch = termObj = termFit = termHost = null;
  if (sid) invoke("term_close", { sessionId: sid, gen: gen || null }).catch(() => {});
  if (!quiet) { renderMain(); homeFocus(); }
}
function onTermOutput(p) {
  if (p.sid !== termSid || (termGen && p.gen !== termGen)) return;
  if (p.end) return closeTerm();
  termQueue.push(p.data);
  if (!termFrame) termFrame = requestAnimationFrame(flushTerm);
}
/** The terminals open now (the session's and Cue's own shells), to recolor when the window's colors change. */
const terms = new Set();
let termKey = "";
const rgbProbe = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
/** A color variable as [r, g, b]: xterm takes only plain colors, and a project's are oklch(). */
function rgbVar(name) {
  const el = document.createElement("i");
  el.style.color = `var(${name})`;
  document.body.append(el);
  const c = getComputedStyle(el).color;
  el.remove();
  rgbProbe.clearRect(0, 0, 1, 1);
  rgbProbe.fillStyle = c;
  rgbProbe.fillRect(0, 0, 1, 1);
  return [...rgbProbe.getImageData(0, 0, 1, 1).data.slice(0, 3)];
}
/** What the window's colors depend on now: the theme, the project's hue, light or dark. */
function themeKey() {
  const root = document.documentElement;
  return [root.className, root.dataset.theme, root.style.getPropertyValue("--ph"), root.style.getPropertyValue("--pk"), matchMedia("(prefers-color-scheme: dark)").matches].join("|");
}
/** The terminals' colors for the window's colors now, worked out once per change of them: reading a color
 *  makes the browser work out the whole window's styles, and a terminal opened under a busy window paid that
 *  four times over on every opening. */
let themeMade = { key: null, theme: null };
function termTheme() {
  const key = themeKey();
  if (themeMade.key === key) return themeMade.theme;
  const hex = (name) => "#" + rgbVar(name).map((v) => v.toString(16).padStart(2, "0")).join("");
  const [r, g, b] = rgbVar("--sel");
  themeMade = { key, theme: { background: hex("--slab"), foreground: hex("--on-slab"), cursor: hex("--sel"), cursorAccent: hex("--slab"), selectionBackground: `rgba(${r}, ${g}, ${b}, 0.35)` } };
  return themeMade.theme;
}
/** The open terminals in the window's colors now: the project's, light or dark. */
function retheme() {
  const key = themeKey();
  if (key === termKey) return;
  termKey = key;
  if (!terms.size) return;
  const theme = termTheme();
  for (const t of terms) t.options.theme = theme;
}
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", retheme);

/** What arrived since the last frame, decoded and written as one. */
function flushTerm() {
  termFrame = 0;
  const chunks = termQueue;
  termQueue = [];
  if (!termObj || !chunks.length) return;
  termObj.write(b64Bytes(chunks));
}
/** What you type into terminal `sid` (shown in `host`), on its way to it. A write that doesn't go (its client
 *  stopped reading: a stopped tmux client, a dead SSH line) stops its input in Cue too (term::write): a line
 *  over the terminal says so, once, and nothing typed is sent until Resume (term_resume), which drops what was
 *  waiting to go in. Also Cue's own shells' (shells.js). */
function termTyping(host, sid) {
  let notice = null;
  const stop = () => {
    if (notice) return;
    notice = document.createElement("div");
    notice.className = "term-stalled";
    notice.innerHTML = `<span>Terminal isn't responding — what you type isn't being sent.</span><button class="btn">Resume</button>`;
    notice.querySelector("button").addEventListener("click", async () => {
      try { await invoke("term_resume", { sessionId: sid }); notice?.remove(); notice = null; }
      catch (e) { toast(`Couldn't resume it: ${e}`); }
    });
    host.appendChild(notice);
  };
  return (data) => { if (!notice) invoke("term_write", { sessionId: sid, data }).catch(stop); };
}
/** An xterm.js terminal in `host`, in Cue's colours, fitted to it: drawn on the GPU where it can be (much
 *  faster than the default; if that's lost, back to the default). Also Cue's own shells' (shells.js).
 *  Its cursor blinks at Cue's pace, a 1.4s beat like the live dots: vendor/xterm/addon-webgl.js is patched
 *  from xterm's fixed 600ms to 700ms (keep that on an update), and the default renderer's in styles/shells.css. */
function xterm(host) {
  const term = new Terminal({
    fontFamily: cssVar("--mono") || "Menlo, monospace", fontSize: 12, lineHeight: 1.1, cursorBlink: true, scrollback: 5000, macOptionIsMeta: true,
    // ⌥-drag: a selection of the terminal's own, even with tmux taking the mouse.
    macOptionClickForcesSelection: true,
    theme: termTheme(),
  });
  terms.add(term);
  const dispose = term.dispose.bind(term);
  term.dispose = () => { terms.delete(term); dispose(); };
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  term.open(host);
  try {
    const gl = new WebglAddon.WebglAddon();
    gl.onContextLoss(() => gl.dispose());
    term.loadAddon(gl);
  } catch {}
  try { fit.fit(); } catch {}
  // What tmux copies (drag over text: it selects, and on letting go hands the text to the terminal as
  // OSC 52, base64 after the ";"), and a ⌥-drag selection once you let go: on your clipboard.
  const copy = (text) => { if (text.trim()) invoke("copy_text", { text }).then(() => toast("Copied"), () => {}); };
  term.parser.registerOscHandler(52, (data) => {
    const b64 = data.slice(data.indexOf(";") + 1);
    if (b64 && b64 !== "?") try { copy(new TextDecoder().decode(Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)))); } catch {}
    return true;
  });
  let picked = false;   // a new ⌥-drag selection, not copied yet (a later click doesn't copy it again)
  term.onSelectionChange(() => { picked = term.hasSelection(); });
  host.addEventListener("mouseup", () => { if (picked) { picked = false; copy(term.getSelection()); } });
  return { term, fit };
}
/** "term" events' chunks (base64), as the bytes to write. */
function b64Bytes(chunks) {
  const bin = chunks.map(atob).join("");
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}
/** After a redraw: the terminal back in its slot, sized, and focused again if it had the keyboard. */
function placeTerm(hadFocus) {
  if (!termHost) return;
  const slot = document.querySelector("[data-term-slot]");
  if (!slot) return;
  if (termHost.parentNode !== slot) slot.appendChild(termHost);
  try { termFit?.fit(); } catch {}
  if (hadFocus) termObj?.focus();
}
