// Fakes for src-tauri/src/ipc/chat.rs, the part about messages: sending, the "/" menu, earlier messages, search,
// by-the-way questions. (Steps are in steps.js, MCP servers in mcp.js.)
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
    cmd("design-login", "Sign in to Claude Design", { kind: "terminal" }),
    skill("commit-push-merge", "Land in-flight work: commit, push, and merge"),
    skill("relay:spawn", "Start an executor on a packet"),
    skill("relay:verify", "Check an executor's report against what it staged"),
  ];
  /** Send fixes (hub/team/mod.rs): the teammate is back at work on its review's findings, so its "Review not sent" row
   *  leaves Open (hub/open.rs). */
  const sentFixes = (agent) => {
    const t = (w.state.sessions || []).flatMap((s) => s.team || []).find((x) => x.agent_id === agent || x.call === agent);
    if (t) t.state = "working";
    for (const l of w.state.open || []) l.rows = l.rows.filter((r) => !(r.kind === "review" && r.agent === agent));
    w.changed();
    return `fixes: ${agent}`;
  };
  /** A crew member's diff as hub/team/mod.rs writes it: its own repo's files from its top, another repo's (its folder's
   *  first two levels) under that repo's name. */
  const mateDiff = (agent) => {
    const t = (w.state.sessions || []).flatMap((s) => s.team || []).find((x) => x.agent_id === agent || x.call === agent);
    const files = t?.files?.length ? t.files : [`${agent}.txt`];
    return files.map((f) => {
      // A file outside any repository (a scratch file under /tmp or /var/folders) isn't in the diff, as the hub says
      // (hub/team/mod.rs changes: "<file>: not in a git repository").
      if (/^(\/private)?\/(tmp|var\/folders)\//.test(f)) return `${f}: not in a git repository\n`;
      const p = f.startsWith("/") ? f.replace(/^\/[^/]+\//, "") : f;
      return `diff --git a/${p} b/${p}\nnew file mode 100644\n--- /dev/null\n+++ b/${p}\n@@ -0,0 +1,2 @@\n+# ${p.split("/").pop()}\n+${t?.name || agent} wrote this\n`;
    }).join("");
  };
  /** A crew lead's job and what it decided on its own (hub: crew_job, crew_log), for a scenario whose lead has a crew
   *  but none yet (world-team, built before the hub kept them): the lead split the work, Cue reviewed and handled
   *  docs and handoff, held rows behind handoff, left spike out. Once, the first time the state is read. */
  const crewLog = (s, at) => [
    { id: `d${at - 40 * 60000}-0`, at_ms: at - 40 * 60000, who: "lead", kind: "split", what: "Split the job into 5: hub, rows, skill, handoff, spike", why: "", one_way: false, sha: "", back: "" },
    { id: `d${at - 25 * 60000}-0`, at_ms: at - 25 * 60000, who: "lead", kind: "decided", what: "Kept rows in one file over a split per kind", why: "two files would both change `ui/core.js`", one_way: false, sha: "", back: "" },
    { id: `d${at - 18 * 60000}-0`, at_ms: at - 18 * 60000, who: "spike", kind: "aside", what: "Left out of the work: tried it, not worth it", why: "", one_way: false, sha: "", back: "" },
    { id: `d${at - 9 * 60000}-0`, at_ms: at - 9 * 60000, who: "rows", kind: "hold", what: "Held until handoff lands", why: "both change `ui/core.js`", one_way: false, sha: "", back: "" },
    { id: `d${at - 4 * 60000}-0`, at_ms: at - 4 * 60000, who: "handoff", kind: "fixes", what: "Reviewed: 1 should-fix, 1 note. Sent the fix back", why: "", one_way: false, sha: "", back: "", call: "call-handoff", head: "reviewed: 1 should-fix, 1 note", steps: ["sent back"] },
    { id: `d${at - 60000}-0`, at_ms: at - 60000, who: "docs", kind: "commit", what: "Reviewed: 1 note, left as is. Committed to `crew/docs`", why: "", one_way: false, sha: "3f2a1c9", back: "", call: "call-docs", head: "reviewed: 1 note", steps: ["left as is", "committed `crew/docs`"] },
  ].filter((d) => d.who === "lead" || s.team.some((t) => t.name === d.who));
  const crewFill = () => {
    for (const s of w.state?.sessions || []) {
      if (!s.team?.length || s.crew_log !== undefined) continue;
      s.crew_job ||= String(s.prompt || "").split("\n")[0].trim().slice(0, 140);
      s.crew_log = crewLog(s, w.state.now_ms || w.now());
    }
  };
  const plainSnapshot = w.snapshot;
  w.snapshot = () => { crewFill(); return plainSnapshot(); };
  const menu = () => w.q.get("ins") === "cue" ? [compact]
    : w.q.get("ins") === "1" ? [compact, skill("relay:handoff", "Hand this lead off to a fresh successor: it writes its notes and opens the next one")]
    : fullMenu;
  /** `n` made-up messages, a minute apart from `from_ms`, alternating you and the agent. */
  const page = (n, from, label) => Array.from({ length: n }, (_, i) => ({ at_ms: from + i * 60000, role: i % 2 ? "agent" : "you", text: `${label} ${i + 1}`, images: [] }));
  const before = (all, { beforeMs, limit }) => all.filter((e) => e.at_ms < beforeMs).slice(-(limit || 50));

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
    // A hosted lead's teammate's buttons (ipc/team.rs): Diff answers with a diff of its files (one in another repo
    // under that repo's name, as the hub writes it), Commit is refused as on main (the scenario's folder), the rest say
    // what was done.
    // A teammate's diff per file (ipc/team.rs team_diff_stat, team_diff), cut from the same fake diff.
    team_diff_stat: ({ agent }) => mateDiff(agent).split(/(?=^diff --git )/m).filter((part) => part.startsWith("diff --git ")).map((part) => ({ path: /^\+\+\+ b\/(.*)$/m.exec(part)?.[1] || "", add: part.split("\n").filter((l) => /^\+(?!\+\+)/.test(l)).length, del: part.split("\n").filter((l) => /^-(?!--)/.test(l)).length })),
    team_diff: ({ agent, path }) => mateDiff(agent).split(/(?=^diff --git )/m).find((part) => part.includes(`+++ b/${path}\n`)) || "",
    team_action: ({ agent, action }) => action === "commit" ? Promise.reject("Not committed: its folder is on main. Cue commits a teammate's work only on a branch of its own: Make a branch and commit puts it on a new one") : action === "branch" ? `Committed 1 file to team/${agent} (c71e04b); your folder stays on main.` : action === "diff" ? mateDiff(agent) : action === "fixes" ? sentFixes(agent) : `${action}: ${agent}`,
    // The Team switch, and a task dropped on a lead (ipc/team.rs): the files it names, those world-team's rows is
    // changing marked.
    team_mode: () => null,
    crew_commit: () => null,
    crew_review: () => null,
    // Go back on a crew decision (ipc/team.rs crew_go_back): the lead is told, the entry marked asked; once only.
    crew_go_back: ({ sessionId, id }) => {
      const d = (w.session(sessionId)?.crew_log || []).find((x) => x.id === id);
      if (!d) return Promise.reject("No such decision");
      if (d.back === "asked") return Promise.reject("Already asked");
      d.back = "asked";
      w.changed();
      return d.one_way ? `Asked the lead to make a revert: ${d.who}` : `Asked the lead to go back on it: ${d.who}`;
    },
    team_hand_out_preview: ({ text }) => {
      const files = [...new Set(String(text).match(/[\w./-]+\.[a-z]{1,6}\b/g) || [])].map((path) => ({ path, by: ["ui/team.js", "ui/core.js"].includes(path) ? "rows is changing it" : "" }));
      return { lead: "cue", files, overlap: files.some((f) => f.by) };
    },
    team_hand_out: () => "Handed to cue as a new teammate",
    // By the way: a side question, answered after a moment (the real one asks a copy of the conversation).
    btw: async () => { await w.wait(600); return "Because the CSV export is read by the **finance script**, which expects the old order. Changing it would break their import, so it only changed the board."; },
    restart_session: async ({ sessionId }) => { await w.wait(300); return `Restarted ${w.session(sessionId)?.project || "it"}`; },
  };
});
