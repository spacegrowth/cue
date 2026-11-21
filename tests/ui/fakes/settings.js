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
  /** How work lands as lands.rs desk_view gives it, for `place` (cue, repo or folder) and what's saved. */
  const LANDS_PARTS = [
    ["where", "Where an item works", [["shared", "Shared folder", "Every session works in the project's own folder."], ["copy", "Own copy", "Each item gets its own copy of the project."], ["branch", "A branch"]]],
    ["how", "How it lands", [["squash", "One commit per item", "The Desk puts each item on main as one commit, named after the item. One at a time."], ["merge", "Merge"], ["pr", "Pull request", "The Desk checks each item, pushes its branch and opens a pull request."], ["owner", "You commit", "The Desk commits nothing."]]],
    ["check", "Check before landing", [["none", "None", "Nothing is run."], ["cue", "Cue's own check", "Run in a clean copy of main plus the item, in order."], ["command", "Your own steps", "Your steps run in order in a clean copy of main plus the item. The first that fails stops the rest, and nothing reaches main. They run as in your terminal. Each step gets up to 15 minutes and all of them 30 minutes together; anything a step leaves running in the background counts toward that time. Steps can't push: a push always asks you first."]]],
    ["after", "After landing", [["nothing", "Nothing", "Landed is the last step."], ["install", "Install or serve Cue", "Window-only changes are served in place, after the check."], ["command", "Your own steps", "Once the item is on main, your steps run in order in the project's main folder. The first that fails stops the rest, and the item waits for you. They run as in your terminal. Each step gets up to 15 minutes and all of them 30 minutes together; anything a step leaves running in the background counts toward that time. Steps can't push: a push always asks you first."]]],
    ["push", "Push to main", [["never", "Never", "The Desk pushes nothing and asks nothing."], ["ask", "Ask each time", "A push to main asks you first."], ["after", "After each landing", "After each landing the Desk pushes main as it is, everything on it, without asking. Never forced. Sessions still never push main."]]],
  ];
  const LANDS_DEFAULT = {
    cue: { where: "copy", how: "squash", check: "cue", after: "install", push: "ask", at_once: 3, check_steps: [], after_steps: [] },
    repo: { where: "copy", how: "squash", check: "none", after: "nothing", push: "ask", at_once: 3, check_steps: [], after_steps: [] },
    folder: { where: "shared", how: "owner", check: "none", after: "nothing", push: "ask", at_once: 3, check_steps: [], after_steps: [] },
  };
  const landsWhy = (part, key, place) => {
    const hint = LANDS_PARTS.find((p) => p[0] === part)[2].find((c) => c[0] === key);
    if (!hint[2]) return "not yet";
    if ((part === "check" && key === "cue") || (part === "after" && key === "install")) return place === "cue" ? "" : "only for Cue's own project";
    if (place === "folder" && ((part === "where" && key === "copy") || (part === "how" && (key === "squash" || key === "pr")))) return "needs a git repository";
    return "";
  };
  const landsView = (project, place, saved) => {
    const set = { ...LANDS_DEFAULT[place], ...saved };
    const short = { shared: "shared folder", copy: "own copy", squash: "one commit per item", owner: "you commit", pr: "pull request" };
    return {
      project, place, set, summary: `${short[set.where]}, ${set.check === "none" ? "not checked" : "checked"}, ${short[set.how]}`,
      parts: LANDS_PARTS.map(([key, label, cs]) => ({ key, label, now: set[key], choices: cs.map(([k, l, hint]) => { const why = landsWhy(key, k, place); return { key: k, label: l, hint: hint || "", ok: !why, why }; }) })),
      at_once: { min: 1, max: 9 },
      steps: [{ label: "Build", hint: "the app built from a clean copy" }, { label: "Rust tests", hint: "the hub's own tests" }, { label: "Window loads", hint: "from the clean copy, failing on a name missing or declared twice" }],
      why: place === "folder" ? "This folder isn't a git repository, so items work in it directly, nothing is checked, and you commit their work yourself." : "",
    };
  };
  // Kept per Desk (#460); Cue's own place by the folder it works in (a Desk with none works in its project's).
  const landsPlace = (d) => d.folder ? (/\/cue$/.test(d.folder) ? "cue" : "repo") : ((d.workspace_project || d.project) === "cue" ? "cue" : "repo");
  const landsFill = () => {
    for (const d of w.state?.desk?.projects || []) d.lands ||= landsView(d.id || d.project, landsPlace(d), {});
  };
  /** A lead's proposed change (desk/hub/lead/lands.rs): its card carries `desk_lands`; your Yes saves it, as the hub does. */
  const plainAnswer = w.answer;
  w.answer = (id, outcome, opts) => {
    const it = plainAnswer(id, outcome, opts);
    const p = it?.tool_input?.desk_lands;
    if (p && outcome === "answered — Yes") {
      landsFill();
      for (const x of w.state.desk?.projects || []) if ((x.id || x.project) === p.project) x.lands = landsView(p.project, x.lands.place, p.set);
      w.changed(10);
    }
    return it;
  };
  const plainSnapshot = w.snapshot;
  w.snapshot = () => { oneWayFill(); landsFill(); return plainSnapshot(); };

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
    // How work lands (desk/hub/core/view.rs desk_lands_set): saved for the Desk alone, refusing a choice not built yet or
    // one for Cue's own project on another's.
    desk_lands_set: ({ desk, lands }) => {
      landsFill();
      const d = (w.state.desk?.projects || []).find((x) => x.id === desk || x.project === desk);
      if (!d) throw "no such Desk";
      const place = d.lands.place;
      for (const [part] of LANDS_PARTS) if (lands[part] && landsWhy(part, lands[part], place)) throw `${part}: ${lands[part]} can't be picked (${landsWhy(part, lands[part], place)})`;
      d.lands = landsView(d.lands.project, place, lands);
      const view = d.lands;
      w.changed(10);
      return view;
    },
    // Pausing a whole Desk (desk/hub/away/pause.rs desk_pause_set): its `paused` view, as the hub returns it.
    desk_pause: ({ desk, on }) => {
      const d = (w.state.desk?.projects || []).find((x) => x.id === desk || x.project === desk);
      if (!d) throw "no such Desk";
      d.paused = on ? { on: true, since: Date.now(), by: "You", held: 0 } : { on: false };
      w.changed(10);
      return d.paused;
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
    clipboard_has_picture: () => false,
    copy_text: () => null,
    page_look_said: () => null,
    test_notification: () => "Sent a test notification",
  };
});
