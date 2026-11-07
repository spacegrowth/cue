// Fakes for src-tauri/src/extensions.rs: the installed extensions, switching them on, what they did, and the git
// and program calls their pages make. ?xt=1: three of them: one on, one updated and asking for more, one off.
CueBridge.area("extensions", (w) => {
  const root = "/Users/me/Library/Application Support/dev.spacegrowth.cue/extensions";
  const ext = (name, title, version, capabilities, o = {}) => ({ name, title, version, capabilities, programs: [], on: true, enabled: true, pending_capabilities: [], pending_programs: [], folder: `${root}/${name}`, ...o });
  const list = w.q.has("xt") ? [
    ext("boxes", "Containers", "0.3.0", ["sessions.read", "programs.run"], { panel: "Containers", add: "A new container", programs: ["docker", "mutagen"] }),
    ext("diff", "Diff", "0.4.0", ["sessions.read", "git.read"], { tab: "Diff" }),
    ext("notes", "Session notes", "0.3.0", ["sessions.read", "cmd.run"], { tab: "Notes", programs: ["git"], enabled: false, pending_capabilities: ["cmd.run"], pending_programs: ["git"] }),
    ext("digest", "Daily digest", "0.1.0", ["sessions.read"], { on: false, enabled: false }),
  ] : [];
  /** "<unix seconds> <line>", as extensions.rs keeps its activity log; `ago` in seconds. */
  const line = (ago, text) => `${Math.floor(w.now() / 1000) - ago} ${text}`;
  const activity = {
    diff: [line(600, "diff switched on in Settings"),
      line(180, "diff [~] git --no-optional-locks -C /Users/me/code/agent-relay diff 3f9a1c2 --no-color --no-ext-diff -> 0"),
      line(180, "diff [~] git --no-optional-locks -C /Users/me/code/agent-relay ls-files --others --exclude-standard -> 0"),
      line(40, "diff [build-box:~] git --no-optional-locks -C /srv/api diff HEAD --no-color -> 0")],
  };

  return {
    ext_list: () => ({ root, programs_allowed: false, list: structuredClone(list) }),
    ext_enable: ({ name, on }) => { const x = list.find((e) => e.name === name); if (!x) throw "No such extension"; x.on = on; x.enabled = on; return null; },
    ext_activity: ({ name, n }) => (activity[name] || []).slice(-n),
    ext_reveal: () => null,
    ext_log: () => null,
    // What git printed, as extensions.rs returns it.
    ext_git: ({ op }) => ({ code: 0, stdout: op === "diff" ? "diff --git a/src/board.ts b/src/board.ts\n+  footer.push(fmt(total));\n" : "", ms: 12 }),
    ext_program: ({ program }) => ({ code: 0, stdout: `${program}: ok\n`, stderr: "" }),
  };
});
