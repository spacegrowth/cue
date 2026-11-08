// The preview's sample world: the window's state (a snapshot, as hub/mod.rs snapshot() builds it) for
// tests/preview.html, shaped by its ?flags, and the window's own saved choices (localStorage) that go with them.
// Also what tests/ui/record.mjs records named scenarios from (tests/ui/scenarios/*.json). Runs in a browser and
// in node: it reads nothing but its arguments.
//   q: URLSearchParams of the flags; now: the clock (ms).
// Returns { state, prefs }: prefs maps localStorage keys to the values to save.
function previewWorld(q, now) {
  const prefs = {};
  // ?parked=det,sec: those sessions start in Need to decide.
  prefs["cue.parked"] = q.get("parked") ? q.get("parked").split(",") : [];
  // ?settab=<tab>: Settings opens on that tab.
  if (q.get("settab")) prefs["cue.setTab"] = q.get("settab");
  // ?qdraft=1: something typed in the working order-engine's box (the queue note by Now, insights.js).
  if (q.get("qdraft")) prefs["preview.drafts"] = { "s:eng": { text: "also keep the old key valid for a day", images: [] } };
  // ?later=1: Need to decide open (it starts folded).
  prefs["cue.laterOpen"] = q.get("later") === "1" ? "1" : "0";
  const M = 60000;
  const seg = (kind, a, b) => ({ kind, start_ms: now - a * M, end_ms: b == null ? null : now - b * M });
  const sess = (sid, project, harness, st, since, segments, prompt = "") => ({ session_id: sid, project, harness, cwd: `/Users/me/code/${project}`, state: st, since_ms: now - since * M, prompt, segments });
  const relayMsg = "Packet 3 is done and staged (not committed).\n\n**What changed**\n- The board now shows token spend per executor, summed from each session's usage log.\n- A new `spend` column sorts high to low; totals sit in the footer.\n- `relay:list` prints the same number.\n\nTested: unit tests pass, and I opened the board against your live sessions — the numbers match `/cost` in each tab.\n\nOne thing to decide: spend resets when an executor is retired. Keep it per session, or carry it over to the successor? I'd carry it over.";
  const state = {
    now_ms: now, window_ms: 30 * M,
    settings: { history: { keep: 300 }, notify: { finished: true, decisions: true }, pi: { gate: "dangerous" }, appearance: { mode: q.get("theme") || "system" }, quick: { phrases: ["Commit", "Go ahead", "Think hard"] }, insights: { handoff: true, cache: true, queue: true, skills: q.get("ins") === "1" ? { "|claude": "relay:handoff", "build-box|codex": "handoff" } : {} } },
    // ?ins=sent: agent-relay sent /handoff and hasn't continued (insights.js handoffNext); ?ins=sentcue: Cue's own;
    // ?ins=continued: it continued in order-engine and is still open (handoffContinued).
    // ?ahead=warm|cold|writing: agent-relay wrote a hand-off note before its cache ran out (insights.js aheadCard);
    // ?ahead=watch|ticked|soon: its "Hand off if I don't come back" (untouched, ticked) and, with 4 minutes left, Write hand-off now.
    // ?ahead=gone|goneticked: its cache ran out 12 hours ago (Start fresh), unticked or ticked.
    ahead_watch: ["ticked", "goneticked"].includes(q.get("ahead")) ? ["relay"] : [],
    skill_handoffs: ["warm", "cold", "writing"].includes(q.get("ahead")) ? { relay: { skill: "cue:handoff", sent_ms: now - (q.get("ahead") === "writing" ? 20000 : 70 * 60000), next: "", continued_ms: 0, resume: false, ahead: 2, note_ms: q.get("ahead") === "writing" ? 0 : now - 69 * 60000 } } : q.get("ins") === "continued" ? { relay: { skill: "handoff", sent_ms: now - 300000, next: "eng", continued_ms: now - 240000, resume: true } } : q.get("ins") === "sentcue" ? { relay: { skill: "cue:handoff", sent_ms: now - 120000, next: "", continued_ms: 0, resume: false } } : q.get("ins") === "sent" ? { relay: { skill: "handoff", sent_ms: now - 120000, next: "", continued_ms: 0, resume: true } } : {},
    connections: { claude: { ok: true, events: ["PermissionRequest", "Stop", "UserPromptSubmit", "SessionEnd"] }, pi: { ok: true } },
    // ?setup=1: first launch, Claude Code and Codex installed but not connected (the setup screen).
    sessions: [
      sess("relay", "agent-relay", "claude", q.get("ahead") === "writing" ? "working" : "waiting", 11, [seg("working", 30, 11), seg("waiting", 11)]),
      { ...sess("det", "backtester", "claude", "waiting", 4, [seg("working", 30, 4), seg("waiting", 4)]), background: 1 },
      sess("sec", "filings-bot", "pi", "waiting", 1, [seg("working", 16, 1), seg("waiting", 1)]),
      sess("pipe", "data-pipeline", "claude", "deciding", 3, [seg("working", 24, 3), seg("waiting", 3)]),
      sess("web", "web-app", "pi", "deciding", 1, [seg("working", 30, 21), seg("waiting", 21, 18), seg("working", 18, 1), seg("waiting", 1)]),
      sess("api", "api-server", "claude", "deciding", 0.2, [seg("working", 12, 0.2), seg("waiting", 0.2)]),
      sess("eng", "order-engine", "claude", "working", 6, [seg("working", 6)], "Refactor the order router so partial fills reuse the same broker session"),
      sess("alerts", "alerts", "pi", "working", 2, [seg("working", 2)], "Find why webhook retries double-fire"),
      // At its prompt while two background agents work (Claude Code's Agent tool): waiting on them, not you.
      { ...sess("names", "cue", "claude", "agent", 2, [seg("working", 5, 2), seg("idle", 2)], "Map every name a session has, backend and UI"), driven_by: "2 background agents", helpers: 2, background: 2,
        thread: [{ role: "you", text: "Map every name a session has, backend and UI.", at_ms: now - 5 * M }, { role: "agent", text: "Two agents are mapping the names. I'll put the answer together when they report.", at_ms: now - 2 * M }] },
    ],
    items: [
      { id: "n1", kind: "waiting", harness: "claude", session_id: "relay", cwd: "/Users/me/code/agent-relay", project: "agent-relay", message: relayMsg, context: [], created_ms: now - 11 * M, status: "pending", outcome: "" },
      { id: "n2", kind: "waiting", harness: "claude", session_id: "det", cwd: "/x/backtester", project: "backtester", message: "The backtest for the breakout setup finished.\n\n- Fills now use the next bar's open, so entries no longer look ahead.\n- Fewer trades and a lower win rate than yesterday's run — that's the look-ahead coming out, not a regression.\n- The report is in `runs/breakout-0103.html`.\n\nNext I'd tune the stop distance. Go ahead, or review the report first?", context: [], created_ms: now - 4 * M, status: "pending", outcome: "" },
      { id: "n3", kind: "waiting", harness: "pi", session_id: "sec", cwd: "/x/filings-bot", project: "filings-bot", message: "Pulled the latest annual report and extracted the KPIs into `kpis`.\n\nData-center revenue moved to a new segment this year, so I mapped it by hand — worth a glance before it feeds the dashboard.\n\nWant me to run the next company? It's about the same cost.", context: [], created_ms: now - 1 * M, status: "pending", outcome: "" },
      { id: "n4", kind: "permission", harness: "claude", session_id: "pipe", cwd: "/x/data-pipeline", project: "data-pipeline", tool_name: "Bash", tool_input: { command: "uv run pytest tests/lines -q -x", description: "Run the chart_lines tests" },
        suggestions: [{ type: "addRules", rules: [{ toolName: "Bash", ruleContent: "uv run pytest:*" }], behavior: "allow", destination: "localSettings" }],
        context: [{ role: "assistant", text: "Checking the fix didn't break other tests." }], created_ms: now - 3 * M, status: "pending", outcome: "" },
      { id: "n5", kind: "permission", harness: "pi", session_id: "web", cwd: "/x/web-app", project: "web-app", tool_name: "edit", tool_input: { file_path: "/x/web-app/src/pages/Cart.tsx", old_string: "const MAX_ITEMS = 10;", new_string: "const MAX_ITEMS = 25; // allow bigger carts" }, suggestions: [], context: [{ role: "assistant", text: "Raising the cart limit." }], created_ms: now - 40000, status: "pending", outcome: "" },
      { id: "n6", kind: "question", harness: "claude", session_id: "api", cwd: "/x/api-server", project: "api-server", tool_name: "AskUserQuestion",
        tool_input: { questions: [{ question: "Which cache should the quotes endpoint use?", header: "Cache", multiSelect: false, options: [{ label: "Redis", description: "shared, 30s TTL" }, { label: "In-process LRU", description: "fastest" }, { label: "No cache" }] }] },
        suggestions: [], context: [], created_ms: now - 12000, status: "pending", outcome: "" },
    ],
    history: [
      { id: "h1", kind: "permission", harness: "claude", project: "data-pipeline", tool_name: "Bash", tool_input: { command: "git status" }, status: "answered", outcome: "allowed", created_ms: now - 15 * M, resolved_ms: now - 14 * M },
      { id: "h2", kind: "permission", harness: "pi", project: "web-app", tool_name: "bash", tool_input: { command: "rm -rf node_modules" }, status: "answered", outcome: "denied — “use npm ci instead”", created_ms: now - 32 * M, resolved_ms: now - 30 * M },
      { id: "h3", kind: "waiting", harness: "claude", project: "agent-relay", message: "Packet 2 done.", status: "answered", outcome: "replied — “ship it”", created_ms: now - 70 * M, resolved_ms: now - 65 * M },
    ],
  };
  // ?usage=meter|out|fable|back&upop=1 — the usage pill and its panel.
  // ?tasks=1: every session tracked, a list of what's left (tasks.js); ?tasks=busy: its first list being read;
  // ?tasks=off: none tracked yet. The Tasks tab opens on the session shown.
  state.tasks = {};
  state.tasks_spent = q.get("tasks") === "1" ? { tokens: 412300, usd: 0.087, calls: 23, tracked: 2 } : {};
  if (["1", "busy"].includes(q.get("tasks"))) {
    const it = (id, group, text, st = "open", detail = "") => ({ id, group, text, state: st, at_ms: now - 30 * M, detail });
    const list = q.get("tasks") === "busy" ? { items: [], busy: "listing", spent: { tokens: 0, usd: 0 } } : {
      items: [
        it("t1", "check", "Run the replay against yesterday's fills", "open", "The new router hasn't seen real traffic yet. Replaying yesterday's orders shows whether partial fills still land on the same broker session."),
        it("t2", "check", "Watch one live partial fill in the log"),
        it("t3", "build", "Drop the old session pool once nothing uses it"),
        it("t4", "build", "A counter for reused sessions"),
        it("t5", "decide", "Keep the old key valid for a day, or not", "open", "Brokers holding the old key keep working for a day if you say yes; say no and they reconnect at once."),
        it("t6", "ok", "Retries stay at 3, as before"),
        it("t7", "check", "The router reuses the broker session", "done"),
        it("t8", "ok", "A second pool for slow brokers", "dropped"),
      ],
      next: 8, said: "1 done (you said the reuse works)", err: "", busy: "", spent: { tokens: 63120, usd: 0.0141 }, updated_ms: now - 2 * M,
      about: "Order router: partial fills reuse one broker session",
      now: "Tests pass on the new router. Waiting on you: keep the old key valid for a day?",
      turns: [
        { at_ms: now - 95 * M, line: "Reworked the router to reuse the session" },
        { at_ms: now - 70 * M, line: "Ran the protocol tests: 41 passed" },
        { at_ms: now - 28 * M, line: "Kept the old key valid behind a flag" },
        { at_ms: now - 12 * M, line: "Listed what's left: 2 to check, 2 to build" },
      ],
    };
    state.settings.tasks = { on: true, auto: false };
    // Last looked at every session 40 minutes ago: "Since you looked" shows where that was.
    prefs["cue.looked"] = Object.fromEntries(state.sessions.map((x) => [x.session_id, now - 40 * M]));
    for (const s of state.sessions) state.tasks[s.session_id] = JSON.parse(JSON.stringify(list));
  }
  if (q.has("usage")) {
    const u = q.get("usage"), H = 3600000;
    state.usage = { five_hour: { pct: u === "out" ? 100 : 62, resets_ms: now + 38 * M }, seven_day: { pct: 41, resets_ms: now + 4 * 24 * H }, models: [{ name: "Fable", pct: u === "fable" ? 100 : 88, resets_ms: now + 4 * 24 * H, fetched_ms: now - 3 * H }], at_ms: now - 20000 };
    const lim = (scope, kind, resets, text) => ({ scope, kind, text, resets_ms: resets, since_ms: now - 2 * M });
    const pause = (sid, l) => { const s = state.sessions.find((x) => x.session_id === sid); s.state = "limited"; s.limit = l; s.prompt = s.prompt || "ok do all"; s.since_ms = now - 2 * M; };
    if (u === "out") { const l = lim("all", "session", now + 38 * M, "You've hit your session limit · resets 4:10am"); pause("eng", l); }
    if (u === "fable") pause("eng", lim("Fable", "model", now + 4 * 24 * H, "You've reached your Fable limit. Run /usage-credits to continue or switch models with /model."));
    if (u === "back") { const l = lim("all", "session", now - 1 * M, "You've hit your session limit · resets 4:10am"); pause("eng", l); }
  }
  if (q.has("multi")) state.items.push({ id: "n7", kind: "question", harness: "claude", session_id: "api", cwd: "/x/api-server", project: "api-server", tool_name: "AskUserQuestion",
    tool_input: { questions: [
      { question: "Which editor do you use most?", header: "Editor", multiSelect: false, options: [{ label: "VS Code", description: "Microsoft's popular extensible editor." }, { label: "JetBrains", description: "IntelliJ, PyCharm, GoLand, etc." }, { label: "Vim/Neovim", description: "Modal terminal-based editing." }] },
      { question: "Which operating systems do you develop on?", header: "OS", multiSelect: true, options: [{ label: "macOS" }, { label: "Linux" }, { label: "Windows" }] },
      { question: "How do you prefer code changes explained?", header: "Explain", multiSelect: false, options: [{ label: "Brief", description: "Just the key points." }, { label: "Detailed", description: "Full reasoning and trade-offs." }] },
      { question: "Which test framework layout do you prefer?", header: "Tests", multiSelect: false, options: [{ label: "Next to the code" }, { label: "Separate tests/ folder" }] } ] },
    suggestions: [], context: [], created_ms: now - 5000, status: "pending", outcome: "" });
  // ?crew=1: claude-relay / pi-lead roles. agent-relay leads; data-pipeline and backtester work for it
  // (backtester's lead isn't armed any more); web-app is a pi-lead executor whose lead Cue hasn't seen.
  // ?live=1: the Sessions view's extras: quiet sessions Cue hasn't heard from, and folder branches.
  if (q.has("live")) {
    state.live = [
      { session_id: "q1", harness: "claude", name: "line-tune", cwd: "/Users/me/code/data-pipeline", pid: 1, status: "idle", since_ms: now - 180 * M },
      { session_id: "q2", harness: "claude", name: "", cwd: "/Users/me/code/order-engine", pid: 2, status: "busy", since_ms: now - 25 * M },
      { session_id: "q3", harness: "pi", name: "", cwd: "/Users/me/code/filings-bot", pid: 3, status: "idle", since_ms: now - 2 * M },
      // A worktree of data-pipeline: its project is the repository, so it shares data-pipeline's tile.
      { session_id: "q4", harness: "claude", name: "", cwd: "/Users/me/code/data-pipeline-wt/fill-gaps", pid: 4, status: "busy", since_ms: now - 6 * M },
    ];
    state.projects = { "/Users/me/code/data-pipeline-wt/fill-gaps": { name: "data-pipeline", root: "/Users/me/code/data-pipeline" } };
    state.about = {
      relay: { title: "Relay board token spend", prompt: "go, and keep the footer total" },
      pipe: { title: "Line detector data gaps", prompt: "make the line detector survive data gaps" },
      det: { title: "Breakout backtest look-ahead", prompt: "rerun with next-bar fills" },
      eng: { title: "Order router partial fills", prompt: "Refactor the order router so partial fills reuse the same broker session" },
      api: { title: "Quotes endpoint caching", prompt: "add caching to quotes" },
      q1: { title: "Chart-line finder tuning", prompt: "/line-tune" },
      q2: { title: "Broker session reconnects", prompt: "why do reconnects drop fills?" },
      q4: { title: "Fill data gaps", prompt: "backfill the missing bars" },
    };
    state.branches = Object.fromEntries(state.sessions.map((s) => [s.cwd, s.project === "web-app" ? "cart-limits" : "main"]).concat([["/Users/me/code/data-pipeline", "chart-lines"], ["/Users/me/code/data-pipeline-wt/fill-gaps", "fill-gaps"]]));
  }
  if (q.has("crew")) state.crew = {
    relay: { role: "lead", plugin: "relay", name: "agent-relay", color: [132, 180, 180], model: "claude-opus-5-5", auto: true, plan_next: "docs + deck refresh", plan_queued: 3, executors: [
      { session_id: "pipe", name: "lines-gaps", model: "claude-sonnet-5-5[1m]", status: "busy", packet: 3, goal: "find the gaps in the 5-minute series where the feed dropped bars, and fill them from the daily file" },
      { session_id: "x-spend", name: "board-spend", model: "claude-sonnet-5-5", status: "reported", packet: 1, goal: "show token spend per executor on the board, summed from each usage log", outcome: "A spend column sorts high to low; totals in the footer; relay:list prints the same.", verify: { verdict: "COUNTS-MATCH" } },
      { session_id: "x-docs", name: "readme-pass", model: "claude-haiku-4-5", status: "busy", packet: 2, goal: "bring the README up to date with the board and spend column" },
      // Reported, but what it lists as changed is not all staged: MISMATCH in the warning color; a long outcome stays on one line.
      { session_id: "x-mm", name: "tz-fix", model: "claude-sonnet-5-5", status: "reported", packet: 1, goal: "read another machine's report times in that machine's zone", outcome: "Times are read in the machine's own zone, and a machine that does not say its offset falls back to this Mac's, with a test for each of the two", verify: { verdict: "MISMATCH", note: "2 mismatches" } },
      // Reported minutes ago, its lead listening and free, never seen: relay's wake-up didn't come.
      { session_id: "x-miss", name: "export-csv", model: "claude-sonnet-5-5", status: "reported", packet: 1, goal: "add CSV export to the trades table", outcome: "Export button writes trades.csv; 4 tests added.", verify: { verdict: "COUNTS-MATCH" } },
      // Its agent exited before it reported (stuck at Claude's folder-trust prompt): Close removes it.
      { session_id: "relay:x-stall", name: "fix-tz", model: "claude-sonnet-5-5", status: "stalled", packet: 1, goal: "make the tz offset line optional" },
      // Had a session, then its process died without reporting.
      { session_id: "x-dead", name: "lint-sweep", model: "claude-haiku-4-5", status: "dead", packet: 2 } ] },
    pipe: { role: "executor", plugin: "relay", name: "lines-gaps", color: [132, 180, 180], model: "claude-sonnet-5-5[1m]", status: "busy", packet: 3, lead: "relay", lead_name: "agent-relay", lead_armed: true },
    det: { role: "executor", plugin: "relay", name: "breakout-bt", model: "claude-sonnet-5-5", status: "reported", packet: 1, lead: "gone-lead", lead_name: "backtests", lead_armed: false, verify: { verdict: "MISMATCH", note: "2 changes it reports are not staged" } },
    eng: { role: "executor", plugin: "relay", name: "router-fills", color: [132, 180, 180], model: "claude-sonnet-5-5", status: "busy", packet: 1, lead: "relay", lead_name: "agent-relay", lead_armed: true },
    web: { role: "executor", plugin: "pilead", name: "cart-limit", model: "deepseek/deepseek-flash", status: "busy", packet: 1, lead: "pi-lead-1", lead_name: "shop", lead_armed: true },
  };
  // A pi-lead crew on another machine (build-box), listed over SSH: same rows; no Diff (pilead has none Cue can fetch there).
  if (q.has("crew")) {
    state.sessions.push(sess("plr", "shop", "pi", "working", 6, [seg("working", 20, null)]));
    state.crew.plr = { role: "lead", plugin: "pilead", name: "shop", machine: "build-box", model: "deepseek/deepseek-flash", executors: [
      { session_id: "pe-1", name: "parser", model: "deepseek/deepseek-flash", status: "reported", packet: 2, plugin: "pilead", machine: "build-box", goal: "write the cart parser", outcome: "Parser handles nested lists and empty carts" },
      { session_id: "pe-2", name: "wire-up", model: "deepseek/deepseek-flash", status: "busy", packet: 1, plugin: "pilead", machine: "build-box", goal: "wire the parser into the checkout page" } ] };
  }
  // ?crew=1&busy=1: a working lead (ledger) whose busy executor works in another repo (ledger-api, its own
  // session in Cue): the executor sits in the lead's tree on its Sessions card, not on a card of its own.
  if (q.has("crew") && q.has("busy")) {
    state.sessions.push(sess("led", "ledger", "claude", "working", 4, [seg("working", 4)], "split the ledger import into packets"),
      { ...sess("lex", "ledger-api", "claude", "working", 2, [seg("working", 2)], "packet 1"), doing: "Running: cargo test -q", doing_ms: now - 40000 });
    state.crew.led = { role: "lead", plugin: "relay", name: "ledger", color: [176, 150, 210], model: "claude-opus-5-5", executors: [
      { session_id: "lex", name: "import-split", model: "claude-sonnet-5-5", status: "busy", packet: 1, goal: "split the CSV import into one job per account" } ] };
    state.crew.lex = { role: "executor", plugin: "relay", name: "import-split", color: [176, 150, 210], model: "claude-sonnet-5-5", status: "busy", packet: 1, lead: "led", lead_name: "ledger", lead_armed: true, goal: "split the CSV import into one job per account" };
  }
  // ?crew=1&remote=1: a relay lead on build-box (infra-ops) with two executors Cue doesn't hold there, each
  // in a tmux session of its own; dns-cutover's row is clicked (twice: still one terminal), so its pane and
  // its own terminal show. body[data-term-opens] counts the term_open calls per session.
  if (q.has("crew") && q.has("remote")) {
    state.sessions.push({ ...sess("rlead", "infra-ops", "claude", "working", 3, [seg("working", 3)], "move DNS to the new provider"), machine: "build-box", tmux_pane: "%7" });
    state.crew.rlead = { role: "lead", plugin: "relay", name: "infra-ops", machine: "build-box", color: [120, 170, 200], model: "claude-opus-5-5", executors: [
      { session_id: "rx-dns", name: "dns-cutover", model: "claude-sonnet-5-5", status: "busy", packet: 2, plugin: "relay", machine: "build-box", tmux_session: "relay-dns-cutover", goal: "move the zone records to the new DNS provider" },
      { session_id: "rx-certs", name: "cert-renew", model: "claude-sonnet-5-5", status: "reported", packet: 1, plugin: "relay", machine: "build-box", tmux_session: "relay-cert-renew", goal: "renew the wildcard certificate", outcome: "Wildcard cert renewed, expires in 90 days", verify: { verdict: "COUNTS-MATCH" } } ] };
    // &held=1: Cue holds dns-cutover too (its hooks reached Cue from the machine), with a crew record of its
    // own: an executor started by a lead there, as a lead run from tmux on that machine starts one.
    if (q.has("held")) {
      state.sessions.push({ ...sess("rx-dns", "infra-ops", "claude", "agent", 2, [seg("working", 2)], "move the zone records to the new DNS provider"), machine: "build-box", tmux_pane: "%9", driven_by: "its relay lead (infra-ops)" });
      state.crew["rx-dns"] = { role: "executor", plugin: "relay", name: "dns-cutover", model: "claude-sonnet-5-5", status: "busy", packet: 2, lead: "rlead", lead_name: "infra-ops", lead_armed: true, machine: "build-box", tmux_session: "relay-dns-cutover", goal: "move the zone records to the new DNS provider" };
    }
  }
  // ?crew=1&handoff=1: agent-relay took over from relay-old (relay's handoff), which is still open and
  // no longer leads; backtester's executors are left by a lead that's gone (Close all 2).
  if (q.has("crew") && q.has("handoff")) {
    state.sessions.push(sess("relay-old", "agent-relay", "claude", "waiting", 14, [seg("working", 30, 14), seg("waiting", 14)]));
    state.crew.relay.predecessor = "relay-old";
    state.crew.relay.started_ms = now - 12 * M;
    state.handoffs = { "relay-old": { to: "relay", name: "agent-relay", at_ms: now - 12 * M, executors: 7 } };
    state.sessions.push(sess("det2", "backtester", "claude", "working", 3, [seg("working", 3)]));
    state.crew.det2 = { role: "executor", plugin: "relay", name: "stop-tune", model: "claude-sonnet-5-5", status: "busy", packet: 2, lead: "gone-lead", lead_name: "backtests", lead_armed: false };
  }
  // Each reported packet that waits on you, as hub/reviews.rs reviews_view words it.
  const review = (exec, st, o = {}) => { const e = state.crew.relay.executors.find((x) => x.session_id === exec); return { state: st, missed: false, name: e.name, lead: "relay", packet: e.packet, goal: e.goal, outcome: e.outcome || "", stat: "", findings: [], ...o }; };
  if (q.has("crew")) state.reviews = {
    "x-spend": review("x-spend", "waiting", { stat: "3 files · +94 −2" }),
    "x-mm": review("x-mm", "reviewed", { findings: ["1. The offset fallback has no test for a machine east of UTC.", "2. `parse_zone` swallows a bad offset silently: say so in the log."] }),
    "x-miss": review("x-miss", "waiting", { missed: true }),
  };
  // ?crew=1&rq=1: two reviews queued for agent-relay (it was busy), shown in its chat.
  if (q.has("crew") && q.has("rq")) { state.review_queue = { relay: [{ exec: "x-spend", name: "board-spend", at_ms: now - 2 * M }, { exec: "x-docs", name: "readme-pass", at_ms: now - M }] }; for (const exec of ["x-spend", "x-docs"]) state.reviews[exec] = review(exec, "queued"); }
  // ?manyanswers=1: a long history, to see Recently answered scroll on its own.
  if (q.has("manyanswers")) for (let n = 0; n < 25; n++) state.history.push({ id: `hx${n}`, kind: "permission", harness: n % 2 ? "pi" : "claude", project: ["data-pipeline", "web-app", "agent-relay"][n % 3], tool_name: "Bash", tool_input: { command: `make step-${n}` }, status: "answered", outcome: "allowed", created_ms: now - (80 + n * 7) * M, resolved_ms: now - (79 + n * 7) * M });
  // Conversations, as the backend keeps them per session (the last agent entry is the finished message).
  const longEarlier = "Here's the plan for packet 3.\n\n1. Read each executor's usage log.\n2. Sum tokens per session.\n3. Add a spend column to the board.\n4. Print the same in relay:list.\n\nI'll stage it, not commit. The usage log format changed in 1.4 so I'll handle both shapes, and skip sessions with no log yet.";
  const code = "```\n" + Array.from({ length: 30 }, (_, n) => `line ${n + 1}: spend[${n}] = tokens * rate`).join("\n") + "\n```";
  state.sessions[0].thread = [
    { role: "agent", text: longEarlier, at_ms: now - 26 * M },
    { role: "you", text: "go, and keep the footer total", at_ms: now - 25 * M, images: [] },
    { role: "agent", text: "Done with the parser. Sample output:\n\n" + code, at_ms: now - 18 * M },
    { role: "you", text: "looks right, finish it", at_ms: now - 17 * M, images: [] },
    { role: "you", text: "and keep the CSV export's old column order", at_ms: now - 15.5 * M, images: [] },   // sent mid-turn
    { role: "agent", text: relayMsg, at_ms: now - 11 * M },
  ];
  state.sessions[3].thread = [
    { role: "you", text: "make the line detector survive data gaps", at_ms: now - 24 * M, images: [] },
    { role: "agent", text: "Bash: uv run pytest tests/lines -q -x", at_ms: now - 3 * M },
  ];
  // ?starred=1: five sessions starred, their list open under the ★ chip.
  if (q.has("starred")) state.sessions.slice(0, 5).forEach((x, n) => (x.starred_ms = now - (50 - n) * M));
  // ?held=1: the working order-engine session has a message kept in Cue, open in Active.
  if (q.has("held")) { const x = state.sessions.find((y) => y.project === "order-engine"); if (x) { x.held = { role: "you", text: "also keep the old broker session open while you test", at_ms: now - M, images: [] }; prefs["cue.spot"] = { sid: x.session_id, view: "board" }; } }
  // ?hosted=1: Settings → Cue-hosted sessions on, and a hosted session asking you to fill in an MCP server's form;
  // ?hosted=paused: a hosted one paused (its Claude ended with the last Cue; your next message resumes it).
  if (q.get("hosted")) {
    state.settings.hosted = { on: true };
    state.sessions.push({ ...sess("form", "signup", "claude", "deciding", 1, [seg("working", 5, 1), seg("waiting", 1)]), hosted: true, agent_pid: 4242 });
    state.sessions.push({ ...sess("nap", "billing", "claude", "waiting", 30, [seg("working", 40, 30), seg("waiting", 30)]), hosted: true, thread: [{ role: "you", text: "Add the proration test.", at_ms: now - 40 * M }, { role: "agent", text: "Added `test_proration_midcycle`; it passes.", at_ms: now - 30 * M }] });
    const form = { type: "object", required: ["email"], properties: { email: { type: "string", title: "Email" }, format: { type: "string", title: "Format", enum: ["pdf", "csv"] }, weekly: { type: "boolean", title: "Every week", description: "sent on Mondays" } } };
    state.items.push({ id: "n7", kind: "input", harness: "claude", session_id: "form", cwd: "/x/signup", project: "signup", tool_name: "reports", tool_input: { server: "reports", message: "Where should the usage report go?", form }, suggestions: [], context: [], created_ms: now - 20000, status: "pending", outcome: "" });
    prefs["cue.spot"] = { sid: q.get("hosted") === "paused" ? "nap" : "form", view: "board" };
  }
  // Two sessions run on other machines (+ New → Machine): one name shortened, one short already.
  for (const x of [...state.sessions, ...state.items]) x.machine = { pipe: "me@my-long-ubuntu-box.lan", det: "build-box", det2: "build-box", plr: "build-box", rlead: "build-box" }[x.session_id] || "";
  if (q.get("setup")) state.connections = { claude: { ok: false, present: true }, codex: { ok: false, present: true }, pi: { ok: true, present: true } };
  // ?ufile=1: meters from your usage file beside Claude Code's; ?ufile=err: a file that doesn't match.
  if (q.get("ufile") === "1") state.usage_file = { path: "~/.cue/usage.csv", as_of_ms: now - 4 * M, meters: [{ label: "Team", spent: 42.6, limit: 50, unit: "$", resets_ms: now + 3 * 24 * 3600000 }, { label: "Credits", spent: 1200, limit: null, unit: "credits", resets_ms: null }] };
  if (q.get("ufile") === "err") state.usage_file = { path: "~/.cue/usage.csv", as_of_ms: now - 4 * M, meters: [], error: "Line 2: \"spent\" isn't a number (\"lots\")." };
  // ?park=1: two parked sessions (stopped, kept to resume) under Sessions; Park, Resume and Unpark work.
  state.parked = q.get("park") ? [
    { ...sess("drift", "infra", "codex", "waiting", 26 * 60, []), machine: "build-box", parked_ms: now - 26 * 3600000, thread: [
      { role: "you", text: "Which workspaces are on staging?", at_ms: now - 27 * 3600000 },
      { role: "agent", text: "Two: `staging` and `staging-eu`.", at_ms: now - 27 * 3600000 + 40000 },
      { role: "you", text: "Check Terraform drift on staging. Don't apply anything.", at_ms: now - 26.5 * 3600000 },
      { role: "agent", text: "Three resources drifted: the two security groups on the staging load balancer and the logs bucket. The plan is saved to `drift.plan`; nothing was applied.", at_ms: now - 26.2 * 3600000 } ] },
    { ...sess("guide", "docs-site", "claude", "waiting", 120, []), name: "auth guide", parked_ms: now - 2 * 3600000, thread: [
      { role: "you", text: "Rewrite the auth guide around the new middleware.", at_ms: now - 2.2 * 3600000 },
      { role: "agent", text: "The draft is in `docs/auth.md`. I held off on the code examples until the API settles.", at_ms: now - 2.1 * 3600000 } ] },
  ] : [];
  // Closed in the last 30 days (the snapshot's `ended`), each with its thread for when it's opened.
  const ended = (sid, project, harness, ago, thread) => ({ session_id: sid, name: "", cwd: `/Users/me/code/${project}`, project, harness, machine: "", ended_ms: now - ago, thread });
  state.ended = [
    ended("lost1", "billing", "claude", 3 * 3600000, [{ role: "you", text: "Add the proration test.", at_ms: now - 3.2 * 3600000 }, { role: "agent", text: "Added `test_proration_midcycle`; it passes.", at_ms: now - 3.1 * 3600000 }]),
    ended("lost2", "notes", "pi", 4 * 86400000, [{ role: "agent", text: "Summarised the meeting into notes/2026-10-06.md.", at_ms: now - 4 * 86400000 }]),
  ];
  for (const sid of (q.get("parked") || "").split(",")) { const s = state.sessions.find((x) => x.session_id === sid); if (s) s.later_ms = now; }
  return { state, prefs };
}

if (typeof module !== "undefined") module.exports = { previewWorld };
