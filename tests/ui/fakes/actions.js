// Fakes for src-tauri/src/actions/mod.rs: a project's actions (its repo's, yours, found ones), running them, their
// output, and saving them. agent-relay has an Install that asks first, a failing Test and an API server running
// (?running=1: its Web one too; ?suggest=1: Test running); order-engine has an Install that asks and a Test.
CueBridge.area("actions", (w) => {
  const M = 60000;
  const RELAY = "/Users/me/code/agent-relay", ENGINE = "/Users/me/code/order-engine";
  const DEFAULT_PROMPT = "Look at this project and set up its Cue actions in .cue/actions.json (shared with the repo) and .cue/actions.local.json (only for me). Read the \"about\" notes at the top of the file first and follow them (if the file doesn't exist yet, create it with those notes). Keep the actions already there unless they're wrong. Then tell me what you added, changed or removed, and why.";
  const A = (id, name, run, source, o = {}) => ({ id, name, run, source, keep: false, ask: false, allowed: true, state: {}, ...o });
  const folders = {};
  let prompt = "";
  /** A folder's actions and which files it has, made the first time it's asked about. */
  const of = (folder) => folders[folder] ||= folder === RELAY ? { files: { shared: true, local: true, localIgnored: false }, actions: [
    A("shared:Install", "Install", "npm ci", "shared", { ask: true, state: { exit: 0, secs: 14, at: w.now() - 26 * 60 * M } }),
    A("shared:Test", "Test", "npm test", "shared", { state: w.q.has("suggest") ? { running: true, since: w.now() - 8000 } : { exit: 1, secs: 41, at: w.now() - 2 * 60 * M } }),
    A("shared:API", "API", "npm run dev:api", "shared", { keep: true, state: { running: true, since: w.now() - 42 * M, port: 4000 } }),
    A("local:Web", "Web", "npm run dev -- --port 5173", "local", { keep: true, state: w.q.has("running") ? { running: true, since: w.now() - 9 * M, port: 5173 } : { exit: 0, secs: 3100, at: w.now() - 20 * 60 * M } }),
  ] } : folder === ENGINE ? { files: { shared: true, local: false, localIgnored: null }, actions: [
    A("shared:Install", "Install", "uv sync", "shared", { ask: true }),
    A("shared:Test", "Test", "uv run pytest -q", "shared", { suggest: ["router/**", "tests/**"], state: { exit: 0, secs: 22, at: w.now() - 50 * M } }),
  ] } : { files: { shared: false, local: false, localIgnored: null }, actions: [] };
  /** actions_list's answer: the actions, the files, the ones waiting to be allowed (new or changed), the set-up prompt. */
  const list = (folder) => {
    const f = of(folder);
    const pending = f.actions.filter((a) => !a.allowed).map((a) => ({ id: a.id, name: a.name, run: a.run, source: a.source, why: a.was ? "changed" : "new", ...(a.was ? { was: a.was } : {}) }));
    return structuredClone({ actions: f.actions, files: f.files, pending, prompt: prompt || DEFAULT_PROMPT });
  };
  const changed = (folder) => setTimeout(() => w.emit("actions", { folder, machine: "" }), 50 * w.latency);
  const action = ({ folder, id }) => { const a = of(folder).actions.find((x) => x.id === id); if (!a) throw `No action ${id}`; return a; };
  const log = (a) => a.name === "Test"
    ? "> agent-relay@1.4.0 test\n> vitest run\n\n ✓ src/board.test.ts (12)\n ✓ src/list.test.ts (8)\n ✗ src/spend.test.ts (3)\n   × sums each executor's usage\n     expected 1.32 to equal 1.31\n\n Test Files  1 failed | 2 passed (3)\n      Tests  1 failed | 22 passed (23)"
    : a.keep ? `> ${a.run}\n\n  VITE v5.4.2  ready in 412 ms\n\n  ➜  Local:   http://localhost:${a.state.port || 5173}/\n  ➜  Network: use --host to expose\n${Array.from({ length: 30 }, (_, n) => `[${n}:0${n % 10}] GET /api/board 200 ${12 + n}ms`).join("\n")}`
    : `> ${a.run}\n\nadded 812 packages in 14s`;
  function run(args) {
    const a = action(args);
    if (!a.allowed) throw "Allow it first";
    // A server keeps running; anything else finishes after a few seconds (Test fails).
    if (a.keep) { a.state = { running: true, since: w.now(), port: a.name === "API" ? 4000 : 5173 }; changed(args.folder); return a.state; }
    a.state = { running: true, since: w.now() };
    setTimeout(() => { a.state = { exit: a.name === "Test" ? 1 : 0, secs: 3, at: w.now() }; changed(args.folder); }, 3000 * w.latency);
    return a.state;
  }
  /** ?trust=1: once agent-relay's actions are read, its agent adds one and changes another (an "actions" event). */
  w.agentChangesActions = () => {
    const f = of(RELAY), t = f.actions.find((x) => x.id === "shared:Test");
    f.actions.push(A("shared:Seed db", "Seed db", "npm run db:seed -- --fresh", "shared", { allowed: false }));
    t.was = t.run;
    t.run = "npm test -- --runInBand";
    t.allowed = false;
    changed(RELAY);
  };

  return {
    actions_list: ({ folder }) => list(folder),
    actions_allow: ({ folder, ids }) => { for (const x of of(folder).actions) if (ids.includes(x.id)) { x.allowed = true; delete x.was; } return list(folder); },
    actions_run: run,
    actions_restart: run,
    actions_stop: (args) => {
      const a = action(args);
      a.state = { exit: 130, secs: Math.round((w.now() - (a.state.since || w.now())) / 1000), at: w.now() };
      changed(args.folder);
      return a.state;
    },
    actions_log: (args) => log(action(args)).split("\n").slice(-(args.lines || 200)).join("\n"),
    actions_attach_id: () => { throw "no tmux here"; },
    actions_term_open: () => 1,
    // Saved from Cue's sheet: into your own file (local), allowed, in place of the one edited.
    actions_save: ({ folder, action: x, oldId }) => {
      const f = of(folder), id = `local:${x.name}`, old = f.actions.findIndex((a) => a.id === (oldId || id));
      const saved = A(id, x.name, x.run, "local", { keep: x.keep, ask: x.ask });
      if (old >= 0) f.actions.splice(old, 1, saved); else f.actions.push(saved);
      return list(folder);
    },
    actions_file: ({ folder }) => `${folder}/.cue/actions.json`,
    actions_delete: ({ folder, id }) => { const f = of(folder); f.actions = f.actions.filter((x) => x.id !== id); return list(folder); },
    actions_running: () => {
      [RELAY, ENGINE].forEach(of);
      const rs = Object.entries(folders).flatMap(([folder, f]) => f.actions.filter((a) => a.state.running && a.keep).map((a) => ({ folder, machine: "", project: folder.split("/").pop(), branch: "main", checkout: folder.split("/").pop(), id: a.id, name: a.name, port: a.state.port || null, since: a.state.since })));
      // ?worktrees=1: a worktree of agent-relay runs its own dev server too.
      if (w.q.has("worktrees")) rs.push({ folder: `${RELAY}-fix`, machine: "", project: "agent-relay", branch: "fix-spend", checkout: "agent-relay-fix", id: "local:Web", name: "Web", port: 5174, since: w.now() - 6 * M });
      return rs;
    },
    actions_prompt_set: ({ text }) => { prompt = text; return prompt; },
    // A folder's project: one you put it in, its repository's, or (scratch, Downloads) its own name.
    project_for_folder: ({ folder }) => {
      const f = folder.replace(/\/+$/, ""), name = f.split("/").pop(), put = w.state.settings.project?.folders?.[f];
      if (put) return { name: put, own: name, how: "put", key: f };
      return { name, own: name, how: /scratch|Downloads/.test(f) ? "folder" : "repo", key: f };
    },
    // + New's Folder field: the folders on "disk" under what's typed (a few made-up ones in ~/code).
    folders_under: ({ typed }) => {
      const cut = typed.lastIndexOf("/"), dir = typed.slice(0, cut + 1), part = typed.slice(cut + 1).toLowerCase();
      const disk = { "~/code/": ["api-server", "api-tools", "docs-site", "web-app"], "~/code/api-tools/": ["cli", "lib"] };
      return (disk[dir] || []).filter((n) => n.toLowerCase().startsWith(part)).map((n) => dir + n);
    },
  };
});
