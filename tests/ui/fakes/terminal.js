// Fakes for src-tauri/src/ipc/terminal.rs: a session's terminal (tmux, attached), Cue's own shells, full screen,
// dictation and the natural voices (cue-speak).
CueBridge.area("terminal", (w) => {
  const term = (sid, text, gen = 1) => w.emit("term", { sid, gen, data: w.b64(text) });
  // ?stalled=1: every session's terminal has a client that stopped reading: what's typed isn't taken until
  // Resume (term_resume), as term::write refuses it.
  const stalled = new Set();
  const stallAll = w.q.has("stalled");
  let gen = 0;
  /** What a session's terminal shows when it's attached: Claude Code's permission menu; a remote executor's
   *  own tmux session (rx-…) at work. */
  const screen = (sid) => sid.startsWith("rx-")
    ? `\x1b[1m[relay-dns-cutover]\x1b[0m  claude · build-box\r\n\r\n\x1b[2m⏺\x1b[0m Update(zones/example.net.tf)\r\n  ⎿ Updated with 14 additions and 3 removals\r\n\x1b[2m⏺\x1b[0m Bash(terraform plan -out dns.plan)\r\n  ⎿ Plan: 12 to add, 0 to change, 3 to destroy.\r\n\r\n\x1b[33m✻ Checking the TTLs… (38s)\x1b[0m\r\n`
    : ["╭──────────────────────────────────────────────────────────╮", "│ \x1b[33mBash command\x1b[0m", "│   rm -rf build && make release", "│", "│ \x1b[1mDo you want to proceed?\x1b[0m", "│ \x1b[32m❯ 1. Yes\x1b[0m", "│   2. Yes, and don't ask again for rm commands here", "│   3. No, and tell Claude what to do differently (esc)", "╰──────────────────────────────────────────────────────────╯"]
      .map((l) => /^[╭│╰]/.test(l) ? `\x1b[2m${l[0]}\x1b[0m${l.slice(1)}` : l).join("\r\n") + "\r\n";
  // Cue's own shells: a prompt that echoes what's typed, Enter a new prompt, Backspace rubs out.
  const shellCwd = {};
  const prompt = (id) => `\x1b[32mme@mac\x1b[0m \x1b[1;34m${(shellCwd[id] || "~").replace(/^\/Users\/[^/]+/, "~")}\x1b[0m % `;
  const echo = (id, data) => term(id, data === "\r" ? `\r\n${prompt(id)}` : data === "\x7f" ? "\b \b" : data);
  /** Speech events, as cue-speak sends them (?voices=1: the engine with Heart and Bella here). */
  const voices = w.q.has("voices") ? ["af_heart", "af_bella"] : [];
  const speak = { id: null, timer: 0, said: [] };
  const tell = (v) => setTimeout(() => w.emit("speak", v), 0);
  function speakLine(L) {
    if (L.status) tell({ status: { ready: voices.length > 0, have: voices, mb: voices.length ? 125 + voices.length : 0, risky: true } });
    // A voice downloads in about a second.
    if (L.get) [0.2, 0.5, 0.8, 1].forEach((p, i) => setTimeout(() => { if (p < 1) return tell({ progress: p }); voices.push(L.get); tell({ got: L.get }); }, 250 * (i + 1) * w.latency));
    if (L.remove) tell({ removed: true });
    if (L.stop && speak.id != null) { clearInterval(speak.timer); tell({ done: speak.id, end: false }); speak.id = null; }
    if (L.say != null) {
      // A sentence "plays" at about 55 ms a letter, saying where it is every 80 ms.
      clearInterval(speak.timer);
      if (speak.id != null) tell({ done: speak.id, end: false });
      const ms = Math.round(L.say.length * 55 / (L.speed || 1)), t0 = w.now(), id = L.id;
      speak.id = id;
      speak.said.push({ say: L.say, voice: L.voice, speed: L.speed });
      tell({ playing: id, ms });
      speak.timer = setInterval(() => { const at = w.now() - t0; if (at < ms) return tell({ at: id, ms: at }); clearInterval(speak.timer); speak.id = null; tell({ done: id, end: true }); }, 80);
    }
  }
  let fullscreen = false;

  return {
    term_open: ({ sessionId }) => {
      if (stallAll) stalled.add(sessionId);
      gen += 1;
      const g = gen;
      setTimeout(() => term(sessionId, screen(sessionId), g), 150 * w.latency);
      return g;
    },
    term_write: ({ sessionId, data }) => {
      if (sessionId.startsWith("shell:")) return setTimeout(() => echo(sessionId, data), 10 * w.latency), null;
      if (stalled.has(sessionId)) throw "its terminal isn't responding: what you type isn't being sent until you resume it";
      return null;
    },
    term_resume: ({ sessionId }) => { stalled.delete(sessionId); return null; },
    term_resize: () => null,
    term_close: () => null,
    session_screen: () => "  Do you want to proceed?\n  ❯ 1. Yes\n    2. Yes, and don't ask again\n    3. No (esc)",
    session_keys: () => null,
    shell_open: ({ id, cwd }) => { shellCwd[id] = cwd; setTimeout(() => term(id, prompt(id)), 50 * w.latency); return { gen: 1, host: "mac" }; },
    shell_list: () => ({ shells: [], host: "mac" }),
    shell_name: () => null,
    shell_clear: () => null,
    hide_window: () => null,
    // Full screen as macOS would: the window's size changes, so the page hears "resize".
    set_fullscreen: ({ on }) => { fullscreen = on; dispatchEvent(new Event("resize")); return null; },
    is_fullscreen: () => fullscreen,
    dictate_start: () => null,
    dictate_stop: () => { w.emit("dictation", { done: true }); return null; },
    speak_send: ({ line }) => { speakLine(line); return null; },
  };
});
