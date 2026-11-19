// Fakes for src-tauri/src/ipc/settings.rs: settings, drafts, the clipboard, links, notifications, updates, and
// connecting an agent's hooks. Drafts the real app keeps in ~/.cue/cue.db are kept in this browser here.
CueBridge.area("settings", (w) => {
  const DRAFTS = "preview.drafts";
  const drafts = () => { try { return JSON.parse(localStorage.getItem(DRAFTS) || "{}"); } catch { return {}; } };
  // A 1×1 PNG: what a pasted or attached image reads as.
  const PIXEL = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

  /** Each session's project as the scenario had it, before any move. */
  const own = new Map();
  /** The sessions in `moves` shown under the project each was moved to, the rest under their own again, as
   *  hub/mod.rs move_projects builds every snapshot. */
  const moveProjects = (moves) => {
    for (const list of ["items", "history", "sessions", "parked", "ended"]) {
      for (const r of w.state[list] || []) {
        if (!own.has(r.session_id)) own.set(r.session_id, r.folder_project ?? r.project);
        const to = moves[r.session_id];
        r.project = to ?? own.get(r.session_id);
        if (to) r.folder_project = own.get(r.session_id); else delete r.folder_project;
      }
    }
  };

  /** Ask me before as config.rs effective gives it (one_way.rs view), for a scenario recorded before it had it: what
   *  you set per project, the projects asked, the kinds with their words, the defaults. Once, the first time the state is read. */
  const ONE_WAY_KINDS = [["main", "Pushing to main", true], ["push", "Pushing any branch", true], ["release", "Releasing", true], ["deploy", "Deploying", false], ["data", "Deleting files", true], ["outside", "Files outside the project", false]];
  const oneWayFill = () => {
    const p = w.state?.settings && (w.state.settings.project ||= {});
    // A Desk's own view of it (one_way.rs desk_view), for a scenario recorded before Desks had it: already asked.
    for (const d of w.state?.desk?.projects || []) d.ask_before ||= { project: d.workspace_project || d.project, kinds: [{ key: "main", label: "Pushing to main" }], asked: true, ticks: ["main"] };
    if (p && !p.oneway) p.oneway ={ set: {}, asked: [], kinds: ONE_WAY_KINDS.map(([key, label, offered]) => ({ key, label, offered, why: offered ? "" : "Cue can't check it" })), default: ["main"], ask_default: ["main", "release", "data"] };
  };
  const plainSnapshot = w.snapshot;
  w.snapshot = () => { oneWayFill(); return plainSnapshot(); };

  return {
    // "a.b" sets settings.a.b, as config.rs set does for the keys the window saves; Ask me before's map is its `set`.
    set_setting: ({ key, value }) => {
      const [a, b] = key.split(".");
      if (key === "project.oneway") { oneWayFill(); w.state.settings.project.oneway.set = value; w.changed(10); return null; }
      if (b == null) w.state.settings[a] = value; else (w.state.settings[a] ||= {})[b] = value;
      if (key === "project.moves") moveProjects(value || {});
      w.changed(10);
      return null;
    },
    // Ask me before (hub/guard.rs ask_before_set): the kinds saved, the project asked; null keeps them.
    ask_before_set: ({ project, kinds }) => {
      oneWayFill();
      const ow = w.state.settings.project.oneway;
      if (kinds) ow.set = { ...ow.set, [project]: kinds };
      if (!ow.asked.includes(project)) ow.asked = [...ow.asked, project];
      for (const d of w.state.desk?.projects || []) if (d.ask_before?.project === project) d.ask_before.asked = true;
      w.changed(10);
      return Array.isArray(ow.set[project]) ? ow.set[project] : ow.default;
    },
    get_drafts: () => drafts(),
    set_draft: ({ key, text, images }) => {
      const all = drafts();
      if (text.trim() || images.length) all[key] = { text, images }; else delete all[key];
      try { localStorage.setItem(DRAFTS, JSON.stringify(all)); } catch {}
      return null;
    },
    agents_installed: () => ["claude", "pi"],
    connect_agent: ({ harness }) => {
      w.state.connections[harness] = { ...(w.state.connections[harness] || {}), ok: true, present: true };
      w.changed();
      return `Connected ${harness}`;
    },
    // Settings rows an add-on adds: none without one.
    ext_settings: () => [],
    ext_set: () => null,
    // Up to date on stable; a newer build on beta (Settings → Get beta builds), as ipc/updates.rs picks the channel.
    // ?nobeta=1: no beta is published, so a beta check answers from stable and says so.
    update_check: () => {
      if (!w.state.settings.updates?.beta) return { current: "0.1.86" };
      if (w.q.has("nobeta")) return { current: "0.1.86", note: "No beta is out right now, so this is the version everyone gets." };
      return { current: "0.1.86", version: "0.1.87", notes: "A beta build." };
    },
    update_install: () => null,
    open_link: () => null,
    // Music paused while Listen reads (and let go after): nothing to pause here.
    music_hold: () => null,
    music_release: () => null,
    // Read's New docs: none unless a test gives some.
    recent_docs: () => [],
    // A markdown file a reply names, as ipc/docs.rs finds it: any .md under the session's folder, with a short text.
    read_doc: ({ path, cwd }) => {
      if (!/\.(md|markdown)$/i.test(path)) throw "Couldn't find it";
      const full = path.startsWith("/") ? path : `${cwd}/${path}`;
      return { path: full, text: `# ${full.split("/").pop()}\n\nThe plan, in three stages.\n\n## Stage 1\n\nPaper accounts only.`, modified_ms: w.now() - 120000 };
    },
    // Any file a click opens in the pop-up viewer (ipc/docs.rs peek_file): a picture, or a few lines of text.
    peek_file: ({ path, cwd }) => {
      const full = path.startsWith("/") ? path : `${cwd}/${path}`;
      if (/\.(png|jpe?g|gif|webp)$/i.test(full)) return { path: full, kind: "image", text: null, size: 2048, modified_ms: w.now() };
      if (/\/$|\.(zip|dmg)$/i.test(full)) return { path: full, kind: "other", text: null, size: 0, modified_ms: w.now() };
      return { path: full, kind: "text", text: "const a = 1;\nconst b = a + 1;\n", size: 32, modified_ms: w.now() };
    },
    image_data: () => `data:image/png;base64,${PIXEL}`,
    clipboard_image: () => null,
    copy_text: () => null,
    test_notification: () => "Sent a test notification",
  };
});
