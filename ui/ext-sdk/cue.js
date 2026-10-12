// Cue's helper for extension pages: <script src="cueext://localhost/_cue/cue.js"></script>.
// The page runs in a sandboxed frame; everything goes through messages to Cue's window, which only
// answers what the extension's manifest asked for.
(() => {
  let next = 0;
  const pending = new Map();
  const subs = {};
  const theme = (t) => {
    const root = document.documentElement;
    for (const [k, v] of Object.entries(t.vars || {})) root.style.setProperty(`--${k}`, v);
    root.style.colorScheme = t.scheme;
    root.dataset.theme = t.scheme;
  };
  addEventListener("message", (e) => {
    if (e.source !== parent) return;
    const m = e.data;
    if (!m || m.cue !== 1) return;
    if (m.id != null && pending.has(m.id)) {
      const p = pending.get(m.id);
      pending.delete(m.id);
      return m.ok ? p.res(m.value) : p.rej(new Error(m.value));
    }
    if (m.event === "theme") theme(m.data);
    for (const f of subs[m.event] || []) f(m.data);
  });
  const call = (method, params = {}) => new Promise((res, rej) => {
    const id = ++next;
    pending.set(id, { res, rej });
    parent.postMessage({ cue: 1, id, method, params }, "*");
  });
  window.cue = {
    call,
    on: (event, f) => { (subs[event] ||= []).push(f); },
    session: () => call("session.current"),
    run: (program, args = [], { cwd, machine } = {}) => call("cmd.run", { program, args, cwd, machine }),
    badge: (session, text) => call("tab.badge", { session, text }),
    toast: (text) => call("ui.toast", { text }),
    log: (text) => call("log", { text }),
    ready: call("hello").then((h) => { theme(h.theme); return h; }),
  };
})();
