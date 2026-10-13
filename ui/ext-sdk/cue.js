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
    badge: (session, text) => call("tab.badge", { session, text }),
    run: (program, args = [], { cwd } = {}) => call("programs.run", { program, args, cwd }),
    git: (session, op, params = {}) => call("git", { session, op, ...params }),
    toast: (text) => call("ui.toast", { text }),
    log: (text) => call("log", { text }),
    ready: call("hello").then((h) => { theme(h.theme); return h; }),
  };
  // What goes wrong on the page is told to Cue (once each, at most 20), so it shows under "Last it did"
  // in the Extensions panel: a page has no console you can see.
  const told = new Set();
  const report = (what) => {
    const t = String(what || "unknown error").slice(0, 300);
    if (told.has(t) || told.size >= 20) return;
    told.add(t);
    call("log", { text: `error: ${t}` }).catch(() => {});
  };
  addEventListener("error", (e) => report(e.error && e.error.stack ? e.error.stack.split("\n").slice(0, 2).join(" ") : `${e.message} (${(e.filename || "").split("/").pop()}:${e.lineno})`));
  addEventListener("unhandledrejection", (e) => report(e.reason && e.reason.message ? e.reason.message : e.reason));
})();
