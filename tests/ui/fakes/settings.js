// Fakes for src-tauri/src/ipc/settings.rs: settings, drafts, the clipboard, links, notifications, updates, and
// connecting an agent's hooks. Drafts the real app keeps in ~/.cue/cue.db are kept in this browser here.
CueBridge.area("settings", (w) => {
  const DRAFTS = "preview.drafts";
  const drafts = () => { try { return JSON.parse(localStorage.getItem(DRAFTS) || "{}"); } catch { return {}; } };
  // A 1×1 PNG: what a pasted or attached image reads as.
  const PIXEL = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

  return {
    // "a.b" sets settings.a.b, as config.rs set does for the keys the window saves.
    set_setting: ({ key, value }) => {
      const [a, b] = key.split(".");
      if (b == null) w.state.settings[a] = value; else (w.state.settings[a] ||= {})[b] = value;
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
    update_check: () => ({ current: "0.1.86" }),
    update_install: () => null,
    open_link: () => null,
    image_data: () => `data:image/png;base64,${PIXEL}`,
    clipboard_image: () => null,
    copy_text: () => null,
    test_notification: () => "Sent a test notification",
  };
});
