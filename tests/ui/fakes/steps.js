// Fakes for src-tauri/src/ipc/chat.rs, the part about steps: what a session did between its messages (read from
// its transcript), a step in full, and its background agents. Turns are made up for the sample sessions the
// preview and the recorded scenarios share (relay, names, q2); any other session has none.
CueBridge.area("steps", (w) => {
  const M = 60000;
  const relayMsg = () => w.session("relay")?.thread?.at(-1)?.text || "";
  /** A step of a turn, `m` minutes ago. */
  const step = (id, kind, verb, subject, result, m, o = {}) => ({ t: "step", id, kind, verb, subject, mono: kind !== "search", result, bad: false, done: true, at_ms: w.now() - m * M, ...o });
  const say = (text, m) => ({ t: "say", text, at_ms: w.now() - m * M });

  // q2: a quiet session (running, never reported to Cue), mid-turn.
  const quiet = () => ({ version: 1, turns: [
    { at_ms: w.now() - 40 * M, prompt: "why do partial fills open a second broker session?", more: 0, items: [say("The router opens a session per fill. I'll reuse the order's session.", 39), step("q2a", "read", "Read", "router.py", "318 lines", 38), say("Found it: `open_session()` is called inside the fill loop.", 36)] },
    { at_ms: w.now() - 25 * M, prompt: "fix it and run the tests", more: 0, items: [step("q2b", "edit", "Edited", "router.py", "+9 −4", 24), step("q2c", "run", "Running", "pytest tests/router -q", "", 1, { done: false })] },
  ] });

  // names: a turn that started two background agents; one still at work, one finished. &many=1: a wider fan-out
  // (one of them stuck); &scripts=1: shell commands in the background too.
  function names() {
    const search = (id, m, o = {}) => step(id, "search", "Searched", "“custom-title|ai-title”", "14 results", m, { mono: true, ...o });
    const read = (id, file, lines, m, o = {}) => step(id, "read", "Read", file, lines, m, o);
    const scripts = w.q.has("scripts"), many = w.q.has("many");
    const helper = (id, desc, kind, model, start, o) => ({ id, desc, kind, model, started_ms: w.now() - start * M, finished_ms: 0, last_ms: w.now(), stuck: false, tokens: 0, running: true, more: 0, items: [], ...o });
    const script = (id, stepId, desc, command, start, o) => helper(id, desc, "script", "", start, { step: stepId, script: true, command, ...o });
    const build = "npx tauri build --bundles app --features ext 2>&1 | grep -E \"Cue.app \\(|error\\[\" | head -2 && ./install.sh --app 2>&1 | tail -1";
    return { version: 1, turns: [{ at_ms: w.now() - 5 * M, prompt: "Map every name a session has, backend and UI.", more: 0, items: [
      step("n1", "agent", "Delegated", "Backend: session name sources", "in the background", 4.9, { mono: false }),
      step("n2", "agent", "Delegated", "UI: every name shown", "in the background", 4.9, { mono: false }),
      ...(scripts ? [
        step("n5", "run", "Ran", "cargo test --lib 2>&1 | tail -40", "in the background", 4.5),
        step("n4", "run", "Ran", "tests/run.sh", "in the background", 4),
        step("n3", "run", "Ran", build, "in the background", 1.2),
      ] : []),
      say("Two agents are mapping the names. I'll put the answer together when they report.", 2)] }],
    helpers: [
      helper("h1", "Backend: session name sources", "Explore", "claude-opus-5-5", 4.9, { step: "n1", last_ms: w.now() - 0.3 * M, tokens: 18400, more: 3, items: [search("h1a", 4),
        ...["model.rs", "db.rs", "leads.rs", "config.rs", "machines.rs", "notify.rs"].map((f, n) => read(`h1r${n}`, f, `${120 + n * 37} lines`, 3.9 - n * 0.1)),
        read("h1b", "sessions.rs", "882 lines", 3), step("h1c", "read", "Reading", "hub.rs", "", 0.3, { done: false })] }),
      helper("h2", "UI: every name shown", "Explore", "claude-opus-5-5", 4.9, { step: "n2", finished_ms: w.now() - 2.5 * M, last_ms: w.now() - 2.5 * M, tokens: 7300, running: false, items: [search("h2a", 4.5, { subject: "“nameSpan|agentName”", result: "9 results" }), read("h2b", "app.js", "4193 lines", 3.5), say("Every name the UI shows comes from `nameSpan` …", 2.5)] }),
      ...(many ? [
        helper("h3", "Docs: names in the README and site", "general-purpose", "claude-sonnet-5-5", 3.2, { step: "n1", last_ms: w.now() - 0.2 * M, tokens: 12100, items: [search("h3a", 3, { subject: "“session name”", result: "22 results" }), read("h3b", "README.md", "310 lines", 2.4), say("The README calls it a title in two places and a name in five.", 1.8), step("h3c", "run", "Running", "grep -rn title site/", "", 0.2, { done: false })] }),
        // h4 is stuck: running, its last step done 16 minutes ago and nothing since (src-tauri/src/sessions/quiet.rs).
        helper("h4", "Tests: what the protocol suite asserts about names", "Explore", "claude-haiku-5-5", 16.6, { step: "n2", last_ms: w.now() - 16 * M, stuck: true, quiet: "stuck", tokens: 2100, items: [step("h4a", "read", "Reading", "tests/test_protocol.py", "", 3.6, { done: false })] }),
        helper("h5", "Hooks: the name Claude Code sends", "Explore", "claude-opus-5-5", 0.4, { step: "n1", last_ms: w.now() - 0.4 * M }),
        // h7 has a long run open: quiet 20 minutes inside one command, left to it.
        helper("h7", "Suite: the full protocol run", "general-purpose", "claude-sonnet-5-5", 21, { step: "n2", last_ms: w.now() - 20 * M, quiet: "tool", quiet_what: "Running npm test", tokens: 900 }),
        helper("h6", "History: names kept per session", "Explore", "claude-sonnet-5-5", 4.8, { step: "n2", finished_ms: w.now() - 3.9 * M, last_ms: w.now() - 3.9 * M, tokens: 4900, running: false, items: [search("h6a", 4.6, { subject: "“custom_title”", result: "3 results" }), say("History keeps `custom_title` and `ai_title`; the UI prefers the first.", 3.9)] }),
      ] : []),
      ...(scripts ? [
        script("b1", "n3", "Build Cue and install it", build, 1.2, { last_ms: w.now() - 0.1 * M, lines: 218, failed: false,
          output: ["serde_json v1.0.140", "tauri v2.8.5", "tauri-plugin-dialog v2.3.3", "tauri-plugin-shell v2.3.1", "tauri-plugin-process v2.3.0", "tauri-plugin-updater v2.9.0", "objc2-app-kit v0.3.1", "portable-pty v0.9.0", "notify v8.0.0", "russh v0.52.1", "cue v0.1.41 (src-tauri)", "cue v0.1.41 (src-tauri) [lib]"].map((c) => `   Compiling ${c}`) }),
        script("b2", "n4", "Protocol suite", "tests/run.sh", 4, { finished_ms: w.now() - 3.2 * M, last_ms: w.now() - 3.2 * M, running: false, lines: 61, failed: true,
          output: ["test_protocol.py::test_hello PASSED", "test_protocol.py::test_list PASSED", "test_protocol.py::test_reply_roundtrip PASSED", "test_protocol.py::test_send_to_remote FAILED", "test_protocol.py::test_remote_focus FAILED", "test_protocol.py::test_history_keep PASSED", "", "=================================== FAILURES ===================================", "____________________________ test_send_to_remote _______________________________", "ConnectionRefusedError: [Errno 61] Connection refused", "========================= 2 failed, 27 passed in 41.80s ========================="] }),
        script("b3", "n5", "cargo test --lib", "cargo test --lib 2>&1 | tail -40", 4.5, { finished_ms: w.now() - 3.4 * M, last_ms: w.now() - 3.4 * M, running: false, lines: 40, failed: false,
          output: ["test logs::steps::tests::helper_finishes_on_notice ... ok", "test logs::steps::tests::helper_reads_meta_type ... ok", "", "test result: ok. 239 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 68.88s"] }),
      ] : []),
    ] };
  }

  // relay: ?ins=1 its turns have grown expensive and its cache has 8 minutes left (insights.js); ?ahead=…: a 320k
  // session on an hour's cache, its last reply as each case needs; otherwise two turns between its messages.
  function relay() {
    if (w.q.has("ins")) {
      const turns = Array.from({ length: 24 }, (_, i) => ({ at_ms: w.now() - (90 - i * 3.5) * M, prompt: i ? "" : "add the tax column", more: 0, items: [say("Done.", 89 - i * 3.5)], tokens: Math.round(40000 + 18000 * Math.pow(i, 1.35)), context: Math.round(470000 + 130000 * Math.pow(i / 23, 1.3)) }));
      return { version: 1, meta: { model: "claude-opus-5-5", context: 600000, window: 1000000, last_call_ms: w.now() - 52 * M, cache_ttl_s: 3600, start_context: 45000, start_ms: w.now() - 6 * 60 * M }, turns };
    }
    if (w.q.get("ahead")) {
      const ago = { watch: 10, ticked: 10, soon: 56, warm: 2, writing: 1, cold: 65, gone: 720, goneticked: 720 }[w.q.get("ahead")] || 10;
      const turns = [{ at_ms: w.now() - (ago + 1) * M, prompt: "carry on", more: 0, items: [say("Done: the spend column sorts high to low, tests pass.", ago)], tokens: 320000, context: 320000 }];
      return { version: 1, meta: { model: "claude-opus-5-5", context: 320000, window: 1000000, last_call_ms: w.now() - ago * M, cache_ttl_s: 3600, start_context: 240000, start_ms: w.now() - 3 * 60 * M, cold_x: 40, cold_cents: 256 }, turns };
    }
    return { version: 1, meta: { model: "claude-opus-5-5", context: 164000, window: 200000, cost: 1.32 }, turns: [
      { at_ms: w.now() - 25 * M, more: 0, items: [step("a1", "read", "Read", "usage.ts", "212 lines", 24.8), step("a2", "search", "Searched", "“tokens_used”", "6 files", 24.6), step("a3", "edit", "Edited", "board.ts", "+48 −3", 22), say("Done with the parser. Sample output:", 18)] },
      // This one a Stop hook sent back to work after its answer (hooked_ms): a checklist, folded under "After a stop hook".
      { at_ms: w.now() - 17 * M, more: 0, hooked_ms: w.now() - 10.8 * M, items: [say("Adding the footer total and the list output.", 16.9), step("b1", "edit", "Edited", "board.ts", "+12 −1", 16.5), step("b2", "edit", "Edited", "list.ts", "+9 −0", 16), step("b3", "run", "Ran", "npm test", "exit 1", 15, { bad: true }), say("One test expected the old column order. Updating it.", 14.8), step("b4", "read", "Read", "board.test.ts", "140 lines", 14.6), step("b5", "edit", "Edited", "board.test.ts", "+2 −2", 14), step("b6", "run", "Ran", "npm test", "Tests: 41 passed", 12.5), say(relayMsg(), 11), step("c1", "read", "Read", "rules-check.md", "64 lines", 10.6), say("1 — Shape: stated before editing.\n2 — Assumptions: the spend column exists (checked).\n3 — Verified: tests pass.", 10.2)] },
    ] };
  }

  const STEPS = { q2: quiet, names, relay };
  const failedRun = { full: "npm test", output: ["> relay@1.4.0 test", "> vitest run", "", " FAIL  test/board.test.ts > columns > spend sorts high to low", "   expected [ 'name', 'status', 'spend' ] to equal [ 'name', 'spend', 'status' ]", "", " Tests  1 failed | 40 passed (41)"], skipped: 0, gap_at: 40 };
  const edit = { full: "/Users/me/code/agent-relay/src/board.ts", diff: [[" ", "  const cols = ['name', 'status'];"], ["-", "  const total = 0;"], ["+", "  const total = rows.reduce((s, r) => s + r.spend, 0);"], ["+", "  footer.push(fmt(total));"]], skipped: 0 };
  const helpersOf = (sid) => STEPS[sid]?.().helpers || [];

  return {
    session_steps: ({ sessionId }) => STEPS[sessionId]?.() || { version: 0, turns: [] },
    step_detail: ({ ids }) => Object.fromEntries(ids.map((id) => [id, id === "b3" ? failedRun : ["b1", "b2", "b5"].includes(id) ? edit : { full: "/Users/me/code/agent-relay/test/board.test.ts" }])),
    // Earlier background agents (not in the live feed): the finished ones, and one of them in full.
    session_bg_history: ({ sessionId }) => helpersOf(sessionId).filter((h) => !h.running).map(({ items, output, ...h }) => h),
    session_bg_helper: ({ sessionId, id }) => helpersOf(sessionId).find((h) => h.id === id) || null,
  };
});
