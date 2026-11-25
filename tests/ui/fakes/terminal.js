// Fakes for src-tauri/src/ipc/terminal.rs: Cue's own shells, full screen,
// dictation and the natural voices (cue-speak).
CueBridge.area("terminal", (w) => {
  const term = (sid, text, gen = 1) => w.emit("term", { sid, gen, data: w.b64(text) });
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
    term_write: ({ sessionId, data }) => {
      if (sessionId.startsWith("shell:")) return setTimeout(() => echo(sessionId, data), 10 * w.latency), null;
      return null;
    },
    term_resize: () => null,
    term_close: () => null,
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
