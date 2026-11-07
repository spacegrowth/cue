// Fakes for src-tauri/src/ipc/state.rs: the window's state, and answering what waits in it.
CueBridge.area("state", (w) => {
  /** Something you said to a session, as its thread keeps it. */
  const said = (text, images = []) => ({ role: "you", text, at_ms: w.now(), images });
  /** A session took a message: it works on it (hub/send.rs). */
  w.working = (s, text, images) => {
    s.state = "working";
    s.since_ms = w.now();
    s.prompt = text;
    s.thread = [...(s.thread || []), said(text, images)];
  };
  /** An item answered: off Waiting and onto History with `outcome`; its session at work again unless `idle`. */
  w.answer = (id, outcome, { text = null, images = [], idle = false } = {}) => {
    const i = w.state.items.findIndex((x) => x.id === id);
    if (i < 0) return null;
    const [it] = w.state.items.splice(i, 1);
    const s = w.session(it.session_id);
    if (s && !idle) w.working(s, text ?? outcome, images);
    w.state.history.unshift({ ...it, status: "answered", outcome, resolved_ms: w.now() });
    w.changed();
    return it;
  };
  const flag = (key) => ({ sessionId, on }) => {
    const s = w.session(sessionId);
    if (s) s[key] = on ? w.now() : 0;
    w.changed();
    return null;
  };

  return {
    get_state: () => w.snapshot(),
    respond: ({ id, decision }) => !!w.answer(id, decision.behavior === "allow" ? (decision.answers ? `answered — ${Object.values(decision.answers).join(", ")}` : "allowed") : decision.message ? `denied — “${decision.message}”` : "denied"),
    dismiss: ({ id }) => { w.answer(id, "dismissed", { idle: true }); return null; },
    reply: ({ id, text, images }) => {
      if (!w.answer(id, `replied — “${text}”`, { text, images })) throw "That message was already answered";
      return "via tmux pane %1";
    },
    answer_terminal: ({ id, n }) => { w.answer(id, `answered ${n} in its terminal`); return `Pressed ${n} in its terminal`; },
    forget_session: ({ sessionId }) => {
      w.state.sessions = w.state.sessions.filter((s) => s.session_id !== sessionId);
      w.state.items = w.state.items.filter((i) => i.session_id !== sessionId);
      w.changed();
      return null;
    },
    unqueue_review: ({ exec }) => {
      for (const lead of Object.keys(w.state.review_queue || {})) w.state.review_queue[lead] = w.state.review_queue[lead].filter((r) => r.exec !== exec);
      if (w.state.reviews?.[exec]?.state === "queued") w.state.reviews[exec].state = "waiting";
      w.changed();
      return null;
    },
    send_to_back: ({ id }) => {
      const i = w.state.items.findIndex((x) => x.id === id);
      if (i >= 0) w.state.items.push(...w.state.items.splice(i, 1));
      w.changed();
      return null;
    },
    rename_dismiss: ({ sessionId }) => { const s = w.session(sessionId); if (s) delete s.rename_error; w.changed(); return null; },
    unhold_message: ({ sessionId }) => {
      const s = w.session(sessionId), held = s?.held || null;
      if (s) delete s.held;
      w.changed(0);
      return held;
    },
    send_held_now: ({ sessionId }) => {
      const s = w.session(sessionId);
      if (!s?.held) throw "Nothing kept for it";
      const { text, images } = s.held;
      delete s.held;
      w.working(s, text, images);
      w.changed();
      return "sent now";
    },
    set_later: flag("later_ms"),
    set_watch: flag("watch_ms"),
    set_starred: flag("starred_ms"),
  };
});
