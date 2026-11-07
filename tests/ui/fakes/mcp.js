// Fakes for src-tauri/src/ipc/chat.rs, the part about MCP servers: what a session's folder knows (one of each
// scope), which it runs with (`selected`), and whether it has a selection of its own (`pinned`, ⋯ → MCP servers…).
// ?pinned=0: not pinned, selected follows enabled. ?restart=done: its live state is known.
CueBridge.area("mcp", (w) => {
  const pinned0 = w.q.get("pinned") !== "0";
  const fx = { calls: 113, pinned: pinned0, running: true, live_known: w.q.get("restart") === "done", servers: [
    { name: "linear", scope: "user", transport: "http", target: "https://mcp.linear.app/mcp", from: "~/.claude.json", enabled: true, selected: !pinned0, toggle: false, used: 2, failed: 0, tools: [{ name: "list_issues", calls: 2, failed: 0 }], live: "on", loaded: ["get_issue", "list_comments", "list_issues", "list_projects", "list_teams", "save_comment", "save_issue", "search_documentation"] },
    { name: "chrome-devtools", scope: "local", transport: "stdio", target: "npx chrome-devtools-mcp@latest", from: "~/.claude.json", enabled: true, selected: true, toggle: true, used: 14, failed: 0, tools: [{ name: "take_screenshot", calls: 14, failed: 0 }], live: "on" },
    { name: "data-tools", scope: "project", transport: "stdio", target: "uv run data-mcp", from: ".mcp.json", enabled: false, selected: false, toggle: true, used: 0, failed: 0, tools: [] },
    { name: "sentry", scope: "available", transport: "http", target: "https://mcp.sentry.dev/mcp", from: "/Users/me/code/billing (local)", enabled: false, selected: pinned0, toggle: true, used: 3, failed: 1, tools: [{ name: "search_issues", calls: 3, failed: 1 }], live: "sign-in" },
    { name: "claude_ai_Claude_Docs", scope: "session", transport: "sse", target: "claude.ai", from: "Claude Code", enabled: true, selected: true, toggle: false, used: 113, failed: 2, tools: [{ name: "read", calls: 80, failed: 0 }, { name: "update", calls: 33, failed: 2 }] },
  ] };
  const session = () => ({ ...fx, servers: fx.servers.map((m) => ({ ...m })) });
  /** A folder's: no session scope, nothing used yet, selected as enabled. */
  const folder = () => ({ calls: 0, pinned: false, servers: fx.servers.filter((m) => m.scope !== "session").map((m) => ({ ...m, selected: m.enabled, used: 0, failed: 0, tools: [] })) });
  const enable = (name, on) => { const m = fx.servers.find((x) => x.name === name); if (m) m.enabled = on; };

  return {
    mcp_for: session,
    mcp_for_folder: folder,
    mcp_set: ({ name, on }) => { enable(name, on); return session(); },
    mcp_set_folder: ({ name, on }) => { enable(name, on); return folder(); },
    mcp_restart: async () => { await w.wait(300); return "Restarted"; },
    mcp_apply: async ({ names }) => {
      await w.wait(300);
      for (const m of fx.servers) if (m.toggle) m.selected = names.includes(m.name);
      fx.pinned = true;
      return `Restarted with ${names.length} MCP server${names.length === 1 ? "" : "s"}`;
    },
    mcp_apply_clear: async () => {
      await w.wait(300);
      for (const m of fx.servers) m.selected = m.enabled;
      fx.pinned = false;
      return "Restarted with its folder's MCP servers";
    },
  };
});
