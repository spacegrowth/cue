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
  /** Your own words on Cue's Yes / No card in a lead's chat (desk/hub/core/desk.rs desk_card_words, #651): not a yes or
   *  a no. The card stays open, your words kept on it, and they go to the lead. False for any other answer. */
  w.cueCardWords = (id, decision) => {
    const it = w.state.items.find((x) => x.id === id && x.status === "pending" && x.desk);
    const cue = it && ["desk_push", "desk_old", "desk_lands"].some((k) => it.tool_input?.[k]);
    const words = decision.behavior !== "deny" && Object.values(decision.answers || {})[0];
    if (!cue || !words || /^(yes|no)$/i.test(String(words).trim())) return false;
    // As hub's desk_card_words (#880): a plain no is the card's No; a question keeps it open (#651); other words close it.
    if (/^(no|n|nope|nah|no thanks|no thank you|not now|don'?t|don't do it|leave it|keep it)[.!]*$/i.test(String(words).trim())) return false;
    const at = w.now();
    const open = String(words).includes("?");
    const q = it.tool_input.questions?.[0]?.question || "";
    if (open) (it.tool_input.desk_said ||= []).push({ text: words, at });
    else w.answer(id, `answered in your words: ${words}`);
    const s = w.session(it.session_id);
    const tail = open ? "The question is still open and waits for their Yes or No: Cue took neither. Answer them here in plain words (what it means, what each choice does, what you'd pick); don't answer it for them." : "Cue took that as their answer: the question is closed and nothing changed. Do what their words ask; if it still needs their yes, ask it again.";
    if (s) s.thread = [...(s.thread || []), { role: "you", at_ms: at, text: `On Cue's question in your chat ("${q}"), the user wrote: "${words}"\n${tail}` }];
    w.changed();
    return true;
  };
  /** Today's numbers as hub/today.rs counts them, from the fake's History (it keeps no other list of answers). */
  const HOUR = 3600000;
  const today = (rows) => {
    const midnight = new Date(w.now()).setHours(0, 0, 0, 0), hour0 = new Date(w.now()).setMinutes(0, 0, 0);
    const day = rows.filter((a) => (a.resolved_ms || 0) >= midnight);
    const waits = day.map((a) => ({ a, w: a.resolved_ms - a.created_ms })).filter((x) => x.w >= 0).sort((x, y) => x.w - y.w);
    const n = (re) => day.filter((a) => re.test(a.outcome || "")).length;
    const longest = waits.at(-1);
    return {
      answered: day.length, median_wait_ms: waits.length ? waits[Math.floor(waits.length / 2)].w : null,
      longest: longest ? { wait_ms: longest.w, session_id: longest.a.session_id, project: longest.a.project } : null,
      replied: n(/^replied/), allowed: n(/^(allowed|answered)/), denied: n(/^denied/),
      hours: Array.from({ length: 12 }, (_, k) => { const from = hour0 - (11 - k) * HOUR; return { from_ms: from, n: rows.filter((a) => (a.resolved_ms || 0) >= from && a.resolved_ms < from + HOUR).length }; }),
    };
  };
  const plain = w.snapshot;
  w.snapshot = () => ({ ...plain(), today: w.state.today || today(w.state.history || []) });
  const flag = (key) => ({ sessionId, on }) => {
    const s = w.session(sessionId);
    if (s) s[key] = on ? w.now() : 0;
    w.changed();
    return null;
  };

  return {
    get_state: () => w.snapshot(),
    state_drawn: () => null,
    history_item: ({ id }) => structuredClone(w.state.items.find((i) => i.id === id) || (w.state.history || []).find((i) => i.id === id) || null),
    respond: ({ id, decision }) => w.cueCardWords(id, decision) || !!w.answer(id, decision.behavior === "allow" ? (decision.answers ? `answered — ${Object.values(decision.answers).join(", ")}` : "allowed") : decision.message ? `denied — “${decision.message}”` : "denied"),
    dismiss: ({ id }) => { w.answer(id, "dismissed", { idle: true }); return null; },
    reply: ({ id, text, images }) => {
      if (!w.answer(id, `replied — “${text}”`, { text, images })) throw "That message was already answered";
      return "via tmux pane %1";
    },
    forget_session: ({ sessionId }) => {
      w.state.sessions = w.state.sessions.filter((s) => s.session_id !== sessionId);
      w.state.items = w.state.items.filter((i) => i.session_id !== sessionId);
      w.changed();
      return null;
    },
    unqueue_review: ({ exec }) => {
      for (const lead of Object.keys(w.state.review_queue || {})) w.state.review_queue[lead] = w.state.review_queue[lead].filter((r) => r.exec !== exec);
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
    // The chats open in the window and whether it's in front (box/end.js chatsOpenTell): for notifications only.
    chats_open: () => null,
    // Send at… (hub/send_at.rs): the message kept on its session as `send_at`, dropped (its text back), or sent now.
    send_at: ({ sessionId, text, atMs }) => {
      const s = w.session(sessionId);
      if (!text?.trim()) throw "nothing to send";
      if (atMs <= w.now()) throw "that time has passed";
      if (!s) throw "that session is gone";
      s.send_at = { text, at_ms: atMs, failed: "" };
      w.changed();
      return "Cue sends it then";
    },
    send_at_drop: ({ sessionId }) => {
      const s = w.session(sessionId), l = s?.send_at;
      if (!l) return null;
      delete s.send_at;
      w.changed();
      return l.text;
    },
    send_at_now: ({ sessionId }) => {
      const s = w.session(sessionId), l = s?.send_at;
      if (!l) throw "nothing to send later";
      delete s.send_at;
      w.working(s, l.text, []);
      w.changed();
      return "Sent";
    },
  };
});
