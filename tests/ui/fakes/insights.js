// Fakes for src-tauri/src/ipc/insights.rs: a session's cache and context (insight_state), the hand-off skills each
// machine has, and hand-offs (sent, written ahead, continued, kept, started fresh).
CueBridge.area("insights", (w) => {
  const M = 60000;
  const handoffs = () => (w.state.skill_handoffs ||= {});
  const counts = [{ rule: "cache", shown: 14, acted: 8, tokens: 2300000 }, { rule: "handoff", shown: 9, acted: 3, tokens: 0 }];
  /** A new session that carries on from `sid`: its id and what the window toasts. */
  const successor = (sid, how) => {
    const s = w.session(sid), next = `${sid}-next`;
    w.state.sessions.push({ ...structuredClone(s), session_id: next, state: "working", since_ms: w.now(), thread: [], segments: [] });
    w.changed();
    return { session_id: next, detail: `${how} in a new session of ${s?.project || "it"}` };
  };

  return {
    // ?cost=cold|warm|soon: every session asked about has 600k of context; its cache gone, or 8 or 3 minutes left.
    insight_state: ({ sessionIds }) => {
      const cost = w.q.get("cost");
      if (!cost) return {};
      const until = { warm: w.now() + 8 * M, soon: w.now() + 3 * M }[cost] || w.now() - 180 * M;
      return Object.fromEntries(sessionIds.map((sid) => [sid, { until_ms: until, tokens: 600000, start: 45000, last_ms: w.now() - 240 * M }]));
    },
    insight_host_skills: () => ({ hosts: { "": ["handoff", "relay:handoff"], "build-box": [], "gpu-1": null }, reading: false }),
    insight_log: () => null,
    insight_counts: () => counts,
    handoff_sent: ({ sessionId, skill, resume }) => { handoffs()[sessionId] = { skill, sent_ms: w.now(), next: "", continued_ms: 0, resume: !!resume }; w.changed(); return null; },
    handoff_cue: ({ sessionId, ahead }) => { handoffs()[sessionId] = { skill: "cue:handoff", sent_ms: w.now(), next: "", continued_ms: 0, resume: false, ...(ahead ? { ahead: 2, note_ms: 0 } : {}) }; w.changed(); return ahead ? "Writing its hand-off note" : "Handing off"; },
    handoff_keep: ({ sessionId }) => { delete handoffs()[sessionId]; w.changed(); return null; },
    ahead_watch: ({ sessionId, on }) => {
      const list = new Set(w.state.ahead_watch || []);
      if (on) list.add(sessionId); else list.delete(sessionId);
      w.state.ahead_watch = [...list];
      w.changed();
      return null;
    },
    handoff_continue: async ({ sessionId }) => { await w.wait(600); return successor(sessionId, "Continued"); },
    handoff_fresh: async ({ sessionId }) => { await w.wait(600); return successor(sessionId, "Started fresh"); },
  };
});
