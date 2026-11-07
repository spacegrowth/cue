// The fake Tauri bridge the real UI runs on in a browser (tests/ui/app.html, tests/preview.html).
// It puts a `window.__TAURI__` in place before ui/core.js reads it: `invoke` answers every command the app
// registers (startup/mod.rs generate_handler!) from the fakes in tests/ui/fakes/, one file per area, each registered
// with CueBridge.area(). `emit` pushes the events the app sends (state, term, speak, upload, …), and every
// invoke is recorded with its arguments in CueBridge.calls, for tests to assert on.
//
// The fakes share one world (`w` below): the window's state (a snapshot, as hub/mod.rs snapshot() builds it),
// the page's query (preview.html's ?flags), the clock, and helpers to change the state and tell the window.
(() => {
  const makers = [];          // [area, (w) => ({ command: (args) => result })], in the order the fakes loaded
  const handlers = {};        // command -> its fake, once start() has run
  const overrides = {};       // command -> a test's own fake, in front of the area's
  const listeners = {};       // event -> [callback]
  let ready = null;           // resolves once the world has its state
  const q = new URLSearchParams(location.search);

  /** The world every fake shares. `latency` scales the waits fakes make to feel like the real app (preview: 1;
   *  tests: 0, so nothing waits on time). */
  const w = {
    q,
    state: null,
    latency: q.has("latency") ? +q.get("latency") : 1,
    now: () => Date.now(),
    /** A pause of about `ms`, as the real command would take (scaled by `latency`). */
    wait: (ms) => new Promise((r) => setTimeout(r, ms * w.latency)),
    /** The window's state as get_state returns it: a copy, its clock read now (hub/mod.rs stamped). */
    snapshot: () => ({ ...structuredClone(w.state), now_ms: Date.now() }),
    /** Send event `name` to the window, as the app's emit does. */
    emit: (name, payload) => { for (const f of listeners[name] || []) f({ event: name, payload }); },
    /** The state changed: the window hears it shortly after, as it does from the hub. */
    changed: (ms = 30) => setTimeout(() => w.emit("state", w.snapshot()), ms * w.latency),
    session: (sid) => w.state.sessions.find((s) => s.session_id === sid),
    /** Text as a term event's data: base64 of its UTF-8 bytes. */
    b64: (text) => btoa(String.fromCharCode(...new TextEncoder().encode(text))),
  };

  async function invoke(cmd, args = {}) {
    const call = { cmd, args: structuredClone(args ?? {}), at: Date.now() };
    B.calls.push(call);
    await ready;
    const f = overrides[cmd] || handlers[cmd];
    if (!f) { call.error = `unknown command ${cmd}`; throw call.error; }
    try { call.result = await f(args ?? {}); return call.result; }
    catch (e) { call.error = String(e); throw e; }
  }

  const B = (window.CueBridge = {
    /** Every invoke so far: { cmd, args, at, result | error }. */
    calls: [],
    world: w,
    /** A fakes file registers its area's commands. */
    area(name, make) { makers.push([name, make]); },
    /** The names of every command some area fakes (for the coverage test). */
    commands: () => Object.keys(handlers),
    /** A test's own answer for `cmd` (a function of its args), in front of the area's; null takes it away. */
    override(cmd, f) { if (f) overrides[cmd] = f; else delete overrides[cmd]; },
    emit: w.emit,
    /** Calls of `cmd` so far (all of them without one). */
    called: (cmd) => B.calls.filter((c) => !cmd || c.cmd === cmd),
    /** The fakes built and the state loaded: `load` returns the snapshot to start from (or a promise of it). */
    start(load) {
      for (const [area, make] of makers) for (const [cmd, f] of Object.entries(make(w))) {
        if (handlers[cmd]) throw new Error(`${cmd} is faked twice (again in ${area})`);
        handlers[cmd] = f;
      }
      ready = Promise.resolve(load()).then((s) => { w.state = s; });
      return ready;
    },
  });

  window.__TAURI__ = {
    app: { getVersion: async () => "0.1.86" },
    core: { invoke, convertFileSrc: (p) => p },
    event: {
      listen: async (name, f) => {
        (listeners[name] ||= []).push(f);
        return () => { listeners[name] = listeners[name].filter((g) => g !== f); };
      },
      // What the window tells the app (chat-open): recorded as a call, so tests can see it.
      emit: async (name, payload) => { B.calls.push({ cmd: `event:${name}`, args: payload, at: Date.now() }); },
    },
  };
})();
