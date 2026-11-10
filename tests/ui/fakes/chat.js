// Fakes for src-tauri/src/ipc/chat.rs, the part about messages: sending, the "/" menu, earlier messages, search,
// by-the-way questions, tasks. (Steps are in steps.js, MCP servers in mcp.js.)
CueBridge.area("chat", (w) => {
  const H = 3600000;
  /** "/" menu entries of a Claude Code session (?ins=1: a hand-off skill too; ?ins=cue: no skill at all). */
  const cmd = (name, desc, o = {}) => ({ name, kind: "command", desc, ...o });
  const skill = (name, desc) => ({ name, kind: "skill", desc });
  const compact = cmd("compact", "Summarize the conversation to free up context");
  const fullMenu = [
    compact,
    cmd("clear", "Start over: clear the conversation"),
    cmd("context", "How full the context is"),
    cmd("model", "Set the AI model for Claude Code", { hint: "<model>", args: [
      { value: "default", label: "Default (recommended)", desc: "Opus 5.5 · Best for everyday, complex tasks" },
      { value: "opus", label: "Opus 5.5", desc: "For complex work and everyday tasks" },
      { value: "sonnet", label: "Sonnet 5.5", desc: "Most efficient for simpler tasks" },
      { value: "haiku", label: "Haiku 4.5", desc: "Fastest for quick answers" }] }),
    cmd("effort", "Set effort level for model usage", { hint: "<low|medium|high|xhigh|max|auto>", args: ["low", "medium", "high", "xhigh", "max", "auto"].map((v) => ({ value: v, label: v, desc: "" })) }),
    cmd("rename", "Rename the current conversation", { hint: "[name]", args: [] }),
    skill("commit-push-merge", "Land in-flight work: commit, push, and merge"),
    skill("relay:spawn", "Start an executor on a packet"),
    skill("relay:verify", "Check an executor's report against what it staged"),
  ];
  const menu = () => w.q.get("ins") === "cue" ? [compact]
    : w.q.get("ins") === "1" ? [compact, skill("relay:handoff", "Hand this lead off to a fresh successor: it writes its notes and opens the next one")]
    : fullMenu;
  /** `n` made-up messages, a minute apart from `from_ms`, alternating you and the agent. */
  const page = (n, from, label) => Array.from({ length: n }, (_, i) => ({ at_ms: from + i * 60000, role: i % 2 ? "agent" : "you", text: `${label} ${i + 1}`, images: [] }));
  const before = (all, { beforeMs, limit }) => all.filter((e) => e.at_ms < beforeMs).slice(-(limit || 50));
  const tasksOf = (sid) => (w.state.tasks ||= {})[sid] ||= { items: [], busy: "", next: 0, spent: { tokens: 0, usd: 0 } };
  /** The settings list `key` with `sid` put in or taken out (ipc/chat.rs status_session, background_session). */
  const listSetting = (area) => ({ sessionId, on }) => {
    const s = (w.state.settings[area] ||= {}), list = new Set(s.sessions || []);
    if (on) list.add(sessionId); else list.delete(sessionId);
    s.sessions = [...list];
    w.changed();
    return null;
  };

  return {
    send_to_session: async ({ sessionId, text, images, now }) => {
      if (w.q.has("slow")) await w.wait(4000);   // ?slow=1: typing into a terminal takes a moment
      const s = w.session(sessionId);
      if (!s) throw "Cue has no record of that session";
      // Mid-turn, and not "now": Cue keeps it and sends it when the turn ends (hub/send.rs).
      if (s.state === "working" && !now) { s.held = { role: "you", text, at_ms: w.now(), images: images || [] }; w.changed(); return "kept in Cue: it goes when this turn ends"; }
      w.working(s, text, images);
      w.changed();
      return now ? "stopped via tmux pane %1" : "via tmux pane %1";
    },
    send_queued_now: ({ sessionId }) => { const s = w.session(sessionId); if (s) delete s.queued; w.changed(); return "sent now"; },
    interrupt_session: ({ sessionId }) => { const s = w.session(sessionId); if (s) { s.state = "waiting"; s.since_ms = w.now(); } w.changed(); return "Pressed Esc in its terminal"; },
    session_command: ({ action }) => `Sent /${action}`,
    session_commands: () => ({ list: menu(), reading: false }),
    models_list: () => ({ models: ["default", "opus", "sonnet", "haiku"].map((value) => ({ value, label: value })), reading: false }),
    focus_session: () => "iTerm session",
    focus_session_id: () => "iTerm session",
    focus_live: () => "iTerm session",
    // ⌘F: one closed session matching (a Claude Code one, so it offers Resume).
    search: () => [{ kind: "session", session_id: "old-1", name: "checkout-flow", project: "web-shop", harness: "claude", at_ms: w.now() - 24 * H, ended: true, last_role: "agent", last: "Merged the coupon fix." }],
    session_log: ({ sessionId }) => w.session(sessionId)?.thread || (w.state.ended || []).find((x) => x.session_id === sessionId)?.thread || [],
    // Earlier messages, a page at a time: 75 made-up ones from Cue's log, 40 more from the transcript.
    session_log_page: (a) => before(page(75, w.now() - 3 * H, "Earlier message"), a),
    transcript_page: (a) => before(page(40, w.now() - 9 * H, "From the transcript"), a),
    // A hosted lead's teammate's buttons (ipc/team.rs): Diff answers with a diff, Commit is refused as on main (the
    // scenario's folder), the rest say what was done.
    team_action: ({ agent, action }) => action === "commit" ? Promise.reject("Not committed: its folder is on main. Cue commits a teammate's work only on a branch of its own: Make a branch and commit puts it on a new one") : action === "branch" ? `Committed 1 file to team/${agent} (c71e04b); your folder stays on main.` : action === "diff" ? `diff --git a/${agent}.txt b/${agent}.txt\nnew file mode 100644\n--- /dev/null\n+++ b/${agent}.txt\n@@ -0,0 +1 @@\n+${agent} wrote this` : `${action}: ${agent}`,
    // The Team switch, and a task dropped on a lead (ipc/team.rs): the files it names, those world-team's rows is
    // changing marked.
    team_mode: () => null,
    team_hand_out_preview: ({ text }) => {
      const files = [...new Set(String(text).match(/[\w./-]+\.[a-z]{1,6}\b/g) || [])].map((path) => ({ path, by: ["ui/team.js", "ui/core.js"].includes(path) ? "rows is changing it" : "" }));
      return { lead: "cue", files, overlap: files.some((f) => f.by) };
    },
    team_hand_out: () => "Handed to cue as a new teammate",
    // By the way: a side question, answered after a moment (the real one asks a copy of the conversation).
    btw: async () => { await w.wait(600); return "Because the CSV export is read by the **finance script**, which expects the old order. Changing it would break their import, so it only changed the board."; },
    catch_up: async () => { await w.wait(600); return { points: ["Finished the spend column; tests pass.", "Kept the CSV export's old column order."], needs: "Carry spend over to a successor, or not?" }; },
    tasks_start: ({ sessionId }) => { tasksOf(sessionId).busy = "listing"; (w.state.settings.tasks ||= {}).on = true; w.changed(); return null; },
    tasks_refresh: ({ sessionId }) => { tasksOf(sessionId).busy = "listing"; w.changed(); return null; },
    tasks_stop: ({ sessionId }) => { delete w.state.tasks?.[sessionId]; w.changed(); return null; },
    status_session: listSetting("status"),
    background_session: listSetting("background"),
    restart_session: async ({ sessionId }) => { await w.wait(300); return `Restarted ${w.session(sessionId)?.project || "it"}`; },
  };
});
