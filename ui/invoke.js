// Cue UI bridge — `invoke`, Tauri's, with the calls that must land in the order they're made kept in order.
// Cue runs every command off the window's thread (src-tauri/src/ipc), so two calls in flight at once can land
// in either order: keystrokes typed into a terminal, a terminal opened twice, a setting switched twice.
// The calls in one lane go one at a time, each once the one before it is back (done or failed); calls in
// different lanes, and calls to any other command, go at once.

/** Each ordered command's lane, from its arguments. */
const INVOKE_LANES = {
  term_write: (a) => `term-input:${a.sessionId}`,
  // After every write sent before it: one still waiting would otherwise go in once its input is let go again.
  term_resume: (a) => `term-input:${a.sessionId}`,
  term_resize: (a) => `term-size:${a.sessionId}`,
  // One terminal's opens (a shell's tab, an action's terminal: the same ids). Not its close: that goes at
  // once, even while an open hangs, and names the opening it means (term::close_if), so it never closes a
  // newer one whichever lands first.
  term_open: (a) => `term:${a.sessionId}`,
  shell_open: (a) => `term:${a.id}`,
  actions_term_open: (a) => `term:${a.sid}`,
  set_setting: (a) => `setting:${a.key}`,
  // These two read the list from the settings and save it back with one session added or taken out.
  status_session: () => "setting:status.sessions",
  background_session: () => "setting:background.sessions",
  set_draft: (a) => `draft:${a.key}`,
  set_later: (a) => `later:${a.sessionId}`,
  set_watch: (a) => `watch:${a.sessionId}`,
  set_starred: (a) => `star:${a.sessionId}`,
  ext_enable: (a) => `ext:${a.name}`,
  ext_set: (a) => `ext-set:${a.id}`,
  // An extension's log lines, in the order its page wrote them.
  ext_log: (a) => `ext-log:${a.ext}`,
  // A session's hand-off: sent, written ahead, continued, kept, started fresh, each after the one before.
  handoff_sent: (a) => `handoff:${a.sessionId}`,
  handoff_cue: (a) => `handoff:${a.sessionId}`,
  handoff_continue: (a) => `handoff:${a.sessionId}`,
  handoff_keep: (a) => `handoff:${a.sessionId}`,
  handoff_fresh: (a) => `handoff:${a.sessionId}`,
  // What an insight did, and then what you did with it (shown before acted): each stamped as it's saved.
  insight_log: (a) => `insight:${a.sessionId}`,
  dictate_start: () => "dictate",
  dictate_stop: () => "dictate",
  speak_send: () => "speak",
};

/** `call` (Tauri's invoke) with the commands in `lanes` kept in order, lane by lane. */
function orderedInvoke(call, lanes = INVOKE_LANES) {
  const last = new Map();   // lane -> the last call in it, settled either way
  return (cmd, args) => {
    const lane = lanes[cmd]?.(args || {});
    if (!lane) return call(cmd, args);
    const go = () => call(cmd, args);
    const before = last.get(lane);
    const next = before ? before.then(go) : go();   // an idle lane: at once, as any other call
    const settled = next.catch(() => {});
    last.set(lane, settled);
    settled.then(() => { if (last.get(lane) === settled) last.delete(lane); });
    return next;
  };
}

const invoke = typeof window !== "undefined" && window.__TAURI__ ? orderedInvoke(window.__TAURI__.core.invoke) : null;

if (typeof module !== "undefined") module.exports = { orderedInvoke, INVOKE_LANES };
