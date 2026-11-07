// Fakes for src-tauri/src/ipc/insights_tab.rs: the Insights view's numbers and the board's "could have delegated".
// The summary is made up but steady (a seeded generator), shaped like the owner's last 30 days: ~5,500 turns,
// ~250 late answers re-writing ~59M tokens (≈ $470). ?itab=empty: no turns read yet. ?relay=off: relay not set up.
CueBridge.area("insights_tab", (w) => {
  const D = 86400000, H = 3600000, M = 60000;
  /** The same "random" number for the same n, every run. */
  const rnd = (n) => { const x = Math.sin(n * 12.9898) * 43758.5453; return x - Math.floor(x); };
  const dayKey = (t) => { const d = new Date(t); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; };
  /** A row naming a session: the fields every list in the summary shares. */
  const who = (session_id, project, o) => ({ session_id, project, cwd: `/x/${project}`, machine: "", harness: "claude", ...o });
  const empty = (days) => ({ days, since_ms: w.now() - days * D, totals: { turns: 0, sessions: 0, tokens: 0, usd: 0, priced: 1 }, by_day: [], waiting: { usd: 0, tokens: 0, turns: 0, share_of_writes: 0, sessions: [] }, long: { usd: 0, tokens: 0, sessions: [], handoffs: { sent: 0, continued: 0 } }, corrections: { count: 0, share: 0, examples: [], by_project: [] }, split: {}, models: [], insights: [] });

  function byDay(days) {
    const rows = [];
    for (let k = days - 1; k >= 0; k--) {
      const t = w.now() - k * D, wd = new Date(t).getDay();
      if ((wd === 0 || wd === 6) && rnd(k) < 0.6) continue;
      const turns = Math.round(130 + rnd(k + 1) * 140), tokens = Math.round(turns * (150000 + rnd(k + 2) * 180000));
      const wait_tokens = Math.round(rnd(k + 8) * rnd(k + 9) * 8400000);
      rows.push({ day: dayKey(t), turns, tokens, usd: +(tokens * 2.6e-6).toFixed(2), median_context: Math.round(60000 + rnd(k + 4) * 90000 + (days - k) * 400), errors: Math.round(rnd(k + 5) * 14), interrupted: Math.round(rnd(k + 6) * 5), corrected: Math.round(rnd(k + 7) * 12), wait_tokens, wait_usd: +(wait_tokens * 7.9e-6).toFixed(2), long_usd: +(rnd(k + 10) * 16).toFixed(2) });
    }
    return rows;
  }

  /** Late answers: mostly left overnight and over lunch on weekdays; when the wait began, how long, what it re-wrote. */
  function waitingOf(rows, days, at) {
    const sum = (k) => rows.reduce((a, r) => a + r[k], 0);
    const usd = sum("wait_usd"), tokens = sum("wait_tokens");
    const ws = [["relay", "agent-relay", 9, 51840000, 0.21], ["c1", "cue", 7, 30 * H, 0.16], ["c2", "order-engine", 5, 2 * D + 5 * H, 0.11], ["q1", "data-pipeline", 6, 9 * H, 0.08], ["c3", "cue", 4, 14 * H, 0.06], ["c4", "web-app", 3, 3 * H + 20 * M, 0.04], ["c5", "filings-bot", 3, 70 * M, 0.03], ["c6", "backtester", 2, 26 * H, 0.02]];
    const waiting = { usd: +usd.toFixed(2), tokens, turns: Math.round(tokens / 236000), share_of_writes: 0.8,
      sessions: ws.map(([sid, project, times, wait, sh], i) => who(sid, project, { usd: +(usd * sh).toFixed(2), tokens: Math.round(tokens * sh), times, longest_wait_ms: wait, last_ms: at(5 + i * 19) })) };
    waiting.answers = Array.from({ length: Math.min(400, waiting.turns) }, (_, i) => {
      const r = ws[Math.floor(rnd(i + 30) * rnd(i + 31) * ws.length)], night = rnd(i + 32) < 0.55;
      const waited = night ? (8 + rnd(i + 33) * 10) * H : rnd(i + 34) < 0.5 ? (6 + rnd(i + 35) * 54) * M : (1 + rnd(i + 36) * 5) * H;
      const tok = Math.round(40000 + rnd(i + 37) * rnd(i + 38) * 560000);
      return who(r[0], r[1], { at_ms: w.now() - rnd(i + 39) * days * D, waited_ms: Math.round(waited), tokens: tok, usd: +(tok * (waited > H ? 7.8e-6 : 4.8e-6)).toFixed(2) });
    }).sort((a, b) => b.usd - a.usd);
    waiting.grid_usd = Array(168).fill(0);
    waiting.grid_turns = Array(168).fill(0);
    waiting.answers.forEach((a, i) => {
      const night = a.waited_ms >= 8 * H, wd = Math.floor(rnd(i + 40) * (rnd(i + 41) < 0.85 ? 5 : 7));
      const h = night ? [17, 18, 19, 20, 21, 22, 23, 0][Math.floor(rnd(i + 42) * 8)] : [9, 10, 11, 12, 12, 13, 14, 15, 16][Math.floor(rnd(i + 43) * 9)];
      waiting.grid_usd[wd * 24 + h] += a.usd;
      waiting.grid_turns[wd * 24 + h]++;
    });
    waiting.grid_usd = waiting.grid_usd.map((x) => +x.toFixed(2));
    return waiting;
  }

  /** Long sessions, each with its climb: slow, then steep past its hand-off point. */
  function longOf(usd, at) {
    const ls = [["c1", "cue", 45000, 610000, 212, 0.18], ["relay", "agent-relay", 38000, 480000, 164, 0.14], ["c7", "order-engine", 52000, 390000, 120, 0.09], ["c3", "cue", 30000, 340000, 96, 0.06], ["c8", "data-pipeline", 41000, 320000, 88, 0.04]];
    const long = { usd: +usd.toFixed(2), tokens: Math.round(usd * 3.2e6), handoffs: { sent: 7, continued: 5 },
      sessions: ls.map(([sid, project, s0, peak, n, sh], i) => who(sid, project, { usd: +(usd * sh).toFixed(2), start_context: s0, peak_context: peak, turns: n, last_ms: at(3 + i * 31) })) };
    long.sessions.forEach((s, j) => {
      const n = s.turns + 40 + j * 15, pts = [];
      for (let i = 0; i < n; i++) pts.push([i, Math.round(s.start_context + (s.peak_context - s.start_context) * Math.pow(i / (n - 1), 1.1 + j * 0.15) * (0.94 + rnd(i + j * 99) * 0.06))]);
      const step = Math.max(1, Math.ceil(n / 60));
      s.climb = pts.filter((_, i) => i % step === 0 || i === n - 1);
      s.climb_turns = n;
      s.handoff_turn = pts.find(([, c]) => c >= 300000 && c >= 4 * s.start_context)?.[0] ?? null;
      if (s.handoff_turn != null) s.turns = n - s.handoff_turn;
    });
    return long;
  }

  function correctionsOf(count, turns, at) {
    const ex = [
      ["c1", "cue", "Make the Back row in the agent view stay put when the steps scroll", "No, the Back row should stay above the steps, not float over them. And Back must land at the chat's end."],
      ["relay", "agent-relay", "Add a spend column to the board", "Sort it high to low, and keep the status column where it was. The test in board.test.ts checks the order."],
      ["c2", "order-engine", "Fix the fill price on partial exits", "That changed entries too. Only exits: the entry price is right. Check it against yesterday's replay."],
      ["q1", "data-pipeline", "Backfill the missing days", "Only the weekdays in September, not the whole year. Dry-run first and show me the count."],
      ["c4", "web-app", "Tidy up the settings page", "I meant the spacing, not the copy. Leave every label as it was."],
      ["c3", "cue", "Show the MCP servers in the session menu", "From the ⋯ menu, not the header. And list what it called through each."],
    ];
    return { count, share: +(count / turns).toFixed(3),
      examples: ex.map(([sid, project, prompt, correction], i) => who(sid, project, { at_ms: at(2 + i * 17), prompt, correction, usd: +(0.6 + rnd(i) * 3).toFixed(2) })),
      by_project: [["web-app", 31, 410], ["cue", 52, 1890], ["order-engine", 24, 720], ["agent-relay", 28, 1240], ["data-pipeline", 9, 530], ["backtester", 4, 260]].map(([project, c, t]) => ({ project, count: c, turns: t })) };
  }

  /** Could have delegated: turns that looked around themselves, and long sessions across several areas. */
  function delegateOf(at) {
    const dlr = [["c1", "cue", 64, 188000, 21, 4.1], ["c2", "order-engine", 52, 141000, 30, 3.2], ["q1", "data-pipeline", 38, 97000, 12, 1.4], ["c4", "web-app", 33, 84000, 6, 0.4]];
    const spread = [["c7", "order-engine", ["src", "tests", "scripts", "web", "/x/shared/lib"], 520000], ["c3", "cue", ["src-tauri", "ui", "tests"], 410000], ["c8", "data-pipeline", ["dags", "lib", "sql"], 330000]];
    return { usd: 12.6, tokens: 25200000, turns: 31, looks_min: 30, growth_min: 80000, areas_min: 3, agents: 412, agent_sessions: 96, relay_spawns: 188, relay: w.q.get("relay") !== "off", spread_count: 3,
      sessions: dlr.map(([sid, project, looks, growth, later, usd], i) => who(sid, project, { last_ms: at(4 + i * 22), at_ms: at(4 + i * 22), prompt: "", looks, growth, later, usd, tokens: Math.round(usd * 2e6), turns: 1 + (i % 2) })),
      spread: spread.map(([sid, project, areas, peak], i) => who(sid, project, { last_ms: at(9 + i * 40), areas, usd: 0, peak_context: peak })) };
  }

  function summary({ days = 30 }) {
    if (w.q.get("itab") === "empty") return empty(days);
    const at = (h) => w.now() - h * H;
    const rows = byDay(days), sum = (k) => rows.reduce((a, r) => a + r[k], 0);
    const turns = sum("turns");
    const totals = { turns, tokens: sum("tokens"), sessions: Math.round(days * 3.4), usd: +sum("usd").toFixed(2), priced: 0.97 };
    const waiting = waitingOf(rows, days, at), long = longOf(sum("long_usd"), at);
    // Where it went: the late answers' re-writes and the long sessions' re-reads, the rest split by kind.
    const rest = totals.usd - waiting.usd - long.usd;
    const split = { fresh: +(rest * 0.04).toFixed(2), output: +(rest * 0.17).toFixed(2), read: +(rest * 0.43).toFixed(2), write: +(rest * 0.36).toFixed(2), rewrite: waiting.usd, long: long.usd };
    const models = [["claude-opus-5-5", 0.71], ["claude-fable-5-1", 0.16], ["claude-sonnet-5-5", 0.09], ["claude-haiku-5-5", 0.01], ["gpt-5.2-codex", 0]].map(([model, sh]) => ({ model, usd: +(totals.usd * sh).toFixed(2), tokens: Math.round(totals.tokens * (sh || 0.03)) }));
    return { days, since_ms: w.now() - days * D, totals, by_day: rows, waiting, long, delegate: delegateOf(at), corrections: correctionsOf(sum("corrected"), turns, at), split, models,
      insights: [{ rule: "cache", shown: 14, acted: 8, tokens: 2300000 }, { rule: "handoff", shown: 9, acted: 3, tokens: 0 }] };
  }

  return {
    insights_summary: summary,
    // ?del=1: the board's sessions could have delegated (relay on): one looked around itself, one ran long across areas.
    insight_delegate: () => !w.q.has("del") ? { relay: false, sessions: {} } : { relay: true, sessions: {
      det: { search: { looks: 46, growth: 132000, tokens: 1840000, usd: 0.92, later: 14 }, searches: 1, search_tokens: 1840000, search_usd: 0.92, split: [], agents: 0, relay: 0 },
      sec: { search: null, searches: 0, search_tokens: 0, search_usd: 0, split: ["src", "tests", "scripts", "/x/shared/lib"], agents: 2, relay: 0 } } },
  };
});
