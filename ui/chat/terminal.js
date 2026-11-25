// Cue UI chat — where a session runs (a terminal app's tab, or hosted by Cue), and the xterm terminals Cue's own
// shells use (shells.js).
const cssVar = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
/** Where a session runs, as a small icon for the lists: Cue's mark (hosted) or a terminal app's. */
function placeIcon(sid) {
  const s = sessionOf(sid);
  if (!s) return "";
  return s.hosted ? `<span class="lv-place">${CUE_MARK_SM}</span>` : `<span class="lv-place">${TERM_ICON}</span>`;
}
function termAppName(s) {
  const t = s?.term_program || "";
  return { "iTerm.app": "iTerm", Apple_Terminal: "Terminal", WezTerm: "WezTerm", ghostty: "Ghostty", vscode: "VS Code", kitty: "kitty" }[t] || (s?.kitty_window_id ? "kitty" : s?.wezterm_pane ? "WezTerm" : "its terminal");
}
/** Where a session runs, set into its reply box's top right corner: a terminal app's (a click goes to its tab). */
function placeTab(sid) {
  const s = sid && sessionOf(sid);
  if (!s) return "";
  // A Desk's lead (#842), handing off or not: its chat is the lead's, with nothing about where it runs.
  if (isDeskLead(sid)) return "";
  // Hosted: no terminal, so nothing here (⋯ has Move to terminal, for moving away from Cue).
  if (s.hosted) return "";
  // A terminal app's: its tab, and for Claude Code, Move to Cue (resumed hosted).
  return `<span class="tplace">${moveBtn(sid, s.harness, s.state, "tplace-btn move")}<button class="tplace-btn" data-act="go-session" data-sid="${esc(sid)}">Open ${esc(termAppName(s))} <span aria-hidden="true">↗</span></button></span>`;
}
/** Move to Cue, for a Claude Code session in a terminal app's tab: ended there, resumed hosted.
 *  Shown even when it can't run yet, greyed, and a click says why, so it never just isn't there. */
function moveBtn(sid, harness, st, cls) {
  if (harness !== "claude") return "";
  const why = st === "working" ? "Available once it finishes what it's doing (or Esc it in its tab)"
    : st === "deciding" ? "Answer what it's asking first"
    : "";
  return `<button class="${cls}" data-act="move-cue" data-sid="${esc(sid)}" ${why ? `aria-disabled="true" data-why="${esc(why)}"` : ``}>Move to Cue</button>`;
}
/** "Open in iTerm ↗": where a terminal-app session's own tab is, for the places that have no reply box. */
const openInLabel = (s) => `Open in ${esc(s ? termAppName(s) : "its terminal")} <span aria-hidden="true">↗</span>`;
/** A session Cue holds, opened from any row, chip or list that names it: its chat. `id`: the card to open it on. */
function openSession(sid, id = null) {
  return setActive(id, sid);
}
/** The terminals open now (Cue's own shells), to recolor when the window's colors change. */
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
/** The terminals' text: the chat's (--type-body), so a terminal reads at the size the chat does. Worked out on an
 *  element: the variable itself reads back as its formula (round(calc(…))), not as pixels. */
function termFontSize() {
  const el = document.createElement("i");
  el.style.fontSize = "var(--type-body)";
  document.body.append(el);
  const px = parseFloat(getComputedStyle(el).fontSize);
  el.remove();
  return px || 14;
}
/** Each open terminal's fit (sized to its pane), for a change of text size. */
const termFits = new Map();
let fontKey = "";
/** The open terminals at the window's text size now (Settings → Text size, ⌘+ / ⌘−). The window's zoom draws each
 *  point with more (or fewer) pixels, and a terminal hears of that only from the browser's own pixel-ratio event,
 *  which the app's zoom doesn't send: each is told here (measured and drawn again, vendor/xterm's render service; keep
 *  that on an update), given the chat's size if that changed, refitted to its pane and left where it was scrolled
 *  (at the bottom: still there). */
function refont() {
  const key = `${state.settings?.appearance?.zoom ?? 1}|${termFontSize()}`;
  if (key === fontKey) return;
  const first = !fontKey;
  fontKey = key;
  if (first || !terms.size) return;
  // After the window's zoom is on (the app sets it as it saves the setting): a frame on, then measured.
  requestAnimationFrame(() => requestAnimationFrame(() => {
    const size = termFontSize();
    for (const t of terms) {
      const buf = t.buffer.active, atEnd = buf.viewportY >= buf.baseY, top = buf.viewportY;
      if (t.options.fontSize !== size) t.options.fontSize = size;
      try { t._core._renderService.handleDevicePixelRatioChange(); } catch {}
      try { termFits.get(t)?.fit(); } catch {}
      if (atEnd) t.scrollToBottom(); else t.scrollToLine(Math.min(top, t.buffer.active.baseY));
    }
  }));
}

/** What you type into Cue's shell `sid` (shown in `host`, shells.js), on its way to it. A write that doesn't go
 *  (its client stopped reading: a stopped tmux client) stops its input in Cue too (term::write): a line over the
 *  terminal says so, once, and nothing typed is sent until Resume (term_resume), which drops what was waiting to
 *  go in. `shown`: whether `host` still shows `sid`. */
function termTyping(host, sid, shown = () => true) {
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
  return (data) => { if (!notice) invoke("term_write", { sessionId: sid, data }).catch(() => { if (shown()) stop(); }); };
}
/** An xterm.js terminal in `host`, in Cue's colours, fitted to it: drawn on the GPU where it can be (much
 *  faster than the default; if that's lost, back to the default). Cue's own shells' (shells.js).
 *  Its cursor blinks at Cue's pace, a 1.4s beat like the live dots: vendor/xterm/addon-webgl.js is patched
 *  from xterm's fixed 600ms to 700ms (keep that on an update), and the default renderer's in styles/shells.css. */
function xterm(host) {
  const fit = new FitAddon.FitAddon();
  const term = new Terminal({
    fontFamily: cssVar("--mono") || "Menlo, monospace", fontSize: termFontSize(), lineHeight: 1.1, cursorBlink: true, scrollback: 5000, macOptionIsMeta: true,
    // ⌥-drag: a selection of the terminal's own, even with tmux taking the mouse.
    macOptionClickForcesSelection: true,
    theme: termTheme(),
  });
  terms.add(term);
  termFits.set(term, fit);
  const dispose = term.dispose.bind(term);
  term.dispose = () => { terms.delete(term); termFits.delete(term); dispose(); };
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
