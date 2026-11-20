// Fakes for src-tauri/src/ipc/sessions.rs: starting, parking, resuming, closing and renaming sessions.
CueBridge.area("sessions", (w) => {
  const where = (x) => x.machine ? `tmux on ${x.machine}` : "tmux";
  /** Session `sid` out of list `key` (sessions, parked, ended), or a refusal the way Cue words it. */
  const take = (key, sid) => {
    const list = w.state[key] ||= [], i = list.findIndex((x) => x.session_id === sid);
    if (i < 0) throw "Cue has no record of that session";
    return list.splice(i, 1)[0];
  };
  /** Back among the live sessions, at work on `message` if one was sent, else at its prompt. */
  const back = (x, message) => {
    const { parked_ms, ended_ms, ...s } = x;
    w.state.sessions.push({ ...s, state: "waiting", since_ms: w.now(), prompt: "" });
    if (message) w.working(w.session(x.session_id), message, []);
    w.changed(10);
  };
  const started = (agent, place) => ({ session_id: "", detail: `Started ${agent} in ${place}` });

  return {
    park_session: ({ sessionId }) => {
      const x = take("sessions", sessionId);
      // What waited on it goes to History, as parked.
      for (const it of w.state.items.filter((i) => i.session_id === sessionId)) w.state.history.unshift({ ...it, status: "gone", outcome: "parked", resolved_ms: w.now() });
      w.state.items = w.state.items.filter((i) => i.session_id !== sessionId);
      (w.state.parked ||= []).unshift({ ...x, parked_ms: w.now() });
      w.changed(300);
      return "Parked";
    },
    resume_parked: async ({ sessionId, message }) => {
      await w.wait(1200);
      const x = take("parked", sessionId);
      back(x, message);
      return `Resumed in ${where(x)}`;
    },
    unpark_session: ({ sessionId }) => { take("parked", sessionId); w.changed(10); return null; },
    // Closed: what ended in the last 30 days (the snapshot's `ended`, each with its thread if it has one).
    closed_sessions: () => structuredClone(w.state.ended || []),
    park_closed: ({ sessionId }) => { const x = take("ended", sessionId); (w.state.parked ||= []).unshift({ ...x, parked_ms: w.now() }); w.changed(10); return null; },
    resume_closed: async ({ sessionId, message }) => { await w.wait(1200); back(take("ended", sessionId), message); return "Resumed in tmux"; },
    resume_session: ({ sessionId }) => ({ session_id: sessionId, detail: "Resumed in Cue" }),
    rename_session: ({ sessionId, name }) => { const s = w.session(sessionId); if (s) s.name = name; w.changed(); return `Renamed to ${name}`; },
    adopt_session: ({ sessionId }) => {
      const q = (w.state.live || []).find((x) => x.session_id === sessionId);
      if (!q) throw "That session isn't running";
      w.state.live = w.state.live.filter((x) => x !== q);
      w.state.sessions.push({ session_id: q.session_id, project: q.cwd.split("/").pop(), harness: q.harness, cwd: q.cwd, state: q.status === "busy" ? "working" : "waiting", since_ms: q.since_ms, prompt: "", segments: [] });
      return null;
    },
    close_session: ({ sessionId }) => {
      const x = take("sessions", sessionId);
      (w.state.ended ||= []).unshift({ session_id: x.session_id, name: x.name || "", cwd: x.cwd, project: x.project, harness: x.harness, machine: x.machine || "", ended_ms: w.now(), thread: x.thread || [] });
      w.changed();
      return "Closed its iTerm tab";
    },
    delete_session: ({ sessionId }) => {
      for (const key of ["sessions", "parked", "ended"]) w.state[key] = (w.state[key] || []).filter((x) => x.session_id !== sessionId);
      w.changed();
      return "Deleted";
    },
    new_session: ({ agent, machine }) => started(agent, machine ? machine : "a new iTerm tab"),
    fork_session: ({ sessionId }) => { const name = `${w.session(sessionId)?.project || "session"}-fork`; return { session_id: "", name, detail: `Forked as ${name} in a new iTerm tab` }; },
    trust_folder: () => null,
    continue_start: () => null,
    // "! line" in a box: run there, what it printed back.
    run_line: async ({ sessionId, cwd, line }) => {
      await w.wait(200);
      const dir = sessionId ? w.session(sessionId)?.cwd : cwd;
      if (!dir) throw "Cue doesn't know that session's folder";
      return { output: line.includes("git status") ? "On branch main\nnothing to commit, working tree clean" : "", code: 0, ms: 42, cwd: dir, timed_out: false };
    },
    move_to_cue: ({ sessionId }) => { const s = w.session(sessionId); if (s) s.tmux_pane = "%1"; w.changed(); return "Moved to Cue"; },
    move_to_terminal: ({ sessionId }) => { const s = w.session(sessionId); if (s) { s.hosted = false; s.term_program = "iTerm.app"; } w.changed(); return "Moved to iTerm"; },
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
    // src-tauri/src/ipc/hosted.rs: quitting (nothing ends with Cue here, so it quits at once), and cue-host as
    // Settings → Agents shows it (reachable, nothing running, no older hosts).
    quit_check: () => [],
    quit_now: () => null,
    host_status: () => ({ reachable: true, build: "test", running: 0, older: [], inside_cue: 0, machines: [] }),
    host_restart: () => "Restarted cue-host",
  };
});
