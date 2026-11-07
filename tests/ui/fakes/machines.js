// Fakes for src-tauri/src/ipc/machines.rs: + New → Machine. Two machines: build-box connected, gpu-1 needing a login.
CueBridge.area("machines", () => {
  const machines = [{ name: "build-box", host: "me@build-box" }, { name: "gpu-1", host: "gpu-1" }];
  const named = (name) => machines.find((m) => m.name === name);
  // Where Cue's hooks are set up already: setting them up again does nothing.
  const hooked = new Set(["build-box"]);

  return {
    machines_list: () => structuredClone(machines),
    machine_add: ({ host, name }) => { const m = { name: name || host.replace(/^[^@]*@/, ""), host }; machines.push(m); return m; },
    machine_rename: ({ name, label }) => { const m = named(name); if (!m) throw `No machine called ${name}`; m.label = label.trim(); return { ...m }; },
    machine_remove: ({ name }) => { machines.splice(machines.indexOf(named(name)), 1); return null; },
    machine_setup: ({ name }) => {
      if (!named(name)) throw "that machine isn't added";
      if (hooked.has(name)) return [];
      hooked.add(name);
      return ["Set up Cue's hooks for Claude Code (~/.claude/settings.json, backed up first)"];
    },
    machine_check: ({ name }) => name === "build-box"
      ? { ok: true, os: "Linux", tools: { claude: true, codex: false, pi: false, tmux: true, node: true, npm: true }, help: { codex: "npm install -g @openai/codex", pi: "npm install -g @earendil-works/pi-coding-agent" } }
      : { ok: false, needs_login: true, error: `${name}: Permission denied (publickey,password).` },
    machine_login: ({ name }) => `Opened a login to ${name} in your terminal`,
  };
});
