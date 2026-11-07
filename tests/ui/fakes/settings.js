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

  return {
    // "a.b" sets settings.a.b, as config.rs set does for the keys the window saves.
    set_setting: ({ key, value }) => {
      const [a, b] = key.split(".");
      if (b == null) w.state.settings[a] = value; else (w.state.settings[a] ||= {})[b] = value;
      if (key === "project.moves") moveProjects(value || {});
      w.changed(10);
      return null;
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
    update_check: () => (w.state.settings.updates?.beta ? { current: "0.1.86", version: "0.1.87", notes: "A beta build." } : { current: "0.1.86" }),
    update_install: () => null,
    open_link: () => null,
    image_data: () => `data:image/png;base64,${PIXEL}`,
    clipboard_image: () => null,
    copy_text: () => null,
    test_notification: () => "Sent a test notification",
  };
});
